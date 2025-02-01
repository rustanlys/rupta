# RUPTA: A Pointer Analysis Framework for Rust

> :warning: [Documentation for RUPTA] is under development.

This open-source framework, RUPTA, supports pointer/alias analysis for Rust, operating on Rust MIR. It currently offers callsite-based pointer analysis, 
as detailed in our CC'24 paper (https://dl.acm.org/doi/10.1145/3640537.3641574). 
## Requirements

* Rust nightly and components, as specified in [rust-toolchain](rust-toolchain.toml).

## Build

1. Clone the repository

2. Build & install
    
    You can build RUPTA in two different ways:    

    ```sh
    $ cargo build
    ```
    
    This command generates two binaries, `cargo-pta` and `pta`, in the `target/debug` directory.

    You can also install RUPTA into `cargo`:

    ```sh
    $ cargo --locked install --path .
    ```
    
    This enables you to perform pointer analysis on a Rust project using the command `cargo pta`, similar to other `cargo` commands such as `cargo fmt`.
    

## Usage

You can run RUPTA for **a Rust project** using the binary `cargo-pta`:

```sh
$ cargo-pta pta -- --entry <entry-function-name> --pta-type <pta-type> --context-depth <N> --dump-call-graph <call-graph-path> --dump-pts <pts-path>
```

You can also use the command `cargo pta` instead of `cargo-pta pta` if RUPTA has been installed into `cargo`.

Alternatively, you can run RUPTA for **a single file** using the binary `pta`:
    
```sh
$ pta <path-to-file> --entry-func <entry-function-name> --pta-type <pta-type> --context-depth <N> --dump-call-graph <call-graph-path> --dump-pts <pts-path>
```

Options:

* `<entry-function-name>`: Specifies the entry function. Default is `main()`.
* `<pta-type>`: Determines the type of pointer analysis. Options are `cs` (callsite-sensitive) or `ander` (andersen), with `cs` as the default.
* `context-depth`: Sets the depth of contexts in callsite-sensitive analysis. Default is 1.
* `dump-call-graph`: Outputs the call graph in DOT format.
* `dump-pts`: Outputs the points-to analysis results.
* `dump-mir`: Outputs the MIR for all reachable functions.

Note: RUPTA requires substantial computational and memory resources to analyze large Rust projects. If you encounter excessively long analysis times—often due to many functions reachable from main() during the analysis—consider upgrading to a more powerful computing platform equipped with additional memory (e.g., 128GB) and faster CPUs.

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

We have released the RUPTA source code to support the wider research community and facilitate advancements in the field. We hope it is valuable to your projects. Please credit our contribution by citing our papers in any publications or presentations that utilize our tool:
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

@inproceedings{li2024context,
  title={Stack Filtering: Elevating Precision and Efficiency in Rust Pointer Analysis}, 
  author={Li, Wei and He, Dongjie and Chen, Wenguang and Xue, Jingling},
  booktitle={Proceedings of the 21st Annual IEEE/ACM International Symposium on Code Generation and Optimization},
  year={2025},
}
```

## Contacts

Any comments, contributions and collaborations are welcome. Please contact the authors [Wei Li](mailto:<liwei@cse.unsw.edu.au>) or [Jingling Xue](mailto:jingling@cse.unsw.edu.au) if you have any questions.
