use super::manifest::{Manifest, ModFile};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SyncPlan {
    pub to_add: Vec<ModFile>,
    pub to_update: Vec<ModFile>,
    pub to_remove: Vec<String>,
    pub excluded: Vec<String>,
}

/// Computes what a sync should do to make `local_files` match `manifest`,
/// honoring `exclusions` and never touching anything we didn't previously
/// place ourselves (tracked via `previously_synced`, the set of paths this
/// app wrote to the destination on the last successful sync).
pub fn compute_plan(
    manifest: &Manifest,
    exclusions: &HashSet<String>,
    local_files: &[ModFile],
    previously_synced: &HashSet<String>,
) -> SyncPlan {
    let local_map: HashMap<&str, &str> = local_files
        .iter()
        .map(|f| (f.path.as_str(), f.sha256.as_str()))
        .collect();

    let mut plan = SyncPlan::default();
    let mut target_paths: HashSet<&str> = HashSet::new();

    for f in &manifest.files {
        if exclusions.contains(&f.path) {
            plan.excluded.push(f.path.clone());
            continue;
        }
        target_paths.insert(f.path.as_str());
        match local_map.get(f.path.as_str()) {
            None => plan.to_add.push(f.clone()),
            Some(hash) if *hash != f.sha256 => plan.to_update.push(f.clone()),
            _ => {}
        }
    }

    for path in previously_synced {
        if !target_paths.contains(path.as_str()) && local_map.contains_key(path.as_str()) {
            plan.to_remove.push(path.clone());
        }
    }
    plan.to_remove.sort();

    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mf(path: &str, sha: &str) -> ModFile {
        ModFile {
            path: path.to_string(),
            sha256: sha.to_string(),
            size: 1,
        }
    }

    fn manifest(files: Vec<ModFile>) -> Manifest {
        Manifest::new("test-pack", 1, files)
    }

    #[test]
    fn adds_new_files_not_present_locally() {
        let m = manifest(vec![mf("a.jar", "h1")]);
        let plan = compute_plan(&m, &HashSet::new(), &[], &HashSet::new());
        assert_eq!(plan.to_add, vec![mf("a.jar", "h1")]);
        assert!(plan.to_update.is_empty());
        assert!(plan.to_remove.is_empty());
    }

    #[test]
    fn updates_files_with_mismatched_hash() {
        let m = manifest(vec![mf("a.jar", "h2")]);
        let local = vec![mf("a.jar", "h1")];
        let plan = compute_plan(&m, &HashSet::new(), &local, &HashSet::new());
        assert_eq!(plan.to_update, vec![mf("a.jar", "h2")]);
        assert!(plan.to_add.is_empty());
    }

    #[test]
    fn removes_files_no_longer_in_manifest_that_we_placed() {
        let m = manifest(vec![]);
        let local = vec![mf("old.jar", "h1")];
        let mut previously_synced = HashSet::new();
        previously_synced.insert("old.jar".to_string());
        let plan = compute_plan(&m, &HashSet::new(), &local, &previously_synced);
        assert_eq!(plan.to_remove, vec!["old.jar".to_string()]);
    }

    #[test]
    fn never_removes_files_we_did_not_place() {
        let m = manifest(vec![]);
        let local = vec![mf("friends-own.jar", "h1")];
        // not in previously_synced -> not ours to touch
        let plan = compute_plan(&m, &HashSet::new(), &local, &HashSet::new());
        assert!(plan.to_remove.is_empty());
    }

    #[test]
    fn excluded_files_are_never_added_and_reported() {
        let m = manifest(vec![mf("mac-incompatible.jar", "h1"), mf("ok.jar", "h2")]);
        let mut exclusions = HashSet::new();
        exclusions.insert("mac-incompatible.jar".to_string());
        let plan = compute_plan(&m, &exclusions, &[], &HashSet::new());
        assert_eq!(plan.to_add, vec![mf("ok.jar", "h2")]);
        assert_eq!(plan.excluded, vec!["mac-incompatible.jar".to_string()]);
    }

    #[test]
    fn excluded_files_stay_excluded_across_upstream_changes() {
        // Simulate: mod was excluded and previously synced (before exclusion took effect),
        // upstream is unchanged, but now the exclusion should force removal from target set,
        // and it should never be re-added on subsequent syncs.
        let m = manifest(vec![mf("mac-incompatible.jar", "h1")]);
        let mut exclusions = HashSet::new();
        exclusions.insert("mac-incompatible.jar".to_string());
        let local = vec![mf("mac-incompatible.jar", "h1")];
        let mut previously_synced = HashSet::new();
        previously_synced.insert("mac-incompatible.jar".to_string());

        let plan = compute_plan(&m, &exclusions, &local, &previously_synced);
        assert!(plan.to_add.is_empty());
        assert_eq!(plan.to_remove, vec!["mac-incompatible.jar".to_string()]);
        assert_eq!(plan.excluded, vec!["mac-incompatible.jar".to_string()]);
    }
}
