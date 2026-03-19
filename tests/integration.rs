use image::{DynamicImage, RgbaImage, Rgba};

use sealed::image_processing::{seal_image, save_artifacts, SealConfig};
use sealed::hashing::{compute_hash_record, compare_hashes, SimilarityConfidence};
use sealed::signing::SealedKeyPair;
use sealed::verification::{verify_image, SealedRecord};

/// Helper: create a test image with a gradient pattern (more realistic than solid color).
fn make_gradient_image(w: u32, h: u32) -> DynamicImage {
    let mut img = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let r = ((x as f32 / w as f32) * 255.0) as u8;
            let g = ((y as f32 / h as f32) * 255.0) as u8;
            let b = (((x + y) as f32 / (w + h) as f32) * 255.0) as u8;
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
    DynamicImage::ImageRgba8(img)
}

/// Full seal → save → verify pipeline with an exact match.
#[test]
fn end_to_end_seal_and_verify_exact_match() {
    let img = make_gradient_image(200, 150);
    let config = SealConfig::default();

    // Seal
    let artifacts = seal_image(&img, &config).expect("seal_image failed");

    // Save to temp dir
    let tmp = std::env::temp_dir().join("sealed_test_e2e_exact");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    save_artifacts(&artifacts, &tmp).expect("save_artifacts failed");

    // Write hashes.json (needed for verify_image)
    let sealed_record = SealedRecord {
        original: artifacts.original_hashes.clone(),
        frame: artifacts.frame_hashes.clone(),
        cropped: artifacts.cropped_hashes.clone(),
        recombined: artifacts.recombined_hashes.clone(),
        share: Some(artifacts.share_hashes.clone()),
        tile_index: None,
        sealed_at: chrono::Utc::now().to_rfc3339(),
        sealed_version: "2.0.0".to_string(),
    };
    let json = serde_json::to_string_pretty(&sealed_record).unwrap();
    std::fs::write(tmp.join("hashes.json"), &json).unwrap();

    // Save the original image as a "suspect" file
    let suspect_path = tmp.join("suspect.png");
    img.save(&suspect_path).unwrap();

    // Verify — should be exact match
    let result = verify_image(&suspect_path, &tmp, None).expect("verify_image failed");

    assert!(result.vs_original.exact_match, "Expected exact match for original");
    assert!(result.vs_original.sha256_match, "SHA-256 should match");
    assert!(result.vs_original.blake3_match, "BLAKE3 should match");
    assert_eq!(result.vs_original.ahash_hamming, 0, "aHash distance should be 0");
    assert_eq!(result.vs_original.dhash_hamming, 0, "dHash distance should be 0");
    assert_eq!(result.vs_original.phash_hamming, 0, "pHash distance should be 0");
    assert_eq!(result.vs_original.confidence, SimilarityConfidence::Exact, "Confidence should be Exact");
    assert!(result.verdict.contains("EXACT MATCH"), "Verdict should contain EXACT MATCH");

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Seal → sign → verify with trusted public key.
#[test]
fn end_to_end_sign_and_verify_with_public_key() {
    let img = make_gradient_image(200, 150);
    let config = SealConfig::default();

    let artifacts = seal_image(&img, &config).expect("seal_image failed");

    let tmp = std::env::temp_dir().join("sealed_test_e2e_signed");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    save_artifacts(&artifacts, &tmp).expect("save_artifacts failed");

    // Write hashes.json
    let sealed_record = SealedRecord {
        original: artifacts.original_hashes.clone(),
        frame: artifacts.frame_hashes.clone(),
        cropped: artifacts.cropped_hashes.clone(),
        recombined: artifacts.recombined_hashes.clone(),
        share: Some(artifacts.share_hashes.clone()),
        tile_index: None,
        sealed_at: chrono::Utc::now().to_rfc3339(),
        sealed_version: "2.0.0".to_string(),
    };
    let json = serde_json::to_string_pretty(&sealed_record).unwrap();
    std::fs::write(tmp.join("hashes.json"), &json).unwrap();

    // Generate keypair and sign
    let keypair = SealedKeyPair::generate();
    let envelope = keypair.sign(&json);
    let signed_json = serde_json::to_string_pretty(&envelope).unwrap();
    std::fs::write(tmp.join("signed_record.json"), &signed_json).unwrap();

    // Save keys
    let pub_path = tmp.join("test.pub");
    keypair.save_public(&pub_path).expect("save_public failed");

    // Save suspect
    let suspect_path = tmp.join("suspect.png");
    img.save(&suspect_path).unwrap();

    // Verify with trusted public key
    let result = verify_image(&suspect_path, &tmp, Some(&pub_path))
        .expect("verify_image failed");

    assert!(result.signature_valid, "Signature should be valid");
    assert!(result.vs_original.exact_match, "Should be exact match");
    assert!(result.verdict.contains("VALID"), "Verdict should say signature is VALID");

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Verify that a different image does NOT match.
#[test]
fn end_to_end_different_image_no_match() {
    let original = make_gradient_image(200, 150);
    let config = SealConfig::default();

    let artifacts = seal_image(&original, &config).expect("seal_image failed");

    let tmp = std::env::temp_dir().join("sealed_test_e2e_nomatch");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    save_artifacts(&artifacts, &tmp).expect("save_artifacts failed");

    let sealed_record = SealedRecord {
        original: artifacts.original_hashes.clone(),
        frame: artifacts.frame_hashes.clone(),
        cropped: artifacts.cropped_hashes.clone(),
        recombined: artifacts.recombined_hashes.clone(),
        share: Some(artifacts.share_hashes.clone()),
        tile_index: None,
        sealed_at: chrono::Utc::now().to_rfc3339(),
        sealed_version: "2.0.0".to_string(),
    };
    let json = serde_json::to_string_pretty(&sealed_record).unwrap();
    std::fs::write(tmp.join("hashes.json"), &json).unwrap();

    let different = DynamicImage::ImageRgba8(RgbaImage::from_fn(200, 150, |x, y| {
        Rgba([((x * 11 + 128) % 256) as u8, ((y * 13 + 64) % 256) as u8, ((x ^ y) % 256) as u8, 255])
    }));
    let suspect_path = tmp.join("suspect_different.png");
    different.save(&suspect_path).unwrap();

    let result = verify_image(&suspect_path, &tmp, None).expect("verify_image failed");

    assert!(!result.vs_original.exact_match, "Should NOT be exact match");
    assert!(!result.vs_original.sha256_match, "SHA-256 should NOT match");
    assert!(!result.vs_original.blake3_match, "BLAKE3 should NOT match");
    assert!(!result.vs_original.perceptually_similar,
        "Completely different patterned image should not be perceptually similar");
    assert!(result.verdict.contains("NO MATCH"), "Verdict should say NO MATCH");

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Verify pHash produces consistent results for the same image.
#[test]
fn phash_deterministic() {
    let img = make_gradient_image(300, 200);

    let h1 = compute_hash_record(&img).expect("hash 1 failed");
    let h2 = compute_hash_record(&img).expect("hash 2 failed");

    assert_eq!(h1.phash, h2.phash, "pHash should be deterministic");
    assert_eq!(h1.ahash, h2.ahash, "aHash should be deterministic");
    assert_eq!(h1.dhash, h2.dhash, "dHash should be deterministic");
    assert_eq!(h1.sha256, h2.sha256, "SHA-256 should be deterministic");

    let report = compare_hashes(&h1, &h2);
    assert_eq!(report.phash_hamming, 0);
    assert_eq!(report.ahash_hamming, 0);
    assert_eq!(report.dhash_hamming, 0);
}

/// Verify encrypted key round-trip works.
#[test]
fn encrypted_key_roundtrip() {
    let keypair = SealedKeyPair::generate();
    let pub_key_b64 = keypair.public_key_base64();

    let tmp = std::env::temp_dir().join("sealed_test_e2e_enckey");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let key_path = tmp.join("sealed.key");
    let password = "test-password-123";

    // Save encrypted
    keypair.save_secret_encrypted(&key_path, password)
        .expect("save_secret_encrypted failed");

    // File should be > 32 bytes (salt + nonce + ciphertext)
    let file_size = std::fs::metadata(&key_path).unwrap().len();
    assert!(file_size > 32, "Encrypted key file should be larger than 32 bytes");

    // Load encrypted
    let loaded = SealedKeyPair::load_encrypted(&key_path, password)
        .expect("load_encrypted failed");

    // Public keys should match
    assert_eq!(loaded.public_key_base64(), pub_key_b64,
        "Loaded key should produce the same public key");

    // Wrong password should fail
    let bad_result = SealedKeyPair::load_encrypted(&key_path, "wrong-password");
    assert!(bad_result.is_err(), "Wrong password should fail decryption");

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Verify recombined image exactly matches original.
#[test]
fn recombined_matches_original() {
    let img = make_gradient_image(200, 150);
    let config = SealConfig::default();

    let artifacts = seal_image(&img, &config).expect("seal_image failed");

    assert_eq!(
        artifacts.original_hashes.sha256,
        artifacts.recombined_hashes.sha256,
        "Recombined SHA-256 should match original"
    );
    assert_eq!(
        artifacts.original_hashes.blake3,
        artifacts.recombined_hashes.blake3,
        "Recombined BLAKE3 should match original"
    );
}
