use crate::core::{apply, diff, manifest::Manifest, mods_folder, zip as core_zip};
use crate::db::{self, CachedModpack, WatchedRepo};
use crate::error::{GhResult, GitHubError};
use crate::github::client::GitHubClient;
use crate::state::{AppState, PendingSync};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

fn db_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("app data dir should be resolvable");
    dir.join("modpacksync.sqlite3")
}

fn open_db(app: &tauri::AppHandle) -> GhResult<rusqlite::Connection> {
    Ok(db::open(&db_path(app))?)
}

#[tauri::command]
pub fn add_watched_repo(app: tauri::AppHandle, owner: String, repo: String) -> Result<(), String> {
    (|| -> GhResult<()> {
        let conn = open_db(&app)?;
        db::add_watched_repo(&conn, &owner, &repo)?;
        Ok(())
    })()
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_watched_repo(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
) -> Result<(), String> {
    (|| -> GhResult<()> {
        let conn = open_db(&app)?;
        db::remove_watched_repo(&conn, &owner, &repo)?;
        Ok(())
    })()
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_watched_repos(app: tauri::AppHandle) -> Result<Vec<WatchedRepo>, String> {
    let conn = open_db(&app).map_err(|e| e.to_string())?;
    db::list_watched_repos(&conn).map_err(|e| e.to_string())
}

/// Global toggle for automatic syncing, default off. When on, the
/// background poller applies updates immediately (for any modpack that's
/// already been synced at least once, so a destination is known) instead
/// of only notifying and waiting for a manual sync.
#[tauri::command]
pub fn get_auto_sync_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    let conn = open_db(&app).map_err(|e| e.to_string())?;
    Ok(db::get_setting(&conn, "auto_sync_enabled")
        .map_err(|e| e.to_string())?
        .as_deref()
        == Some("1"))
}

#[tauri::command]
pub fn set_auto_sync_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let conn = open_db(&app).map_err(|e| e.to_string())?;
    db::set_setting(&conn, "auto_sync_enabled", if enabled { "1" } else { "0" })
        .map_err(|e| e.to_string())
}

/// Anonymously re-fetches `index.json` for one watched repo and refreshes
/// its rows in the local modpack cache.
#[tauri::command]
pub async fn refresh_repo(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
) -> Result<Vec<CachedModpack>, String> {
    refresh_repo_impl(app, owner, repo).await.map_err(|e| e.to_string())
}

async fn refresh_repo_impl(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
) -> GhResult<Vec<CachedModpack>> {
    let client = GitHubClient::new();
    let index = match client.get_contents(None, &owner, &repo, "index.json").await? {
        Some(file) => serde_json::from_slice(&file.bytes).unwrap_or_default(),
        None => crate::github::index::Index::default(),
    };
    let mut conn = open_db(&app)?;
    db::replace_modpack_cache_for_repo(&mut conn, &owner, &repo, &index.modpacks)?;
    Ok(db::list_cached_modpacks(&conn)?
        .into_iter()
        .filter(|m| m.owner == owner && m.repo == repo)
        .collect())
}

#[derive(Debug, Serialize)]
pub struct ModpackListItem {
    #[serde(flatten)]
    pub modpack: CachedModpack,
    pub synced_version: Option<u32>,
    pub excluded_count: usize,
    pub destination_path: Option<String>,
}

#[tauri::command]
pub fn list_modpacks(app: tauri::AppHandle) -> Result<Vec<ModpackListItem>, String> {
    (|| -> GhResult<Vec<ModpackListItem>> {
        let conn = open_db(&app)?;
        let modpacks = db::list_cached_modpacks(&conn)?;
        let mut out = Vec::with_capacity(modpacks.len());
        for m in modpacks {
            let state = db::get_sync_state(&conn, &m.owner, &m.repo, &m.modpack_id)?;
            let excluded_count =
                db::list_exclusions(&conn, &m.owner, &m.repo, &m.modpack_id)?.len();
            out.push(ModpackListItem {
                synced_version: state.as_ref().map(|s| s.synced_version),
                destination_path: state.map(|s| s.destination_path),
                excluded_count,
                modpack: m,
            });
        }
        Ok(out)
    })()
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_exclusions(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
    modpack_id: String,
) -> Result<Vec<String>, String> {
    let conn = open_db(&app).map_err(|e| e.to_string())?;
    db::list_exclusions(&conn, &owner, &repo, &modpack_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_exclusion(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
    modpack_id: String,
    filename: String,
    excluded: bool,
) -> Result<(), String> {
    let conn = open_db(&app).map_err(|e| e.to_string())?;
    db::set_exclusion(&conn, &owner, &repo, &modpack_id, &filename, excluded)
        .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct ModpackFiles {
    pub files: Vec<String>,
    pub excluded: Vec<String>,
    pub synced_files: Vec<String>,
    pub destination_path: Option<String>,
}

/// Fetches just `manifest.json` (not the mods.zip) for a modpack, so the
/// exclusion UI can list every mod without requiring a destination folder
/// or a full sync first.
#[tauri::command]
pub async fn get_modpack_files(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
    modpack_id: String,
) -> Result<ModpackFiles, String> {
    get_modpack_files_impl(app, owner, repo, modpack_id)
        .await
        .map_err(|e| e.to_string())
}

async fn get_modpack_files_impl(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
    modpack_id: String,
) -> GhResult<ModpackFiles> {
    let (entry, excluded, sync_state) = {
        let conn = open_db(&app)?;
        let entry = db::list_cached_modpacks(&conn)?
            .into_iter()
            .find(|m| m.owner == owner && m.repo == repo && m.modpack_id == modpack_id)
            .ok_or_else(|| GitHubError::Api {
                status: 0,
                message: "modpack not found in local cache — refresh the repo first".to_string(),
            })?;
        let excluded = db::list_exclusions(&conn, &owner, &repo, &modpack_id)?;
        let sync_state = db::get_sync_state(&conn, &owner, &repo, &modpack_id)?;
        (entry, excluded, sync_state)
    };

    let client = GitHubClient::new();
    let assets = client
        .get_release_by_tag(&owner, &repo, &entry.latest_tag)
        .await?;
    let manifest_asset = assets
        .iter()
        .find(|a| a.name == entry.manifest_asset)
        .ok_or_else(|| GitHubError::Api {
            status: 0,
            message: format!("release {} is missing {}", entry.latest_tag, entry.manifest_asset),
        })?;
    let manifest_bytes = client.download_bytes(&manifest_asset.browser_download_url).await?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)?;

    let synced_files = sync_state
        .as_ref()
        .map(|s| {
            serde_json::from_str::<Manifest>(&s.synced_manifest_json)
                .map(|m| m.files.into_iter().map(|f| f.path).collect())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    Ok(ModpackFiles {
        files: manifest.files.into_iter().map(|f| f.path).collect(),
        excluded,
        synced_files,
        destination_path: sync_state.map(|s| s.destination_path),
    })
}

/// Deletes a single file from the last-synced destination folder, if it's
/// still there. Used when a friend excludes a mod they already have synced
/// and wants it gone immediately rather than waiting for the next sync.
#[tauri::command]
pub fn delete_synced_file(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
    modpack_id: String,
    filename: String,
) -> Result<bool, String> {
    (|| -> GhResult<bool> {
        let conn = open_db(&app)?;
        let Some(state) = db::get_sync_state(&conn, &owner, &repo, &modpack_id)? else {
            return Ok(false);
        };
        let path = PathBuf::from(&state.destination_path).join("mods").join(&filename);
        if path.exists() {
            fs::remove_file(&path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    })()
    .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct SyncPreview {
    pub session_id: String,
    pub to_add: Vec<String>,
    pub to_update: Vec<String>,
    pub to_remove: Vec<String>,
    pub excluded: Vec<String>,
}

#[tauri::command]
pub async fn preview_sync(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    owner: String,
    repo: String,
    modpack_id: String,
    destination_path: String,
) -> Result<SyncPreview, String> {
    preview_sync_impl(app, state, owner, repo, modpack_id, destination_path)
        .await
        .map_err(|e| e.to_string())
}

/// Fetches the manifest + mods zip for a modpack and computes the diff
/// against `destination_path`, without touching any pending-sync state —
/// shared by the manual preview flow and the automatic-sync path.
async fn fetch_and_diff(
    app: &tauri::AppHandle,
    owner: &str,
    repo: &str,
    modpack_id: &str,
    destination_path: &str,
) -> GhResult<(diff::SyncPlan, Manifest, PathBuf)> {
    let (entry, exclusions, previously_synced) = {
        let conn = open_db(app)?;
        let entry = db::list_cached_modpacks(&conn)?
            .into_iter()
            .find(|m| m.owner == owner && m.repo == repo && m.modpack_id == modpack_id)
            .ok_or_else(|| GitHubError::Api {
                status: 0,
                message: "modpack not found in local cache — refresh the repo first".to_string(),
            })?;
        let exclusions: HashSet<String> =
            db::list_exclusions(&conn, owner, repo, modpack_id)?.into_iter().collect();
        let previously_synced: HashSet<String> = db::get_sync_state(&conn, owner, repo, modpack_id)?
            .map(|s| {
                serde_json::from_str::<Manifest>(&s.synced_manifest_json)
                    .map(|m| m.files.into_iter().map(|f| f.path).collect())
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        (entry, exclusions, previously_synced)
    };

    let client = GitHubClient::new();
    let assets = client
        .get_release_by_tag(owner, repo, &entry.latest_tag)
        .await?;
    let manifest_asset = assets
        .iter()
        .find(|a| a.name == entry.manifest_asset)
        .ok_or_else(|| GitHubError::Api {
            status: 0,
            message: format!("release {} is missing {}", entry.latest_tag, entry.manifest_asset),
        })?;
    let mods_asset = assets
        .iter()
        .find(|a| a.name == entry.mods_asset)
        .ok_or_else(|| GitHubError::Api {
            status: 0,
            message: format!("release {} is missing {}", entry.latest_tag, entry.mods_asset),
        })?;

    let manifest_bytes = client.download_bytes(&manifest_asset.browser_download_url).await?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)?;
    let zip_bytes = client.download_bytes(&mods_asset.browser_download_url).await?;

    let session_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let extracted_dir = std::env::temp_dir().join(format!("modpacksync-sync-{session_id}"));
    let zip_path = extracted_dir.join("mods.zip");
    fs::create_dir_all(&extracted_dir)?;
    fs::write(&zip_path, &zip_bytes)?;
    core_zip::extract_all(&zip_path, &extracted_dir)?;
    let _ = fs::remove_file(&zip_path);

    let mods_dir = PathBuf::from(destination_path).join("mods");
    fs::create_dir_all(&mods_dir)?;
    let local_files = mods_folder::scan(&mods_dir)?;
    let plan = diff::compute_plan(&manifest, &exclusions, &local_files, &previously_synced);

    Ok((plan, manifest, extracted_dir))
}

async fn preview_sync_impl(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    owner: String,
    repo: String,
    modpack_id: String,
    destination_path: String,
) -> GhResult<SyncPreview> {
    let (plan, manifest, extracted_dir) =
        fetch_and_diff(&app, &owner, &repo, &modpack_id, &destination_path).await?;

    let session_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();

    let preview = SyncPreview {
        session_id: session_id.clone(),
        to_add: plan.to_add.iter().map(|f| f.path.clone()).collect(),
        to_update: plan.to_update.iter().map(|f| f.path.clone()).collect(),
        to_remove: plan.to_remove.clone(),
        excluded: plan.excluded.clone(),
    };

    state.pending_syncs.lock().unwrap().insert(
        session_id,
        PendingSync {
            extracted_dir,
            plan,
            manifest,
            owner,
            repo,
            modpack_id,
            destination_path,
        },
    );

    Ok(preview)
}

/// Fetches, diffs, and immediately applies a sync with no manual preview
/// step — used by the background poller when auto-sync is enabled for a
/// modpack the friend has already synced at least once before (so a
/// destination folder is already known).
pub async fn auto_sync_modpack(
    app: &tauri::AppHandle,
    owner: &str,
    repo: &str,
    modpack_id: &str,
    destination_path: &str,
) -> GhResult<SyncResult> {
    let (plan, manifest, extracted_dir) =
        fetch_and_diff(app, owner, repo, modpack_id, destination_path).await?;

    let dest = PathBuf::from(destination_path).join("mods");
    apply::apply_plan(&plan, &extracted_dir, &dest)?;
    let _ = fs::remove_dir_all(&extracted_dir);

    let exclusions: HashSet<String> = plan.excluded.iter().cloned().collect();
    let synced_manifest = manifest.without_excluded(&exclusions);

    let conn = open_db(app)?;
    db::set_sync_state(
        &conn,
        owner,
        repo,
        modpack_id,
        destination_path,
        manifest.version,
        &serde_json::to_string(&synced_manifest)?,
    )?;

    Ok(SyncResult {
        added: plan.to_add.len(),
        updated: plan.to_update.len(),
        removed: plan.to_remove.len(),
    })
}

#[derive(Debug, Serialize)]
pub struct SyncResult {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

#[tauri::command]
pub fn apply_sync(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<SyncResult, String> {
    apply_sync_impl(app, state, session_id).map_err(|e| e.to_string())
}

fn apply_sync_impl(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> GhResult<SyncResult> {
    let pending = state
        .pending_syncs
        .lock()
        .unwrap()
        .remove(&session_id)
        .ok_or_else(|| GitHubError::Api {
            status: 0,
            message: "sync session expired or already applied — preview again".to_string(),
        })?;

    let dest = PathBuf::from(&pending.destination_path).join("mods");
    apply::apply_plan(&pending.plan, &pending.extracted_dir, &dest)?;
    let _ = fs::remove_dir_all(&pending.extracted_dir);

    let exclusions: HashSet<String> = pending.plan.excluded.iter().cloned().collect();
    let synced_manifest = pending.manifest.without_excluded(&exclusions);

    let conn = open_db(&app)?;
    db::set_sync_state(
        &conn,
        &pending.owner,
        &pending.repo,
        &pending.modpack_id,
        &pending.destination_path,
        pending.manifest.version,
        &serde_json::to_string(&synced_manifest)?,
    )?;

    Ok(SyncResult {
        added: pending.plan.to_add.len(),
        updated: pending.plan.to_update.len(),
        removed: pending.plan.to_remove.len(),
    })
}
