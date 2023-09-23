#![feature(proc_macro_hygiene, decl_macro)]
#![allow(non_snake_case)]

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
use std::path::{Path};
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
    //println!("{:?}", img);  // print the resized image
    let mut ahash: u64 = 0;
    let avg: u64 = img.pixels().map(|p| u64::from(p[0])).sum::<u64>() / ((hash_size * hash_size) as u64);
    //println!("{}", avg);  // print the average pixel value
    for (_i, pixel) in img.pixels().enumerate() {
        //println!("{}", pixel[0]);  // print each pixel value
        ahash <<= 1;
        if u64::from(pixel[0]) >= avg {
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

fn remove_whitespace(img: &DynamicImage) -> DynamicImage {
    let img_gray = img.to_luma8();
    let (width, height) = img_gray.dimensions();

    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;

    for y in 0..height {
        for x in 0..width {
            let pixel = img_gray.get_pixel(x, y).0[0];
            if pixel < 255 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    let cropped = img.clone().crop(min_x, min_y, max_x - min_x + 1, max_y - min_y + 1);
    cropped
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
    let middle_part_dynamic = DynamicImage::ImageRgba8(middle_part.clone());
    let cropped_no_whitespace = remove_whitespace(&middle_part_dynamic);
    let cropped_no_whitespace_path = format!("{}/share.jpg", upload_dir);
    cropped_no_whitespace.save_with_format(&cropped_no_whitespace_path, ImageFormat::Jpeg).unwrap();


    let cropped_no_whitespace_hash = image_hash(&cropped_no_whitespace);

    let parts_img_gray = DynamicImage::ImageRgba8(parts_img.clone()).to_luma8();
    let recombined_img_gray = DynamicImage::ImageRgba8(recombined_img.clone()).to_luma8();
    let middle_part_dynamic_gray = middle_part_dynamic.to_luma8();
    

    let original_dhash = dhash(&img_gray, HASH_SIZE);
    let original_ahash = ahash(&img_gray, HASH_SIZE);
    let parts_dhash = dhash(&parts_img_gray, HASH_SIZE);
    let parts_ahash = ahash(&parts_img_gray, HASH_SIZE);
    let cropped_dhash = dhash(&middle_part_dynamic_gray, HASH_SIZE);
    let cropped_ahash = ahash(&middle_part_dynamic_gray, HASH_SIZE);
    let recombined_dhash = dhash(&recombined_img_gray, HASH_SIZE);
    let recombined_ahash = ahash(&recombined_img_gray, HASH_SIZE);
    let response = json!({
        "originalImagePath": format!("{}/original.png", upload_dir),
        "originalDhash": format!("{:016x}", original_dhash),
        "originalAhash": format!("{:016x}", original_ahash),
        "partsImagePath": format!("{}/frame.png", upload_dir),
        "partsDhash": format!("{:016x}", parts_dhash),
        "partsAhash": format!("{:016x}", parts_ahash),
        "shareImagePath": format!("{}/share.jpg", upload_dir),
        "originalImageHash": format!("{:016x}", image_hash(&img)),
        "partsImageHash": format!("{:016x}", image_hash(&DynamicImage::ImageRgba8(parts_img))),
        "shareImageHash": format!("{:016x}", cropped_no_whitespace_hash),
        "recombinedImageHash": format!("{:016x}", image_hash(&DynamicImage::ImageRgba8(recombined_img))),
        "shareImageDhash": format!("{:016x}", cropped_dhash),
        "shareImageAhash": format!("{:016x}", cropped_ahash),
        "recombinedImagePath": format!("{}/recombined.png", upload_dir),
        "recombinedDhash": format!("{:016x}", recombined_dhash),
        "recombinedAhash": format!("{:016x}", recombined_ahash),
        "archiveURL": format!("{}", upload_dir),
    });

    let mut file = File::create(format!("{}/hashes.txt", upload_dir)).unwrap();
    write!(file, "Archive URL: {}\n", response["archiveURL"]).unwrap();
    write!(file, "Original Image Path: {}\n", response["originalImagePath"]).unwrap();
    write!(file, "Original Image Hash: {}\n", response["originalImageHash"]).unwrap();
    write!(file, "Original Image Dhash: {}\n", response["originalDhash"]).unwrap();
    write!(file, "Original Image Ahash: {}\n", response["originalAhash"]).unwrap();
    write!(file, "Frame Image Path: {}\n", response["partsImagePath"]).unwrap();
    write!(file, "Frame Image Hash: {}\n", response["partsImageHash"]).unwrap();
    write!(file, "Frame Image Dhash: {}\n", response["partsDhash"]).unwrap();
    write!(file, "Frame Image Ahash: {}\n", response["partsAhash"]).unwrap();
    write!(file, "Share Image Path: {}\n", response["shareImagePath"]).unwrap();
    write!(file, "Share Image Hash: {}\n", response["shareImageHash"]).unwrap();
    write!(file, "Share Image Dhash: {}\n", response["shareImageDhash"]).unwrap();
    write!(file, "Share Image Ahash: {}\n", response["shareImageAhash"]).unwrap();
    write!(file, "Recombined Image Path: {}\n", response["recombinedImagePath"]).unwrap();
    write!(file, "Recombined Image Hash: {}\n", response["recombinedImageHash"]).unwrap();
    write!(file, "Recombined Image Dhash: {}\n", response["recombinedDhash"]).unwrap();
    write!(file, "Recombined Image Ahash: {}\n", response["recombinedAhash"]).unwrap();


    let hashes_path = format!("{}/hashes.json", upload_dir);
    let file = File::create(hashes_path).unwrap();
    to_writer_pretty(file, &response).unwrap();

    let path_str = format!("{}/sealed.zip", upload_dir);
    let path = Path::new(&path_str);
    let file = File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o755);

    zip.start_file("share.jpg", options).unwrap();
    let img_data = std::fs::read(&cropped_no_whitespace_path).unwrap();
    zip.write_all(&img_data).unwrap();

    zip.start_file("original.png", options).unwrap();
    let img_data = std::fs::read(&original_path).unwrap();
    zip.write_all(&img_data).unwrap();

    zip.start_file("recombined.png", options).unwrap();
    let img_data = std::fs::read(&original_path).unwrap();
    zip.write_all(&img_data).unwrap();

    zip.start_file("frame.png", options).unwrap();
    let img_data = std::fs::read(&parts_path).unwrap();
    zip.write_all(&img_data).unwrap();

    zip.start_file("hashes.json", options).unwrap();
    let json_data = std::fs::read(format!("{}/hashes.json", upload_dir)).unwrap();
    zip.write_all(&json_data).unwrap();

    zip.start_file("hashes.txt", options).unwrap();
    let txt_data = std::fs::read(format!("{}/hashes.txt", upload_dir)).unwrap();
    zip.write_all(&txt_data).unwrap();

    zip.finish().unwrap();

    println!("Response: {}", response);
}

fn process_directory(directory: &str) {
    let paths = fs::read_dir(directory).unwrap();

    for path in paths {
        let entry = path.unwrap();
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
        println!("Please provide a file or directory as an argument or -h for help");
        return;
    }

    match args[1].as_str() {
        "-v" => {
            println!("SealedCh - Simple Media Ownership, Copyright and License Protection Utility 1.2");
            println!("Copyright © 2023 iBinary LLC = License MIT – <https://spdx.org/licenses/MIT.html>.");
            println!("This is free software, there is no warranty: you are free to change and redistribute it with attribution.");
            println!("Written by Jake Kitchen and Ken Nickerson. More information at: https://sealed.ch");
            return;
        },
        "-h" => {
            println!("SealedCh - Simple Media Ownership, Copyright and License Protection Utility 1.2");
            println!("Copyright © 2023 iBinary LLC = License MIT – <https://spdx.org/licenses/MIT.html> - https://sealed.ch");
            println!("This is free software, there is no warranty: you are free to change and redistribute it with attribution.\n");
            println!("Usage: sealed-ch [OPTION]… [FILE]…");
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
    }
    else {
        println!("Invalid argument. Please provide a valid file or directory as an argument or -h for help");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma};
    use std::time::Instant;

    #[test]
    fn test_hash_speed() {
        let img = ImageBuffer::from_fn(1000, 1000, |x, y| {
            if (x % 2 == 0) && (y % 2 == 0) {
                Luma([0u8])
            } else {
                Luma([255u8])
            }
        });
        let img = DynamicImage::ImageLuma8(img);

        let start = Instant::now();
        let _ = image_hash(&img);
        println!("image_hash: {:?}", start.elapsed());

        let start = Instant::now();
        let _ = ahash(&img.to_luma8(), 8);  // assuming hash_size is 8
        println!("ahash: {:?}", start.elapsed());

        let start = Instant::now();
        let _ = dhash(&img.to_luma8(), 8);  // assuming hash_size is 8
        println!("dhash: {:?}", start.elapsed());
    }

    #[test]
    fn test_hash_functions() {
        let img = ImageBuffer::from_fn(10, 10, |_x, _y| Luma([0u8]));
        let img = DynamicImage::ImageLuma8(img);
    
        assert_ne!(image_hash(&img), 0, "image_hash failed on black image");
        assert_eq!(ahash(&img.to_luma8(), 8), 0xFFFFFFFFFFFFFFFF, "ahash failed on black image");
        assert_eq!(dhash(&img.to_luma8(), 8), 0, "dhash failed on black image");
    
        let img = ImageBuffer::from_fn(10, 10, |_x, _y| Luma([255u8]));
        let img = DynamicImage::ImageLuma8(img);
    
        assert_ne!(image_hash(&img), 0, "image_hash failed on white image");
        assert_eq!(ahash(&img.to_luma8(), 8), 0xFFFFFFFFFFFFFFFF, "ahash failed on white image");
        assert_eq!(dhash(&img.to_luma8(), 8), 0, "dhash failed on white image");
    }
}
