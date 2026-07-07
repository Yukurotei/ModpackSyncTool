use super::diff::SyncPlan;
use crate::error::CoreResult;
use std::fs;
use std::path::Path;

/// Applies a `SyncPlan` against `dest_dir`, reading added/updated files from
/// `source_dir` (an already-extracted mods folder). Writes go through a temp
/// file + rename so a crash mid-copy never leaves a partially-written jar.
pub fn apply_plan(plan: &SyncPlan, source_dir: &Path, dest_dir: &Path) -> CoreResult<()> {
    fs::create_dir_all(dest_dir)?;

    for f in plan.to_add.iter().chain(plan.to_update.iter()) {
        let src = source_dir.join(&f.path);
        let tmp = dest_dir.join(format!(".{}.tmp", f.path));
        fs::copy(&src, &tmp)?;
        fs::rename(&tmp, dest_dir.join(&f.path))?;
    }

    for path in &plan.to_remove {
        let target = dest_dir.join(path);
        if target.exists() {
            fs::remove_file(target)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::manifest::ModFile;
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn adds_updates_and_removes_files() {
        let source = tempdir().unwrap();
        let dest = tempdir().unwrap();

        fs::write(source.path().join("new.jar"), b"new-content").unwrap();
        fs::write(source.path().join("changed.jar"), b"v2").unwrap();
        fs::write(dest.path().join("changed.jar"), b"v1").unwrap();
        fs::write(dest.path().join("stale.jar"), b"old").unwrap();

        let plan = SyncPlan {
            to_add: vec![ModFile {
                path: "new.jar".into(),
                sha256: "x".into(),
                size: 11,
            }],
            to_update: vec![ModFile {
                path: "changed.jar".into(),
                sha256: "y".into(),
                size: 2,
            }],
            to_remove: vec!["stale.jar".into()],
            excluded: vec![],
        };

        apply_plan(&plan, source.path(), dest.path()).unwrap();

        assert!(dest.path().join("new.jar").exists());
        assert_eq!(
            fs::read(dest.path().join("changed.jar")).unwrap(),
            b"v2"
        );
        assert!(!dest.path().join("stale.jar").exists());
    }
}
