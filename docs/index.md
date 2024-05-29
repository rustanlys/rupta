---
layout: default
---


## What is RUPTA 

RUPTA is a pointer analysis framework for Rust, identifiying relationships between pointer variables and the memory locations that they point to in a program. It functions on Rust's Mid-level Intermediate Representation (MIR) and supports k-callsite context-sensitivity. 

RUPTA can also be used to [construct precise call graphs](https://github.com/rustanlys/rupta/wiki/Analyze-a-Simple-Rust-Program#dump-the-call-graph) for Rust programs.

We refer to [SVF](https://svf-tools.github.io/SVF) for a pointer analysis framework that works for C and C++, and [Qilin](https://qilinpta.github.io/Qilin) for a pointer analysis framework that works for Java.

## How to setup RUPTA?

Please download the [source code]({{ site.github.source_code_url }}) of RUPTA and refer to this [step-by-step guide](https://github.com/rustanlys/rupta/wiki/Setup-Guide) to setup RUPTA.

## How to use RUPTA?

The main routine RUPTA executable is a stub that invokes the RUST compiler with a call back that invokes RUPTA as part of the rust compilation. It analyzes a Rust program by taking the source code as its input. 
RUPTA also smoothly integrates with Cargo, making it possible to compile a Rust project with dependencies and analyze with RUPTA with a single command `cargo pta`.

Please refer to [this user guide](https://github.com/rustanlys/rupta/wiki/User-Guide) to run RUPTA with [a simple example](https://github.com/rustanlys/rupta/wiki/Analyze-a-Simple-Rust-Program) and generate the [analysis outputs](https://github.com/rustanlys/rupta/wiki/User-Guide#output-options) on your local machine.

Please refer to this [documentation]({{ site.github.doc_url }}) to understand the internal working of RUPTA.

## License

GPLv3

## Reference

You are welcome to use RUPTA for research and development purposes under the license given. Please acknowledge the use of this tool by citing our CC'24 paper.

Wei Li, Dongjie He, Yujiang Gui, Wenguang Chen, and Jingling Xue. [A Context-Sensitive Pointer Analysis Framework for Rust and Its Application to Call Graph Construction](https://doi.org/10.1145/3640537.3641574), 33rd ACM SIGPLAN International Conference on Compiler Construction (CC'24). 

## Contacts

Any comments, contributions and collaborations are welcomed. Please contact the authors [Wei Li](mailto:liwei@cse.unsw.edu.au) or [Jingling Xue](mailto:jingling@cse.unsw.edu.au) if you have any questions.

