use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tiny_http::{Server, Request, Response, Header, Method, StatusCode};
use tracing::info;

use crate::image_processing::{seal_image, save_artifacts, open_image_by_content, SealConfig};
use crate::hashing::HashRecord;
use crate::tile_hashing::generate_tile_index;
use crate::archive::create_archive;
use crate::signing::SealedKeyPair;
use crate::verification::verify_image;
use crate::ipfs::{pin_to_ipfs, IpfsConfig};
use crate::video::process_video;
use crate::pdf::process_pdf;

/// Web server config.
pub struct ServeConfig {
    pub port: u16,
    pub static_dir: PathBuf,
    pub uploads_dir: PathBuf,
    pub key_path: Option<PathBuf>,
}

/// Start the HTTP server.
pub fn run_server(config: ServeConfig) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", config.port);
    let server = Server::http(&addr)
        .map_err(|e| anyhow::anyhow!("Failed to start server: {}", e))?;

    info!("Sealed web server listening on http://{}", addr);
    info!("Static dir: {}", config.static_dir.display());
    info!("Uploads dir: {}", config.uploads_dir.display());

    fs::create_dir_all(&config.uploads_dir)?;
    fs::create_dir_all(&config.static_dir)?;

    let config = Arc::new(config);

    for request in server.incoming_requests() {
        let config = Arc::clone(&config);
        if let Err(e) = handle_request(request, &config) {
            info!("Request error: {}", e);
        }
    }

    Ok(())
}

fn respond_json(request: Request, status: u16, body: &str) -> anyhow::Result<()> {
    let resp = Response::from_string(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
    request.respond(resp)?;
    Ok(())
}

fn handle_request(request: Request, config: &ServeConfig) -> anyhow::Result<()> {
    let url = request.url().to_string();
    let method = request.method().clone();

    info!("{} {}", method, url);

    match (&method, url.as_str()) {
        (Method::Post, "/image") => handle_seal(request, config, "image"),
        (Method::Post, "/video") => handle_seal(request, config, "video"),
        (Method::Post, "/pdf")   => handle_seal(request, config, "pdf"),
        (Method::Post, "/verify") => handle_verify(request, config),
        (Method::Get, _) => handle_static(request, config),
        _ => respond_json(request, 404, "{\"error\":\"Not found\"}"),
    }
}

/// Handle POST /image, /video, /pdf.
fn handle_seal(mut request: Request, config: &ServeConfig, file_type: &str) -> anyhow::Result<()> {
    let ipfs_url = request.headers().iter()
        .find(|h| h.field.equiv("X-IPFS-Url"))
        .map(|h| h.value.as_str().to_string());
    let ipfs_key = request.headers().iter()
        .find(|h| h.field.equiv("X-IPFS-Key"))
        .map(|h| h.value.as_str().to_string());

    let mut body = Vec::new();
    request.as_reader().read_to_end(&mut body)?;

    if body.is_empty() {
        return respond_json(request, 400, "{\"error\":\"Empty upload\"}");
    }

    let id = format!("demo-{}", uuid::Uuid::new_v4().to_string().replace("-", "").get(..16).unwrap_or("unknown"));
    let upload_dir = config.uploads_dir.join(&id);
    fs::create_dir_all(&upload_dir)?;

    let ext = match file_type {
        "video" => "mp4",
        "pdf" => "pdf",
        _ => "png",
    };
    let input_path = upload_dir.join(format!("input.{}", ext));
    fs::write(&input_path, &body)?;

    info!("Upload saved: {} ({} bytes, type={})", input_path.display(), body.len(), file_type);

    match seal_uploaded_file(&input_path, &upload_dir, config, file_type, ipfs_url, ipfs_key) {
        Ok(json) => respond_json(request, 200, &json),
        Err(e) => {
            info!("Seal error: {}", e);
            respond_json(request, 500, &format!("{{\"error\":\"{}\"}}", e))
        }
    }
}

/// Seal an uploaded file and return JSON response.
fn seal_uploaded_file(input_path: &Path, upload_dir: &Path, config: &ServeConfig, file_type: &str, ipfs_url: Option<String>, ipfs_key: Option<String>) -> anyhow::Result<String> {
    let seal_config = SealConfig::default();

    let artifacts = match file_type {
        "video" => process_video(input_path, upload_dir, 1, None, &seal_config)?,
        "pdf" => process_pdf(input_path, upload_dir, &seal_config)?,
        _ => {
            let img = open_image_by_content(input_path)?;
            let arts = seal_image(&img, &seal_config)?;
            save_artifacts(&arts, upload_dir)?;
            arts
        }
    };

    let tile_index = generate_tile_index(&artifacts.original);

    let tile_json = serde_json::to_string_pretty(&tile_index)?;
    fs::write(upload_dir.join("tile_index.json"), &tile_json)?;

    let url_prefix = format!("/uploads/{}", upload_dir.file_name().unwrap().to_string_lossy());

    let signed_by = if let Some(ref key_path) = config.key_path {
        match sign_record(upload_dir, &artifacts, key_path) {
            Ok(pub_key) => Some(pub_key),
            Err(e) => {
                info!("Signing skipped: {}", e);
                None
            }
        }
    } else {
        None
    };

    let hashes_json = build_hashes_json(&artifacts)?;
    fs::write(upload_dir.join("hashes.json"), &hashes_json)?;

    create_archive(upload_dir, "sealed")?;

    let mut response: serde_json::Value = serde_json::from_str(&hashes_json)?;
    response["originalImagePath"] = serde_json::json!(format!("{}/original.png", url_prefix));
    response["partsImagePath"] = serde_json::json!(format!("{}/frame.png", url_prefix));
    response["shareImagePath"] = serde_json::json!(format!("{}/share.png", url_prefix));
    response["recombinedImagePath"] = serde_json::json!(format!("{}/recombined.png", url_prefix));
    response["archiveURL"] = serde_json::json!(format!("{}/sealed.zip", url_prefix));

    if let Some(pub_key) = signed_by {
        response["signedBy"] = serde_json::json!(pub_key);
    }

    if let Some(url) = ipfs_url {
        let ipfs_config = IpfsConfig {
            api_url: url.clone(),
            api_key: ipfs_key,
            gateway_url: if url.contains("pinata") {
                "https://gateway.pinata.cloud/ipfs".to_string()
            } else {
                "https://ipfs.io/ipfs".to_string()
            },
        };

        match pin_to_ipfs(&upload_dir.join("hashes.json"), &ipfs_config) {
            Ok(record) => {
                info!("IPFS pinned: {}", record.cid);
                response["ipfsCid"] = serde_json::json!(record.cid);
                response["ipfsGateway"] = serde_json::json!(record.gateway_url);
            }
            Err(e) => info!("IPFS pin failed: {}", e),
        }
    }

    response["tile_index"] = serde_json::json!({
        "block_size": tile_index.block_size,
        "cols": tile_index.cols,
        "rows": tile_index.rows,
        "blocks_count": tile_index.blocks.len(),
    });

    Ok(serde_json::to_string(&response)?)
}

fn build_hashes_json(
    artifacts: &crate::image_processing::SealedArtifacts,
) -> anyhow::Result<String> {
    #[derive(serde::Serialize)]
    struct Record<'a> {
        original: &'a HashRecord,
        frame: &'a HashRecord,
        cropped: &'a HashRecord,
        recombined: &'a HashRecord,
        share: &'a HashRecord,
        sealed_at: String,
        sealed_version: &'static str,
    }

    let record = Record {
        original: &artifacts.original_hashes,
        frame: &artifacts.frame_hashes,
        cropped: &artifacts.cropped_hashes,
        recombined: &artifacts.recombined_hashes,
        share: &artifacts.share_hashes,
        sealed_at: chrono::Utc::now().to_rfc3339(),
        sealed_version: env!("CARGO_PKG_VERSION"),
    };

    Ok(serde_json::to_string_pretty(&record)?)
}

fn sign_record(
    upload_dir: &Path,
    artifacts: &crate::image_processing::SealedArtifacts,
    key_path: &Path,
) -> anyhow::Result<String> {
    let hashes_json = build_hashes_json(artifacts)?;
    let keypair = SealedKeyPair::load(key_path)?;
    let envelope = keypair.sign(&hashes_json);
    let signed_json = serde_json::to_string_pretty(&envelope)?;
    fs::write(upload_dir.join("signed_record.json"), &signed_json)?;
    Ok(envelope.public_key)
}

/// Handle POST /verify.
fn handle_verify(mut request: Request, config: &ServeConfig) -> anyhow::Result<()> {
    let content_type = request.headers().iter()
        .find(|h| h.field.equiv("Content-Type"))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default();

    if !content_type.contains("multipart/form-data") {
        return respond_json(request, 400, "{\"error\":\"Expected multipart/form-data\"}");
    }

    let boundary = match content_type.split("boundary=").nth(1) {
        Some(b) => b.to_string(),
        None => return respond_json(request, 400, "{\"error\":\"Missing boundary\"}"),
    };

    let mut body = Vec::new();
    request.as_reader().read_to_end(&mut body)?;

    info!("Verify: received {} bytes, boundary={}", body.len(), boundary);

    match do_verify(&body, &boundary, config) {
        Ok(json_str) => respond_json(request, 200, &json_str),
        Err(e) => {
            info!("Verify error: {}", e);
            respond_json(request, 500, &format!("{{\"error\":\"{}\"}}", e))
        }
    }
}

/// Inner verify logic.
fn do_verify(body: &[u8], boundary: &str, config: &ServeConfig) -> anyhow::Result<String> {
    let parts = parse_multipart(body, boundary);

    info!("Verify: parsed {} multipart parts: [{}]",
        parts.len(),
        parts.iter().map(|p| format!("{}={} bytes", p.name, p.data.len())).collect::<Vec<_>>().join(", ")
    );

    let archive_bytes = parts.iter()
        .find(|p| p.name == "archive")
        .map(|p| &p.data)
        .ok_or_else(|| anyhow::anyhow!("Missing 'archive' field"))?;
    let suspect_bytes = parts.iter()
        .find(|p| p.name == "suspect")
        .map(|p| &p.data)
        .ok_or_else(|| anyhow::anyhow!("Missing 'suspect' field"))?;

    info!("Verify: archive={} bytes, suspect={} bytes", archive_bytes.len(), suspect_bytes.len());

    let verify_id = format!("verify-{}", uuid::Uuid::new_v4().to_string().replace("-", "").get(..12).unwrap_or("tmp"));
    let verify_dir = config.uploads_dir.join(&verify_id);
    fs::create_dir_all(&verify_dir)?;

    fs::write(verify_dir.join("archive.zip"), archive_bytes)?;
    fs::write(verify_dir.join("suspect.png"), suspect_bytes)?;

    let sealed_dir = verify_dir.join("sealed");
    fs::create_dir_all(&sealed_dir)?;
    let file = fs::File::open(verify_dir.join("archive.zip"))?;
    let mut zip = zip::ZipArchive::new(file)?;

    info!("Verify: zip contains {} entries", zip.len());
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_file() {
            let name = entry.name().to_string();
            let out_path = sealed_dir.join(&name);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out_file = fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
            info!("Verify: extracted {}", name);
        }
    }

    let result = verify_image(&verify_dir.join("suspect.png"), &sealed_dir, None)?;

    let response = serde_json::json!({
        "verdict": result.verdict,
        "signature_valid": result.signature_valid,
        "vs_original": result.vs_original,
        "vs_cropped": result.vs_cropped,
        "tile_match": result.tile_match,
        "suspect_hashes": result.suspect_hashes,
    });

    let json_str = serde_json::to_string(&response)?;

    let _ = fs::remove_dir_all(&verify_dir);
    Ok(json_str)
}

/// Parsed multipart form field.
struct MultipartField {
    name: String,
    data: Vec<u8>,
}

/// Simple multipart/form-data parser.
fn parse_multipart(body: &[u8], boundary: &str) -> Vec<MultipartField> {
    let delim = format!("--{}", boundary);
    let delim_bytes = delim.as_bytes();
    let mut fields = Vec::new();

    let mut start = 0;
    let mut parts: Vec<&[u8]> = Vec::new();
    while let Some(pos) = find_bytes(&body[start..], delim_bytes) {
        if start > 0 {
            // The part is between the previous boundary end and this boundary start
            parts.push(&body[start..start + pos]);
        }
        start += pos + delim_bytes.len();
        // Skip \r\n or -- after boundary
        if start < body.len() && body[start] == b'-' { break; } // final --
        if start < body.len() && body[start] == b'\r' { start += 1; }
        if start < body.len() && body[start] == b'\n' { start += 1; }
    }

    for part in parts {
        if part.len() < 4 { continue; }
        if let Some(hdr_end) = find_bytes(part, b"\r\n\r\n") {
            let headers_str = String::from_utf8_lossy(&part[..hdr_end]);
            let data = &part[hdr_end + 4..];
            let data = if data.ends_with(b"\r\n") { &data[..data.len() - 2] } else { data };

            if let Some(name) = extract_field_name(&headers_str) {
                fields.push(MultipartField { name, data: data.to_vec() });
            }
        }
    }

    fields
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn extract_field_name(headers: &str) -> Option<String> {
    for line in headers.lines() {
        if line.to_lowercase().contains("content-disposition") {
            if let Some(idx) = line.find("name=\"") {
                let rest = &line[idx + 6..];
                if let Some(end) = rest.find('"') {
                    return Some(rest[..end].to_string());
                }
            }
        }
    }
    None
}

/// Handle GET requests.
fn handle_static(request: Request, config: &ServeConfig) -> anyhow::Result<()> {
    let url = request.url().to_string();
    let url_path = url.split('?').next().unwrap_or(&url);

    let file_path = if url_path.starts_with("/uploads/") {
        let rel = url_path.strip_prefix("/uploads/").unwrap_or("");
        config.uploads_dir.join(rel)
    } else {
        let rel = if url_path == "/" { "index.html" } else { url_path.trim_start_matches('/') };
        config.static_dir.join(rel)
    };

    if url_path.contains("..") {
        let resp = Response::from_string("Forbidden").with_status_code(StatusCode(403));
        request.respond(resp)?;
        return Ok(());
    }

    if file_path.is_file() {
        let content_type = guess_content_type(&file_path);
        let file = fs::File::open(&file_path)?;
        let len = file.metadata()?.len();
        let resp = Response::new(
            StatusCode(200),
            vec![Header::from_bytes("Content-Type", content_type).unwrap()],
            file,
            Some(len as usize),
            None,
        );
        request.respond(resp)?;
    } else {
        let resp = Response::from_string("Not found").with_status_code(StatusCode(404));
        request.respond(resp)?;
    }

    Ok(())
}

fn guess_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css")  => "text/css",
        Some("js")   => "application/javascript",
        Some("json") => "application/json",
        Some("png")  => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif")  => "image/gif",
        Some("svg")  => "image/svg+xml",
        Some("ico")  => "image/x-icon",
        Some("webp") => "image/webp",
        Some("zip")  => "application/zip",
        Some("pdf")  => "application/pdf",
        Some("mp4")  => "video/mp4",
        Some("webm") => "video/webm",
        Some("woff") | Some("woff2") => "font/woff2",
        Some("ttf")  => "font/ttf",
        Some("txt")  => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
