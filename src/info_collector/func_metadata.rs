use std::path::PathBuf;

use serde::{ser::SerializeStruct, Serialize};

use crate::mir::analysis_context;

/// 存放一个函数或方法的元数据。
/// 包含该函数或方法的def_id，以及该函数或方法在源代码中的文件路径和行号。
/// 除此以外，还有该函数所属的crate的元数据。
#[derive(Debug, Clone)]
pub struct FuncMetadata {
    pub def_id: rustc_span::def_id::DefId,
    pub define_path: Option<PathBuf>,
    pub line_num: usize,
    pub crate_metadata_idx: Option<usize>,
}

impl FuncMetadata {
    pub fn new(
        def_id: rustc_span::def_id::DefId,
        define_path: Option<PathBuf>,
        line_num: usize,
        crate_metadata_idx: Option<usize>,
    ) -> Self {
        Self {
            def_id,
            define_path,
            line_num,
            crate_metadata_idx,
        }
    }

    pub fn from_info(
        acx: &mut analysis_context::AnalysisContext,
        def_id_of_func: rustc_hir::def_id::DefId,
    ) -> Self {
        let cur_session = acx.tcx.sess;
        let source_map = cur_session.source_map();
        let span = acx.tcx.def_span(def_id_of_func);
        let file = source_map.lookup_source_file(span.lo());
        let line_num = if let Ok(file_and_line) = source_map.lookup_line(span.lo()) {
            // assert_eq!(file_and_line.sf.name, file.name);
            file_and_line.line
        } else {
            0
        };

        // 沃趣，找到了这个函数定义在哪个文件里头！！！！
        // Real(Remapped { local_path: Some("/home/endericedragon/.rustup/toolchains/nightly-2024-02-03-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/core/src/ops/range.rs"), virtual_name: "/rustc/bf3c6c5bed498f41ad815641319a1ad9bcecb8e8/library/core/src/ops/range.rs" })
        // Real(LocalPath("/home/endericedragon/playground/example_crate/fastrand-2.1.0/src/lib.rs"))
        // 枚举的完整类型定义于rustc_span/src/lib.rs
        let filename = &file.name;
        let source_file_path = super::get_pathbuf_from_filename_struct(filename);

        let manifest_path = match &source_file_path {
            Ok(path_buf) => super::get_cargo_toml_path_from_source_file_path_buf(&path_buf),
            Err(message) => Err(message.to_owned()),
        };

        let crate_metadata_idx = if let Some(crate_metadata) = match manifest_path {
            Ok(path) => Some(super::CrateMetadata::new(&path, &acx.working_dir)),

            Err(message) => {
                eprintln!("Error: {}", message);
                None
            }
        } {
            Some(acx.overall_metadata.crate_metadata.insert(crate_metadata))
        } else {
            None
        };

        let func_metadata = FuncMetadata::new(
            def_id_of_func,
            match source_file_path {
                Ok(path_buf) => Some(path_buf),
                _ => None,
            },
            line_num,
            crate_metadata_idx,
        );

        func_metadata
    }
}

impl Serialize for FuncMetadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("FuncMetadata", 4)?;
        state.serialize_field("def_id", &format!("{:?}", &self.def_id))?;
        state.serialize_field("define_path", &self.define_path)?;
        state.serialize_field("line_num", &self.line_num)?;
        state.serialize_field("crate_metadata_idx", &self.crate_metadata_idx)?;
        state.end()
    }
}

// 为了使得FuncMetadata可以在HashMap中作为key，需要实现对应的trait
impl std::cmp::PartialEq for FuncMetadata {
    fn eq(&self, other: &Self) -> bool {
        self.def_id == other.def_id
    }
}

impl std::cmp::Eq for FuncMetadata {}

impl std::hash::Hash for FuncMetadata {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.def_id.hash(state);
    }
}
