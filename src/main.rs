#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use] extern crate rocket;
extern crate image;

use rocket::Data;
use rocket::response::content;
use std::path::{Path, PathBuf};
use rocket::response::NamedFile;
use rocket::http::Method;
use rocket_cors::{AllowedOrigins, CorsOptions};
use image::{GenericImageView, ImageFormat, Rgba};
use serde_json::json;
use image::{imageops::FilterType, GrayImage, ImageBuffer, DynamicImage};
use std::fs::File;
use std::io::Write;
use zip::write::FileOptions;
use zip::CompressionMethod::Stored;
use std::hash::{Hash, Hasher};
use fnv::FnvHasher;

const PART_SIZE: u32 = 20;
const HASH_SIZE: u32 = 32;

fn image_hash(img: &DynamicImage) -> u64 {
    let mut hasher = FnvHasher::default();
    let (width, height) = img.dimensions();
    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            let pixel_str = format!("{},{},{},{},{}", x, y, pixel[0], pixel[1], pixel[2]);
            pixel_str.hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn ahash(img: &GrayImage, hash_size: u32) -> u64 {
    let img = image::imageops::resize(img, hash_size, hash_size, FilterType::Nearest);
    let mut ahash: u64 = 0;
    let avg: u64 = img.pixels().map(|p| p[0] as u64).sum::<u64>() / ((hash_size * hash_size) as u64);
    for (_i, pixel) in img.pixels().enumerate() {
        ahash <<= 1;
        if pixel[0] as u64 >= avg {
            ahash |= 1;
        }
    }
    ahash
}

fn dhash(img: &GrayImage, hash_size: u32) -> u64 {
    let img = image::imageops::resize(img, hash_size + 1, hash_size, FilterType::Nearest);
    let mut dhash: u64 = 0;
    for row in 0..hash_size {
        for col in 0..hash_size {
            let pixel = img.get_pixel(col, row).0[0] as u64;
            let pixel_right = img.get_pixel(col + 1, row).0[0] as u64;
            dhash <<= 1;
            if pixel > pixel_right {
                dhash |= 1;
            }
        }
    }
    dhash
}

#[get("/")]
fn index() -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join("index.html")).ok()
}

#[get("/<file..>", rank = 2)]
fn files(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join(file)).ok()
}

#[get("/uploads/<file..>")]
fn uploads(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("uploads/").join(file)).ok()
}

#[post("/", data = "<data>")]
fn upload(data: Data) -> content::Json<String> {
    // Read the image data
    let mut img_data = vec![];
    if let Err(e) = data.stream_to(&mut img_data) {
        return content::Json(json!({ "error": format!("Error reading image data: {}", e) }).to_string());
    }
    // Open the image file
    let img = image::load_from_memory(&img_data).unwrap();
    let (width, height) = img.dimensions();
    let img_gray = img.to_luma8();
    // Save a copy of the original image
    img.save("uploads/original.png").unwrap();
    let img = image::open("uploads/original.png").unwrap();
    // Split the image into top, bottom, left, right and middle parts
    let top_part = img.view(0, 0, width, PART_SIZE).to_image();
    let bottom_part = img.view(0, height - PART_SIZE, width, PART_SIZE).to_image();
    let left_part = img.view(0, PART_SIZE, PART_SIZE, height - 2 * PART_SIZE).to_image();
    let right_part = img.view(width - PART_SIZE, PART_SIZE, PART_SIZE, height - 2 * PART_SIZE).to_image();
    let middle_part = img.view(PART_SIZE, PART_SIZE, width - 2 * PART_SIZE, height - 2 * PART_SIZE).to_image();

    // Create a new image to hold the four parts
    let mut parts_img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);

    // Paste the top, bottom, left and right parts into the new image
    image::imageops::replace(&mut parts_img, &top_part, 0, 0);
    image::imageops::replace(&mut parts_img, &bottom_part, 0, height - 20);
    image::imageops::replace(&mut parts_img, &left_part, 0, 20);
    image::imageops::replace(&mut parts_img, &right_part, width - 20, 20);

    // Save the parts image to a file
    parts_img.save_with_format("uploads/parts.png", ImageFormat::Png).unwrap();

    // Create a new image to hold the recombined parts
    let mut recombined_img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);

    // Paste (recombine) the top, bottom, left, right and middle parts into the new (recombined) image
    image::imageops::replace(&mut recombined_img, &top_part, 0, 0);
    image::imageops::replace(&mut recombined_img, &bottom_part, 0, height - 20);
    image::imageops::replace(&mut recombined_img, &left_part, 0, 20);
    image::imageops::replace(&mut recombined_img, &right_part, width - 20, 20);
    image::imageops::replace(&mut recombined_img, &middle_part, 20, 20);

    // Save the recombined image to a file
    recombined_img.save_with_format("uploads/recombined.png", ImageFormat::Png).unwrap();

    // Create a new image to hold the middle part
    let mut middle_img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);

    // Paste the middle part into the new image
    image::imageops::replace(&mut middle_img, &middle_part, 20, 20);

    // Save the middle image to a file
    middle_img.save_with_format("uploads/cropped.png", ImageFormat::Png).unwrap();

    // Convert the images to grayscale before calculating the dHash and aHash
    let original_dhash = dhash(&img_gray, HASH_SIZE);
    let original_ahash = ahash(&img_gray, HASH_SIZE);

    let parts_img_clone = parts_img.clone();
    let parts_img_gray = DynamicImage::ImageRgba8(parts_img).to_luma8();
    let parts_dhash = dhash(&parts_img_gray, HASH_SIZE);
    let parts_ahash = ahash(&parts_img_gray, HASH_SIZE);

    let middle_img_clone = middle_img.clone();
    let middle_img_gray = DynamicImage::ImageRgba8(middle_img).to_luma8();
    let middle_dhash = dhash(&middle_img_gray, HASH_SIZE);
    let middle_ahash = ahash(&middle_img_gray, HASH_SIZE);

    let recombined_img_clone = recombined_img.clone();
    let recombined_img_gray = DynamicImage::ImageRgba8(recombined_img).to_luma8();
    let recombined_dhash = dhash(&recombined_img_gray, HASH_SIZE);
    let recombined_ahash = ahash(&recombined_img_gray, HASH_SIZE);

    let original_hash = image_hash(&img);
    let parts_hash = image_hash(&DynamicImage::ImageRgba8(parts_img_clone));
    let cropped_hash = image_hash(&DynamicImage::ImageRgba8(middle_img_clone));
    let recombined_hash = image_hash(&DynamicImage::ImageRgba8(recombined_img_clone));
    let response = json!({
        "originalImagePath": "uploads/original.png",
        "originalDhash": format!("{:016x}", original_dhash),
        "originalAhash": format!("{:016x}", original_ahash),
        "partsImagePath": "uploads/parts.png",
        "partsDhash": format!("{:016x}", parts_dhash),
        "partsAhash": format!("{:016x}", parts_ahash),
        "croppedImagePath": "uploads/cropped.png",
        "originalImageHash": format!("{:016x}", original_hash),
        "partsImageHash": format!("{:016x}", parts_hash),
        "croppedImageHash": format!("{:016x}", cropped_hash),
        "recombinedImageHash": format!("{:016x}", recombined_hash),
        "croppedImageDhash": format!("{:016x}", middle_dhash),
        "croppedImageAhash": format!("{:016x}", middle_ahash),
        "recombinedImagePath": "uploads/recombined.png",
        "recombinedDhash": format!("{:016x}", recombined_dhash),
        "recombinedAhash": format!("{:016x}", recombined_ahash)
    });

    // Generate QR code
    // let code = QrCode::new(format!("File name: {}\nContact: {}\nURL: {}\nIPFS: {}", file_name, contact, url, ipfs)).unwrap();
    // let image = code.render::<Luma<u8>>().build();
    // image.save("uploads/qr.png").unwrap();



    // Write ahash, phash and pixelhash to a .txt file
    let mut file = File::create("uploads/hashes.txt").unwrap();
    writeln!(file, "Original image dHash: {:016x}, aHash: {:016x}, pHash: {:016x}", original_dhash, original_ahash, original_hash).ok();
    writeln!(file, "Cropped image dHash: {:016x}, aHash: {:016x}, pHash: {:016x}", middle_dhash, middle_ahash, cropped_hash).ok();
    writeln!(file, "Parts image dHash: {:016x}, aHash: {:016x}, pHash: {:016x}", parts_dhash, parts_ahash, parts_hash).ok();
    writeln!(file, "Recombined image dHash: {:016x}, aHash: {:016x}, pHash: {:016x}", recombined_dhash, recombined_ahash, recombined_hash).ok();

    // Create a .zip file
    let path = Path::new("uploads/archive.zip");
    let file = File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(Stored)
        .unix_permissions(0o755);

    // Add the cropped image to the .zip file
    zip.start_file("cropped.png", options).unwrap();
    let img_data = std::fs::read("uploads/cropped.png").unwrap();
    zip.write_all(&img_data).unwrap();

    // Add the parts image to the .zip file
    zip.start_file("parts.png", options).unwrap();
    let img_data = std::fs::read("uploads/parts.png").unwrap();
    zip.write_all(&img_data).unwrap();

    // Add the checksums .txt file to the .zip file
    zip.start_file("hashes.txt", options).unwrap();
    let txt_data = std::fs::read("uploads/hashes.txt").unwrap();
    zip.write_all(&txt_data).unwrap();

    // Finish the .zip file
    zip.finish().unwrap();

    // Return the JSON response
    content::Json(response.to_string())
 }

#[cfg(test)]
mod tests {
    // test image upload and hash generation
    
    #[test]
    fn test_image_upload() {
        // TODO add tests
    }

    #[test]
    fn test_image_hash() {
        // TODO add tests
    }

    #[test]
    fn test_image_dhash() {
        // TODO add tests
    }

    #[test]
    fn test_image_ahash() {
        // TODO add tests
    }
}

fn main() {
    let allowed_origins = AllowedOrigins::some_exact(&[
        "http://localhost:8000",
    ]);

    let cors = CorsOptions {
        allowed_origins,
        allowed_methods: vec![Method::Get, Method::Post].into_iter().map(From::from).collect(),
        allow_credentials: true,
        ..Default::default()
    }
    .to_cors()
    .expect("error while building CORS");

    rocket::ignite()
    .mount("/", routes![index, files, upload, uploads])
    .attach(cors)
    .launch();
}
