use std::{hash::Hash, path::PathBuf};

use cargo_metadata::{Metadata, MetadataCommand, PackageId};
use rustc_span::{FileName, RealFileName};
use serde::{ser::SerializeStruct, Serialize};

/// 针对rustlib/src/rust/compiler中的crate进行特别修复，将其中错误的路径替换成正确的
pub fn fix_incorrect_local_path(incorrect_path_buf: &PathBuf) -> PathBuf {
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

/// 从rustc_span::FileName中提取该文件在文件系统中的路径。
/// 若提取失败，则返回以`Virtual: `或`Other: `开头的字符串，报告错误类型。
pub fn get_pathbuf_from_filename_struct(filename: &FileName) -> core::result::Result<PathBuf, String> {
    match filename {
        FileName::Real(real_file_name) => match real_file_name {
            RealFileName::LocalPath(path_buf) => Ok(fix_incorrect_local_path(path_buf)),
            RealFileName::Remapped {
                local_path: path_buf_optional,
                virtual_name: virtual_path_buf,
            } => {
                if let Some(path_buf) = path_buf_optional {
                    Ok(fix_incorrect_local_path(path_buf))
                } else {
                    Err(format!("Virtual: {}", virtual_path_buf.to_string_lossy()))
                }
            }
        },
        _ => Err(format!("Other: {:?}", filename)),
    }
}

/// 存放一个crate依赖项的元数据。
/// 包含该crate的Cargo.toml文件路径，以及该crate的根package_id。
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

/// 存放一个函数或方法的元数据。
/// 包含该函数或方法的def_id，以及该函数或方法在源代码中的文件路径和行号。
/// 除此以外，还有该函数所属的crate的元数据。
/// todo: 每个函数都存储一个crate元数据的做法太奢侈了，需要优化。
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
        let mut state = serializer.serialize_struct("FuncMetadata", 4)?;
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

/// 存放一次函数调用的元数据。
/// 包含调用者和被调用者的DefId，以及
/// 调用发生所在的文件在文件系统中的路径、文件中的具体行号。
/// 如果行号为0，那么文件系统路径一定是None。表明找不到真实的文件路径。
#[derive(Eq)]
pub struct CallSiteMetadata {
    pub caller_def_id: rustc_span::def_id::DefId,
    pub callee_def_id: rustc_span::def_id::DefId,
    pub caller_file_path: Option<PathBuf>,
    pub caller_line_num: usize,
}

impl PartialEq for CallSiteMetadata {
    fn eq(&self, other: &Self) -> bool {
        self.caller_def_id == other.caller_def_id
            && self.callee_def_id == other.callee_def_id
            && self.caller_line_num == other.caller_line_num
    }
}

impl Hash for CallSiteMetadata {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.caller_def_id.hash(state);
        self.callee_def_id.hash(state);
        self.caller_line_num.hash(state);
    }
}

impl Serialize for CallSiteMetadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("CallSiteMetadata", 4)?;
        state.serialize_field("caller_def_id", &format!("{:?}", self.caller_def_id))?;
        state.serialize_field("callee_def_id", &format!("{:?}", self.callee_def_id))?;
        state.serialize_field("caller_file_path", &self.caller_file_path)?;
        state.serialize_field("caller_line_num", &self.caller_line_num)?;
        state.end()
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
