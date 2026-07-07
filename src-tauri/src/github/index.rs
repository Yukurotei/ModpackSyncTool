use serde::{Deserialize, Serialize};

/// One entry per modpack in a repo's `index.json` — the friend-side app polls
/// this file (anonymously, via raw.githubusercontent.com) to discover and
/// track updates to modpacks without ever touching the rate-limited REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackIndexEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub latest_tag: String,
    pub latest_version: u32,
    pub updated_at: String,
    pub manifest_asset: String,
    pub mods_asset: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub modpacks: Vec<ModpackIndexEntry>,
}

impl Default for Index {
    fn default() -> Self {
        Index {
            schema_version: 1,
            modpacks: Vec::new(),
        }
    }
}

fn default_schema_version() -> u32 {
    1
}
