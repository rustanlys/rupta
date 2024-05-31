# RUPTA: A Pointer Analysis Framework for Rust

> :warning: [Documentation for RUPTA] is under development.

This open-source framework, RUPTA, supports pointer/alias analysis for Rust, operating on Rust MIR. It currently offers callsite-based pointer analysis, 
as detailed in our CC'04 paper (https://dl.acm.org/doi/10.1145/3640537.3641574). 
## Requirements

* Rust nightly and components, as specified in [rust-toolchain](rust-toolchain.toml).

## Build

1. Clone the repository

2. Build & install

    ```sh
    # You can build and install RUPTA using the cargo subcommand:
    $ cargo --locked install --path .

    # Or, you can build only RUPTA itself:
    $ cargo build
    ```

## Usage

Before using RUPTA, please ensure your Rust project compiles without errors or warnings.

```sh
# You can run RUPTA for a rust project:
$ cargo pta -- --entry <entry-function-name> --pta-type <pta-type> --context-depth <N> --dump-call-graph <call-graph-path> --dump-pts <pts-path>

# Or, you can run RUPTA for a single file:
$ target/debug/pta <path-to-file> --entry <entry-function-name> --pta-type <pta-type> --context-depth <N> --dump-call-graph <call-graph-path> --dump-pts <pts-path>
```

* `<entry-function-name>`: Specifies the entry function. Default is `main()`.
* `<pta-type>`: Determines the type of pointer analysis. Options are `cs` (context-sensitive) or `ander` (andersen), with `cs` as the default.
* `context-depth`: Sets the depth of contexts in context-sensitive analysis. Default is 1.
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
