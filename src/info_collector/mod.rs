pub mod callsite_metadata;
pub mod crate_metadata;
pub mod func_metadata;
pub mod vec_set;

use rustc_span::{FileName, RealFileName};
use serde::{ser::SerializeStruct, Serialize};
use std::{collections::HashSet, path::PathBuf};
use vec_set::VecSet;

pub use callsite_metadata::CallSiteMetadata;
pub use crate_metadata::CrateMetadata;
pub use func_metadata::FuncMetadata;

/// 将函数定义、crate、调用点信息等收集到一起的结构体
#[derive(Default)]
pub struct OverallMetadata {
    pub callsite_metadata: HashSet<CallSiteMetadata>,
    pub crate_metadata: VecSet<CrateMetadata>,
    pub func_metadata: HashSet<FuncMetadata>,
}

impl Serialize for OverallMetadata{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        let mut state = serializer.serialize_struct("OverallMetadata", 3)?;
        state.serialize_field("crate_metadata", &self.crate_metadata)?;
        state.serialize_field("func_metadata", &self.func_metadata)?;
        state.serialize_field("callsite_metadata", &self.callsite_metadata)?;

        state.end()
    }
}

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
/// 若提取失败，则返回以`Virtual: `或`Other: `开头的字符串，以报告错误类型。
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

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn it_works() {}
}
