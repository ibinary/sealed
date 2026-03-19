use std::path::Path;
use serde::{Serialize, Deserialize};
use tracing::info;

use crate::errors::{SealedError, SealedResult};

/// OpenTimestamps proof record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampRecord {
    pub hash: String,
    pub calendars: Vec<String>,
    pub ots_file: String,
    pub status: String,
    pub submitted_at: String,
}

/// OpenTimestamps calendar servers.
const OTS_CALENDARS: &[&str] = &[
    "https://a.pool.opentimestamps.org",
    "https://b.pool.opentimestamps.org",
];

/// OTS proof file magic header.
const OTS_MAGIC: &[u8] = b"\x00OpenTimestamps\x00\x00Proof\x00\xbf\x89\xe2\xe8\x84\xe8\x92\x94";

/// OTS v1 version byte.
const OTS_VERSION: u8 = 0x01;

/// OTS SHA-256 tag.
const OTS_TAG_SHA256: u8 = 0x08;

/// Build a .ots proof file from a SHA-256 hash and calendar response.
fn build_ots_proof(hash_bytes: &[u8], _calendar_url: &str, calendar_response: &[u8]) -> Vec<u8> {
    let mut proof = Vec::with_capacity(128 + calendar_response.len());

    proof.extend_from_slice(OTS_MAGIC);
    proof.push(OTS_VERSION);
    proof.push(OTS_TAG_SHA256);
    proof.extend_from_slice(hash_bytes);
    proof.extend_from_slice(calendar_response);

    proof
}

/// Submit a SHA-256 hash to OpenTimestamps. Returns a .ots proof.
pub fn submit_to_opentimestamps(sha256_hex: &str) -> SealedResult<(Vec<u8>, String)> {
    let hash_bytes = hex::decode(sha256_hex).map_err(|e| {
        SealedError::InvalidInput(format!("Invalid SHA-256 hex: {}", e))
    })?;

    if hash_bytes.len() != 32 {
        return Err(SealedError::InvalidInput(
            "SHA-256 hash must be 32 bytes".to_string(),
        ));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| SealedError::TimestampError(format!("HTTP client error: {}", e)))?;

    let mut last_error = String::new();
    for calendar in OTS_CALENDARS {
        let url = format!("{}/digest", calendar);
        info!("Submitting hash to OpenTimestamps calendar: {}", calendar);

        match client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .header("User-Agent", "sealed-ch/2.0")
            .header("Accept", "application/vnd.opentimestamps.v1")
            .body(hash_bytes.clone())
            .send()
        {
            Ok(response) => {
                if response.status().is_success() {
                    let calendar_response = response.bytes().map_err(|e| {
                        SealedError::TimestampError(format!("Failed to read OTS response: {}", e))
                    })?;
                    info!(
                        "OpenTimestamps attestation received from {} ({} bytes)",
                        calendar,
                        calendar_response.len()
                    );
                    let ots_proof = build_ots_proof(&hash_bytes, calendar, &calendar_response);
                    return Ok((ots_proof, calendar.to_string()));
                } else {
                    last_error = format!(
                        "{} returned status {}",
                        calendar,
                        response.status()
                    );
                    info!("Calendar {} failed: {}", calendar, last_error);
                }
            }
            Err(e) => {
                last_error = format!("{} connection failed: {}", calendar, e);
                info!("{}", last_error);
            }
        }
    }

    Err(SealedError::TimestampError(format!(
        "All OpenTimestamps calendars failed. Last error: {}",
        last_error
    )))
}

/// Poll a calendar server to upgrade a pending OTS proof to a Bitcoin-confirmed one.
/// Returns the upgraded proof bytes if successful, None if still pending.
pub fn try_upgrade_ots(sha256_hex: &str) -> SealedResult<Option<Vec<u8>>> {
    let hash_bytes = hex::decode(sha256_hex).map_err(|e| {
        SealedError::InvalidInput(format!("Invalid SHA-256 hex: {}", e))
    })?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| SealedError::TimestampError(format!("HTTP client error: {}", e)))?;

    for calendar in OTS_CALENDARS {
        let url = format!("{}/timestamp", calendar);

        match client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .header("User-Agent", "sealed-ch/2.0")
            .header("Accept", "application/vnd.opentimestamps.v1")
            .body(hash_bytes.clone())
            .send()
        {
            Ok(response) if response.status().is_success() => {
                let body = response.bytes().map_err(|e| {
                    SealedError::TimestampError(format!("Failed to read upgrade response: {}", e))
                })?;
                if body.len() > 0 {
                    let upgraded = build_ots_proof(&hash_bytes, calendar, &body);
                    return Ok(Some(upgraded));
                }
            }
            _ => continue,
        }
    }

    Ok(None)
}

/// Spawn a detached background process that polls for OTS Bitcoin confirmation.
/// Re-invokes the current binary with the hidden `ots-upgrade` subcommand.
pub fn spawn_upgrade_listener(sha256_hex: &str, output_dir: &Path, ipfs_url: Option<&str>, ipfs_key: Option<&str>) {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            info!("OTS upgrade: could not determine executable path: {}", e);
            return;
        }
    };

    let log_path = output_dir.join("ots_upgrade.log");
    let log_file = match std::fs::File::create(&log_path) {
        Ok(f) => f,
        Err(e) => {
            info!("OTS upgrade: could not create log file: {}", e);
            return;
        }
    };

    let mut cmd = std::process::Command::new(exe);
    cmd.arg("ots-upgrade")
        .arg("--hash")
        .arg(sha256_hex)
        .arg("--output-dir")
        .arg(output_dir);

    if let Some(url) = ipfs_url {
        cmd.arg("--ipfs-url").arg(url);
    }
    if let Some(key) = ipfs_key {
        cmd.arg("--ipfs-key").arg(key);
    }

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::from(log_file));

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const DETACHED_PROCESS: u32 = 0x00000008;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }

    match cmd.spawn() {
        Ok(child) => {
            info!("OTS upgrade listener spawned (pid {})", child.id());
            drop(child);
        }
        Err(e) => info!("OTS upgrade: failed to spawn background process: {}", e),
    }
}

/// Run the OTS upgrade polling loop (called by the hidden `ots-upgrade` subcommand).
pub fn run_upgrade_loop(sha256_hex: &str, output_dir: &Path, ipfs_url: Option<&str>, ipfs_key: Option<&str>) {
    let intervals = [30, 60, 120, 300, 600, 900, 1800, 3600];

    for attempt in 0..24 {
        let wait = intervals[attempt.min(intervals.len() - 1)];
        eprintln!("[OTS] attempt {}: sleeping {}s", attempt + 1, wait);
        std::thread::sleep(std::time::Duration::from_secs(wait as u64));

        match try_upgrade_ots(sha256_hex) {
            Ok(Some(upgraded_proof)) => {
                let ots_path = output_dir.join("timestamp.ots");
                if let Err(e) = std::fs::write(&ots_path, &upgraded_proof) {
                    eprintln!("[OTS] failed to write proof: {}", e);
                    return;
                }

                let record_path = output_dir.join("timestamp_record.json");
                if let Ok(json) = std::fs::read_to_string(&record_path) {
                    if let Ok(mut record) = serde_json::from_str::<TimestampRecord>(&json) {
                        record.status = "confirmed".to_string();
                        record.ots_file = ots_path.display().to_string();
                        if let Ok(updated) = serde_json::to_string_pretty(&record) {
                            let _ = std::fs::write(&record_path, updated);
                        }
                    }
                }

                if let Some(url) = ipfs_url {
                    eprintln!("[OTS] re-pinning to IPFS with confirmed proof...");
                    repin_with_ots(output_dir, url, ipfs_key);
                }

                eprintln!("[OTS] confirmed! proof upgraded.");
                return;
            }
            Ok(None) => {
                eprintln!("[OTS] attempt {}: still pending", attempt + 1);
            }
            Err(e) => {
                eprintln!("[OTS] attempt {}: error: {}", attempt + 1, e);
            }
        }
    }
    eprintln!("[OTS] gave up after 24 attempts");
}

/// Re-pin hashes.json + confirmed OTS proof to IPFS.
fn repin_with_ots(output_dir: &Path, ipfs_url: &str, ipfs_key: Option<&str>) {
    use crate::ipfs::{pin_to_ipfs, IpfsConfig};

    let config = IpfsConfig {
        api_url: ipfs_url.to_string(),
        api_key: ipfs_key.map(|k| k.to_string()),
        gateway_url: if ipfs_url.contains("pinata") {
            "https://gateway.pinata.cloud/ipfs".to_string()
        } else {
            "https://ipfs.io/ipfs".to_string()
        },
    };

    let hashes_path = output_dir.join("hashes.json");
    if hashes_path.exists() {
        match pin_to_ipfs(&hashes_path, &config) {
            Ok(record) => {
                eprintln!("[OTS] IPFS re-pin: CID={}", record.cid);
                let ipfs_json = serde_json::to_string_pretty(&record).unwrap_or_default();
                let _ = std::fs::write(output_dir.join("ipfs_record.json"), ipfs_json);
            }
            Err(e) => eprintln!("[OTS] IPFS re-pin failed: {}", e),
        }
    }

    let ots_path = output_dir.join("timestamp.ots");
    if ots_path.exists() {
        match pin_to_ipfs(&ots_path, &config) {
            Ok(record) => {
                eprintln!("[OTS] IPFS pin (ots proof): CID={}", record.cid);
                let ipfs_json = serde_json::to_string_pretty(&record).unwrap_or_default();
                let _ = std::fs::write(output_dir.join("ipfs_ots_record.json"), ipfs_json);
            }
            Err(e) => eprintln!("[OTS] IPFS pin (ots proof) failed: {}", e),
        }
    }
}

/// Submit a hash and save the .ots proof file.
pub fn timestamp_hash(sha256_hex: &str, output_dir: &Path) -> SealedResult<TimestampRecord> {
    let (ots_proof, calendar_used) = submit_to_opentimestamps(sha256_hex)?;

    let ots_path = output_dir.join("timestamp.ots");
    std::fs::write(&ots_path, &ots_proof)?;
    info!("OpenTimestamps proof saved to {} ({} bytes)", ots_path.display(), ots_proof.len());

    let record = TimestampRecord {
        hash: sha256_hex.to_string(),
        calendars: vec![calendar_used],
        ots_file: "timestamp.ots".to_string(),
        status: "pending".to_string(),
        submitted_at: chrono::Utc::now().to_rfc3339(),
    };

    let record_json = serde_json::to_string_pretty(&record)?;
    let record_path = output_dir.join("timestamp_record.json");
    std::fs::write(&record_path, &record_json)?;

    Ok(record)
}
