use image::{DynamicImage, RgbaImage, Rgba};
use sealed::tile_hashing::{generate_tile_index, compare_against_tiles};

fn make_patterned_image(w: u32, h: u32) -> DynamicImage {
    let mut img = RgbaImage::new(w, h);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        *pixel = Rgba([
            ((x * 7 + y * 3) % 256) as u8,
            ((x * 11 + y * 5) % 256) as u8,
            ((x * 13 + y * 7) % 256) as u8,
            255,
        ]);
    }
    DynamicImage::ImageRgba8(img)
}

#[test]
fn tile_index_generated() {
    let img = make_patterned_image(400, 300);
    let index = generate_tile_index(&img);
    assert!(!index.blocks.is_empty(), "Should generate blocks");
    assert_eq!(index.source_width, 400);
    assert_eq!(index.source_height, 300);
}

#[test]
fn exact_crop_detected() {
    let img = make_patterned_image(400, 300);
    let index = generate_tile_index(&img);

    let crop = img.crop_imm(100, 75, 200, 150);
    let result = compare_against_tiles(&crop, &img, &index);

    assert!(
        result.crop_detected,
        "Should detect a crop from the center (votes: {}, matches: {})",
        result.consistent_votes, result.total_matches
    );
}

#[test]
fn unrelated_image_not_matched() {
    let img = make_patterned_image(400, 300);
    let index = generate_tile_index(&img);

    let other = DynamicImage::ImageRgba8(RgbaImage::from_fn(200, 150, |x, y| {
        Rgba([
            ((x * 31 + 128) % 256) as u8,
            ((y * 37 + 64) % 256) as u8,
            ((x ^ y) % 256) as u8,
            255,
        ])
    }));
    let result = compare_against_tiles(&other, &img, &index);

    assert!(
        !result.crop_detected,
        "Unrelated image should not match (votes: {})",
        result.consistent_votes
    );
}

#[test]
fn dct_basic_sanity() {
    // All-constant block should have energy only in DC
    let img = DynamicImage::ImageRgba8(RgbaImage::from_fn(400, 300, |_, _| {
        Rgba([128, 128, 128, 255])
    }));
    let index = generate_tile_index(&img);
    // a uniform image should produce blocks with identical descriptors
    let first_desc = index.blocks[0].descriptor;
    for block in &index.blocks {
        assert_eq!(block.descriptor, first_desc, "Uniform image should produce identical block descriptors");
    }
}
