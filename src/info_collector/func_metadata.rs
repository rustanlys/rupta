use std::path::PathBuf;

use serde::{ser::SerializeStruct, Serialize};

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
            line_num: line_num,
            crate_metadata_idx,
        }
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
