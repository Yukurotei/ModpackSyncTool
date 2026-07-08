pub mod background;
pub mod commands;
pub mod core;
pub mod db;
pub mod error;
pub mod github;
pub mod state;

use commands::publish::{
    clear_github_token, get_or_create_publish_repo, has_github_token, list_published_modpacks,
    publish_modpack, set_github_token,
};
use commands::sync::{
    add_watched_repo, apply_sync, delete_synced_file, get_auto_sync_enabled, get_exclusions,
    get_modpack_files, list_modpacks, list_watched_repos, preview_sync, refresh_repo,
    remove_watched_repo, set_auto_sync_enabled, set_exclusion,
};
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch was attempted — surface the existing window
            // instead of letting a whole new process start up.
            if let Some(window) = app.webview_windows().values().next() {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            background::tray::setup(&handle)?;
            background::poller::spawn(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_github_token,
            has_github_token,
            clear_github_token,
            get_or_create_publish_repo,
            list_published_modpacks,
            publish_modpack,
            add_watched_repo,
            remove_watched_repo,
            list_watched_repos,
            refresh_repo,
            list_modpacks,
            get_exclusions,
            set_exclusion,
            get_modpack_files,
            delete_synced_file,
            preview_sync,
            apply_sync,
            get_auto_sync_enabled,
            set_auto_sync_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
