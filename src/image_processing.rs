use std::path::Path;

use image::{DynamicImage, GenericImageView, ImageBuffer, ImageFormat, ImageReader, Rgba, RgbaImage};
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use tracing::info;
use uuid::Uuid;

use crate::errors::{SealedError, SealedResult};
use crate::hashing::{compute_hash_record, compute_hash_record_from_rgba, HashRecord};

/// Sealing configuration.
#[derive(Debug, Clone)]
pub struct SealConfig {
    pub edge_width: u32,
    pub crop_margin_min: u32,
    pub crop_margin_max: u32,
}

impl Default for SealConfig {
    fn default() -> Self {
        Self {
            edge_width: 20,
            crop_margin_min: 10,
            crop_margin_max: 21,
        }
    }
}

/// All generated artifacts and their hashes from sealing.
#[derive(Debug)]
pub struct SealedArtifacts {
    pub original: DynamicImage,
    pub original_hashes: HashRecord,
    pub frame: RgbaImage,
    pub frame_hashes: HashRecord,
    pub cropped: RgbaImage,
    pub cropped_hashes: HashRecord,
    pub recombined: RgbaImage,
    pub recombined_hashes: HashRecord,
    pub share: RgbaImage,
    pub share_hashes: HashRecord,
}

/// Seal a single image: extract edges, generate artifacts, compute all hashes.
pub fn seal_image(img: &DynamicImage, config: &SealConfig) -> SealedResult<SealedArtifacts> {
    let (width, height) = img.dimensions();
    let ew = config.edge_width;

    if width <= 2 * ew || height <= 2 * ew {
        return Err(SealedError::InvalidInput(format!(
            "Image too small ({}x{}) for edge width {}. Need at least {}x{}.",
            width, height, ew, 2 * ew + 1, 2 * ew + 1
        )));
    }

    let top = img.view(0, 0, width, ew).to_image();
    let bottom = img.view(0, height - ew, width, ew).to_image();
    let left = img.view(0, ew, ew, height - 2 * ew).to_image();
    let right = img.view(width - ew, ew, ew, height - 2 * ew).to_image();
    let middle = img.view(ew, ew, width - 2 * ew, height - 2 * ew).to_image();

    let mut frame: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);
    image::imageops::replace(&mut frame, &top, 0, 0);
    image::imageops::replace(&mut frame, &bottom, 0, (height - ew) as i64);
    image::imageops::replace(&mut frame, &left, 0, ew as i64);
    image::imageops::replace(&mut frame, &right, (width - ew) as i64, ew as i64);

    let mut cropped: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);
    image::imageops::replace(&mut cropped, &middle, ew as i64, ew as i64);

    let share = middle.clone();

    let mut recombined: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);
    image::imageops::replace(&mut recombined, &top, 0, 0);
    image::imageops::replace(&mut recombined, &bottom, 0, (height - ew) as i64);
    image::imageops::replace(&mut recombined, &left, 0, ew as i64);
    image::imageops::replace(&mut recombined, &right, (width - ew) as i64, ew as i64);
    image::imageops::replace(&mut recombined, &middle, ew as i64, ew as i64);

    let original_hashes = compute_hash_record(img)?;
    let frame_hashes = compute_hash_record_from_rgba(&frame)?;
    let cropped_hashes = compute_hash_record_from_rgba(&cropped)?;
    let recombined_hashes = compute_hash_record_from_rgba(&recombined)?;
    let share_hashes = compute_hash_record_from_rgba(&share)?;

    Ok(SealedArtifacts {
        original: img.clone(),
        original_hashes,
        frame,
        frame_hashes,
        cropped,
        cropped_hashes,
        recombined,
        recombined_hashes,
        share,
        share_hashes,
    })
}

/// Crop inward from first non-white pixel on each edge.
pub fn crop_towards_center(img: &DynamicImage, config: &SealConfig) -> SealedResult<DynamicImage> {
    let (width, height) = img.dimensions();
    let mut top = 0u32;
    let mut bottom = height;
    let mut left = 0u32;
    let mut right = width;
    let mut found_content = false;

    'top_scan: for y in 0..height {
        for x in 0..width {
            let p = img.get_pixel(x, y);
            if p[0] != 255 || p[1] != 255 || p[2] != 255 {
                top = y;
                found_content = true;
                break 'top_scan;
            }
        }
    }

    if !found_content {
        return Err(SealedError::InvalidInput(
            "Image is entirely whitespace — no content to seal".to_string(),
        ));
    }

    'bottom_scan: for y in (0..height).rev() {
        for x in 0..width {
            let p = img.get_pixel(x, y);
            if p[0] != 255 || p[1] != 255 || p[2] != 255 {
                bottom = y;
                break 'bottom_scan;
            }
        }
    }

    'left_scan: for x in 0..width {
        for y in 0..height {
            let p = img.get_pixel(x, y);
            if p[0] != 255 || p[1] != 255 || p[2] != 255 {
                left = x;
                break 'left_scan;
            }
        }
    }

    'right_scan: for x in (0..width).rev() {
        for y in 0..height {
            let p = img.get_pixel(x, y);
            if p[0] != 255 || p[1] != 255 || p[2] != 255 {
                right = x;
                break 'right_scan;
            }
        }
    }

    let rgba = img.to_rgba8();
    let content_hash = blake3::hash(rgba.as_raw());
    let seed: [u8; 32] = *content_hash.as_bytes();
    let mut rng = StdRng::from_seed(seed);
    let margin = rng.gen_range(config.crop_margin_min..config.crop_margin_max);

    let x = (left + margin).min(width.saturating_sub(1));
    let y = (top + margin).min(height.saturating_sub(1));
    let w = (right - left).saturating_sub(2 * margin).max(1);
    let h = (bottom - top).saturating_sub(2 * margin).max(1);

    Ok(img.crop_imm(x, y, w, h))
}

/// XOR-composite an overlay onto a base image (pixel-by-pixel).
pub fn xor_composite(base: &mut DynamicImage, overlay: &DynamicImage) {
    let (bw, bh) = base.dimensions();
    let (ow, oh) = overlay.dimensions();
    let w = bw.min(ow);
    let h = bh.min(oh);
    let overlay_rgba = overlay.to_rgba8();
    let mut base_rgba = base.to_rgba8();
    for y in 0..h {
        for x in 0..w {
            let bp = base_rgba.get_pixel(x, y);
            let op = overlay_rgba.get_pixel(x, y);
            base_rgba.put_pixel(x, y, Rgba([
                bp[0] ^ op[0],
                bp[1] ^ op[1],
                bp[2] ^ op[2],
                255,
            ]));
        }
    }
    *base = DynamicImage::ImageRgba8(base_rgba);
}

/// XOR random pixels deterministically (content-seeded RNG).
pub fn xor_random_pixels(img: &mut DynamicImage, percentage: f32) {
    let (width, height) = img.dimensions();
    let num_pixels = ((width as f32) * (height as f32) * percentage).round() as u32;

    let rgba_for_seed = img.to_rgba8();
    let content_hash = blake3::hash(rgba_for_seed.as_raw());
    let seed: [u8; 32] = *content_hash.as_bytes();
    let mut rng = StdRng::from_seed(seed);

    let mut rgba_img = img.to_rgba8();

    for _ in 0..num_pixels {
        let x = rng.gen_range(0..width);
        let y = rng.gen_range(0..height);
        let pixel = rgba_img.get_pixel_mut(x, y);

        let rv = rng.gen::<u8>().max(1);
        pixel[0] ^= rv;
        pixel[1] ^= rv;
        pixel[2] ^= rv;
    }

    *img = DynamicImage::ImageRgba8(rgba_img);
}

/// Save sealed artifacts to a directory.
pub fn save_artifacts(
    artifacts: &SealedArtifacts,
    output_dir: &std::path::Path,
) -> SealedResult<Vec<std::path::PathBuf>> {
    std::fs::create_dir_all(output_dir)?;

    let mut paths = Vec::new();

    let original_path = output_dir.join("original.png");
    artifacts.original.save_with_format(&original_path, ImageFormat::Png)?;
    paths.push(original_path);

    let frame_path = output_dir.join("frame.png");
    image::save_buffer(&frame_path, artifacts.frame.as_raw(), artifacts.frame.width(), artifacts.frame.height(), image::ColorType::Rgba8)?;
    paths.push(frame_path);

    let cropped_path = output_dir.join("cropped.png");
    image::save_buffer(&cropped_path, artifacts.cropped.as_raw(), artifacts.cropped.width(), artifacts.cropped.height(), image::ColorType::Rgba8)?;
    paths.push(cropped_path);

    let share_path = output_dir.join("share.png");
    image::save_buffer(&share_path, artifacts.share.as_raw(), artifacts.share.width(), artifacts.share.height(), image::ColorType::Rgba8)?;
    paths.push(share_path);

    let recombined_path = output_dir.join("recombined.png");
    image::save_buffer(&recombined_path, artifacts.recombined.as_raw(), artifacts.recombined.width(), artifacts.recombined.height(), image::ColorType::Rgba8)?;
    paths.push(recombined_path);

    Ok(paths)
}

/// Open an image with content-based format detection. Falls back to ffmpeg.
pub fn open_image_by_content(path: &Path) -> SealedResult<DynamicImage> {
    let reader = ImageReader::open(path)
        .map_err(|e| SealedError::Io(e))?
        .with_guessed_format()
        .map_err(|e| SealedError::Io(e))?;

    match reader.decode() {
        Ok(img) => Ok(img),
        Err(e) => {
            info!(
                "Native decode failed ({}), attempting ffmpeg conversion...",
                e
            );
            convert_with_ffmpeg(path)
        }
    }
}

/// Convert unsupported formats to PNG via ffmpeg.
fn convert_with_ffmpeg(path: &Path) -> SealedResult<DynamicImage> {
    let temp_png = std::env::temp_dir().join(format!("sealed_convert_{}.png", Uuid::new_v4()));

    let status = std::process::Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(path.as_os_str())
        .arg(temp_png.as_os_str())
        .status()
        .map_err(|_| SealedError::ExternalTool {
            tool: "ffmpeg".to_string(),
            message: "ffmpeg not found. Install ffmpeg to support AVIF and other formats.".to_string(),
        })?;

    if !status.success() {
        return Err(SealedError::ExternalTool {
            tool: "ffmpeg".to_string(),
            message: format!("ffmpeg failed to convert {}. The file may be corrupted.", path.display()),
        });
    }

    let img = image::open(&temp_png).map_err(|e| SealedError::Image(e))?;

    let _ = std::fs::remove_file(&temp_png);

    Ok(img)
}
