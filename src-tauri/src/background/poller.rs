use crate::db;
use crate::error::GhResult;
use crate::github::client::GitHubClient;
use crate::github::index::{Index, ModpackIndexEntry};
use std::collections::HashMap;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

/// Modpack updates aren't time-critical, and raw.githubusercontent.com reads
/// don't count against the unauthenticated REST rate limit, so a generous
/// interval is just about being a good citizen rather than a real constraint.
const POLL_INTERVAL_SECS: u64 = 20 * 60;

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            if let Err(e) = poll_once(&app).await {
                eprintln!("[poller] error: {e}");
            }
            tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
        }
    });
}

fn db_path(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_data_dir()
        .expect("app data dir should be resolvable")
        .join("modpacksync.sqlite3")
}

async fn poll_once(app: &AppHandle) -> GhResult<()> {
    let mut conn = db::open(&db_path(app))?;
    let repos = db::list_watched_repos(&conn)?;
    let client = GitHubClient::new();

    for r in repos {
        let etag = db::get_repo_etag(&conn, &r.owner, &r.repo)?;
        let fetch = match client
            .fetch_raw_conditional(&r.owner, &r.repo, "index.json", etag.as_deref())
            .await
        {
            Ok(f) => f,
            Err(e) => {
                eprintln!("[poller] {}/{}: {e}", r.owner, r.repo);
                continue;
            }
        };
        let Some((bytes, new_etag)) = fetch else {
            continue; // 304 Not Modified -- nothing changed since last poll
        };

        let index: Index = serde_json::from_slice(&bytes).unwrap_or_default();
        let previous: HashMap<String, u32> = db::list_cached_modpacks(&conn)?
            .into_iter()
            .filter(|m| m.owner == r.owner && m.repo == r.repo)
            .map(|m| (m.modpack_id, m.latest_version))
            .collect();

        db::replace_modpack_cache_for_repo(&mut conn, &r.owner, &r.repo, &index.modpacks)?;
        db::set_repo_etag(&conn, &r.owner, &r.repo, &new_etag)?;

        let auto_sync_enabled =
            db::get_setting(&conn, "auto_sync_enabled")?.as_deref() == Some("1");

        for entry in &index.modpacks {
            match previous.get(&entry.id) {
                None => notify(app, entry, true),
                Some(v) if *v < entry.latest_version => {
                    let sync_state = db::get_sync_state(&conn, &r.owner, &r.repo, &entry.id)?;
                    match (auto_sync_enabled, sync_state) {
                        (true, Some(state)) => {
                            match crate::commands::sync::auto_sync_modpack(
                                app,
                                &r.owner,
                                &r.repo,
                                &entry.id,
                                &state.destination_path,
                            )
                            .await
                            {
                                Ok(_) => notify_synced(app, entry),
                                Err(e) => {
                                    eprintln!("[poller] auto-sync {}/{}/{}: {e}", r.owner, r.repo, entry.id);
                                    notify(app, entry, false);
                                }
                            }
                        }
                        _ => notify(app, entry, false),
                    }
                }
                _ => {}
            }
        }

        let _ = app.emit("modpacks-updated", ());
    }
    Ok(())
}

fn notify(app: &AppHandle, entry: &ModpackIndexEntry, is_new: bool) {
    let title = if is_new {
        format!("New modpack: {}", entry.name)
    } else {
        format!("{} updated", entry.name)
    };
    let body = format!("v{} is now available", entry.latest_version);
    let _ = app.notification().builder().title(title).body(body).show();
}

fn notify_synced(app: &AppHandle, entry: &ModpackIndexEntry) {
    let title = format!("{} auto-synced", entry.name);
    let body = format!("Updated to v{}", entry.latest_version);
    let _ = app.notification().builder().title(title).body(body).show();
}
