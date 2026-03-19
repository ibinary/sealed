use std::path::Path;
use serde::{Serialize, Deserialize};
use tracing::info;

use crate::errors::{SealedError, SealedResult};
use crate::hashing::{compute_hash_record, compare_hashes, HashRecord, SimilarityReport, SimilarityConfidence};
use crate::image_processing::open_image_by_content;
use crate::signing::SignedEnvelope;
use crate::tile_hashing::{TileHashIndex, TileMatchResult, compare_against_tiles};

/// Verification result for a suspect image against a sealed record.
#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationResult {
    pub signature_valid: bool,
    pub vs_original: SimilarityReport,
    pub vs_cropped: SimilarityReport,
    pub tile_match: Option<TileMatchResult>,
    pub sealed_record: SealedRecord,
    pub suspect_hashes: HashRecord,
    pub verdict: String,
}

/// Sealed record as stored in hashes.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SealedRecord {
    pub original: HashRecord,
    pub frame: HashRecord,
    pub cropped: HashRecord,
    pub recombined: HashRecord,
    #[serde(default)]
    pub share: Option<HashRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tile_index: Option<TileHashIndex>,
    #[serde(default)]
    pub sealed_at: String,
    #[serde(default)]
    pub sealed_version: String,
}

/// Verify a suspect image against a sealed record directory.
pub fn verify_image(
    suspect_path: &Path,
    sealed_dir: &Path,
    public_key_path: Option<&Path>,
) -> SealedResult<VerificationResult> {
    if !suspect_path.exists() {
        return Err(SealedError::FileNotFound(suspect_path.display().to_string()));
    }
    if !sealed_dir.exists() {
        return Err(SealedError::FileNotFound(sealed_dir.display().to_string()));
    }

    let record_path = sealed_dir.join("hashes.json");
    if !record_path.exists() {
        return Err(SealedError::FileNotFound(
            "hashes.json not found in sealed directory".to_string(),
        ));
    }

    let record_json = std::fs::read_to_string(&record_path)?;
    let sealed_record: SealedRecord = serde_json::from_str(&record_json)?;

    let signed_path = sealed_dir.join("signed_record.json");
    let signature_valid = if signed_path.exists() {
        let signed_json = std::fs::read_to_string(&signed_path)?;
        let envelope: SignedEnvelope = serde_json::from_str(&signed_json)?;
        let result = match public_key_path {
            Some(pk_path) => {
                info!("Verifying signature against trusted public key: {}", pk_path.display());
                envelope.verify_with_key(pk_path)
            }
            None => envelope.verify(),
        };
        match result {
            Ok(()) => {
                // Verify that the signed payload actually matches the hashes.json content.
                // Without this check, someone could have a valid signature over a different
                // hash record than the one stored in the sealed directory.
                let payload_record: Result<SealedRecord, _> = serde_json::from_str(&envelope.payload);
                match payload_record {
                    Ok(signed_record) => {
                        if signed_record.original.sha256 != sealed_record.original.sha256
                            || signed_record.original.blake3 != sealed_record.original.blake3
                        {
                            info!("Signature valid but payload doesn't match hashes.json");
                            false
                        } else {
                            info!("Signature verified (payload matches hashes.json)");
                            true
                        }
                    }
                    Err(e) => {
                        info!("Digital signature valid but signed payload could not be parsed: {}", e);
                        false
                    }
                }
            }
            Err(e) => {
                info!("Digital signature verification FAILED: {}", e);
                false
            }
        }
    } else {
        info!("No digital signature found in sealed directory");
        false
    };

    let suspect_img = open_image_by_content(suspect_path)?;
    let suspect_hashes = compute_hash_record(&suspect_img)?;

    let vs_original = compare_hashes(&suspect_hashes, &sealed_record.original);
    let vs_cropped = match &sealed_record.share {
        Some(share_hashes) => compare_hashes(&suspect_hashes, share_hashes),
        None => compare_hashes(&suspect_hashes, &sealed_record.cropped),
    };

    let tile_index_loaded = sealed_record.tile_index.clone().or_else(|| {
        let tile_path = sealed_dir.join("tile_index.json");
        if tile_path.exists() {
            std::fs::read_to_string(&tile_path)
                .ok()
                .and_then(|json| serde_json::from_str(&json).ok())
        } else {
            None
        }
    });

    let tile_match = if let Some(ref index) = tile_index_loaded {
        let original_path = sealed_dir.join("original.png");
        if original_path.exists() {
            info!("Running tile-based crop detection ({} blocks)...", index.blocks.len());
            match open_image_by_content(&original_path) {
                Ok(original_img) => Some(compare_against_tiles(&suspect_img, &original_img, index)),
                Err(e) => {
                    info!("Could not load original.png for tile refinement: {}", e);
                    None
                }
            }
        } else {
            info!("original.png not found in sealed directory — skipping tile refinement");
            None
        }
    } else {
        None
    };

    let verdict = generate_verdict(&vs_original, &vs_cropped, tile_match.as_ref(), signature_valid);

    info!("Verification complete: {}", verdict);

    Ok(VerificationResult {
        signature_valid,
        vs_original,
        vs_cropped,
        tile_match,
        sealed_record,
        suspect_hashes,
        verdict,
    })
}

/// Verify a suspect image directly against a HashRecord.
pub fn verify_against_record(
    suspect_path: &Path,
    record: &SealedRecord,
) -> SealedResult<(SimilarityReport, SimilarityReport)> {
    let suspect_img = open_image_by_content(suspect_path)?;
    let suspect_hashes = compute_hash_record(&suspect_img)?;

    let vs_original = compare_hashes(&suspect_hashes, &record.original);
    let vs_cropped = match &record.share {
        Some(share_hashes) => compare_hashes(&suspect_hashes, share_hashes),
        None => compare_hashes(&suspect_hashes, &record.cropped),
    };

    Ok((vs_original, vs_cropped))
}

fn generate_verdict(
    vs_original: &SimilarityReport,
    vs_cropped: &SimilarityReport,
    tile_match: Option<&TileMatchResult>,
    signature_valid: bool,
) -> String {
    if vs_original.exact_match {
        return format!(
            "EXACT MATCH: Suspect image is byte-identical to the sealed original. {}",
            sig_note(signature_valid)
        );
    }

    if vs_cropped.exact_match {
        return format!(
            "EXACT MATCH (CROPPED): Suspect image matches the sealed cropped/share version. {}",
            sig_note(signature_valid)
        );
    }

    let best_confidence = best_of(vs_original.confidence, vs_cropped.confidence);
    let best_ahash = vs_original.ahash_hamming.min(vs_cropped.ahash_hamming);
    let best_dhash = vs_original.dhash_hamming.min(vs_cropped.dhash_hamming);
    let best_phash = vs_original.phash_hamming.min(vs_cropped.phash_hamming);

    if let Some(tm) = tile_match {
        if tm.crop_detected {
            let offset_info = match tm.estimated_offset {
                Some((dx, dy)) => format!("estimated offset: ({}, {})", dx, dy),
                None => String::new(),
            };
            let refined_info = match &tm.refined_similarity {
                Some(r) => format!(
                    "Refined comparison: {:?} confidence (aHash={}, dHash={}, pHash={})",
                    r.confidence, r.ahash_hamming, r.dhash_hamming, r.phash_hamming
                ),
                None => format!(
                    "{} consistent block votes, {} total matches",
                    tm.consistent_votes, tm.total_matches
                ),
            };
            return format!(
                "SUB-REGION CROP DETECTED: Suspect image matches a region of the sealed original \
                 ({} blocks checked, {} consistent votes). {} — {} {}",
                tm.tiles_checked, tm.consistent_votes, offset_info, refined_info, sig_note(signature_valid)
            );
        }
    }

    match best_confidence {
        SimilarityConfidence::Exact => unreachable!(),
        SimilarityConfidence::High => format!(
            "PERCEPTUALLY SIMILAR (HIGH confidence): Suspect image is very likely derived from \
             the sealed content (best hamming: aHash={}, dHash={}, pHash={}). {}",
            best_ahash, best_dhash, best_phash, sig_note(signature_valid)
        ),
        SimilarityConfidence::Medium => format!(
            "PERCEPTUALLY SIMILAR (MEDIUM confidence): Suspect image appears visually similar to \
             sealed content (best hamming: aHash={}, dHash={}, pHash={}). {}",
            best_ahash, best_dhash, best_phash, sig_note(signature_valid)
        ),
        SimilarityConfidence::Low => format!(
            "PERCEPTUALLY SIMILAR (LOW confidence): Suspect image has loose visual similarity to \
             sealed content (best hamming: aHash={}, dHash={}, pHash={}). May be coincidental. {}",
            best_ahash, best_dhash, best_phash, sig_note(signature_valid)
        ),
        SimilarityConfidence::None => format!(
            "NO MATCH: Suspect image does not appear to match the sealed content. \
             aHash distance: {}/{}, dHash distance: {}/{}, pHash distance: {}/{}. {}",
            vs_original.ahash_hamming, vs_cropped.ahash_hamming,
            vs_original.dhash_hamming, vs_cropped.dhash_hamming,
            vs_original.phash_hamming, vs_cropped.phash_hamming,
            sig_note(signature_valid)
        ),
    }
}

/// Higher of two confidence levels.
fn best_of(a: SimilarityConfidence, b: SimilarityConfidence) -> SimilarityConfidence {
    let rank = |c: SimilarityConfidence| match c {
        SimilarityConfidence::Exact => 4,
        SimilarityConfidence::High => 3,
        SimilarityConfidence::Medium => 2,
        SimilarityConfidence::Low => 1,
        SimilarityConfidence::None => 0,
    };
    if rank(a) >= rank(b) { a } else { b }
}

fn sig_note(valid: bool) -> &'static str {
    if valid {
        "Sealed record signature is VALID."
    } else {
        "No valid signature found on sealed record."
    }
}
