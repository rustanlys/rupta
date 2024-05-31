# RUPTA: A Pointer Analysis Framework for Rust

> :warning: [Documentation for RUPTA] is under development.

This is an open-source framework being developed to support pointer/alias analysis for Rust, operating on ust MIR. Currently, it supports
callsite-based pointer analysis, as described in our CC'04 paper (https://dl.acm.org/doi/10.1145/3640537.3641574).

## Requirements

* Rust nightly and components, as specified in [rust-toolchain](rust-toolchain.toml).

## Build

1. Clone the repository

2. Build & install

    ```sh
    # You can build and install the cargo subcommand:
    $ cargo --locked install --path .

    # Or, you can only build the checker itself:
    $ cargo build
    ```

## Usage

Before using this tool, make sure your Rust project compiles without any errors or warnings.

```sh
# You can run the pta for a rust project
$ cargo pta -- --entry <entry-function-name> --pta-type <pta-type> --context-depth <N> --dump-call-graph <call-graph-path> --dump-pts <pts-path>

# Or, you can directly run the pta for a single file
$ target/debug/pta <path-to-file> --entry <entry-function-name> --pta-type <pta-type> --context-depth <N> --dump-call-graph <call-graph-path> --dump-pts <pts-path>
```

* `<entry-function-name>` is the entry function. The default value is `main`.
* `<pta-type>` is the pointer analysis type. Currently, `cs` (`context-sensitive`) and `ander`(`andersen`) are supported. The default value is `cs`.
* `context-depth` controls the depth of contexts in a context-sensitive pointer analysis. The default value is 1.
* `dump-call-graph` dumps the generated call graph in DOT format to the given path. 
* `dump-pts` dumps the points-to result to the given path.
* `dump-mir` dumps the mir of reachable functions.

## LOG

Set `PTA_LOG` environment variable to enable logging:

```sh
$ export PTA_LOG=info
```

## Troubleshooting

You may encounter error while loading shared libraries: librustc_driver.so, try setting:

```sh
$ export LD_LIBRARY_PATH=$(rustc --print sysroot)/lib:$LD_LIBRARY_PATH
```

## License

See [LICENSE](LICENSE)

## Reference

We have released the RUPTA source code with the hope that it will benefit others. Please acknowledge the use of this tool by citing the following paper:

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

Any comments, contributions and collaborations are welcomed. Please contact the authors [Wei Li](mailto:<liwei@cse.unsw.edu.au>) or [Jingling Xue](mailto:jingling@cse.unsw.edu.au) if you have any questions.
