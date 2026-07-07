use crate::core::diff::SyncPlan;
use crate::core::manifest::Manifest;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// A computed-but-not-yet-applied sync, keyed by an opaque session id handed
/// back to the frontend after `preview_sync` so `apply_sync` can act on
/// exactly what was previewed without re-downloading anything.
pub struct PendingSync {
    pub extracted_dir: PathBuf,
    pub plan: SyncPlan,
    pub manifest: Manifest,
    pub owner: String,
    pub repo: String,
    pub modpack_id: String,
    pub destination_path: String,
}

#[derive(Default)]
pub struct AppState {
    pub pending_syncs: Mutex<HashMap<String, PendingSync>>,
}
