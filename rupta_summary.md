# Rupta改造 - 总结

## Rupta项目结构

Rupta基于MIRAI改进而来，因此在项目结构上具有一定相似性。

### 程序入口

通过`cargo metadata`命令获取关于Rupta crate的元信息，得知该crate有三个编译目标（target）：

- `src/lib.rs` (lib目标)
- `src/bin/cargo-pta.rs` (bin目标)
  - 分析Rust Package时使用的`cargo-pta pta ...`
  - 其实际作用是：初步解析命令行参数，根据参数判断调用rust编译器（下称rustc）还是pta（即`src/bin/pta.rs`）
- `src/bin/pta.rs` (bin目标)
  - 分析单个.rs文件时使用的`pta ...`
  - 其实际作用是：调用`rustc_driver::catch_fatal_errors`函数，调动rustc进行编译，并利用指定的回调函数对编译完成的MIR进行分析
    - 回调函数定义在`src/pta/mod.rs`的`PTACallbacks`中。从回调函数开始，Rupta的分析才真正开始

### 代码关键模块

#### FuncPAGBuilder

该结构体位于`src/builder/fpag_builder.rs`中，Rupta每分析一个函数都会执行一次`FuncPAGBuilder::new`，从这个构造函数中提取函数的`DefId`非常合适。

```rust
impl<'pta, 'tcx, 'compilation> FuncPAGBuilder<'pta, 'tcx, 'compilation> {
    pub fn new(
        acx: &'pta mut AnalysisContext<'tcx, 'compilation>,
        func_id: FuncId,
        mir: &'tcx mir::Body<'tcx>,
        fpag: &'pta mut FuncPAG,
    ) -> FuncPAGBuilder<'pta, 'tcx, 'compilation> {
    // -- snip --
    }
}
```

可以看到其中的函数是以`FuncId`唯一标记的。好在存在办法将`FuncId`转换为MIR中常见的`DefId`。

#### add_call_edge函数

该函数位于`src/pta/context_sensitive.rs`的`ContextSensitivePTA`的`impl`块中。作为负责向调用图添加代表调用关系的有向边的函数，它非常适合用于提取函数直接的调用信息，再将其收集到指定位置。它的函数签名如下：

```rust
impl<...> ContextSensitivePTA<...> {
    fn add_call_edge(&mut self, callsite: &Rc<CSCallSite>, callee: &CSFuncId) {
        // --snip --
    }
}
```

#### dump_result函数

Rupta输出信息的总出口位于`src/util/results_dumper.rs`的`dump_result`函数中，函数签名如下：

```rust
pub fn dump_results<P: PAGPath, F, S>(
    acx: &AnalysisContext,
    call_graph: &CallGraph<F, S>,
    pt_data: &DiffPTDataTy,
    pag: &PAG<P>,
) where
    F: CGFunction + Into<FuncId>,
    S: CGCallSite + Into<BaseCallSite>,
    <P as PAGPath>::FuncTy: Ord + std::fmt::Debug + Into<FuncId> + Copy
{
    // --snip --
}
```

虽已知该函数输出分析信息，但这些信息实际上源自于`ContextSensitivePTA`的成员变量，它们在`finalize`方法中被真正输出：

```rust
impl<...> ContextSensitivePTA<...> {
    pub fn finalize(&self) {
        // dump call graph, points-to results
        results_dumper::dump_results(self.acx, &self.call_graph, &self.pt_data, &self.pag);

        // dump pta statistics
        let pta_stat = ContextSensitiveStat::new(self);
        pta_stat.dump_stats();
    }
}
```

## 修改记录

### 信息收集方法

#### DefId

总结起来，获取分析对象中的全部`DefId`大致需要如下步骤：

1. 从回调函数`after_analysis`的参数`rustc_interface::queries::Queries`，调用其`.global_ctxt().unwrap().enter(|tcx| {...})`方法。
   1. 该步骤是Rupta的内置源代码，因此笔者未做任何修改
2. 对那个闭包的唯一参数`tcx`，调用迭代器`.hir().body_owners()`，每次迭代都能获得一个`LocalDefId`。
3. 最后使用`LocalDefId::to_def_id()`方法获得`DefId`。

> 还有一种将Rupta定制的`FuncId`函数唯一标识转换为MIR中常用的`DefId`的办法，大致如下：
>
> ```rust
> // 利用acx把FuncId转换为DefId
> // 假定已经给定了FuncId，记为func_id
> let func_ref = self.acx.get_function_reference(func_id);
> let func_def_id = caller_ref.def_id;
> ```

#### Span

如何获得某个函数（以`DefId`唯一标记）一条语句的Span信息：

1. 首先获得函数的`DefId`。结合`queries...enter(|tcx| {...})`回调函数给的`tcx`参数，可以获得该函数的MIR，算法为：

   ```rust
   let mir = if tcx.is_const_fn_raw(def_id) {
       tcx.mir_for_ctfe(def_id)
   } else {
       let def = rustc_middle::ty::InstanceDef::Item(def_id);
       tcx.instance_mir(def)
   };
   ```

2. 直接从`mir.basic_blocks`获取该函数所包含的全部基本块。

3. 对每一个基本块`bb`，利用`mir[bb]`获取其包含的语句数组`statements`，并对每个语句`stmt`调用`let mir::Statement { kind, source_info } = statement;`解包获得`source_info`信息。

4. 最后，利用`source_info.span`获得语句的位置。

5. 进一步地，可以从Span信息获得源文件路径和在文件中的行号信息。

  ```rust
// loc的类型就是rustc_span::Span
let source_loc = loc.source_callsite();
if let Ok(line_and_file) = source_map.span_to_lines(source_loc) {
  // line_and_file的类型是FileLines
  // pub struct FileLines {
  //   pub file: Lrc<SourceFile>,
  //   pub lines: Vec<LineInfo>,
  //}
  // 现在已经可以得知该语句的位置了。
}
  ```

#### Crate的相关信息

Rupta本身并未提供有关Crate信息收集的机能，只能利用上文提及的Span收集法，先找到某个函数所在的源文件路径，然后逐级向上寻找，直至找到第一个Cargo.toml，该Cargo.toml所在的目录即为该函数所属Crate的所在目录。

```rust
// 给定tcx和待分析函数的DefId，记为def_id_of_func
let cur_session = tcx.sess;
let source_map = cur_session.source_map();
let span = cur_tcx.def_span(def_id_of_func);
let file = source_map.lookup_source_file(span.lo());
// 找到了这个函数定义在哪个文件里！
let filename = file.name.clone();
```

`filename`的类型是`rustc_span::FileName`，它是个枚举。这里出现的是`Real`类型。后者也是个枚举，此处最常见的两种`Real`枚举类型是`Remapped`和`LocalPath`。

- `Remapped { local_path: Some("/home/.../core/src/ops/range.rs"), virtual_name: "/rustc/bf3c6.../library/core/src/ops/range.rs" }`
- `LocalPath("/home/endericedragon/playground/example_crate/fastrand-2.1.0/src/lib.rs")`

以上两个枚举的详细定义信息可以参考doc.rs或者`rustc_span/src/lib.rs`文件。

有了上述信息准备，即可找到Crate所在的目录了：

```rust
let file_path = match filename {
    FileName::Real(real_file_name) => match real_file_name {
        RealFileName::LocalPath(path_buf) => {
            get_cargo_toml_path_from_source_file_path_buf(path_buf)
        }
        RealFileName::Remapped {
            local_path: path_buf_optional,
            virtual_name: _virtual_path_buf, // 我们不关心虚拟路径，直接弃用
        } => {
            if let Some(path_buf) = path_buf_optional {
                // 该函数负责在文件系统中逐级向上寻找，直至找到第一个Cargo.toml
                get_cargo_toml_path_from_source_file_path_buf(path_buf)
            } else {
                String::from("Virtual")
            }
        }
    },
    _ => String::from("Other"),
};
```

其中`get_cargo_toml_path_from_source_file_path_buf`函数负责在文件系统中逐级向上寻找，直至找到第一个Cargo.toml。其实现如下：

```rust
/// 和真正的文件系统交互，从源代码文件逐层向上查找直至找到第一个Cargo.toml，以定位该Crate的路径。
fn get_cargo_toml_path_from_source_file_path_buf(file_path: PathBuf) -> String {
    let mut path = file_path;
    while let Some(parent) = path.parent() {
        if parent.join("Cargo.toml").exists() {
            return parent.to_path_buf().to_string_lossy().into();
        }
        path = parent.to_path_buf();
    }

    unreachable!()
}
```

#### Caller、Callee信息

这里需要借助Rupta原有的设施进行改造。Rupta原生支持dot格式的函数调用图生成，其为调用图添加有向边的函数位于`::pta::context_sensitive::ContextSensitivePTA::add_call_edge`中。

与MIR所使用的DefId不同，Rupta内部采用其定制的`CSFuncId`与`FuncId`（事实上`CSFuncId`内含一个`FuncId`）唯一标识一个函数，为使其与MIR兼容，需要进行转化。其转化方法如下所示：

```rust
fn add_call_edge(&mut self, callsite: &Rc<CSCallSite>, callee: &CSFuncId) {
    let caller = callsite.func;
    if !self.call_graph.add_edge(callsite.into(), caller, *callee) {
        return;
    }
    // 利用acx把FuncId转换为DefId，这样函数的所有信息都能知道
    let caller_ref = self.acx.get_function_reference(caller.func_id);
    let caller_def_id = caller_ref.def_id;
    let callee_ref = self.acx.get_function_reference(callee.func_id);
    let callee_def_id = callee_ref.def_id;
    println!("{:?} --> {:?}", caller_def_id, callee_def_id);
    // -- snip --
}
```

`callsite`参数标记了函数调用的发生，其中包含调用者的`FuncId`。而`callee`参数则给出了被调用者的`FuncId`。在`acx`（其本质是Rupta定制的`AnalysisContext`结构体）的`get_function_reference`方法帮助下得以获得`caller`和`callee`两者各自的`DefId`。

### 实际改造记录

#### 新建的info_collector模块

该模块位于`src/info_collector`中，主要定义了OverallMetadata结构体（并且实现了基于`serde`库的序列化）：

```rust
pub struct OverallMetadata {
    // 记录：调用者和被调用者的DefId、调用发生的源文件路径及行号
    pub callsite_metadata: HashSet<CallSiteMetadata>,
    // 记录：Crate的清单文件Cargo.toml所在路径
    // 和对应的cargo_metadata::Metadata
    pub crate_metadata: VecSet<CrateMetadata>,
    // 记录：函数DefId、定义所在的路径和行号、
    // 及函数所在的Crate在上一成员crate_metadata中的索引
    pub func_metadata: HashSet<FuncMetadata>,
}
```

#### 收集并存储待输出的信息

首先， 在`AnalysisContext`中新增字段`overall_metadata`，以存储待输出的全部信息：

```rust
pub struct AnalysisContext<'tcx, 'compilation> {
    // --snip --
    
    /// 存储所有元数据
    pub overall_metadata: OverallMetadata,
}
```

在`FuncPAGBuilder::new`构造函数中：

- 构造函数的`FuncMetadata`并加入`overall_metadata.func_metadata`中
- 构造Crate的`CrareMetadata`并加入`overall_metadata.crate_metadata`中

在`ContextSensitivePTA::add_call_edge`方法中，构造表征函数调用关系的`CallSiteMetadata`，并加入`overall_metadata.callsite_metadata`中。

#### 优化内存结构的VecSet

在存储及输出函数及其所属Crate的过程中，每个`FuncMetadata`都会存储一个`CrateMetadata`结构。然而，一个crate中大概率有不止一个函数，这意味着相同内容的`CrateMetadata`会在数个`FuncMetadata`中存储多次，显然十分浪费内存。

一种自然的想法是：开个数组存`CrateMetadata`，只在`FuncMetadata`中存储这个`CrateMetadata`在数组中的下标。但是这个数组同时需要具有去重的功能，因为不同函数可以属于同一个Crate。

基于上述需求，设想并实现了一个结合`HashMap`和`Vec`的新数据结构`VecSet`，它的定义长这样：

```rust
pub struct VecSet<T: Eq + Hash> {
    // 真正存储数据的数组
    data: Vec<Rc<T>>,
    // 记录每个数据项在数组中的下标，用于去重
    included: HashMap<Rc<T>, usize>,
}
```

使用`Rc<T>`，可以有效避免同一数据项存储两遍的问题。经过测试，使用`Rc<T>`的`VecSet`比未使用`Rc<T>`的朴素版本节省了将近一半的内存用量（1608KB 减小到 868KB）。

#### 将整合的完整信息输出到文件

在`src/util/results_dumper.rs`中，编辑`dump_results`函数，补充一些内容：

```rust
pub fn dump_results<P: PAGPath, F, S>(
    acx: &AnalysisContext,
    call_graph: &CallGraph<F, S>,
    pt_data: &DiffPTDataTy,
    pag: &PAG<P>,
) where ... {
	// -- snip --

    // dump call graph
    if let Some(cg_output) = &acx.analysis_options.call_graph_output {
        let cg_path = std::path::Path::new(cg_output);
        info!("Dumping call graph...");
        dump_call_graph(acx, call_graph, cg_path);

        // 因为尚未修改命令行参数，因此只好先暂且把输出crate元数据的部分硬编码在这里了
        let mut om_path_buf = PathBuf::from(cg_output);
        om_path_buf.pop();
        om_path_buf.push("overall_metadata.json");
        info!("Dumping overall metadata...");
        let om_data = &acx.overall_metadata;
        let om_file = File::create(om_path_buf.as_path()).expect("Unable to create overall_metadata file");
        serde_json::to_writer(om_file, &om_data).expect("Unable to serialize overall_metadata");
    }
}
```



