// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

//! This provides an implementation for the "cargo pta" subcommand.
//! 以下代码是"cargo pta"子命令的实现。
//!
//! The subcommand is the same as "cargo check" but with three differences:
//! 该子命令和"cargo check"几乎相同，但有三个不同之处：
//! 1) It implicitly adds the options "-Z always_encode_mir" to the rustc invocation.
//!    它隐式地将选项"-Z always_encode_mir"添加到调用中。
//! 2) It calls `pta` rather than `rustc` for all the targets of the current package.
//!    它为当前Rust Package的所有目标调用`pta`，而不是`rustc`。
//! 3) It runs `cargo test --no-run` for test targets.
//!    它运行`cargo test --no-run`来测试目标。

use cargo_metadata::Package;
use log::info;
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
/// 似乎设置了这个环境变量之后，在运行pta时才能构建rust-std标准库。
const PTA_BUILD_STD: &str = "PTA_BUILD_STD";

pub fn main() {
    //! 注意：std::env::agrs()中的内容类似如下形式：
    //! ["cargo-pta", "pta", "--", "--entry-func", ...]

    // 1. 取所有 -- 之前的参数
    // 2. 如果有 --help 或 -h 参数，打印帮助信息并退出
    if std::env::args()
        .take_while(|a| a != "--")
        .any(|a| a == "--help" || a == "-h")
    {
        println!("{}", CARGO_PTA_HELP);
        return;
    }

    // 4. 排除以上情况，进入主业务逻辑
    match std::env::args().nth(1).as_ref().map(AsRef::<str>::as_ref) {
        Some(s) if s.ends_with("pta") => {
            // Get here for the top level cargo execution, i.e. "cargo pta".
            // 对pta的直接调用将会来到该分支，例如"cargo pta"。
            call_cargo();
        }
        Some(s) if s.ends_with("rustc") => {
            // 'cargo rustc ..' redirects here because RUSTC_WRAPPER points to this binary.
            // 由于RUSTC_WRAPPER指向了本程序，因此'cargo rustc ..'将会重定向到这里。
            // execute rustc with PTA applicable parameters for dependencies and call PTA
            // to analyze targets in the current package.
            // 运行call_cargo()之后，由于RUSTC_WRAPPER的存在，会调用本程序，因此这里会代替rustc执行命令。
            call_rustc_or_pta();
        }
        // 以下均为错误处理逻辑，不必过多在意
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
/// 读取当前目录下的Cargo.toml文件，并递归地对其中每个可用的Rust Package目标/工作区成员执行cargo命令。
fn call_cargo() {
    // 如果有指定待分析Rust Package的Cargo.toml路径，则使用该路径；否则使用当前目录下的Cargo.toml文件。
    let manifest_path = get_arg_flag_value("--manifest-path").map(|m| {
        Path::new(&m)
            .canonicalize() /* 规格化，将类似于test/../test之类的东西干掉，形成最简的路径表示 */
            .unwrap()
    });

    // 运行一次`cargo metadata`来获取当前Rust Package的相关信息
    let mut cmd = cargo_metadata::MetadataCommand::new();
    // 如果有指定待分析Rust Package的Cargo.toml路径，则使用该路径；否则使用当前目录下的Cargo.toml文件。
    if let Some(ref manifest_path) = manifest_path {
        cmd.manifest_path(manifest_path);
    }
    // 运行`cargo metadata`命令，并获取结果
    let metadata = if let Ok(metadata) = cmd.exec() {
        metadata
    } else {
        eprintln!("Could not obtain Cargo metadata; likely an ill-formed manifest");
        std::process::exit(1);
    };
    // 接下来分为几种不同情况：
    // 1. 如果用户指定了分析某一个特定的bin目标，则只分析该目标
    if let Some(target) = get_arg_flag_value("--bin") {
        call_cargo_on_target(&target, "bin");
        return;
    }
    // 2. 如果metadata指示当前Workspace存在根Package，则分析该Package中的所有目标
    //    这里解释一下：Workspace就是一大堆Package，它们共享同一个输出目录（/target）和同一个Cargo.lock文件。
    //    每一个Workspace下的Package都成为该Workspace的成员（member）。
    //    如果存在根Package，那么它就是整个Workspace的入口，从此处进入分析即可。
    //    否则，说明当前Workspace没有根Package，因此需要分析每个成员Package中的目标。
    if let Some(root) = metadata.root_package() {
        call_cargo_on_each_package_target(root);
        return;
    }

    // There is no root, this must be a workspace, so call_cargo_on_each_package_target on each workspace member
    // 3. 没有根Package，只能对Workspace中的每个成员进行分析
    for package_id in &metadata.workspace_members {
        let package = metadata.index(package_id);
        call_cargo_on_each_package_target(package);
    }
}

/// 对Package内的所有target，先获取他们的类型（bin、lib、test）和名字，然后运行`call_cargo_on_target`
fn call_cargo_on_each_package_target(package: &Package) {
    let lib_only = has_arg_flag("--lib");
    for target in &package.targets {
        let kind = target
            .kind
            .first()
            .expect("bad cargo metadata: target::kind");
        if lib_only && kind != "lib" {
            continue;
        }
        call_cargo_on_target(&target.name, kind);
    }
}

/// 构造cargo命令，分析某个特定的目标，例如"cargo pta --bin my_bin"。
fn call_cargo_on_target(target: &String, kind: &str) {
    // 准备运行`cargo`命令。先试图从环境变量$CARGO中寻找cargo可执行文件，如果找不到，则使用默认值"cargo"。
    let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo")));
    match kind {
        "bin" => {
            cmd.arg("check");
            // 按理来说，应该是从命令行里找到待分析的bin目标，并添加到命令中。
            // 但是如果找不到，就得用传入的target来指定了。
            if get_arg_flag_value("--bin").is_none() {
                cmd.arg("--bin").arg(target);
            }
        }
        "lib" => {
            cmd.arg("check");
            // 由于一个Package只能有一个lib目标，因此无需显式指定，cargo自己能找到的。
            cmd.arg("--lib");
        }
        "test" => {
            cmd.arg("test");
            // 还记得pta和cargo check的第3点区别吗？
            // 这里的--no-run参数是为了避免运行测试用例，而只分析测试用例的函数调用关系。
            cmd.arg("--no-run");
        }
        _ => {
            return;
        }
    }

    // verbose adj. 冗长的，啰嗦的，详细的
    cmd.arg("--verbose");

    // 将形如下列中括号中的内容（前两个参数之后，--之前）传递给`cargo check`或`cargo test`。
    // cargo-pta pta [--release] -- --entry-func main
    let mut args = std::env::args().skip(2);
    // 涨知识：by_ref能借走迭代器，而不会把迭代器直接用掉，真厉害！
    for arg in args.by_ref() {
        if arg == "--" {
            break;
        }
        if arg == "--lib" {
            continue;
        }
        cmd.arg(arg);
    }

    // Enable Cargo to compile the standard library from source code as part of a crate graph compilation.
    if env::var(PTA_BUILD_STD).is_ok() {
        // 给cargo指定build-std，它就会把rust-std现场编译一遍，不会使用已经编译好的成品
        cmd.arg("-Zbuild-std");
        // 如果传入的命令行参数中没有指定工具链，就自己找一个
        if !has_arg_flag("--target") {
            let toolchain_target = toolchain_target() /* 类似于x86_64-unknown-linux-gnu的东西 */
                .expect("could not get toolchain target");
            cmd.arg("--target").arg(toolchain_target);
        }
    }

    // 这个args就是传入的命令行参数，跳过了最开头的两个，剩下的所有内容了
    let args_vec: Vec<String> = args.collect();
    // 把这些剩下的参数序列化为json格式，然后塞进环境变量$PTA_FLAGS里
    if !args_vec.is_empty() {
        cmd.env(
            "PTA_FLAGS",
            serde_json::to_string(&args_vec).expect("failed to serialize args"),
        );
    }

    // 如果在这儿设置了日志相关的环境变量的话，原样传递到pta去
    // if let Ok(pta_log_level) = env::var("PTA_LOG") {
    //     println!("PTA_LOG={}", pta_log_level);
    //     cmd.env("PTA_LOG", pta_log_level);
    // }

    //* 接下来，要通过环境变量的方式将一些必要的信息传递给pta进程 */
    // Force cargo to recompile all dependencies with PTA friendly flags
    // 还记得pta和cargo check的第1点区别吗？
    cmd.env("RUSTFLAGS", "-Z always_encode_mir");

    // Replace the rustc executable through RUSTC_WRAPPER environment variable so that rustc
    // calls generated by cargo come back to cargo-pta.
    // 还记得pta和cargo check的第2点区别吗？利用RUSTC_WRAPPER环境变量来重定向rustc的调用到自身。
    let path = std::env::current_exe().expect("current executable path invalid");
    cmd.env("RUSTC_WRAPPER", path);

    // Communicate the name of the root crate to the calls to cargo-pta that are invoked via
    // the RUSTC_WRAPPER setting.
    // 例如"cargo pta --bin my_bin"中的my_bin。
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
    // 1. 检查命令行参数指定的crate名字是否存在
    if let Some(crate_name) = get_arg_flag_value("--crate-name") {
        // 2. 检查环境变量指定的crate名字是否存在
        if let Ok(pta_crate) = std::env::var("PTA_CRATE") {
            // 3. 如果都存在，则检查二者是否一致
            if crate_name.eq(&pta_crate) {
                // 4. 检查环境变量指定的crate类型Kind是否存在
                if let Ok(kind) = std::env::var("PTA_TARGET_KIND") {
                    // 5. 检查命令行参数指定的crate类型Kind是否存在
                    if let Some(t) = get_arg_flag_value("--crate-type") {
                        // 5.1. 若二者一致，则调用PTA
                        if kind.eq(&t) {
                            call_pta();
                            return;
                        }
                    } else if kind == "test" {
                        // 5.2. 虽然命令行参数没指定crate类型，但环境变量声称类型是test，也调用PTA
                        call_pta();
                        return;
                    }
                }
            }
        }
    }
    // 只要以上条件有任意一个没满足，就拒绝启动pta，转而调用rustc
    call_rustc()
}

fn call_pta() {
    // 当前可执行文件的所在路径
    let mut path = std::env::current_exe().expect("current executable path invalid");
    // 当前可执行文件的扩展名
    let extension = path.extension().map(|e| e.to_owned());
    // 去掉当前的可执行文件（一般是cargo_pta[.exe]）只留下可执行文件所在的目录的路径
    path.pop(); // remove the cargo_pta bit
                // 加入pta可执行文件，令path指向pta可执行文件
    path.push("pta");
    // 如果旧可执行文件带有扩展名，那么新构造的也应当加上扩展名
    if let Some(ext) = extension {
        path.set_extension(ext);
    }
    // 构造pta命令，类似于在命令行中键入`pta ...`
    let mut cmd = Command::new(path);
    // 把命令行参数的前两个跳过去，像这样：
    // cargo rustc [...] <-- 只取中括号包裹的部分
    cmd.args(std::env::args().skip(2));

    // if let Ok(pta_log_level) = env::var("PTA_LOG") {
    //     println!("PTA_LOG={}", pta_log_level);
    //     cmd.env("PTA_LOG", pta_log_level);
    // }

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
    // 尝试用环境变量中获得rustc所在的路径来运行rustc，如果没有，则用默认值"rustc"。
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

/// 取命令行参数中 -- 之前的内容，然后在其中寻找键等于`name`的值。
/// 支持--key value和--key=value两种格式。
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
        // 此时找到的参数的前缀和name吻合，现在判断是否有等号：
        let suffix = &arg[name.len()..];
        if suffix.is_empty() {
            // 1. 没有等号（--key value），返回--key的下一个参数value
            return args.next();
        } else if let Some(arg_value) = suffix.strip_prefix('=') {
            // 2. 有等号（--key=value），返回等号后面的value
            return Some(arg_value.to_owned());
        }
    }
}

/// Returns the target of the toolchain, e.g. "x86_64-unknown-linux-gnu".
/// 而且要求sysroot中安装的目标要和rustup支持的目标吻合。
fn toolchain_target() -> Option<String> {
    let sysroot = util::find_sysroot();

    // 运行rustup target list命令，获取rustup支持的所有编译目标
    // 其中已安装的目标会在后面注明(installed)
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
    // 在以上支持的目标中一个一个地找（一行就是一个）
    //
    let target = output.lines().find_map(|line| {
        // 把空格后面的东西（也就是那个"(installed)"）去掉，防止匹配不上
        let target = line.split_whitespace().next().unwrap().to_owned();
        if sysroot.ends_with(&target) {
            // rustup支持的目标和我们已安装的工具链的目标吻合，就用它辣！
            Some(target)
        } else {
            None
        }
    });

    target
}
