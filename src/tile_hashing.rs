use std::collections::HashMap;
use std::f64::consts::PI;

use image::{DynamicImage, GenericImageView, GrayImage};
use image::imageops::FilterType;
use serde::{Serialize, Deserialize};
use tracing::info;

use crate::hashing::{compute_hash_record, compare_hashes, SimilarityReport};

const BLOCK_PX: u32 = 32;
const BLOCK_OVERLAP: f32 = 0.5;
const DCT_KEEP: usize = 4;
const BLOCK_MATCH_THRESHOLD: u32 = 3;
const MIN_CONSISTENT_VOTES: usize = 4;

/// Block DCT descriptor with grid position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDescriptor {
    pub col: u32,
    pub row: u32,
    pub px: u32,
    pub py: u32,
    pub descriptor: u16,
}

/// Block descriptor index for an image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileHashIndex {
    pub source_width: u32,
    pub source_height: u32,
    pub block_size: u32,
    pub cols: u32,
    pub rows: u32,
    pub blocks: Vec<BlockDescriptor>,
}

/// Result of tile-based crop detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileMatchResult {
    pub crop_detected: bool,
    pub consistent_votes: usize,
    pub total_matches: usize,
    pub estimated_offset: Option<(i32, i32)>,
    pub tiles_checked: usize,
    pub refined_similarity: Option<SimilarityReport>,
}

/// 2D DCT-II on an 8×8 block.
fn dct2_8x8(block: &[f64; 64]) -> [f64; 64] {
    let n = 8usize;
    let mut out = [0.0f64; 64];
    let mut row_tmp = [0.0f64; 64];
    for i in 0..n {
        for k in 0..n {
            let mut sum = 0.0;
            for j in 0..n {
                sum += block[i * n + j] * ((PI * (2 * j + 1) as f64 * k as f64) / (2.0 * n as f64)).cos();
            }
            let ck = if k == 0 { (1.0 / n as f64).sqrt() } else { (2.0 / n as f64).sqrt() };
            row_tmp[i * n + k] = ck * sum;
        }
    }
    for k in 0..n {
        for i in 0..n {
            let mut sum = 0.0;
            for j in 0..n {
                sum += row_tmp[j * n + k] * ((PI * (2 * j + 1) as f64 * i as f64) / (2.0 * n as f64)).cos();
            }
            let ci = if i == 0 { (1.0 / n as f64).sqrt() } else { (2.0 / n as f64).sqrt() };
            out[i * n + k] = ci * sum;
        }
    }
    out
}

/// Extract a compact binary descriptor from a grayscale block.
fn block_descriptor(gray: &GrayImage, x: u32, y: u32, w: u32, h: u32) -> u16 {
    let block_view = image::imageops::crop_imm(gray, x, y, w, h);
    let resized = image::imageops::resize(&*block_view, 8, 8, FilterType::Triangle);

    let mut data = [0.0f64; 64];
    for (i, px) in resized.pixels().enumerate() {
        data[i] = px.0[0] as f64;
    }

    let dct = dct2_8x8(&data);

    let mut coeffs = Vec::with_capacity(DCT_KEEP * DCT_KEEP - 1);
    for r in 0..DCT_KEEP {
        for c in 0..DCT_KEEP {
            if r == 0 && c == 0 { continue; } // skip DC
            coeffs.push(dct[r * 8 + c]);
        }
    }

    let mut sorted = coeffs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted[sorted.len() / 2];

    let mut desc: u16 = 0;
    for val in &coeffs {
        desc <<= 1;
        if *val > median {
            desc |= 1;
        }
    }
    desc
}

/// Hamming distance between two block descriptors.
fn block_hamming(a: u16, b: u16) -> u32 {
    (a ^ b).count_ones()
}

/// Generate a block descriptor index for an image.
pub fn generate_tile_index(img: &DynamicImage) -> TileHashIndex {
    let (width, height) = img.dimensions();
    let gray = image::imageops::grayscale(&img.to_rgba8());

    let step = ((BLOCK_PX as f32) * (1.0 - BLOCK_OVERLAP)).max(1.0) as u32;
    let mut blocks = Vec::new();
    let mut col_count = 0u32;
    let mut py = 0u32;
    let mut row = 0u32;
    while py + BLOCK_PX <= height {
        let mut px = 0u32;
        let mut col = 0u32;
        while px + BLOCK_PX <= width {
            let desc = block_descriptor(&gray, px, py, BLOCK_PX, BLOCK_PX);
            blocks.push(BlockDescriptor { col, row, px, py, descriptor: desc });
            px += step;
            col += 1;
        }
        if col > col_count { col_count = col; }
        py += step;
        row += 1;
    }
    let row_count = row;

    info!(
        "Generated {} block descriptors ({}×{} grid, {}px blocks)",
        blocks.len(), col_count, row_count, BLOCK_PX
    );

    TileHashIndex {
        source_width: width,
        source_height: height,
        block_size: BLOCK_PX,
        cols: col_count,
        rows: row_count,
        blocks,
    }
}

/// Compare a suspect image against a block descriptor index.
pub fn compare_against_tiles(
    suspect: &DynamicImage,
    original: &DynamicImage,
    index: &TileHashIndex,
) -> TileMatchResult {
    let (s_width, s_height) = suspect.dimensions();

    let scale_x = index.source_width as f32 / s_width as f32;
    let scale_y = index.source_height as f32 / s_height as f32;

    let block_sizes: Vec<u32> = {
        let base = (BLOCK_PX as f32 / scale_x.max(scale_y)).round() as u32;
        let mut sizes: Vec<u32> = vec![BLOCK_PX];
        if base >= 8 && base != BLOCK_PX {
            sizes.push(base);
        }
        for &factor in &[0.5f32, 0.75, 1.25, 1.5] {
            let s = (BLOCK_PX as f32 * factor).round() as u32;
            if s >= 8 && !sizes.contains(&s) {
                sizes.push(s);
            }
        }
        sizes
    };

    let mut overall_best = TileMatchResult {
        crop_detected: false,
        consistent_votes: 0,
        total_matches: 0,
        estimated_offset: None,
        tiles_checked: index.blocks.len(),
        refined_similarity: None,
    };

    for &suspect_block_size in &block_sizes {
        if suspect_block_size > s_width || suspect_block_size > s_height {
            continue;
        }

        let suspect_gray = image::imageops::grayscale(&suspect.to_rgba8());
        let step = ((suspect_block_size as f32) * (1.0 - BLOCK_OVERLAP)).max(1.0) as u32;

        let mut suspect_blocks = Vec::new();
        let mut spy = 0u32;
        while spy + suspect_block_size <= s_height {
            let mut spx = 0u32;
            while spx + suspect_block_size <= s_width {
                let desc = block_descriptor(&suspect_gray, spx, spy, suspect_block_size, suspect_block_size);
                suspect_blocks.push((spx, spy, desc));
                spx += step;
            }
            spy += step;
        }

        let scale_ratio = BLOCK_PX as f32 / suspect_block_size as f32;
        let mut offset_votes: HashMap<(i32, i32), usize> = HashMap::new();
        let mut total_matches = 0usize;

        for &(spx, spy, s_desc) in &suspect_blocks {
            let mut best_dist = u32::MAX;
            let mut best_orig: Option<&BlockDescriptor> = None;

            for orig_block in &index.blocks {
                let dist = block_hamming(s_desc, orig_block.descriptor);
                if dist < best_dist {
                    best_dist = dist;
                    best_orig = Some(orig_block);
                }
            }

            if best_dist <= BLOCK_MATCH_THRESHOLD {
                if let Some(orig) = best_orig {
                    total_matches += 1;
                    let mapped_x = (spx as f32 * scale_ratio).round() as i32;
                    let mapped_y = (spy as f32 * scale_ratio).round() as i32;
                    let dx = orig.px as i32 - mapped_x;
                    let dy = orig.py as i32 - mapped_y;
                    let quant = (BLOCK_PX / 2).max(1) as i32;
                    let qdx = (dx / quant) * quant;
                    let qdy = (dy / quant) * quant;
                    *offset_votes.entry((qdx, qdy)).or_insert(0) += 1;
                }
            }
        }

        if let Some((&best_offset, &votes)) = offset_votes.iter().max_by_key(|&(_, v)| *v) {
            if votes > overall_best.consistent_votes {
                overall_best.consistent_votes = votes;
                overall_best.total_matches = total_matches;
                overall_best.estimated_offset = Some(best_offset);
                overall_best.crop_detected = votes >= MIN_CONSISTENT_VOTES;
            }
        }
    }

    // Refinement: if crop detected, use the estimated offset to crop the original,
    // align with the suspect, and run a full hash comparison.
    if overall_best.crop_detected {
        if let Some((dx, dy)) = overall_best.estimated_offset {
            info!(
                "Block-DCT crop detected: {} consistent votes (offset: dx={}, dy={}), {} total matches",
                overall_best.consistent_votes, dx, dy, overall_best.total_matches
            );

            match refine_with_offset(suspect, original, dx, dy) {
                Ok(report) => {
                    info!(
                        "Refined comparison: confidence={:?}, aHash={}, dHash={}, pHash={}",
                        report.confidence, report.ahash_hamming, report.dhash_hamming, report.phash_hamming
                    );
                    overall_best.refined_similarity = Some(report);
                }
                Err(e) => info!("Refinement failed: {}", e),
            }
        }
    }

    overall_best
}

/// Given an estimated translation offset (dx, dy) that maps the suspect image's
/// coordinate space into the original, try multiple scale ratios to crop the
/// original to the overlapping region, resize both to the same dimensions, and
/// return the best full hash comparison.
fn refine_with_offset(
    suspect: &DynamicImage,
    original: &DynamicImage,
    dx: i32,
    dy: i32,
) -> crate::errors::SealedResult<SimilarityReport> {
    let (orig_w, orig_h) = original.dimensions();
    let (sus_w, sus_h) = suspect.dimensions();

    let target_w = sus_w.min(512);
    let target_h = sus_h.min(512);
    let suspect_resized = DynamicImage::ImageRgba8(
        image::imageops::resize(&suspect.to_rgba8(), target_w, target_h, FilterType::Lanczos3),
    );
    let suspect_hashes = compute_hash_record(&suspect_resized)?;

    let mut best_report: Option<SimilarityReport> = None;
    let mut best_score = u32::MAX;

    // Try multiple scale ratios and offset jitter — the suspect could be at any
    // zoom level and the quantised offset may be off by a block or two.
    let jitter = BLOCK_PX as i32;
    for &scale in &[0.5f32, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2, 1.4, 1.6, 2.0] {
        for &jdx in &[-jitter, 0, jitter] {
            for &jdy in &[-jitter, 0, jitter] {
                let ox = (dx + jdx).max(0) as u32;
                let oy = (dy + jdy).max(0) as u32;
                if ox >= orig_w || oy >= orig_h { continue; }
                let ow = ((sus_w as f32 * scale).round() as u32).min(orig_w - ox);
                let oh = ((sus_h as f32 * scale).round() as u32).min(orig_h - oy);

                if ow < 16 || oh < 16 { continue; }

                let orig_crop = original.crop_imm(ox, oy, ow, oh);
                let resized_orig = DynamicImage::ImageRgba8(
                    image::imageops::resize(&orig_crop.to_rgba8(), target_w, target_h, FilterType::Lanczos3),
                );
                let orig_hashes = compute_hash_record(&resized_orig)?;
                let report = compare_hashes(&orig_hashes, &suspect_hashes);

                let score = report.ahash_hamming
                    .min(report.dhash_hamming)
                    .min(report.phash_hamming);

                if score < best_score {
                    best_score = score;
                    best_report = Some(report);
                }
            }
        }
    }

    best_report.ok_or_else(|| {
        crate::errors::SealedError::InvalidInput("Refined crop region too small at all scales".to_string())
    })
}
