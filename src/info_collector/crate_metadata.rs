use std::hash::Hash;
use std::path::PathBuf;

use cargo_metadata::{Metadata, MetadataCommand, PackageId};
use serde::{ser::SerializeStruct, Serialize};

/// 存放一个crate依赖项的元数据。
/// 包含该crate的Cargo.toml文件路径，以及该crate的根package_id。
#[derive(Debug, Clone)]
pub struct CrateMetadata {
    manifest_path: PathBuf,
    metadata: Metadata,
}

impl PartialEq for CrateMetadata {
    fn eq(&self, other: &Self) -> bool {
        self.manifest_path == other.manifest_path
    }
}

impl Eq for CrateMetadata {}

impl Hash for CrateMetadata {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.manifest_path.hash(state);
    }
}

impl CrateMetadata {
    pub fn new(manifest_path: &str, working_dir: &std::path::PathBuf) -> Self {
        let mut cmd = MetadataCommand::new();
        cmd.manifest_path(manifest_path);
        cmd.current_dir(working_dir); // default to current directory
        // 遇到 v4 的 Cargo.lock 文件时，需要加上 -Znext-lockfile-bump 选项，否则会报错。
        cmd.other_options(vec!["-Znext-lockfile-bump".to_string()]);

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
