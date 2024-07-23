---
layout: default
---


## What is RUPTA?

RUPTA is a pointer analysis framework designed for Rust. It identifies relationships between pointer variables and the memory locations they reference within a Rust program. RUPTA operates on Rust's Mid-level Intermediate Representation (MIR) and currently supports k-callsite context-sensitivity.

RUPTA can also be used to [construct precise call graphs](https://github.com/rustanlys/rupta/wiki/Analyze-a-Simple-Rust-Program#dump-the-call-graph) for Rust programs.

We refer to [SVF](https://svf-tools.github.io/SVF) for a pointer analysis framework that works for C/C++, and [Qilin](https://qilinpta.github.io/Qilin) for a pointer analysis framework that works for Java.

## How to setup RUPTA?

Please download the [source code]({{ site.github.source_code_url }}) of RUPTA and consult the [step-by-step guide](https://github.com/rustanlys/rupta/wiki/Setup-Guide) for setting up RUPTA.
## How to use RUPTA?

The main RUPTA executable is a stub that triggers the RUST compiler, incorporating a callback that activates RUPTA during the Rust compilation process. It analyzes a Rust program by processing the source code as its input. Additionally, RUPTA integrates seamlessly with Cargo, enabling the compilation of a Rust project along with its dependencies and allowing analysis through RUPTA using a single command, `cargo pta`.

Please refer to [this user guide](https://github.com/rustanlys/rupta/wiki/User-Guide) to run RUPTA with [a simple example](https://github.com/rustanlys/rupta/wiki/Analyze-a-Simple-Rust-Program) and generate the [analysis outputs](https://github.com/rustanlys/rupta/wiki/User-Guide#output-options) on your local machine.

Please refer to this [documentation]({{ site.github.doc_url }}) to understand the internal workings of RUPTA.

## License

GPLv3

## Reference

You are welcome to use RUPTA for research and development purposes under the provided license. Please acknowledge the use of this tool by citing our CC'24 paper, and possibly the relevant CGO paper listed below.

Wei Li, Dongjie He, Wenguang Chen and Jingling Xue. [Stack Filtering: Elevating Precision and Efficiency in Rust Pointer Analysis](), IEEE/ACM International Symposium on Code Generation and Optimization (CGO'25). [Accepted]()

Wei Li, Dongjie He, Yujiang Gui, Wenguang Chen, and Jingling Xue. [A Context-Sensitive Pointer Analysis Framework for Rust and Its Application to Call Graph Construction](https://doi.org/10.1145/3640537.3641574), 33rd ACM SIGPLAN International Conference on Compiler Construction (CC'24). 

## Contacts

Any comments, contributions and collaborations are welcomed. Please contact the authors [Wei Li](mailto:liwei@cse.unsw.edu.au) or [Jingling Xue](mailto:jingling@cse.unsw.edu.au) if you have any questions.

