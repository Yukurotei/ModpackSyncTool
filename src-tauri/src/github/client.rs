use crate::error::{GhResult, GitHubError};
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{Client, Response, StatusCode, Url};
use serde_json::{json, Value};

const API_VERSION: &str = "2022-11-28";
const USER_AGENT: &str = "ModpackSync/0.1";

pub struct ReleaseInfo {
    pub id: u64,
    pub html_url: String,
    /// The `upload_url` template from GitHub's response, e.g.
    /// `https://uploads.github.com/repos/o/r/releases/123/assets{?name,label}`.
    pub upload_url_template: String,
}

pub struct ContentsFile {
    pub bytes: Vec<u8>,
    pub sha: String,
}

/// Thin wrapper over the handful of GitHub REST endpoints ModpackSync needs.
/// Deliberately not using an SDK crate (e.g. octocrab): asset uploads POST to
/// a different host (`uploads.github.com`) with a raw byte body, which is
/// awkward to express through higher-level clients, and we only need ~5
/// endpoints total.
pub struct GitHubClient {
    http: Client,
}

impl GitHubClient {
    pub fn new() -> Self {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("failed to build reqwest client");
        Self { http }
    }

    pub async fn validate_token(&self, token: &str) -> GhResult<String> {
        let resp = self
            .http
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .send()
            .await?;
        let resp = check_status(resp).await?;
        let json: Value = resp.json().await?;
        Ok(json["login"].as_str().unwrap_or_default().to_string())
    }

    pub async fn create_release(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        tag: &str,
        name: &str,
    ) -> GhResult<ReleaseInfo> {
        let url = format!("https://api.github.com/repos/{owner}/{repo}/releases");
        let body = json!({
            "tag_name": tag,
            "name": name,
            "draft": false,
            "prerelease": false,
        });
        let resp = self
            .http
            .post(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .json(&body)
            .send()
            .await?;
        let resp = check_status(resp).await?;
        let json: Value = resp.json().await?;
        Ok(ReleaseInfo {
            id: json["id"].as_u64().unwrap_or_default(),
            html_url: json["html_url"].as_str().unwrap_or_default().to_string(),
            upload_url_template: json["upload_url"].as_str().unwrap_or_default().to_string(),
        })
    }

    pub async fn upload_release_asset(
        &self,
        token: &str,
        upload_url_template: &str,
        asset_name: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> GhResult<()> {
        let base = upload_url_template
            .split('{')
            .next()
            .unwrap_or(upload_url_template);
        let mut url = Url::parse(base).map_err(|e| GitHubError::Api {
            status: 0,
            message: format!("invalid upload_url from GitHub: {e}"),
        })?;
        url.query_pairs_mut().append_pair("name", asset_name);

        let resp = self
            .http
            .post(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("Content-Type", content_type)
            .header("X-GitHub-Api-Version", API_VERSION)
            .body(bytes)
            .send()
            .await?;
        check_status(resp).await?;
        Ok(())
    }

    /// Checks whether `owner/repo` exists and is reachable with `token`.
    pub async fn repo_exists(&self, token: &str, owner: &str, repo: &str) -> GhResult<bool> {
        let url = format!("https://api.github.com/repos/{owner}/{repo}");
        let resp = self
            .http
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .send()
            .await?;
        Ok(resp.status().is_success())
    }

    /// Creates a new repo owned by the authenticated user, pre-initialized
    /// with a first commit (`auto_init: true`) so releases can be created
    /// against it immediately — GitHub's Releases API requires an existing
    /// default branch, which an empty repo doesn't have.
    pub async fn create_repo(&self, token: &str, name: &str, description: &str) -> GhResult<()> {
        let url = "https://api.github.com/user/repos";
        let body = json!({
            "name": name,
            "description": description,
            "private": false,
            "auto_init": true,
        });
        let resp = self
            .http
            .post(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .json(&body)
            .send()
            .await?;
        check_status(resp).await?;
        Ok(())
    }

    /// Reads a file from the repo's default branch via the Contents API.
    /// `token` is optional — friend-side reads should stay anonymous, though
    /// in practice friends fetch `index.json`/`manifest.json` via the raw
    /// CDN path instead of this API to avoid the REST rate limit entirely.
    pub async fn get_contents(
        &self,
        token: Option<&str>,
        owner: &str,
        repo: &str,
        path: &str,
    ) -> GhResult<Option<ContentsFile>> {
        let url = format!("https://api.github.com/repos/{owner}/{repo}/contents/{path}");
        let mut req = self
            .http
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION);
        if let Some(t) = token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }
        let resp = req.send().await?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let resp = check_status(resp).await?;
        let json: Value = resp.json().await?;
        let content_b64 = json["content"].as_str().unwrap_or_default().replace('\n', "");
        let bytes = STANDARD.decode(content_b64)?;
        let sha = json["sha"].as_str().unwrap_or_default().to_string();
        Ok(Some(ContentsFile { bytes, sha }))
    }

    /// Creates or updates a file on the default branch via the Contents API.
    /// Pass `sha` from a prior `get_contents` call when updating an existing
    /// file — GitHub uses it for optimistic concurrency and returns 409 if
    /// it's stale.
    pub async fn put_contents(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        path: &str,
        message: &str,
        content: &[u8],
        sha: Option<&str>,
    ) -> GhResult<()> {
        let url = format!("https://api.github.com/repos/{owner}/{repo}/contents/{path}");
        let encoded = STANDARD.encode(content);
        let mut body = json!({ "message": message, "content": encoded });
        if let Some(sha) = sha {
            body["sha"] = json!(sha);
        }
        let resp = self
            .http
            .put(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .json(&body)
            .send()
            .await?;
        check_status(resp).await?;
        Ok(())
    }

    pub fn is_conflict(err: &GitHubError) -> bool {
        matches!(err, GitHubError::Api { status: 409, .. })
    }

    /// Anonymously fetches a file from a repo's default branch via
    /// `raw.githubusercontent.com` (the `HEAD` ref resolves to whatever the
    /// default branch is, so we don't need a separate lookup). Doesn't count
    /// against the unauthenticated REST rate limit — safe for background
    /// polling. Returns `None` on a 304 when `etag` matches what's cached.
    pub async fn fetch_raw_conditional(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        etag: Option<&str>,
    ) -> GhResult<Option<(Vec<u8>, String)>> {
        let url = format!("https://raw.githubusercontent.com/{owner}/{repo}/HEAD/{path}");
        let mut req = self.http.get(url);
        if let Some(t) = etag {
            req = req.header("If-None-Match", t);
        }
        let resp = req.send().await?;
        if resp.status() == StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        let resp = check_status(resp).await?;
        let etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = resp.bytes().await?.to_vec();
        Ok(Some((bytes, etag)))
    }

    /// Anonymous lookup of a release's assets by tag — used by friends to find
    /// the `mods.zip`/`manifest.json` download URLs for a given modpack version.
    pub async fn get_release_by_tag(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
    ) -> GhResult<Vec<ReleaseAsset>> {
        let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}");
        let resp = self
            .http
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .send()
            .await?;
        let resp = check_status(resp).await?;
        let json: Value = resp.json().await?;
        let assets = json["assets"].as_array().cloned().unwrap_or_default();
        Ok(assets
            .into_iter()
            .map(|a| ReleaseAsset {
                name: a["name"].as_str().unwrap_or_default().to_string(),
                browser_download_url: a["browser_download_url"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            })
            .collect())
    }

    /// Downloads a public asset (e.g. a release's `browser_download_url`)
    /// anonymously, with no auth header.
    pub async fn download_bytes(&self, url: &str) -> GhResult<Vec<u8>> {
        let resp = self
            .http
            .get(url)
            .header("Accept", "application/octet-stream")
            .send()
            .await?;
        let resp = check_status(resp).await?;
        Ok(resp.bytes().await?.to_vec())
    }
}

pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new()
    }
}

async fn check_status(resp: Response) -> GhResult<Response> {
    if resp.status().is_success() {
        Ok(resp)
    } else {
        let status = resp.status().as_u16();
        let message = resp.text().await.unwrap_or_default();
        Err(GitHubError::Api { status, message })
    }
}
