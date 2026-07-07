//! M1 test harness: exercises the pack/scan/diff/apply pipeline against real
//! folders on disk without any GitHub involvement.
use modpacksync_lib::core::{apply, diff, manifest::Manifest, mods_folder, zip as core_zip};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("pack") => cmd_pack(&args[2..]),
        Some("sync") => cmd_sync(&args[2..]),
        _ => {
            eprintln!(
                "usage:\n  \
                 modpacksync-cli pack <mods_dir> <modpack_id> <version> <out_dir>\n  \
                 modpacksync-cli sync <manifest_json> <source_dir> <dest_dir> \
                 [--exclude a.jar,b.jar] [--last-synced <prev_manifest.json>]"
            );
            std::process::exit(1);
        }
    }
}

fn cmd_pack(args: &[String]) {
    let [mods_dir, modpack_id, version, out_dir] = args else {
        eprintln!("pack requires: <mods_dir> <modpack_id> <version> <out_dir>");
        std::process::exit(1);
    };
    let mods_dir = PathBuf::from(mods_dir);
    let version: u32 = version.parse().expect("version must be a number");
    let out_dir = PathBuf::from(out_dir);
    fs::create_dir_all(&out_dir).expect("create out_dir");

    let files = mods_folder::scan(&mods_dir).expect("scan mods_dir");
    let manifest = Manifest::new(modpack_id.clone(), version, files.clone());

    let manifest_path = out_dir.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .expect("write manifest");

    let zip_path = out_dir.join("mods.zip");
    core_zip::build_mods_zip(&mods_dir, &zip_path, &files).expect("build zip");

    let total_bytes: u64 = files.iter().map(|f| f.size).sum();
    println!(
        "Packed {} mods ({total_bytes} bytes) -> {}",
        files.len(),
        out_dir.display()
    );
    println!("  manifest: {}", manifest_path.display());
    println!("  zip:      {}", zip_path.display());
}

fn cmd_sync(args: &[String]) {
    if args.len() < 3 {
        eprintln!("sync requires: <manifest_json> <source_dir> <dest_dir> [--exclude a,b] [--last-synced <path>]");
        std::process::exit(1);
    }
    let manifest_path = PathBuf::from(&args[0]);
    let source_dir = PathBuf::from(&args[1]);
    let dest_dir = PathBuf::from(&args[2]);

    let mut exclusions: HashSet<String> = HashSet::new();
    let mut last_synced: Option<Manifest> = None;

    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--exclude" => {
                i += 1;
                for name in args[i].split(',') {
                    exclusions.insert(name.to_string());
                }
            }
            "--last-synced" => {
                i += 1;
                let s = fs::read_to_string(&args[i]).expect("read --last-synced manifest");
                last_synced = Some(serde_json::from_str(&s).expect("parse --last-synced manifest"));
            }
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    let manifest_json = fs::read_to_string(&manifest_path).expect("read manifest");
    let manifest: Manifest = serde_json::from_str(&manifest_json).expect("parse manifest");

    fs::create_dir_all(&dest_dir).expect("create dest_dir");
    let local_files = mods_folder::scan(&dest_dir).expect("scan dest_dir");

    let previously_synced: HashSet<String> = last_synced
        .as_ref()
        .map(|m| m.files.iter().map(|f| f.path.clone()).collect())
        .unwrap_or_default();

    let plan = diff::compute_plan(&manifest, &exclusions, &local_files, &previously_synced);

    println!(
        "Plan: add {}, update {}, remove {}, excluded {}",
        plan.to_add.len(),
        plan.to_update.len(),
        plan.to_remove.len(),
        plan.excluded.len()
    );
    for f in &plan.to_add {
        println!("  + {}", f.path);
    }
    for f in &plan.to_update {
        println!("  ~ {}", f.path);
    }
    for p in &plan.to_remove {
        println!("  - {p}");
    }
    for p in &plan.excluded {
        println!("  x {p} (excluded)");
    }

    apply::apply_plan(&plan, &source_dir, &dest_dir).expect("apply plan");

    // Record exactly what we just synced (post-exclusion) so the next run
    // knows what it's allowed to remove and never re-adds an excluded mod.
    let synced_manifest = manifest.without_excluded(&exclusions);
    let synced_manifest_path = dest_dir.join(".modpacksync-synced.json");
    fs::write(
        &synced_manifest_path,
        serde_json::to_string_pretty(&synced_manifest).unwrap(),
    )
    .expect("write synced state");
    println!("Synced. State written to {}", synced_manifest_path.display());
}
