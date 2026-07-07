use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub modpack_id: String,
    pub version: u32,
    pub tag: String,
    pub created_at: String,
    pub files: Vec<ModFile>,
}

impl Manifest {
    pub fn new(modpack_id: impl Into<String>, version: u32, files: Vec<ModFile>) -> Self {
        let modpack_id = modpack_id.into();
        let tag = format!("{modpack_id}-v{version}");
        Manifest {
            schema_version: 1,
            modpack_id,
            version,
            tag,
            created_at: unix_timestamp(),
            files,
        }
    }

    pub fn without_excluded(&self, excluded: &std::collections::HashSet<String>) -> Manifest {
        Manifest {
            files: self
                .files
                .iter()
                .filter(|f| !excluded.contains(&f.path))
                .cloned()
                .collect(),
            ..self.clone()
        }
    }
}

fn unix_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    secs.to_string()
}
