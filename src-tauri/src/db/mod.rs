use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS watched_repos (
            owner TEXT NOT NULL,
            repo TEXT NOT NULL,
            added_at TEXT NOT NULL,
            last_index_etag TEXT,
            last_index_fetched_at TEXT,
            PRIMARY KEY (owner, repo)
        );
        CREATE TABLE IF NOT EXISTS modpack_cache (
            owner TEXT NOT NULL,
            repo TEXT NOT NULL,
            modpack_id TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT NOT NULL,
            latest_tag TEXT NOT NULL,
            latest_version INTEGER NOT NULL,
            manifest_asset TEXT NOT NULL,
            mods_asset TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (owner, repo, modpack_id)
        );
        CREATE TABLE IF NOT EXISTS friend_sync_state (
            owner TEXT NOT NULL,
            repo TEXT NOT NULL,
            modpack_id TEXT NOT NULL,
            destination_path TEXT NOT NULL,
            synced_version INTEGER NOT NULL,
            synced_manifest_json TEXT NOT NULL,
            last_synced_at TEXT NOT NULL,
            PRIMARY KEY (owner, repo, modpack_id)
        );
        CREATE TABLE IF NOT EXISTS friend_exclusions (
            owner TEXT NOT NULL,
            repo TEXT NOT NULL,
            modpack_id TEXT NOT NULL,
            filename TEXT NOT NULL,
            excluded_at TEXT NOT NULL,
            PRIMARY KEY (owner, repo, modpack_id, filename)
        );",
    )?;
    Ok(conn)
}

pub fn now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WatchedRepo {
    pub owner: String,
    pub repo: String,
}

pub fn add_watched_repo(conn: &Connection, owner: &str, repo: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO watched_repos (owner, repo, added_at) VALUES (?1, ?2, ?3)",
        params![owner, repo, now()],
    )?;
    Ok(())
}

pub fn remove_watched_repo(conn: &Connection, owner: &str, repo: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM watched_repos WHERE owner = ?1 AND repo = ?2",
        params![owner, repo],
    )?;
    conn.execute(
        "DELETE FROM modpack_cache WHERE owner = ?1 AND repo = ?2",
        params![owner, repo],
    )?;
    Ok(())
}

pub fn list_watched_repos(conn: &Connection) -> rusqlite::Result<Vec<WatchedRepo>> {
    let mut stmt = conn.prepare("SELECT owner, repo FROM watched_repos ORDER BY added_at")?;
    let rows = stmt.query_map([], |row| {
        Ok(WatchedRepo {
            owner: row.get(0)?,
            repo: row.get(1)?,
        })
    })?;
    rows.collect()
}

pub fn get_repo_etag(
    conn: &Connection,
    owner: &str,
    repo: &str,
) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT last_index_etag FROM watched_repos WHERE owner = ?1 AND repo = ?2",
        params![owner, repo],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map(|v: Option<Option<String>>| v.flatten())
}

pub fn set_repo_etag(conn: &Connection, owner: &str, repo: &str, etag: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE watched_repos SET last_index_etag = ?3, last_index_fetched_at = ?4
         WHERE owner = ?1 AND repo = ?2",
        params![owner, repo, etag, now()],
    )?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CachedModpack {
    pub owner: String,
    pub repo: String,
    pub modpack_id: String,
    pub name: String,
    pub description: String,
    pub latest_tag: String,
    pub latest_version: u32,
    pub manifest_asset: String,
    pub mods_asset: String,
    pub updated_at: String,
}

pub fn replace_modpack_cache_for_repo(
    conn: &mut Connection,
    owner: &str,
    repo: &str,
    entries: &[crate::github::index::ModpackIndexEntry],
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM modpack_cache WHERE owner = ?1 AND repo = ?2",
        params![owner, repo],
    )?;
    for e in entries {
        tx.execute(
            "INSERT INTO modpack_cache
                (owner, repo, modpack_id, name, description, latest_tag, latest_version, manifest_asset, mods_asset, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                owner,
                repo,
                e.id,
                e.name,
                e.description,
                e.latest_tag,
                e.latest_version,
                e.manifest_asset,
                e.mods_asset,
                e.updated_at,
            ],
        )?;
    }
    tx.commit()
}

pub fn list_cached_modpacks(conn: &Connection) -> rusqlite::Result<Vec<CachedModpack>> {
    let mut stmt = conn.prepare(
        "SELECT owner, repo, modpack_id, name, description, latest_tag, latest_version, manifest_asset, mods_asset, updated_at
         FROM modpack_cache ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CachedModpack {
            owner: row.get(0)?,
            repo: row.get(1)?,
            modpack_id: row.get(2)?,
            name: row.get(3)?,
            description: row.get(4)?,
            latest_tag: row.get(5)?,
            latest_version: row.get(6)?,
            manifest_asset: row.get(7)?,
            mods_asset: row.get(8)?,
            updated_at: row.get(9)?,
        })
    })?;
    rows.collect()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncState {
    pub destination_path: String,
    pub synced_version: u32,
    pub synced_manifest_json: String,
}

pub fn get_sync_state(
    conn: &Connection,
    owner: &str,
    repo: &str,
    modpack_id: &str,
) -> rusqlite::Result<Option<SyncState>> {
    conn.query_row(
        "SELECT destination_path, synced_version, synced_manifest_json
         FROM friend_sync_state WHERE owner = ?1 AND repo = ?2 AND modpack_id = ?3",
        params![owner, repo, modpack_id],
        |row| {
            Ok(SyncState {
                destination_path: row.get(0)?,
                synced_version: row.get(1)?,
                synced_manifest_json: row.get(2)?,
            })
        },
    )
    .optional()
}

#[allow(clippy::too_many_arguments)]
pub fn set_sync_state(
    conn: &Connection,
    owner: &str,
    repo: &str,
    modpack_id: &str,
    destination_path: &str,
    synced_version: u32,
    synced_manifest_json: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO friend_sync_state
            (owner, repo, modpack_id, destination_path, synced_version, synced_manifest_json, last_synced_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(owner, repo, modpack_id) DO UPDATE SET
            destination_path = excluded.destination_path,
            synced_version = excluded.synced_version,
            synced_manifest_json = excluded.synced_manifest_json,
            last_synced_at = excluded.last_synced_at",
        params![
            owner,
            repo,
            modpack_id,
            destination_path,
            synced_version,
            synced_manifest_json,
            now()
        ],
    )?;
    Ok(())
}

pub fn list_exclusions(
    conn: &Connection,
    owner: &str,
    repo: &str,
    modpack_id: &str,
) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT filename FROM friend_exclusions WHERE owner = ?1 AND repo = ?2 AND modpack_id = ?3",
    )?;
    let rows = stmt.query_map(params![owner, repo, modpack_id], |row| row.get(0))?;
    rows.collect()
}

pub fn set_exclusion(
    conn: &Connection,
    owner: &str,
    repo: &str,
    modpack_id: &str,
    filename: &str,
    excluded: bool,
) -> rusqlite::Result<()> {
    if excluded {
        conn.execute(
            "INSERT OR IGNORE INTO friend_exclusions (owner, repo, modpack_id, filename, excluded_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![owner, repo, modpack_id, filename, now()],
        )?;
    } else {
        conn.execute(
            "DELETE FROM friend_exclusions WHERE owner = ?1 AND repo = ?2 AND modpack_id = ?3 AND filename = ?4",
            params![owner, repo, modpack_id, filename],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusion_roundtrip_and_sync_state_lookup() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE friend_exclusions (
                owner TEXT NOT NULL, repo TEXT NOT NULL, modpack_id TEXT NOT NULL,
                filename TEXT NOT NULL, excluded_at TEXT NOT NULL,
                PRIMARY KEY (owner, repo, modpack_id, filename)
            );
            CREATE TABLE friend_sync_state (
                owner TEXT NOT NULL, repo TEXT NOT NULL, modpack_id TEXT NOT NULL,
                destination_path TEXT NOT NULL, synced_version INTEGER NOT NULL,
                synced_manifest_json TEXT NOT NULL, last_synced_at TEXT NOT NULL,
                PRIMARY KEY (owner, repo, modpack_id)
            );",
        )
        .unwrap();

        assert!(list_exclusions(&conn, "o", "r", "m").unwrap().is_empty());

        set_exclusion(&conn, "o", "r", "m", "bad.jar", true).unwrap();
        assert_eq!(list_exclusions(&conn, "o", "r", "m").unwrap(), vec!["bad.jar"]);

        // Excluding twice is idempotent (INSERT OR IGNORE).
        set_exclusion(&conn, "o", "r", "m", "bad.jar", true).unwrap();
        assert_eq!(list_exclusions(&conn, "o", "r", "m").unwrap().len(), 1);

        set_exclusion(&conn, "o", "r", "m", "bad.jar", false).unwrap();
        assert!(list_exclusions(&conn, "o", "r", "m").unwrap().is_empty());

        assert!(get_sync_state(&conn, "o", "r", "m").unwrap().is_none());
        set_sync_state(&conn, "o", "r", "m", "/dest", 2, "{}").unwrap();
        let state = get_sync_state(&conn, "o", "r", "m").unwrap().unwrap();
        assert_eq!(state.destination_path, "/dest");
        assert_eq!(state.synced_version, 2);
    }
}
