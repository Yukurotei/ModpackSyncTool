use crate::core::{manifest::Manifest, mods_folder, zip as core_zip};
use crate::db;
use crate::error::{GhResult, GitHubError};
use crate::github::auth;
use crate::github::client::GitHubClient;
use crate::github::index::{Index, ModpackIndexEntry};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

/// Fixed name for the single repo we auto-create per host to hold all of
/// their modpacks (each modpack is its own set of tagged releases inside
/// it, tracked via one shared `index.json` — see `github/index.rs`).
const PUBLISH_REPO_NAME: &str = "modpacksync-modpacks";

fn db_path(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
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
pub async fn set_github_token(app: tauri::AppHandle, token: String) -> Result<String, String> {
    set_github_token_impl(app, token)
        .await
        .map_err(|e| e.to_string())
}

async fn set_github_token_impl(app: tauri::AppHandle, token: String) -> GhResult<String> {
    let client = GitHubClient::new();
    let login = client.validate_token(&token).await?;
    auth::store_token(&token)?;
    let conn = open_db(&app)?;
    db::set_setting(&conn, "github_login", &login)?;
    Ok(login)
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishRepo {
    pub owner: String,
    pub repo: String,
}

/// Resolves the one repo this host publishes all their modpacks into,
/// creating it on GitHub (pre-initialized so releases can target it right
/// away) the first time this is called, then remembering the choice.
#[tauri::command]
pub async fn get_or_create_publish_repo(app: tauri::AppHandle) -> Result<PublishRepo, String> {
    get_or_create_publish_repo_impl(app)
        .await
        .map_err(|e| e.to_string())
}

async fn get_or_create_publish_repo_impl(app: tauri::AppHandle) -> GhResult<PublishRepo> {
    let conn = open_db(&app)?;
    if let (Some(owner), Some(repo)) = (
        db::get_setting(&conn, "publish_repo_owner")?,
        db::get_setting(&conn, "publish_repo_name")?,
    ) {
        return Ok(PublishRepo { owner, repo });
    }

    let token = auth::get_token()?.ok_or(GitHubError::MissingToken)?;
    let client = GitHubClient::new();
    let owner = match db::get_setting(&conn, "github_login")? {
        Some(login) => login,
        None => {
            let login = client.validate_token(&token).await?;
            db::set_setting(&conn, "github_login", &login)?;
            login
        }
    };

    if !client.repo_exists(&token, &owner, PUBLISH_REPO_NAME).await? {
        client
            .create_repo(
                &token,
                PUBLISH_REPO_NAME,
                "My ModpackSync modpacks — managed by the ModpackSync app.",
            )
            .await?;
    }

    db::set_setting(&conn, "publish_repo_owner", &owner)?;
    db::set_setting(&conn, "publish_repo_name", PUBLISH_REPO_NAME)?;
    Ok(PublishRepo {
        owner,
        repo: PUBLISH_REPO_NAME.to_string(),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishedModpack {
    #[serde(flatten)]
    pub entry: ModpackIndexEntry,
    /// Last instance folder used to publish this modpack, remembered
    /// locally (never written to the shared `index.json` — it's a local
    /// filesystem path, not something friends need or should see).
    pub instance_path: Option<String>,
}

fn instance_path_key(modpack_id: &str) -> String {
    format!("instance_path:{modpack_id}")
}

/// Lists modpacks already published to this host's repo, so the publish
/// form can offer "update an existing one" instead of always starting fresh.
#[tauri::command]
pub async fn list_published_modpacks(app: tauri::AppHandle) -> Result<Vec<PublishedModpack>, String> {
    list_published_modpacks_impl(app).await.map_err(|e| e.to_string())
}

async fn list_published_modpacks_impl(app: tauri::AppHandle) -> GhResult<Vec<PublishedModpack>> {
    let PublishRepo { owner, repo } = get_or_create_publish_repo_impl(app.clone()).await?;
    let client = GitHubClient::new();
    let (index, _) = fetch_index(&client, &owner, &repo).await?;
    let conn = open_db(&app)?;
    index
        .modpacks
        .into_iter()
        .map(|entry| {
            let instance_path = db::get_setting(&conn, &instance_path_key(&entry.id))?;
            Ok(PublishedModpack { entry, instance_path })
        })
        .collect()
}

#[tauri::command]
pub fn has_github_token() -> bool {
    auth::get_token().ok().flatten().is_some()
}

#[tauri::command]
pub fn clear_github_token() -> Result<(), String> {
    auth::delete_token().map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct PublishResult {
    pub tag: String,
    pub version: u32,
    pub mod_count: usize,
    pub release_url: String,
}

#[tauri::command]
pub async fn publish_modpack(
    app: tauri::AppHandle,
    instance_path: String,
    modpack_id: String,
    name: String,
    description: String,
) -> Result<PublishResult, String> {
    publish_impl(app, instance_path, modpack_id, name, description)
        .await
        .map_err(|e| e.to_string())
}

async fn publish_impl(
    app: tauri::AppHandle,
    instance_path: String,
    modpack_id: String,
    name: String,
    description: String,
) -> GhResult<PublishResult> {
    let token = auth::get_token()?.ok_or(GitHubError::MissingToken)?;
    let PublishRepo { owner, repo } = get_or_create_publish_repo_impl(app.clone()).await?;
    let client = GitHubClient::new();

    let mods_dir = PathBuf::from(&instance_path).join("mods");
    if !mods_dir.is_dir() {
        return Err(GitHubError::ModsFolderNotFound(instance_path));
    }
    let files = mods_folder::scan(&mods_dir)?;

    {
        let conn = open_db(&app)?;
        db::set_setting(&conn, &instance_path_key(&modpack_id), &instance_path)?;
    }

    let (mut index, mut index_sha) = fetch_index(&client, &owner, &repo).await?;
    let next_version = index
        .modpacks
        .iter()
        .find(|m| m.id == modpack_id)
        .map(|m| m.latest_version + 1)
        .unwrap_or(1);

    let manifest = Manifest::new(modpack_id.clone(), next_version, files.clone());

    let tmp_dir = std::env::temp_dir().join(format!(
        "modpacksync-publish-{}-{}",
        std::process::id(),
        manifest.created_at
    ));
    fs::create_dir_all(&tmp_dir)?;
    let zip_path = tmp_dir.join("mods.zip");
    core_zip::build_mods_zip(&mods_dir, &zip_path, &files)?;
    let zip_bytes = fs::read(&zip_path)?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let _ = fs::remove_dir_all(&tmp_dir);

    let release = client
        .create_release(&token, &owner, &repo, &manifest.tag, &name)
        .await?;
    client
        .upload_release_asset(
            &token,
            &release.upload_url_template,
            "mods.zip",
            "application/zip",
            zip_bytes,
        )
        .await?;
    client
        .upload_release_asset(
            &token,
            &release.upload_url_template,
            "manifest.json",
            "application/json",
            manifest_bytes,
        )
        .await?;

    let entry = ModpackIndexEntry {
        id: modpack_id.clone(),
        name,
        description,
        latest_tag: manifest.tag.clone(),
        latest_version: next_version,
        updated_at: manifest.created_at.clone(),
        manifest_asset: "manifest.json".to_string(),
        mods_asset: "mods.zip".to_string(),
    };
    upsert_entry(&mut index, entry.clone());

    let commit_message = format!("Publish {} v{}", modpack_id, next_version);
    let put_result = client
        .put_contents(
            &token,
            &owner,
            &repo,
            "index.json",
            &commit_message,
            &serde_json::to_vec_pretty(&index)?,
            index_sha.as_deref(),
        )
        .await;

    if let Err(e) = put_result {
        if GitHubClient::is_conflict(&e) {
            // Someone else (or another publish) updated index.json concurrently.
            // Re-fetch, re-apply our merge, and retry exactly once.
            let (mut fresh_index, fresh_sha) = fetch_index(&client, &owner, &repo).await?;
            upsert_entry(&mut fresh_index, entry);
            index_sha = fresh_sha;
            client
                .put_contents(
                    &token,
                    &owner,
                    &repo,
                    "index.json",
                    &commit_message,
                    &serde_json::to_vec_pretty(&fresh_index)?,
                    index_sha.as_deref(),
                )
                .await?;
        } else {
            return Err(e);
        }
    }

    Ok(PublishResult {
        tag: manifest.tag,
        version: next_version,
        mod_count: files.len(),
        release_url: release.html_url,
    })
}

fn upsert_entry(index: &mut Index, entry: ModpackIndexEntry) {
    if let Some(pos) = index.modpacks.iter().position(|m| m.id == entry.id) {
        index.modpacks[pos] = entry;
    } else {
        index.modpacks.push(entry);
    }
}

async fn fetch_index(
    client: &GitHubClient,
    owner: &str,
    repo: &str,
) -> GhResult<(Index, Option<String>)> {
    match client.get_contents(None, owner, repo, "index.json").await? {
        Some(file) => {
            let index: Index = serde_json::from_slice(&file.bytes).unwrap_or_default();
            Ok((index, Some(file.sha)))
        }
        None => Ok((Index::default(), None)),
    }
}
