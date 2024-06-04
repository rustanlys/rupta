// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

//! This provides an implementation for the "cargo pta" subcommand.
//! 
//! The subcommand is the same as "cargo check" but with three differences:
//! 1) It implicitly adds the options "-Z always_encode_mir" to the rustc invocation.
//! 2) It calls `pta` rather than `rustc` for all the targets of the current package.
//! 3) It runs `cargo test --no-run` for test targets.

use cargo_metadata::Package;
use log::info;
use rustc_tools_util::VersionInfo;
use serde_json;
use std::env;
use std::ffi::OsString;
use std::ops::Index;
use std::path::Path;
use std::process::{Command, Stdio};

use rupta::util;

/// The help message for `cargo-pta`
const CARGO_PTA_HELP: &str = r#"Pointer analysis tool for Rust programs
Usage:
    cargo pta
"#;

/// Set the environment variable `PTA_BUILD_STD` to enable the building of std library when running pta.
const PTA_BUILD_STD: &str = "PTA_BUILD_STD";

pub fn main() {
    if std::env::args().take_while(|a| a != "--").any(|a| a == "--help" || a == "-h") {
        println!("{}", CARGO_PTA_HELP);
        return;
    }
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        let version_info = rustc_tools_util::get_version_info!();
        println!("{}", version_info);
        return;
    }

    match std::env::args().nth(1).as_ref().map(AsRef::<str>::as_ref) {
        Some(s) if s.ends_with("pta") => {
            // Get here for the top level cargo execution, i.e. "cargo pta".
            call_cargo();
        }
        Some(s) if s.ends_with("rustc") => {
            // 'cargo rustc ..' redirects here because RUSTC_WRAPPER points to this binary.
            // execute rustc with PTA applicable parameters for dependencies and call PTA
            // to analyze targets in the current package.
            call_rustc_or_pta();
        }
        Some(arg) => {
            eprintln!(
                "`cargo-pta` called with invalid first argument: {arg}; please only invoke this binary through `cargo pta`" 
            );
        }
        _ => {
            eprintln!("current args: {:?}", std::env::args());
            eprintln!("`cargo-pta` called without first argument; please only invoke this binary through `cargo pta`");
        }
    }
}

/// Read the toml associated with the current directory and
/// recursively execute cargo for each applicable package target/workspace member in the toml
fn call_cargo() {
    let manifest_path = get_arg_flag_value("--manifest-path").map(|m| Path::new(&m).canonicalize().unwrap());

    let mut cmd = cargo_metadata::MetadataCommand::new();
    if let Some(ref manifest_path) = manifest_path {
        cmd.manifest_path(manifest_path);
    }

    let metadata = if let Ok(metadata) = cmd.exec() {
        metadata
    } else {
        eprintln!("Could not obtain Cargo metadata; likely an ill-formed manifest");
        std::process::exit(1);
    };

    // If a binary is specified, analyze this binary only.
    if let Some(target) = get_arg_flag_value("--bin") {
        call_cargo_on_target(&target, "bin");
        return;
    }

    if let Some(root) = metadata.root_package() {
        call_cargo_on_each_package_target(root);
        return;
    }

    // There is no root, this must be a workspace, so call_cargo_on_each_package_target on each workspace member
    for package_id in &metadata.workspace_members {
        let package = metadata.index(package_id);
        call_cargo_on_each_package_target(package);
    }
}

fn call_cargo_on_each_package_target(package: &Package) {
    for target in &package.targets {
        let kind = target.kind.get(0).expect("bad cargo metadata: target::kind");
        call_cargo_on_target(&target.name, kind);
    }
}

fn call_cargo_on_target(target: &String, kind: &str) {
    // Build a cargo command for target
    let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo")));
    match kind {
        "bin" => {
            cmd.arg("check");
            if get_arg_flag_value("--bin").is_none() {
                cmd.arg("--bin").arg(target);
            }
        }
        "lib" => {
            cmd.arg("check");
            cmd.arg("--lib");
        }
        "test" => {
            cmd.arg("test");
            cmd.arg("--no-run");
        }
        _ => {
            return;
        }
    }
    cmd.arg("--verbose");

    let mut args = std::env::args().skip(2);
    // Add cargo args to cmd until first `--`.
    for arg in args.by_ref() {
        if arg == "--" {
            break;
        }
        cmd.arg(arg);
    }

    // Enable Cargo to compile the standard library from source code as part of a crate graph compilation.
    if env::var(PTA_BUILD_STD).is_ok() {
        cmd.arg("-Zbuild-std");

        if !has_arg_flag("--target") {
            let toolchain_target = toolchain_target().expect("could not get toolchain target");
            cmd.arg("--target").arg(toolchain_target);
        }
    }

    // Serialize the remaining args into an environment variable.
    let args_vec: Vec<String> = args.collect();
    if !args_vec.is_empty() {
        cmd.env(
            "PTA_FLAGS",
            serde_json::to_string(&args_vec).expect("failed to serialize args"),
        );
    }

    // Force cargo to recompile all dependencies with PTA friendly flags
    cmd.env("RUSTFLAGS", "-Z always_encode_mir");

    // Replace the rustc executable through RUSTC_WRAPPER environment variable so that rustc
    // calls generated by cargo come back to cargo-pta.
    let path = std::env::current_exe().expect("current executable path invalid");
    cmd.env("RUSTC_WRAPPER", path);

    // Communicate the name of the root crate to the calls to cargo-pta that are invoked via
    // the RUSTC_WRAPPER setting.
    cmd.env("PTA_CRATE", target.replace('-', "_"));

    // Communicate the target kind of the root crate to the calls to cargo-pta that are invoked via
    // the RUSTC_WRAPPER setting.
    cmd.env("PTA_TARGET_KIND", kind);

    // Set the tool chain to be compatible with pta
    if let Some(toolchain) = option_env!("RUSTUP_TOOLCHAIN") {
        cmd.env("RUSTUP_TOOLCHAIN", toolchain);
    }

    // Execute cmd
    info!("cmd: {:?}", cmd);
    let exit_status = cmd
        .spawn()
        .expect("could not run cargo")
        .wait()
        .expect("failed to wait for cargo");

    if !exit_status.success() {
        std::process::exit(exit_status.code().unwrap_or(-1))
    }
}

fn call_rustc_or_pta() {
    if let Some(crate_name) = get_arg_flag_value("--crate-name") {
        if let Ok(pta_crate) = std::env::var("PTA_CRATE") {
            if crate_name.eq(&pta_crate) {
                if let Ok(kind) = std::env::var("PTA_TARGET_KIND") {
                    if let Some(t) = get_arg_flag_value("--crate-type") {
                        if kind.eq(&t) {
                            call_pta();
                            return;
                        }
                    } else if kind == "test" {
                        call_pta();
                        return;
                    }
                }
            }
        }
    }
    call_rustc()
}

fn call_pta() {
    let mut path = std::env::current_exe().expect("current executable path invalid");
    let extension = path.extension().map(|e| e.to_owned());
    path.pop(); // remove the cargo_pta bit
    path.push("pta");
    if let Some(ext) = extension {
        path.set_extension(ext);
    }
    let mut cmd = Command::new(path);
    cmd.args(std::env::args().skip(2));
    let exit_status = cmd
        .spawn()
        .expect("could not run pta")
        .wait()
        .expect("failed to wait for pta");

    if !exit_status.success() {
        std::process::exit(exit_status.code().unwrap_or(-1))
    }
}

fn call_rustc() {
    // todo: invoke the rust compiler for the appropriate tool chain?
    let mut cmd = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc")));
    cmd.args(std::env::args().skip(2));
    let exit_status = cmd
        .spawn()
        .expect("could not run rustc")
        .wait()
        .expect("failed to wait for rustc");

    if !exit_status.success() {
        std::process::exit(exit_status.code().unwrap_or(-1))
    }
}

/// Determines whether a flag `name` is present before `--`.
/// For example, has_arg_flag("-v")
fn has_arg_flag(name: &str) -> bool {
    let mut args = std::env::args().take_while(|val| val != "--");
    args.any(|val| val == name)
}

/// Gets the value of `name`.
/// `--name value` or `--name=value`
fn get_arg_flag_value(name: &str) -> Option<String> {
    let mut args = std::env::args().take_while(|val| val != "--");
    loop {
        let arg = match args.next() {
            Some(arg) => arg,
            None => return None,
        };
        if !arg.starts_with(name) {
            continue;
        }
        // Strip `name`.
        let suffix = &arg[name.len()..];
        if suffix.is_empty() {
            // This argument is `name` and the next one is the value.
            return args.next();
        } else if let Some(arg_value) = suffix.strip_prefix('=') {
            return Some(arg_value.to_owned());
        }
    }
}

/// Returns the target of the toolchain, e.g. "x86_64-unknown-linux-gnu".
fn toolchain_target() -> Option<String> {
    let sysroot = util::find_sysroot();

    // get the supported rustup targets
    let output = String::from_utf8(
        Command::new("rustup")
            .arg("target")
            .arg("list")
            .stdout(Stdio::piped())
            .output()
            .expect("could not run 'rustup target list'")
            .stdout,
    )
    .unwrap();

    let target = output.lines().find_map(|line| {
        let target = line.split_whitespace().next().unwrap().to_owned();
        if sysroot.ends_with(&target) {
            Some(target)
        } else {
            None
        }
    });

    target
}
