use serde::ser::SerializeStruct;
use serde::Serialize;
use std::hash::Hash;
use std::path::PathBuf;

/// 存放一次函数调用的元数据。
/// 包含调用者和被调用者的DefId，以及
/// 调用发生所在的文件在文件系统中的路径、文件中的具体行号。
/// 如果行号为0，那么文件系统路径一定是None。表明找不到真实的文件路径。
#[derive(Eq)]
pub struct CallSiteMetadata {
    /// 该调用的调用者caller的`DefId`。
    pub caller_def_id: rustc_span::def_id::DefId,
    /// 该调用的被调用者callee的`DefId`。
    pub callee_def_id: rustc_span::def_id::DefId,
    /// 调用所在的文件在文件系统中的路径。
    pub caller_file_path: Option<PathBuf>,
    /// 调用在源文件中的具体行号。
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
        self.caller_file_path.hash(state);
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
