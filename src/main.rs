extern crate image;

use indicatif::{ProgressBar, ProgressStyle};
use std::fs::read_dir;
use std::process::Command;
use rand::Rng;
use std::io;
use rand::thread_rng;
use image::GenericImage;
use image::io::Reader as ImageReader;
use std::time::{SystemTime, UNIX_EPOCH};
use image::{GenericImageView, ImageFormat, Rgba, RgbaImage};
use serde_json::json;
use image::{imageops::FilterType, GrayImage, ImageBuffer, DynamicImage};
use std::fs::File;
use std::io::Write;
use rand::seq::SliceRandom;
use std::hash::{Hash, Hasher};
use fnv::FnvHasher;
use std::fs;
use std::path::{Path};
use std::env;
const PART_SIZE: u32 = 20;
const HASH_SIZE: u32 = 32;

fn crop_towards_center(img: &mut DynamicImage) -> DynamicImage {
    let (width, height) = img.dimensions();
    let mut top = 0;
    let mut bottom = height;
    let mut left = 0;
    let mut right = width;

    'outer: for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            if pixel[0] != 255 || pixel[1] != 255 || pixel[2] != 255 {
                top = y;
                break 'outer;
            }
        }
    }

    'outer: for y in (0..height).rev() {
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            if pixel[0] != 255 || pixel[1] != 255 || pixel[2] != 255 {
                bottom = y;
                break 'outer;
            }
        }
    }

    'outer: for x in 0..width {
        for y in 0..height {
            let pixel = img.get_pixel(x, y);
            if pixel[0] != 255 || pixel[1] != 255 || pixel[2] != 255 {
                left = x;
                break 'outer;
            }
        }
    }

    'outer: for x in (0..width).rev() {
        for y in 0..height {
            let pixel = img.get_pixel(x, y);
            if pixel[0] != 255 || pixel[1] != 255 || pixel[2] != 255 {
                right = x;
                break 'outer;
            }
        }
    }

    // Crop 8-10px in from the found edges
    let mut rng = rand::thread_rng();
    let crop_margin = rng.gen_range(10..21);
    img.crop(
        (left + crop_margin).min(width),
        (top + crop_margin).min(height),
        (right - left).saturating_sub(2 * crop_margin),
        (bottom - top).saturating_sub(2 * crop_margin)
    )
}

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


fn xor_random_pixels(img: &mut DynamicImage, percentage: f32) {
    let (width, height) = img.dimensions();
    let num_pixels_to_xor = ((width as f32) * (height as f32) * percentage).round() as u32;
    let mut rng = thread_rng();

    // Create an RGBA image to work with
    let mut rgba_img = img.to_rgba8();

    for _ in 0..num_pixels_to_xor {
        let x = rng.gen_range(0..width);
        let y = rng.gen_range(0..height);

        let mut current_pixel = rgba_img.get_pixel_mut(x, y);

        // Check if the pixel is black
        if current_pixel[0] == 0 && current_pixel[1] == 0 && current_pixel[2] == 0 {
            // Apply the XOR operation to black pixels
            let random_value = rng.gen::<u8>();
            let random_value = if random_value == 0 { 1 } else { random_value };
            current_pixel[0] ^= random_value;
            current_pixel[1] ^= random_value;
            current_pixel[2] ^= random_value;
            current_pixel[3] = 0; // Set alpha to 0 to make black XOR'd pixels transparent
        }
    }

    // Update the input image with the modified RGBA image
    *img = DynamicImage::ImageRgba8(rgba_img);
}
fn process_pdf(pdf_file: &str) {
    let mut path = env::var("PATH").unwrap();
    path.push_str(";./libs");
    env::set_var("PATH", &path);

    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .tick_chars("/|\\- ")
        .template("{spinner:.green} {wide_msg}"));

    let pdf_path = Path::new(pdf_file);
    let pdf_name = pdf_path.file_stem().unwrap().to_str().unwrap();
    let output_dir = format!("sealed/{}-{}", pdf_name, generate_uuid());
    fs::create_dir_all(&output_dir).unwrap();

    let status = Command::new("pdftopng")
        .arg(pdf_file)
        .arg(format!("{}/page", output_dir))
        .status()
        .expect("Failed to execute command");

    if !status.success() {
        panic!("Failed to convert PDF to images");
    }

    pb.set_message("Processing pages...");
    pb.tick();

    let page_files: Vec<_> = read_dir(&output_dir)
        .unwrap()
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    // Assuming the first page is representative for all pages
    let first_page_path = page_files.get(0).expect("No pages found");
    let mut img = image::open(first_page_path).expect("Failed to open the first page");

    // Crop towards the center until a non-white pixel is found
    img = crop_towards_center(&mut img);

    // Apply XOR to random pixels in the image
    xor_random_pixels(&mut img, 0.05); // 5% of the pixels

    // Save the processed image
    let processed_image_path = format!("{}/processed_image.png", output_dir);
    img.save(&processed_image_path).expect("Failed to save processed image");

    process_image(&processed_image_path);

    // Calculate the hash of the processed image
    let processed_hash = image_hash(&img);
    println!("Processed PDF saved in directory: {}", output_dir);
    println!("Processed hash: {}", processed_hash);

    pb.finish_with_message("Processing complete.");

}

fn process_video(video_file: &str, frame_interval: u64, sample: Option<usize>) {
    let mut path = env::var("PATH").unwrap();
    path.push_str(";./libs");
    env::set_var("PATH", &path);
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .tick_chars("/|\\- ")
        .template("{spinner:.green} {wide_msg}"));

    let video_path = Path::new(video_file);
    let video_name = video_path.file_stem().unwrap().to_str().unwrap();
    let output_dir = format!("sealed/{}-{}", video_name, generate_uuid());
    std::fs::create_dir_all(&output_dir).unwrap();

    // Extract frames from the video using ffmpeg
    let output_frames = format!("{}/frame-%04d.png", output_dir);
    let status = Command::new("ffmpeg")
        .arg("-i")
        .arg(video_file)
        .arg("-vf")
        .arg(format!("fps=1/{}", frame_interval)) // Extract 1 frame every 'frame_interval' seconds
        .arg(output_frames)
        .status()
        .expect("Failed to execute ffmpeg command");

    if !status.success() {
        panic!("Failed to extract frames from video");
    }

    // XOR the frames to a single image
    let mut xor_image: Option<DynamicImage> = None;
    let frame_files: Vec<_> = read_dir(&output_dir)
    .unwrap()
    .map(|res| res.map(|e| e.path()))
    .collect::<Result<Vec<_>, _>>()
    .unwrap();

    let frame_files = match sample {
        Some(n) => {
            let mut rng = thread_rng();
            frame_files.choose_multiple(&mut rng, n).cloned().collect()
        }
        None => frame_files,
    };

    for entry in frame_files {
        pb.set_message("Processing frames...");
        pb.tick();
        let path = entry;
        if path.extension().unwrap_or_default() == "png" {
            let img = ImageReader::open(path).unwrap().decode().unwrap();
            if let Some(ref mut xor_img) = xor_image {
                let img_buffer = img.to_rgba8();
                for (x, y, pixel) in img_buffer.enumerate_pixels() {
                    let xor_pixel = xor_img.get_pixel(x, y);
                    xor_img.put_pixel(x, y, Rgba([
                        xor_pixel[0] ^ pixel[0],
                        xor_pixel[1] ^ pixel[1],
                        xor_pixel[2] ^ pixel[2],
                        xor_pixel[3] ^ pixel[3],
                    ]));
                }
            } else {
                xor_image = Some(img);
            }
        }
    }

    // Save the XOR image
    let xor_image_path = format!("{}/xor_image.png", output_dir);
    let xor_image_unwrapped = xor_image.unwrap();
    xor_image_unwrapped.save(&xor_image_path).unwrap();


    // Process the XOR image using the Sealed 1.0 process
    process_image(&xor_image_path);
    let processed_hash = image_hash(&xor_image_unwrapped);
    println!("Processed video saved in directory: {}", output_dir);
    println!("Processed hash: {}", processed_hash);
}

fn process_image(image_file: &str) {
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
    image::imageops::replace(&mut parts_img, &bottom_part, 0, height - 20);
    image::imageops::replace(&mut parts_img, &left_part, 0, 20);
    image::imageops::replace(&mut parts_img, &right_part, width - 20, 20);

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
    let _file = File::create(format!("{}/hashes.json", upload_dir)).unwrap();
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
    let img_data = std::fs::read(&original_path).unwrap();
    zip.write_all(&img_data).unwrap();

    zip.start_file("frame.png", options).unwrap();
    let img_data = std::fs::read(&parts_path).unwrap();
    zip.write_all(&img_data).unwrap();

    zip.start_file("recombined.png", options).unwrap();
    let img_data = std::fs::read(&recombined_path).unwrap();
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
        let _file_name = entry.file_name().into_string().unwrap();
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
        println!("Usage: {} <file_or_directory_path>", args[0]);
        return;
    }

    let file_or_directory_path = &args[1];

    println!("Choose an option:");
    println!("1. Process a video");
    println!("2. Process a single image");
    println!("3. Process a directory of images");
    println!("4. Process a PDF file");

    let mut choice = String::new();
    io::stdin().read_line(&mut choice).expect("Failed to read line");
    }
}
