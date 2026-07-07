use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum GitHubError {
    #[error("no GitHub token configured — add one in Settings")]
    MissingToken,
    #[error("no \"mods\" folder found in \"{0}\" — are you sure this is your instance folder? (It should be the folder that contains a mods/ subfolder, not the mods folder itself.)")]
    ModsFolderNotFound(String),
    #[error("GitHub API error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("keyring error: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error(transparent)]
    Core(#[from] CoreError),
}

pub type GhResult<T> = Result<T, GitHubError>;
