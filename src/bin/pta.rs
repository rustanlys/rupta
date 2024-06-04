// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

//! The main routine of `rupta`.
//! 
//! Implemented as a stub that invokes the rust compiler with a call back to execute 
//! pointer analysis during rust compilation.

#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_session;

use log::*;
use std::env;

use rupta::pta::PTACallbacks;
use rupta::util;
use rupta::util::options::AnalysisOptions;

fn main() {
    let early_dcx =
        rustc_session::EarlyDiagCtxt::new(rustc_session::config::ErrorOutputType::default());

    // Initialize loggers.
    if env::var("RUSTC_LOG").is_ok() {
        rustc_driver::init_rustc_env_logger(&early_dcx);
    }
    if env::var("PTA_LOG").is_ok() {
        let e = env_logger::Env::new()
            .filter("PTA_LOG")
            .write_style("PTA_LOG_STYLE");
        env_logger::init_from_env(e);
    }

    // Get any options specified via the PTA_FLAGS environment variable
    let mut options = AnalysisOptions::default();
    let pta_flags = env::var("PTA_FLAGS").unwrap_or_default();
    let pta_args: Vec<String> = serde_json::from_str(&pta_flags).unwrap_or_default();
    let rustc_args = options.parse_from_args(&pta_args[..], true);

    // Let arguments supplied on the command line override the environment variable.
    let mut args = env::args_os()
        .enumerate()
        .map(|(i, arg)| {
            arg.into_string().unwrap_or_else(|arg| {
                early_dcx.early_fatal(format!("Argument {i} is not valid Unicode: {arg:?}"))
            })
        })
        .collect::<Vec<_>>();

    // Setting RUSTC_WRAPPER causes Cargo to pass 'rustc' as the first argument.
    // We're invoking the compiler programmatically, so we remove it if present.
    if args.len() > 1 && std::path::Path::new(&args[1]).file_stem() == Some("rustc".as_ref()) {
        args.remove(1);
    }

    let mut rustc_command_line_arguments = options.parse_from_args(&args[1..], false);
    info!("PTA Options: {:?}", options);

    let result = rustc_driver::catch_fatal_errors(move || {
        // Add back the binary name
        rustc_command_line_arguments.insert(0, args[0].clone());

        // Add rustc arguments supplied via the MIRAI_FLAGS environment variable
        rustc_command_line_arguments.extend(rustc_args);

        let sysroot: String = "--sysroot".into();
        if !rustc_command_line_arguments.iter().any(|arg| arg.starts_with(&sysroot)) {
            // Tell compiler where to find the std library and so on.
            // The compiler relies on the standard rustc driver to tell it, so we have to do likewise.
            rustc_command_line_arguments.push(sysroot);
            rustc_command_line_arguments.push(util::find_sysroot());
        }

        let always_encode_mir: String = "always-encode-mir".into();
        if !rustc_command_line_arguments.iter().any(|arg| arg.ends_with(&always_encode_mir))
        {
            // Tell compiler to emit MIR into crate for every function with a body.
            rustc_command_line_arguments.push("-Z".into());
            rustc_command_line_arguments.push(always_encode_mir);
        }
        debug!("rustc command line arguments: {:?}", rustc_command_line_arguments);
        
        let mut callbacks = PTACallbacks::new(options);
        let compiler = rustc_driver::RunCompiler::new(&rustc_command_line_arguments, &mut callbacks);
        compiler.run()
    })
    .and_then(|result| result);

    let exit_code = match result {
        Ok(_) => rustc_driver::EXIT_SUCCESS,
        Err(_) => rustc_driver::EXIT_FAILURE,
    };

    std::process::exit(exit_code);
}
