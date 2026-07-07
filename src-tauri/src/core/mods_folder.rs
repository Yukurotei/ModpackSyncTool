use super::manifest::ModFile;
use crate::error::CoreResult;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;

/// Scans a flat mods folder for `.jar` files (skipping anything else, e.g.
/// `.jar.disabled` files left by launchers that support disabling mods —
/// we only ever sync what would actually load).
pub fn scan(dir: &Path) -> CoreResult<Vec<ModFile>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.to_lowercase().ends_with(".jar") {
            continue;
        }
        let (sha256, size) = hash_file(&path)?;
        out.push(ModFile {
            path: name.to_string(),
            sha256,
            size,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn hash_file(path: &Path) -> CoreResult<(String, u64)> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((format!("{:x}", hasher.finalize()), size))
}
