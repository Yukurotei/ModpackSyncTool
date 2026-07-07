use crate::core::{manifest::Manifest, mods_folder, zip as core_zip};
use crate::error::{GhResult, GitHubError};
use crate::github::auth;
use crate::github::client::GitHubClient;
use crate::github::index::{Index, ModpackIndexEntry};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

#[tauri::command]
pub async fn set_github_token(token: String) -> Result<String, String> {
    set_github_token_impl(token).await.map_err(|e| e.to_string())
}

async fn set_github_token_impl(token: String) -> GhResult<String> {
    let client = GitHubClient::new();
    let login = client.validate_token(&token).await?;
    auth::store_token(&token)?;
    Ok(login)
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
    instance_path: String,
    modpack_id: String,
    name: String,
    description: String,
    owner: String,
    repo: String,
) -> Result<PublishResult, String> {
    publish_impl(instance_path, modpack_id, name, description, owner, repo)
        .await
        .map_err(|e| e.to_string())
}

async fn publish_impl(
    instance_path: String,
    modpack_id: String,
    name: String,
    description: String,
    owner: String,
    repo: String,
) -> GhResult<PublishResult> {
    let token = auth::get_token()?.ok_or(GitHubError::MissingToken)?;
    let client = GitHubClient::new();

    let mods_dir = PathBuf::from(&instance_path).join("mods");
    let files = mods_folder::scan(&mods_dir)?;

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
