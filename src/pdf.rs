use std::path::Path;
use std::process::Command;
use image::{DynamicImage, ImageReader};
use tracing::info;

use crate::errors::{SealedError, SealedResult};
use crate::image_processing::{
    crop_towards_center, xor_random_pixels, xor_composite, seal_image, save_artifacts, SealConfig, SealedArtifacts,
};

/// Convert PDF pages to images, XOR-composite into a fingerprint, then seal.
pub fn process_pdf(
    pdf_file: &Path,
    output_dir: &Path,
    config: &SealConfig,
) -> SealedResult<SealedArtifacts> {
    if !pdf_file.exists() {
        return Err(SealedError::FileNotFound(pdf_file.display().to_string()));
    }

    let pages_dir = output_dir.join("pages");
    std::fs::create_dir_all(&pages_dir)?;

    info!("Converting PDF to images: {}", pdf_file.display());

    let page_prefix = pages_dir.join("page");
    let status = Command::new("pdftopng")
        .arg(pdf_file.as_os_str())
        .arg(page_prefix.as_os_str())
        .status()
        .map_err(|e| SealedError::ExternalTool {
            tool: "pdftopng".to_string(),
            message: format!("Failed to execute: {}. Is poppler-utils installed?", e),
        })?;

    if !status.success() {
        return Err(SealedError::ExternalTool {
            tool: "pdftopng".to_string(),
            message: "pdftopng exited with non-zero status".to_string(),
        });
    }

    let mut page_files: Vec<_> = std::fs::read_dir(&pages_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |ext| ext == "png"))
        .collect();

    page_files.sort();

    if page_files.is_empty() {
        return Err(SealedError::InvalidInput(
            "No pages extracted from PDF".to_string(),
        ));
    }

    info!("Processing {} pages via XOR compositing", page_files.len());

    let mut xor_image: Option<DynamicImage> = None;
    for page_path in &page_files {
        let page_img = ImageReader::open(page_path)
            .map_err(|e| SealedError::Io(e))?
            .decode()?;

            let cropped_page = crop_towards_center(&page_img, config)?;

        match xor_image {
            Some(ref mut xor_img) => {
                xor_composite(xor_img, &cropped_page);
            }
            None => {
                xor_image = Some(cropped_page);
            }
        }
    }

    let mut img = xor_image.ok_or_else(|| {
        SealedError::InvalidInput("Failed to produce XOR composite from PDF pages".to_string())
    })?;

    xor_random_pixels(&mut img, 0.05);

    let processed_path = output_dir.join("processed_page.png");
    img.save(&processed_path)?;
    info!("XOR composite of {} pages saved to {}", page_files.len(), processed_path.display());

    let artifacts = seal_image(&img, config)?;
    save_artifacts(&artifacts, output_dir)?;

    Ok(artifacts)
}
