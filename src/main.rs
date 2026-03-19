use std::path::{Path, PathBuf};
use std::io::Write;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::{info, error};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use sealed::cli::{Cli, Commands};
use sealed::errors::SealedError;
use sealed::image_processing::{seal_image, save_artifacts, open_image_by_content, SealConfig};
use sealed::signing::SealedKeyPair;
use sealed::verification::{verify_image, SealedRecord};
use sealed::archive::create_archive;
use sealed::ipfs::{pin_to_ipfs, IpfsConfig};
use sealed::video::process_video;
use sealed::pdf::process_pdf;
use sealed::timestamp::{timestamp_hash, spawn_upgrade_listener, run_upgrade_loop};
use sealed::tile_hashing::generate_tile_index;

fn main() -> Result<()> {
    let cli = Cli::parse();

    let filter = if cli.verbose {
        EnvFilter::new("sealed=debug,info")
    } else {
        EnvFilter::new("sealed=info,warn")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    match cli.command {
        Commands::Seal {
            input,
            output,
            edge_width,
            key,
            ipfs,
            ipfs_url,
            ipfs_key,
            frame_interval,
            sample_frames,
            timestamp,
        } => {
            cmd_seal(
                &input, output.as_deref(), edge_width, key.as_deref(),
                ipfs, &ipfs_url, ipfs_key, frame_interval, sample_frames,
                timestamp,
            )?;
        }

        Commands::Verify {
            suspect,
            sealed_dir,
            public_key,
        } => {
            cmd_verify(&suspect, &sealed_dir, public_key.as_deref())?;
        }

        Commands::Keygen { output, password } => {
            cmd_keygen(&output, password)?;
        }

        Commands::Serve {
            port,
            static_dir,
            uploads_dir,
            key,
        } => {
            sealed::web_server::run_server(sealed::web_server::ServeConfig {
                port,
                static_dir,
                uploads_dir,
                key_path: key,
            })?;
        }

        Commands::OtsUpgrade { hash, output_dir, ipfs_url, ipfs_key } => {
            run_upgrade_loop(&hash, &output_dir, ipfs_url.as_deref(), ipfs_key.as_deref());
        }

        Commands::IpfsPin {
            sealed_dir,
            ipfs_url,
            ipfs_key,
        } => {
            cmd_ipfs_pin(&sealed_dir, &ipfs_url, ipfs_key)?;
        }
    }

    Ok(())
}

fn cmd_seal(
    input: &Path,
    output: Option<&Path>,
    edge_width: u32,
    key_path: Option<&Path>,
    ipfs: bool,
    ipfs_url: &str,
    ipfs_key: Option<String>,
    frame_interval: u64,
    sample_frames: Option<usize>,
    timestamp: bool,
) -> Result<()> {
    let config = SealConfig {
        edge_width,
        ..SealConfig::default()
    };

    let file_stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "sealed".to_string());

    let final_dir = match output {
        Some(p) => p.to_path_buf(),
        None => PathBuf::from(format!("sealed/{}-{}", file_stem, Uuid::new_v4())),
    };

    let temp_dir = final_dir.with_extension("tmp");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)
            .context("Failed to clean up previous temp directory")?;
    }
    std::fs::create_dir_all(&temp_dir)
        .context("Failed to create temp output directory")?;
    let output_dir = temp_dir.clone();

    let ext = input
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let artifacts = if input.is_dir() {
        info!("Processing directory: {}", input.display());
        let mut count = 0;
        let mut last_artifacts = None;
        for entry in std::fs::read_dir(input)? {
            let entry = entry?;
            let path = entry.path();
            if is_image_file(&path) {
                let img = open_image_by_content(&path)?;
                let sub_dir = output_dir.join(format!("{}", count));
                let arts = seal_image(&img, &config)?;
                save_artifacts(&arts, &sub_dir)?;
                write_hash_record(&arts, &sub_dir, key_path)?;
                info!("Sealed: {} -> {}", path.display(), sub_dir.display());
                last_artifacts = Some(arts);
                count += 1;
            }
        }
        info!("Sealed {} images from directory", count);
        match last_artifacts {
            Some(arts) => arts,
            None => {
                println!("No images found in directory.");
                return Ok(());
            }
        }
    } else if matches!(ext.as_str(), "mp4" | "avi" | "mov" | "mkv" | "webm") {
        info!("Processing video: {}", input.display());
        process_video(input, &output_dir, frame_interval, sample_frames, &config)?
    } else if ext == "pdf" {
        info!("Processing PDF: {}", input.display());
        process_pdf(input, &output_dir, &config)?
    } else {
        // Try to open as an image regardless of extension (detect format from content)
        info!("Processing image: {}", input.display());
        let img = open_image_by_content(input)?;
        let arts = seal_image(&img, &config)?;
        save_artifacts(&arts, &output_dir)?;
        arts
    };

    if !input.is_dir() {
        write_hash_record(&artifacts, &output_dir, key_path)?;
    }

    let archive_path = create_archive(&output_dir, &file_stem)?;
    info!("Archive: {}", archive_path.display());

    let ipfs_key_for_ots = ipfs_key.clone();
    if ipfs {
        let ipfs_key_resolved = ipfs_key.or_else(|| std::env::var("SEALED_IPFS_KEY").ok());
        let ipfs_config = if let Some(ref api_key) = ipfs_key_resolved {
            IpfsConfig::pinata(api_key)
        } else {
            IpfsConfig {
                api_url: ipfs_url.to_string(),
                ..IpfsConfig::default()
            }
        };

        let hashes_path = output_dir.join("hashes.json");
        match pin_to_ipfs(&hashes_path, &ipfs_config) {
            Ok(record) => {
                info!("IPFS CID (hashes): {}", record.cid);
                info!("IPFS Gateway: {}", record.gateway_url);
                let ipfs_json = serde_json::to_string_pretty(&record)?;
                let ipfs_path = output_dir.join("ipfs_record.json");
                std::fs::write(&ipfs_path, ipfs_json)?;
            }
            Err(e) => {
                error!("IPFS pinning failed: {}. Sealed record saved locally.", e);
            }
        }

        let signed_path = output_dir.join("signed_record.json");
        if signed_path.exists() {
            match pin_to_ipfs(&signed_path, &ipfs_config) {
                Ok(record) => {
                    info!("IPFS CID (signed): {}", record.cid);
                    info!("IPFS Gateway (signed): {}", record.gateway_url);
                    let ipfs_json = serde_json::to_string_pretty(&record)?;
                    let ipfs_path = output_dir.join("ipfs_signed_record.json");
                    std::fs::write(&ipfs_path, ipfs_json)?;
                }
                Err(e) => {
                    error!("IPFS pinning of signed record failed: {}", e);
                }
            }
        }
    }

    let mut ots_submitted = false;
    if timestamp {
        info!("Submitting hash to OpenTimestamps...");
        match timestamp_hash(&artifacts.original_hashes.sha256, &output_dir) {
            Ok(record) => {
                info!("OpenTimestamps proof saved: {}", record.ots_file);
                println!("OpenTimestamps: proof submitted (pending Bitcoin confirmation)");
                ots_submitted = true;
            }
            Err(e) => {
                error!("OpenTimestamps failed: {}. Sealed record saved locally.", e);
            }
        }
    }

    if final_dir.exists() {
        std::fs::remove_dir_all(&final_dir)
            .context("Failed to remove existing output directory")?;
    }
    std::fs::rename(&temp_dir, &final_dir)
        .context("Failed to move sealed output to final directory")?;

    if ots_submitted {
        let ipfs_url_arg: Option<&str> = if ipfs { Some(ipfs_url) } else { None };
        let ipfs_key_arg = if ipfs { ipfs_key_for_ots.as_deref() } else { None };
        spawn_upgrade_listener(&artifacts.original_hashes.sha256, &final_dir, ipfs_url_arg, ipfs_key_arg);
    }

    println!("\n=== SEALED SUCCESSFULLY ===");
    println!("Output: {}", final_dir.display());
    println!("SHA-256: {}", artifacts.original_hashes.sha256);
    println!("BLAKE3:  {}", artifacts.original_hashes.blake3);

    if ots_submitted {
        println!("OTS: background process polling for Bitcoin confirmation.");
    }

    Ok(())
}

fn write_hash_record(
    artifacts: &sealed::image_processing::SealedArtifacts,
    output_dir: &Path,
    key_path: Option<&Path>,
) -> Result<()> {
    info!("Generating tile hash index for crop detection...");
    let tile_index = generate_tile_index(&artifacts.original);

    let tile_json = serde_json::to_string_pretty(&tile_index)?;
    let tile_path = output_dir.join("tile_index.json");
    std::fs::write(&tile_path, &tile_json)?;
    info!("Tile index: {} ({} blocks)", tile_path.display(), tile_index.blocks.len());

    let sealed_record = SealedRecord {
        original: artifacts.original_hashes.clone(),
        frame: artifacts.frame_hashes.clone(),
        cropped: artifacts.cropped_hashes.clone(),
        recombined: artifacts.recombined_hashes.clone(),
        share: Some(artifacts.share_hashes.clone()),
        tile_index: None,
        sealed_at: chrono::Utc::now().to_rfc3339(),
        sealed_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let json = serde_json::to_string_pretty(&sealed_record)?;
    let hashes_path = output_dir.join("hashes.json");
    std::fs::write(&hashes_path, &json)?;
    info!("Hash record: {}", hashes_path.display());

    let txt_path = output_dir.join("hashes.txt");
    let mut f = std::fs::File::create(&txt_path)?;
    writeln!(f, "=== Sealed Hash Record ===")?;
    writeln!(f, "Sealed at: {}", sealed_record.sealed_at)?;
    writeln!(f, "Version: {}", sealed_record.sealed_version)?;
    writeln!(f)?;
    writeln!(f, "Original SHA-256: {}", sealed_record.original.sha256)?;
    writeln!(f, "Original BLAKE3:  {}", sealed_record.original.blake3)?;
    writeln!(f, "Original aHash:   {}", sealed_record.original.ahash)?;
    writeln!(f, "Original dHash:   {}", sealed_record.original.dhash)?;
    writeln!(f, "Original pHash:   {}", sealed_record.original.phash)?;
    writeln!(f)?;
    writeln!(f, "Frame SHA-256:    {}", sealed_record.frame.sha256)?;
    writeln!(f, "Frame BLAKE3:     {}", sealed_record.frame.blake3)?;
    writeln!(f)?;
    writeln!(f, "Cropped SHA-256:  {}", sealed_record.cropped.sha256)?;
    writeln!(f, "Cropped BLAKE3:   {}", sealed_record.cropped.blake3)?;
    writeln!(f)?;
    writeln!(f, "Recombined SHA-256: {}", sealed_record.recombined.sha256)?;
    writeln!(f, "Recombined BLAKE3:  {}", sealed_record.recombined.blake3)?;

    if let Some(key_file) = key_path {
            let is_encrypted = {
            let data = std::fs::read(key_file).unwrap_or_default();
            data.starts_with(sealed::signing::ENCRYPTED_KEY_MAGIC)
        };
        let keypair = if is_encrypted {
            let password = rpassword::prompt_password("Enter key password: ")
                .context("Failed to read password")?;
            SealedKeyPair::load_encrypted(key_file, &password)
                .context("Failed to decrypt signing key")?  
        } else {
            SealedKeyPair::load(key_file)
                .context("Failed to load signing key")?
        };
        let envelope = keypair.sign(&json);
        let signed_json = serde_json::to_string_pretty(&envelope)?;
        let signed_path = output_dir.join("signed_record.json");
        std::fs::write(&signed_path, &signed_json)?;
        info!("Signed record: {}", signed_path.display());
        writeln!(f)?;
        writeln!(f, "Digitally signed with Ed25519")?;
        writeln!(f, "Public key: {}", envelope.public_key)?;
    }

    Ok(())
}

fn cmd_verify(suspect: &Path, sealed_dir: &Path, public_key: Option<&Path>) -> Result<()> {
    info!("Verifying {} against {}", suspect.display(), sealed_dir.display());

    let result = verify_image(suspect, sealed_dir, public_key)?;

    println!("\n=== VERIFICATION RESULT ===");
    println!("Verdict: {}", result.verdict);
    println!();
    println!("Signature valid: {}", result.signature_valid);
    println!();
    println!("vs Original:");
    println!("  Confidence:    {}", result.vs_original.confidence);
    println!("  Exact match:   {}", result.vs_original.exact_match);
    println!("  SHA-256 match: {}", result.vs_original.sha256_match);
    println!("  BLAKE3 match:  {}", result.vs_original.blake3_match);
    println!("  aHash distance: {}", result.vs_original.ahash_hamming);
    println!("  dHash distance: {}", result.vs_original.dhash_hamming);
    println!("  pHash distance: {}", result.vs_original.phash_hamming);
    println!();
    println!("vs Cropped/Share:");
    println!("  Confidence:    {}", result.vs_cropped.confidence);
    println!("  Exact match:   {}", result.vs_cropped.exact_match);
    println!("  SHA-256 match: {}", result.vs_cropped.sha256_match);
    println!("  aHash distance: {}", result.vs_cropped.ahash_hamming);
    println!("  dHash distance: {}", result.vs_cropped.dhash_hamming);
    println!("  pHash distance: {}", result.vs_cropped.phash_hamming);
    println!();
    println!("Suspect image hashes:");
    println!("  SHA-256: {}", result.suspect_hashes.sha256);
    println!("  BLAKE3:  {}", result.suspect_hashes.blake3);

    let result_json = serde_json::to_string_pretty(&result)?;
    let result_path = suspect
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(
            "{}_verification.json",
            suspect.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "suspect".to_string())
        ));
    std::fs::write(&result_path, &result_json)?;
    info!("Verification result saved to {}", result_path.display());

    Ok(())
}

fn cmd_keygen(output_dir: &Path, encrypt: bool) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;

    let keypair = SealedKeyPair::generate();

    let secret_path = output_dir.join("sealed.key");
    let public_path = output_dir.join("sealed.pub");

    if encrypt {
        let password = rpassword::prompt_password("Enter password to encrypt secret key: ")
            .context("Failed to read password")?;
        let confirm = rpassword::prompt_password("Confirm password: ")
            .context("Failed to read password confirmation")?;
        if password != confirm {
            anyhow::bail!("Passwords do not match");
        }
        keypair.save_secret_encrypted(&secret_path, &password)
            .context("Failed to save encrypted secret key")?;
        println!("=== Ed25519 Keypair Generated (encrypted) ===");
        println!("Secret key: {} (PASSWORD-ENCRYPTED - KEEP THIS SAFE)", secret_path.display());
    } else {
        keypair.save_secret(&secret_path)
            .context("Failed to save secret key")?;
        println!("=== Ed25519 Keypair Generated ===");
        println!("Secret key: {} (KEEP THIS SAFE - DO NOT SHARE)", secret_path.display());
    }

    keypair.save_public(&public_path)
        .context("Failed to save public key")?;

    println!("Public key: {} (share freely for verification)", public_path.display());
    println!("Public key (base64): {}", keypair.public_key_base64());

    Ok(())
}

fn cmd_ipfs_pin(sealed_dir: &Path, ipfs_url: &str, ipfs_key: Option<String>) -> Result<()> {
    let hashes_path = sealed_dir.join("hashes.json");
    if !hashes_path.exists() {
        return Err(SealedError::FileNotFound(
            "hashes.json not found in sealed directory".to_string(),
        ).into());
    }

    let ipfs_key_resolved = ipfs_key.or_else(|| std::env::var("SEALED_IPFS_KEY").ok());
    let config = if let Some(ref api_key) = ipfs_key_resolved {
        IpfsConfig::pinata(api_key)
    } else {
        IpfsConfig {
            api_url: ipfs_url.to_string(),
            ..IpfsConfig::default()
        }
    };

    let record = pin_to_ipfs(&hashes_path, &config)?;

    println!("\n=== IPFS Pin Successful ===");
    println!("CID: {}", record.cid);
    println!("Gateway: {}", record.gateway_url);
    println!("Pinned at: {}", record.pinned_at);
    println!("Service: {}", record.service);

    let ipfs_json = serde_json::to_string_pretty(&record)?;
    let ipfs_path = sealed_dir.join("ipfs_record.json");
    std::fs::write(&ipfs_path, &ipfs_json)?;

    let signed_path = sealed_dir.join("signed_record.json");
    if signed_path.exists() {
        info!("Also pinning signed record...");
        let signed_record = pin_to_ipfs(&signed_path, &config)?;
        println!("Signed record CID: {}", signed_record.cid);
        println!("Signed record Gateway: {}", signed_record.gateway_url);
    }

    Ok(())
}

/// Check if a file is likely an image based on extension.
fn is_image_file(path: &Path) -> bool {
    let ext = path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "tiff" | "tif" | "webp" |
        "gif" | "avif" | "ico" | "pnm" | "pbm" | "pgm" | "ppm" | "qoi"
    )
}
