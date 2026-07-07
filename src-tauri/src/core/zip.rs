use super::manifest::ModFile;
use crate::error::CoreResult;
use std::fs::File;
use std::io;
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Streams the given mod files (by relative path within `mods_dir`) into a
/// single zip at `dest_zip`, without holding the whole archive in memory.
pub fn build_mods_zip(mods_dir: &Path, dest_zip: &Path, files: &[ModFile]) -> CoreResult<()> {
    let file = File::create(dest_zip)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for f in files {
        writer.start_file(&f.path, options)?;
        let mut src = File::open(mods_dir.join(&f.path))?;
        io::copy(&mut src, &mut writer)?;
    }
    writer.finish()?;
    Ok(())
}

/// Extracts every entry in the zip into `dest_dir`, writing to a temp file
/// and renaming into place so a crash mid-extract never leaves a half-written jar.
pub fn extract_all(zip_path: &Path, dest_dir: &Path) -> CoreResult<()> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    std::fs::create_dir_all(dest_dir)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let tmp_path = dest_dir.join(format!(".{}.tmp", name.display()));
        {
            let mut out = File::create(&tmp_path)?;
            io::copy(&mut entry, &mut out)?;
        }
        std::fs::rename(&tmp_path, dest_dir.join(&name))?;
    }
    Ok(())
}
