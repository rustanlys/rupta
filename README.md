# RUPTA: A Pointer Analysis Framework for Rust

> :warning: [Documentation for RUPTA] is under development.

This open-source framework, RUPTA, supports pointer/alias analysis for Rust, operating on Rust MIR. It currently offers callsite-based pointer analysis, 
as detailed in our CC'24 paper (https://dl.acm.org/doi/10.1145/3640537.3641574). 
## Requirements

* Rust nightly and components, as specified in [rust-toolchain](rust-toolchain.toml).

## Build

1. Clone the repository

2. Build & install
    
    Build RUPTA:    

    ```sh
    $ cargo build
    ```
    
    This command produces two binaries `cargo-pta` and `pta` at the directory `target/debug`. 
    
    You can also install RUPTA into `cargo`:

    ```sh
    $ cargo --locked install --path .
    ```
    
    This allows you to run pointer analysis for a Rust project using the command `cargo pta`, similar to using other `cargo` commands like `cargo fmt`.
    

## Usage

You can run RUPTA for **a Rust project** using the binary `cargo-pta`. 

```sh
$ cargo-pta pta -- --entry <entry-function-name> --pta-type <pta-type> --context-depth <N> --dump-call-graph <call-graph-path> --dump-pts <pts-path>
```

You can use the command `cargo pta` instead of `cargo-pta pta` here if RUPTA has been installed into `cargo`.
    
Or, you can run RUPTA for **a single file** using the binary `pta`:
    
```sh
$ pta <path-to-file> --entry <entry-function-name> --pta-type <pta-type> --context-depth <N> --dump-call-graph <call-graph-path> --dump-pts <pts-path>
```

Options:

* `<entry-function-name>`: Specifies the entry function. Default is `main()`.
* `<pta-type>`: Determines the type of pointer analysis. Options are `cs` (callsite-sensitive) or `ander` (andersen), with `cs` as the default.
* `context-depth`: Sets the depth of contexts in callsite-sensitive analysis. Default is 1.
* `dump-call-graph`: Outputs the call graph in DOT format.
* `dump-pts`: Outputs the points-to analysis results.
* `dump-mir`: Outputs the MIR for all reachable functions.

## LOG

Set the `PTA_LOG` environment variable to enable logging:

```sh
$ export PTA_LOG=info
```

## Troubleshooting

If you encounter errors loading shared libraries, such as `librustc_driver.so`, try setting:

```sh
$ export LD_LIBRARY_PATH=$(rustc --print sysroot)/lib:$LD_LIBRARY_PATH
```

## License

See [LICENSE](LICENSE)

## Reference

We have released the RUPTA source code to support the wider research community and facilitate advancements in the field. We hope it is valuable to your projects. Please credit our contribution by citing the following paper in any publications or presentations that utilize our tool:
```
@inproceedings{li2024context,
  title={A Context-Sensitive Pointer Analysis Framework for Rust and Its Application to Call Graph Construction},
  author={Li, Wei and He, Dongjie and Gui, Yujiang and Chen, Wenguang and Xue, Jingling},
  booktitle={Proceedings of the 33rd ACM SIGPLAN International Conference on Compiler Construction},
  pages={60--72},
  year={2024},
  publisher={ACM},
  doi = {10.1145/3640537.3641574}
}
```

## Contacts

Any comments, contributions and collaborations are welcome. Please contact the authors [Wei Li](mailto:<liwei@cse.unsw.edu.au>) or [Jingling Xue](mailto:jingling@cse.unsw.edu.au) if you have any questions.
