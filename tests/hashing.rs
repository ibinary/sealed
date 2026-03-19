use image::{DynamicImage, RgbaImage, Rgba};
use sealed::hashing::{compute_hash_record, compare_hashes, hamming_distance, SimilarityConfidence};

fn make_test_image(w: u32, h: u32, color: [u8; 4]) -> DynamicImage {
    let mut img = RgbaImage::new(w, h);
    for pixel in img.pixels_mut() {
        *pixel = Rgba(color);
    }
    DynamicImage::ImageRgba8(img)
}

#[test]
fn identical_images_produce_identical_hashes() {
    let img1 = make_test_image(100, 100, [255, 0, 0, 255]);
    let img2 = make_test_image(100, 100, [255, 0, 0, 255]);

    let h1 = compute_hash_record(&img1).unwrap();
    let h2 = compute_hash_record(&img2).unwrap();

    assert_eq!(h1.sha256, h2.sha256);
    assert_eq!(h1.blake3, h2.blake3);
}

#[test]
fn different_images_produce_different_hashes() {
    let img1 = make_test_image(100, 100, [255, 0, 0, 255]);
    let img2 = make_test_image(100, 100, [0, 0, 255, 255]);

    let h1 = compute_hash_record(&img1).unwrap();
    let h2 = compute_hash_record(&img2).unwrap();

    assert_ne!(h1.sha256, h2.sha256);
    assert_ne!(h1.blake3, h2.blake3);
}

#[test]
fn hamming_distance_works() {
    assert_eq!(hamming_distance(0b1111, 0b1111), 0);
    assert_eq!(hamming_distance(0b1111, 0b0000), 4);
    assert_eq!(hamming_distance(0b1010, 0b0101), 4);
}

#[test]
fn compare_identical_images() {
    let img = make_test_image(100, 100, [128, 64, 32, 255]);
    let h1 = compute_hash_record(&img).unwrap();
    let h2 = compute_hash_record(&img).unwrap();

    let report = compare_hashes(&h1, &h2);
    assert!(report.exact_match);
    assert!(report.sha256_match);
    assert!(report.blake3_match);
    assert_eq!(report.ahash_hamming, 0);
    assert_eq!(report.dhash_hamming, 0);
    assert_eq!(report.confidence, SimilarityConfidence::Exact);
}

#[test]
fn confidence_exact_for_identical() {
    let img = make_test_image(100, 100, [50, 100, 150, 255]);
    let h = compute_hash_record(&img).unwrap();
    let report = compare_hashes(&h, &h);
    assert_eq!(report.confidence, SimilarityConfidence::Exact);
    assert!(report.perceptually_similar);
}

#[test]
fn confidence_none_for_totally_different() {
    let mut img1 = RgbaImage::new(100, 100);
    for (x, y, pixel) in img1.enumerate_pixels_mut() {
        *pixel = Rgba([((x * 7) % 256) as u8, ((y * 3) % 256) as u8, 0, 255]);
    }
    let mut img2 = RgbaImage::new(100, 100);
    for (x, y, pixel) in img2.enumerate_pixels_mut() {
        *pixel = Rgba([0, ((x * 11 + 128) % 256) as u8, ((y * 13 + 64) % 256) as u8, 255]);
    }
    let h1 = compute_hash_record(&DynamicImage::ImageRgba8(img1)).unwrap();
    let h2 = compute_hash_record(&DynamicImage::ImageRgba8(img2)).unwrap();
    let report = compare_hashes(&h1, &h2);
    assert!(!report.exact_match);
    assert_eq!(report.confidence, SimilarityConfidence::None);
    assert!(!report.perceptually_similar);
}
