#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;
extern crate image;

use serde_json::to_writer_pretty;
use rand::Rng;
use std::time::{SystemTime, UNIX_EPOCH};
use image::{GenericImageView, ImageFormat, Rgba};
use serde_json::json;
use image::{imageops::FilterType, GrayImage, ImageBuffer, DynamicImage};
use std::fs::File;
use std::io::Write;
use std::hash::{Hash, Hasher};
use fnv::FnvHasher;
use std::fs;
use std::path::{Path, PathBuf};
use std::env;
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

fn generate_uuid() -> String {
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let in_ms = since_the_epoch.as_secs() * 1000 +
        since_the_epoch.subsec_nanos() as u64 / 1_000_000;

    let mut rng = rand::thread_rng();
    let random: u64 = rng.gen();

    format!("{:x}-{:x}", in_ms, random)
}

fn process_image(image_file: &str) {
        let mut rng = rand::thread_rng();
    let PART_SIZE: u32 = rng.gen_range(3..11);
    let img = image::open(image_file).unwrap();
    let (width, height) = img.dimensions();
    let img_gray = img.to_luma8();

    let file_name = Path::new(image_file).file_name().unwrap().to_str().unwrap();
    let upload_dir = format!("sealed/{}-{}", file_name, generate_uuid());
    std::fs::create_dir_all(&upload_dir).unwrap();
    let original_path = format!("{}/original.png", upload_dir);
    img.save(&original_path).unwrap();
    let img = image::open(&original_path).unwrap();

    let top_part = img.view(0, 0, width, PART_SIZE).to_image();
    let bottom_part = img.view(0, height - PART_SIZE, width, PART_SIZE).to_image();
    let left_part = img.view(0, PART_SIZE, PART_SIZE, height - 2 * PART_SIZE).to_image();
    let right_part = img.view(width - PART_SIZE, PART_SIZE, PART_SIZE, height - 2 * PART_SIZE).to_image();
    let middle_part = img.view(PART_SIZE, PART_SIZE, width - 2 * PART_SIZE, height - 2 * PART_SIZE).to_image();

    let mut parts_img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);
    image::imageops::replace(&mut parts_img, &top_part, 0, 0);
    image::imageops::replace(&mut parts_img, &bottom_part, 0, height - PART_SIZE);
    image::imageops::replace(&mut parts_img, &left_part, 0, PART_SIZE);
    image::imageops::replace(&mut parts_img, &right_part, width - PART_SIZE, PART_SIZE);

    let parts_path = format!("{}/frame.png", upload_dir);
    parts_img.save_with_format(&parts_path, ImageFormat::Png).unwrap();

    let mut recombined_img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);
    image::imageops::replace(&mut recombined_img, &top_part, 0, 0);
    image::imageops::replace(&mut recombined_img, &bottom_part, 0, height - PART_SIZE);
    image::imageops::replace(&mut recombined_img, &left_part, 0, PART_SIZE);
    image::imageops::replace(&mut recombined_img, &right_part, width - PART_SIZE, PART_SIZE);
    image::imageops::replace(&mut recombined_img, &middle_part, PART_SIZE, PART_SIZE);

    let recombined_path = format!("{}/recombined.png", upload_dir);
    recombined_img.save_with_format(&recombined_path, ImageFormat::Png).unwrap();

    let mut middle_img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);
    let mut cropped_no_whitespace_img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);
    image::imageops::replace(&mut cropped_no_whitespace_img, &middle_part, 20, 20);

    for pixel in cropped_no_whitespace_img.pixels_mut() {
        if pixel[0] == 255 && pixel[1] == 255 && pixel[2] == 255 {
            *pixel = Rgba([0, 0, 0, 0]);
        }
    }

    let cropped_no_whitespace_path = format!("{}/share.png", upload_dir);
    cropped_no_whitespace_img.save_with_format(&cropped_no_whitespace_path, ImageFormat::Png).unwrap();

    image::imageops::replace(&mut middle_img, &middle_part, 20, 20);
    let cropped_path = format!("{}/cropped.png", upload_dir);
    middle_img.save_with_format(&cropped_path, ImageFormat::Png).unwrap();

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
        "originalImagePath": format!("{}/original.png", upload_dir),
        "originalDhash": format!("{:016x}", original_dhash),
        "originalAhash": format!("{:016x}", original_ahash),
        "partsImagePath": format!("{}/frame.png", upload_dir),
        "partsDhash": format!("{:016x}", parts_dhash),
        "partsAhash": format!("{:016x}", parts_ahash),
        "croppedImagePath": format!("{}/cropped.png", upload_dir),
        "originalImageHash": format!("{:016x}", original_hash),
        "partsImageHash": format!("{:016x}", parts_hash),
        "croppedImageHash": format!("{:016x}", cropped_hash),
        "recombinedImageHash": format!("{:016x}", recombined_hash),
        "croppedImageDhash": format!("{:016x}", middle_dhash),
        "croppedImageAhash": format!("{:016x}", middle_ahash),
        "recombinedImagePath": format!("{}/recombined.png", upload_dir),
        "recombinedDhash": format!("{:016x}", recombined_dhash),
        "recombinedAhash": format!("{:016x}", recombined_ahash),
        "archiveURL": format!("{}", upload_dir),
    });

    // Save the response JSON to a file
    let mut file = File::create(format!("{}/hashes.json", upload_dir)).unwrap();
    let response_json = serde_json::to_string_pretty(&response).unwrap();
    let mut response_file = File::create(format!("{}/hashes.json", upload_dir)).unwrap();
    response_file.write_all(response_json.as_bytes()).unwrap();

    let mut file = File::create(format!("{}/hashes.txt", upload_dir)).unwrap();
    writeln!(file, "Original image dHash: {:016x}, aHash: {:016x}, pHash: {:016x}", original_dhash, original_ahash, original_hash).ok();
    writeln!(file, "Cropped image dHash: {:016x}, aHash: {:016x}, pHash: {:016x}", middle_dhash, middle_ahash, cropped_hash).ok();
    writeln!(file, "Frame image dHash: {:016x}, aHash: {:016x}, pHash: {:016x}", parts_dhash, parts_ahash, parts_hash).ok();
    writeln!(file, "Recombined image dHash: {:016x}, aHash: {:016x}, pHash: {:016x}", recombined_dhash, recombined_ahash, recombined_hash).ok();

    let archive_name = format!("{}.zip", file_name);
    let path_str = format!("{}/{}", upload_dir, archive_name);
    let path = Path::new(&path_str);
    let file = File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o755);


    zip.start_file("cropped.png", options).unwrap();
    let img_data = std::fs::read(&cropped_path).unwrap();
    zip.write_all(&img_data).unwrap();

    zip.start_file("original.png", options).unwrap();
    let img_data = std::fs::read(&cropped_path).unwrap();
    zip.write_all(&img_data).unwrap();

    zip.start_file("frame.png", options).unwrap();
    let img_data = std::fs::read(&parts_path).unwrap();
    zip.write_all(&img_data).unwrap();

    zip.start_file("hashes.txt", options).unwrap();
    let txt_data = std::fs::read(format!("{}/hashes.txt", upload_dir)).unwrap();
    zip.write_all(&txt_data).unwrap();

    zip.start_file("hashes.json", options).unwrap();
    let json_data = std::fs::read(format!("{}/hashes.json", upload_dir)).unwrap();
    zip.write_all(&json_data).unwrap();

    zip.finish().unwrap();

    println!("Response: {}", response);
}

fn process_directory(directory: &str) {
    let paths = fs::read_dir(directory).unwrap();

    for path in paths {
        let entry = path.unwrap();
        let file_name = entry.file_name().into_string().unwrap();
        let file_path = entry.path();
        let extension = file_path.extension().unwrap_or_default();
        if extension == "png" || extension == "jpg" || extension == "jpeg" {
            println!("Processing image file: {:?}", file_path);
            process_image(file_path.to_str().unwrap());
            println!("------------------------------");
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Please provide a file or directory as an argument or -h for Help.");
        return;
    }

    match args[1].as_str() {
        "-v" => {
            println!("SealedCh - Simple Media Ownership, Copyright and License Protection Utility 1.3");
            println!("Copyright © 2023 iBinary LLC = License MIT – <https://spdx.org/licenses/MIT.html>.");
            println!("This is free software, there is no warranty: you are free to change and redistribute it with attribution.");
            println!("Written by Jake Kitchen and Ken Nickerson. More information at: https://sealed.ch");
            return;
        },
        "-h" => {
            println!("SealedCh - Simple Media Ownership, Copyright and License Protection Utility 1.3");
            println!("Copyright © 2023 iBinary LLC = License MIT – <https://spdx.org/licenses/MIT.html> - https://sealed.ch");
            println!("This is free software, there is no warranty: you are free to change and redistribute it with attribution.");
            println!("Usage: sealed [OPTION]… [FILE]… \n");
            println!("Generate shareable media file(s) (the current directory by default) resulting in compressed files with the framed media, frames, and hash codes in text for protection of source media.");
            println!("Mandatory arguments: <file name> or <directory name>");
            println!("Optional arguments: -v for VERSION and/or -h for HELP");
            return;
        },
        _ => {}
    }

    let path = &args[1];

    if Path::new(path).is_dir() {
        process_directory(path);
    } else if Path::new(path).exists() {
        process_image(path);
    } else {
        println!("Invalid argument. Please provide a valid file, directory or -h for Help.");
    }
}
