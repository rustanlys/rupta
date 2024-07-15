use std::path::PathBuf;

use cargo_metadata::{Metadata, MetadataCommand, PackageId};
use serde::{ser::SerializeStruct, Serialize};

/// 针对rustlib/src/rust/compiler中的crate进行特别修复，将其中错误的路径替换成正确的
pub fn fix_incorrect_local_path(incorrect_path_buf: PathBuf) -> PathBuf {
    // 特殊处理rustc_*，它们的源代码位置和Span给出的不一样，需要进行替换
    // replace rustlib/src/rust/compiler with rustlib/rustc-src/rust/compiler
    let file_path_string = incorrect_path_buf.to_string_lossy();
    PathBuf::from(file_path_string.replace(
        "lib/rustlib/src/rust/compiler",
        "lib/rustlib/rustc-src/rust/compiler",
    ))
}

/// 和真正的文件系统交互，从源代码文件逐层向上查找直至找到第一个Cargo.toml，以定位该Crate的路径。
pub fn get_cargo_toml_path_from_source_file_path_buf(
    file_path: &PathBuf,
) -> core::result::Result<String, String> {
    let original_path = (*file_path).clone();
    let mut path = (*file_path).clone();
    while let Some(parent) = path.parent() {
        if parent.join("Cargo.toml").exists() {
            let mut result_path_buf = parent.to_path_buf();
            result_path_buf.push("Cargo.toml");
            return Ok(result_path_buf.to_string_lossy().into_owned());
        }
        path = parent.to_path_buf();
    }

    Err(format!(
        "No Cargo.toml found: {}",
        original_path.to_string_lossy()
    ))
}

#[derive(Debug, Clone)]
pub struct CrateMetadata {
    manifest_path: PathBuf,
    metadata: Metadata,
}

impl CrateMetadata {
    pub fn new(manifest_path: &str) -> Self {
        let mut cmd = MetadataCommand::new();
        cmd.manifest_path(manifest_path);

        let metadata = cmd.exec().unwrap();
        Self {
            manifest_path: PathBuf::from(manifest_path),
            metadata,
        }
    }

    pub fn root_package_id(&self) -> Option<PackageId> {
        self.metadata.root_package().map(|pkg| pkg.id.clone())
    }
}

impl Serialize for CrateMetadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("CrateMetadata", 2)?;
        state.serialize_field("manifest_path", &self.manifest_path)?;
        state.serialize_field("root_package_id", &self.root_package_id())?;
        state.end()
    }
}

#[derive(Debug, Clone)]
pub struct FuncMetadata {
    pub def_id: rustc_span::def_id::DefId,
    pub define_path: Option<PathBuf>,
    pub line_num: usize,
    pub crate_metadata: Option<CrateMetadata>,
}

impl FuncMetadata {
    pub fn new(
        def_id: rustc_span::def_id::DefId,
        define_path: Option<PathBuf>,
        line_num: usize,
        crate_metadata: Option<CrateMetadata>,
    ) -> Self {
        Self {
            def_id,
            define_path,
            line_num: line_num,
            crate_metadata,
        }
    }
}

impl Serialize for FuncMetadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("FuncMetadata", 5)?;
        state.serialize_field("def_id", &format!("{:?}", &self.def_id))?;
        state.serialize_field("define_path", &self.define_path)?;
        state.serialize_field("line_num", &self.line_num)?;
        state.serialize_field("crate_metadata", &self.crate_metadata)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let crate_metadata =
            CrateMetadata::new("/home/endericedragon/repos/substrate-node-template-copy/Cargo.toml");
        println!("root package id = {:?}", crate_metadata.root_package_id());
    }
}
