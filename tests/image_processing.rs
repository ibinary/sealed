use image::{DynamicImage, RgbaImage, Rgba};
use sealed::image_processing::{seal_image, SealConfig};

fn make_test_image(w: u32, h: u32) -> DynamicImage {
    let mut img = RgbaImage::new(w, h);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        *pixel = Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]);
    }
    DynamicImage::ImageRgba8(img)
}

#[test]
fn seal_produces_all_artifacts() {
    let img = make_test_image(200, 200);
    let config = SealConfig::default();
    let result = seal_image(&img, &config).unwrap();

    assert_eq!(result.frame.width(), 200);
    assert_eq!(result.frame.height(), 200);
    assert_eq!(result.cropped.width(), 200);
    assert_eq!(result.recombined.width(), 200);
}

#[test]
fn too_small_image_returns_error() {
    let img = make_test_image(30, 30);
    let config = SealConfig::default();
    let result = seal_image(&img, &config);
    assert!(result.is_err());
}
