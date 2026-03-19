use std::path::Path;
use std::process::Command;
use image::DynamicImage;
use image::ImageReader;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand::rngs::StdRng;
use tracing::info;

use crate::errors::{SealedError, SealedResult};
use crate::image_processing::{seal_image, save_artifacts, xor_composite, SealConfig, SealedArtifacts};

/// Extract video frames, XOR-composite into a fingerprint image, then seal.
pub fn process_video(
    video_file: &Path,
    output_dir: &Path,
    frame_interval: u64,
    sample: Option<usize>,
    config: &SealConfig,
) -> SealedResult<SealedArtifacts> {
    if !video_file.exists() {
        return Err(SealedError::FileNotFound(
            video_file.display().to_string(),
        ));
    }

    let frames_dir = output_dir.join("frames");
    std::fs::create_dir_all(&frames_dir)?;

    info!("Extracting frames from video: {}", video_file.display());

    let output_pattern = frames_dir.join("frame-%04d.png");
    let status = Command::new("ffmpeg")
        .arg("-i")
        .arg(video_file.as_os_str())
        .arg("-vf")
        .arg(format!("fps=1/{}", frame_interval))
        .arg(output_pattern.as_os_str())
        .status()
        .map_err(|e| SealedError::ExternalTool {
            tool: "ffmpeg".to_string(),
            message: format!("Failed to execute: {}. Is ffmpeg installed?", e),
        })?;

    if !status.success() {
        return Err(SealedError::ExternalTool {
            tool: "ffmpeg".to_string(),
            message: "ffmpeg exited with non-zero status".to_string(),
        });
    }

    let mut frame_files: Vec<_> = std::fs::read_dir(&frames_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |ext| ext == "png"))
        .collect();

    frame_files.sort();

    if frame_files.is_empty() {
        return Err(SealedError::InvalidInput(
            "No frames extracted from video".to_string(),
        ));
    }

    let frame_files = match sample {
        Some(n) if n < frame_files.len() => {
            let mut file = std::fs::File::open(video_file)?;
            let mut buf = vec![0u8; 65536];
            let bytes_read = std::io::Read::read(&mut file, &mut buf)?;
            buf.truncate(bytes_read);
            let seed_hash = blake3::hash(&buf);
            let seed: [u8; 32] = *seed_hash.as_bytes();
            let mut rng = StdRng::from_seed(seed);
            frame_files.choose_multiple(&mut rng, n).cloned().collect()
        }
        _ => frame_files,
    };

    info!("XOR-combining {} frames into fingerprint image", frame_files.len());

    let mut xor_image: Option<DynamicImage> = None;
    for path in &frame_files {
        let img = ImageReader::open(path)
            .map_err(|e| SealedError::Io(e))?
            .decode()?;

        match xor_image {
            Some(ref mut xor_img) => {
                xor_composite(xor_img, &img);
            }
            None => {
                xor_image = Some(img);
            }
        }
    }

    let xor_img = xor_image.ok_or_else(|| {
        SealedError::InvalidInput("Failed to produce XOR composite".to_string())
    })?;

    let xor_path = output_dir.join("xor_composite.png");
    xor_img.save(&xor_path)?;
    info!("XOR composite saved to {}", xor_path.display());

    let artifacts = seal_image(&xor_img, config)?;
    save_artifacts(&artifacts, output_dir)?;

    Ok(artifacts)
}
