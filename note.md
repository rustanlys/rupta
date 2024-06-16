# Rupta代码阅读分析笔记

## rust-analyzer对rustc_private组件报红线的解决方案

[这个问答](https://users.rust-lang.org/t/rust-analyzer-fails-to-index-due-to-unresolved-external-crate-in-a-rustc-private-project/105909) 回答了Rust Analyzer对rustc_private组件报unresolve extern crate的解决方案，总结为4步：

1. 给rustup安装新组件，`rustup component add rustc-dev`

2. 在VS Code的设置中，将`rust-analyzer.rustc.source`设置为`discover`

3. 在当前`crate`包的`Cargo.toml`中填上这样两行：

   ```toml
   [package.metadata.rust-analyzer]
   rustc_private = true
   ```

4. 重启Rust Analyzer

## 入口在哪里？

通过`cargo metadata`命令获取关于Rupta crate的元信息。得知该crate有三个编译目标（target）：

- `src/lib.rs` (lib目标)
- `src/bin/cargo-pta.rs` (bin目标)
  - 分析Rust Package时使用的`cargo-pta pta ...`
- `src/bin/pta.rs` (bin目标)
  - 分析单个.rs文件时使用的`pta ...`

由于我们的需求是分析整个Rust Package，故我们的代码阅读分析笔记也将从该文件开始。

## `/src/bin/cargo-pta.rs`的分析

```mermaid
graph
Main[main]
BranchMain{命令行参数是什么？}
CallCargo[call_cargo]
GetCargoMegadata["run `cargo metadata`"]
BranchCallCargo{情况是什么？}
CallCargoOnTarget[call_cargo_on_target]
RunCargoCheck[run `cargo check`，过程中对rustc的调用会重定向到call_rustc_or_pta函数]
CallCargoOnEachPackageTarget[call_cargo_on_each_package_target]

CallRustcOrPta[call_rustc_or_pta]
BranchCallRustcOrPta{环境变量和命令行参数的一致性？}
RunPta[run `pta`]
RunRustc[run `rustc`]


Main --> BranchMain
BranchMain --"若命令为cargo pta ..."--> CallCargo
BranchMain --"若命令为cargo rustc ..."--> CallRustcOrPta

CallCargo --"为了获取待分析Package的元信息"--> GetCargoMegadata
CallCargo --> BranchCallCargo
BranchCallCargo --"指定了bin目标"--> CallCargoOnTarget --> RunCargoCheck
BranchCallCargo --"发现了根Package"--> CallCargoOnEachPackageTarget --> 遍历该Package中的所有target --> CallCargoOnTarget
BranchCallCargo --"else"--> 遍历该Package中Workspace的所有成员Package --> CallCargoOnEachPackageTarget

CallRustcOrPta --> BranchCallRustcOrPta --"一致"--> RunPta --> 控制权交给src/bin/pta.rs
BranchCallRustcOrPta --"else"--> RunRustc
```

## `/src/bin/pta.rs`的分析

```mermaid
graph
RustcAPI[rustc_driver::catch_fatal_errors，利用PTACallbacks中定义的回调函数进行分析]
Main[main] --> 从PTA_FLAGS中加载参数 --> 从命令行参数中加载参数 --> RustcAPI
```

main(45)
AnalysisOptions::parse_from_args

## 总体修改思路
