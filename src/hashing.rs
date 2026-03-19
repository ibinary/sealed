use image::{DynamicImage, GrayImage};
use image::imageops::FilterType;
use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use tracing::warn;

use crate::errors::SealedResult;

const DEFAULT_PERCEPTUAL_HASH_SIZE: u32 = 8;

/// Hash record for a single image artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashRecord {
    pub sha256: String,
    pub blake3: String,
    pub ahash: String,
    pub dhash: String,
    #[serde(default)]
    pub phash: String,
    pub width: u32,
    pub height: u32,
}

/// SHA-256 over raw RGBA pixel bytes (format-independent).
pub fn sha256_pixel_hash(img: &DynamicImage) -> String {
    let rgba = img.to_rgba8();
    let raw_bytes = rgba.as_raw();
    let mut hasher = Sha256::new();
    hasher.update(raw_bytes);
    let result = hasher.finalize();
    hex::encode(result)
}

/// BLAKE3 over raw RGBA pixel bytes.
pub fn blake3_pixel_hash(img: &DynamicImage) -> String {
    let rgba = img.to_rgba8();
    let raw_bytes = rgba.as_raw();
    let hash = blake3::hash(raw_bytes);
    hash.to_hex().to_string()
}

/// Average Hash (aHash): resize to small grayscale, threshold against average.
pub fn ahash(img: &GrayImage, hash_size: u32) -> u64 {
    let resized = image::imageops::resize(img, hash_size, hash_size, FilterType::Nearest);
    let total_pixels = (hash_size * hash_size) as u64;
    let avg: u64 = resized.pixels().map(|p| p[0] as u64).sum::<u64>() / total_pixels;
    let mut hash: u64 = 0;
    for pixel in resized.pixels() {
        hash <<= 1;
        if pixel[0] as u64 >= avg {
            hash |= 1;
        }
    }
    hash
}

/// Difference Hash (dHash): compare adjacent pixels in a resized grayscale.
pub fn dhash(img: &GrayImage, hash_size: u32) -> u64 {
    let resized = image::imageops::resize(img, hash_size + 1, hash_size, FilterType::Nearest);
    let mut hash: u64 = 0;
    for row in 0..hash_size {
        for col in 0..hash_size {
            let left = resized.get_pixel(col, row).0[0] as u64;
            let right = resized.get_pixel(col + 1, row).0[0] as u64;
            hash <<= 1;
            if left > right {
                hash |= 1;
            }
        }
    }
    hash
}

/// Perceptual Hash (pHash): DCT-based, threshold low-frequency coefficients against median.
pub fn phash(img: &GrayImage) -> u64 {
    let dct_size: u32 = 32;
    let hash_size: u32 = 8;

    let resized = image::imageops::resize(img, dct_size, dct_size, FilterType::Lanczos3);

    let pixels: Vec<f64> = resized.pixels().map(|p| p[0] as f64).collect();

    let mut dct = vec![0.0f64; (dct_size * dct_size) as usize];
    for u in 0..dct_size {
        for v in 0..dct_size {
            let mut sum = 0.0f64;
            for x in 0..dct_size {
                for y in 0..dct_size {
                    let px = pixels[(x * dct_size + y) as usize];
                    let cos_x = ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI
                        / (2.0 * dct_size as f64))
                        .cos();
                    let cos_y = ((2 * y + 1) as f64 * v as f64 * std::f64::consts::PI
                        / (2.0 * dct_size as f64))
                        .cos();
                    sum += px * cos_x * cos_y;
                }
            }
            dct[(u * dct_size + v) as usize] = sum;
        }
    }

    let mut low_freq: Vec<f64> = Vec::with_capacity((hash_size * hash_size) as usize);
    for u in 0..hash_size {
        for v in 0..hash_size {
            if u == 0 && v == 0 {
                continue; // skip DC component
            }
            low_freq.push(dct[(u * dct_size + v) as usize]);
        }
    }

    let mut sorted = low_freq.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted[sorted.len() / 2];

    let mut hash: u64 = 0;
    for val in &low_freq {
        hash <<= 1;
        if *val > median {
            hash |= 1;
        }
    }
    hash
}

/// Hamming distance between two perceptual hashes. 0 = identical.
pub fn hamming_distance(h1: u64, h2: u64) -> u32 {
    (h1 ^ h2).count_ones()
}

/// Compute all hashes for an image.
pub fn compute_hash_record(img: &DynamicImage) -> SealedResult<HashRecord> {
    let rgba = img.to_rgba8();
    compute_hash_record_from_rgba(&rgba)
}

/// Compute all hashes from an already-decoded RGBA buffer.
pub fn compute_hash_record_from_rgba(rgba: &image::RgbaImage) -> SealedResult<HashRecord> {
    let (width, height) = rgba.dimensions();
    let raw_bytes = rgba.as_raw();

    let mut sha_hasher = Sha256::new();
    sha_hasher.update(raw_bytes);
    let sha256 = hex::encode(sha_hasher.finalize());

    let blake3 = blake3::hash(raw_bytes).to_hex().to_string();

    let gray = image::imageops::grayscale(rgba);

    Ok(HashRecord {
        sha256,
        blake3,
        ahash: format!("{:016x}", ahash(&gray, DEFAULT_PERCEPTUAL_HASH_SIZE)),
        dhash: format!("{:016x}", dhash(&gray, DEFAULT_PERCEPTUAL_HASH_SIZE)),
        phash: format!("{:016x}", phash(&gray)),
        width,
        height,
    })
}

/// Confidence level for perceptual similarity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SimilarityConfidence {
    Exact,
    High,
    Medium,
    Low,
    None,
}

impl std::fmt::Display for SimilarityConfidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact => write!(f, "EXACT"),
            Self::High => write!(f, "HIGH"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::Low => write!(f, "LOW"),
            Self::None => write!(f, "NONE"),
        }
    }
}

/// Compare two hash records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarityReport {
    pub exact_match: bool,
    pub sha256_match: bool,
    pub blake3_match: bool,
    pub ahash_hamming: u32,
    pub dhash_hamming: u32,
    pub phash_hamming: u32,
    pub perceptually_similar: bool,
    pub confidence: SimilarityConfidence,
}

fn parse_phash_hex(hex_str: &str, label: &str) -> u64 {
    match u64::from_str_radix(hex_str, 16) {
        Ok(v) => v,
        Err(e) => {
            warn!("Corrupted perceptual hash for {}: '{}' ({}). Defaulting to 0 — comparison may be unreliable.", label, hex_str, e);
            0
        }
    }
}

pub fn compare_hashes(a: &HashRecord, b: &HashRecord) -> SimilarityReport {
    let sha256_match = a.sha256 == b.sha256;
    let blake3_match = a.blake3 == b.blake3;

    let a_ahash = parse_phash_hex(&a.ahash, "a.ahash");
    let b_ahash = parse_phash_hex(&b.ahash, "b.ahash");
    let a_dhash = parse_phash_hex(&a.dhash, "a.dhash");
    let b_dhash = parse_phash_hex(&b.dhash, "b.dhash");

    let a_phash = parse_phash_hex(&a.phash, "a.phash");
    let b_phash = parse_phash_hex(&b.phash, "b.phash");

    let ahash_hamming = hamming_distance(a_ahash, b_ahash);
    let dhash_hamming = hamming_distance(a_dhash, b_dhash);
    let phash_hamming = hamming_distance(a_phash, b_phash);

    let exact_match = sha256_match && blake3_match;

    let best_hamming = ahash_hamming.min(dhash_hamming).min(phash_hamming);
    let avg_hamming = (ahash_hamming + dhash_hamming + phash_hamming) / 3;

    let confidence = if exact_match {
        SimilarityConfidence::Exact
    } else if best_hamming <= 5 && avg_hamming <= 7 {
        SimilarityConfidence::High
    } else if best_hamming <= 10 && avg_hamming <= 12 {
        SimilarityConfidence::Medium
    } else if best_hamming <= 15 && avg_hamming <= 18 {
        SimilarityConfidence::Low
    } else {
        SimilarityConfidence::None
    };

    let perceptually_similar = matches!(
        confidence,
        SimilarityConfidence::Exact
            | SimilarityConfidence::High
            | SimilarityConfidence::Medium
    );

    SimilarityReport {
        exact_match,
        sha256_match,
        blake3_match,
        ahash_hamming,
        dhash_hamming,
        phash_hamming,
        perceptually_similar,
        confidence,
    }
}
