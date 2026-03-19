use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;
use tracing::info;

use crate::errors::SealedResult;

/// Create a ZIP archive of all sealed artifacts.
pub fn create_archive(
    output_dir: &Path,
    archive_name: &str,
) -> SealedResult<std::path::PathBuf> {
    let archive_path = output_dir.join(format!("{}.zip", archive_name));
    let file = File::create(&archive_path)?;
    let mut zip = ZipWriter::new(file);

    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    for entry in fs::read_dir(output_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() || path.extension().map_or(false, |ext| ext == "zip") {
            continue;
        }

        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        if file_name == "tile_index.json" || file_name == "ots_upgrade.log" {
            continue;
        }

        let data = fs::read(&path)?;

        zip.start_file(&file_name, options)?;
        zip.write_all(&data)?;

        info!("Added to archive: {}", file_name);
    }

    zip.finish()?;
    info!("Archive created: {}", archive_path.display());

    Ok(archive_path)
}
