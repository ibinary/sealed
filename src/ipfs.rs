use serde::{Serialize, Deserialize};
use std::path::Path;
use tracing::info;

use crate::errors::{SealedError, SealedResult};

/// IPFS pin response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpfsRecord {
    pub cid: String,
    pub gateway_url: String,
    pub service: String,
    pub pinned_at: String,
}

/// IPFS pinning config.
#[derive(Debug, Clone)]
pub struct IpfsConfig {
    pub api_url: String,
    pub api_key: Option<String>,
    pub gateway_url: String,
}

impl Default for IpfsConfig {
    fn default() -> Self {
        Self {
            api_url: "http://127.0.0.1:5001".to_string(),
            api_key: None,
            gateway_url: "https://ipfs.io/ipfs".to_string(),
        }
    }
}

impl IpfsConfig {
    /// Pinata pinning service.
    pub fn pinata(api_key: &str) -> Self {
        Self {
            api_url: "https://api.pinata.cloud".to_string(),
            api_key: Some(api_key.to_string()),
            gateway_url: "https://gateway.pinata.cloud/ipfs".to_string(),
        }
    }

    /// Local IPFS node.
    pub fn local() -> Self {
        Self::default()
    }
}

/// Pin a file via a local IPFS node.
pub fn pin_to_local_ipfs(file_path: &Path, config: &IpfsConfig) -> SealedResult<IpfsRecord> {
    if !file_path.exists() {
        return Err(SealedError::FileNotFound(file_path.display().to_string()));
    }

    let file_data = std::fs::read(file_path)?;
    let file_name = file_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let url = format!("{}/api/v0/add", config.api_url);

    info!("Pinning {} to IPFS at {}", file_name, config.api_url);

    let form = reqwest::blocking::multipart::Form::new()
        .part("file", reqwest::blocking::multipart::Part::bytes(file_data)
            .file_name(file_name));

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(&url)
        .multipart(form)
        .send()
        .map_err(|e| SealedError::IpfsError(format!(
            "Failed to connect to IPFS node at {}: {}. Is your IPFS daemon running?",
            config.api_url, e
        )))?;

    if !response.status().is_success() {
        return Err(SealedError::IpfsError(format!(
            "IPFS API returned status {}: {}",
            response.status(),
            response.text().unwrap_or_default()
        )));
    }

    let body: serde_json::Value = response.json().map_err(|e| {
        SealedError::IpfsError(format!("Failed to parse IPFS response: {}", e))
    })?;

    let cid = body["Hash"]
        .as_str()
        .ok_or_else(|| SealedError::IpfsError("No CID in IPFS response".to_string()))?
        .to_string();

    let gateway = format!("{}/{}", config.gateway_url, cid);
    let pinned_at = chrono::Utc::now().to_rfc3339();

    info!("Pinned to IPFS: CID={}, Gateway={}", cid, gateway);

    Ok(IpfsRecord {
        cid,
        gateway_url: gateway,
        service: "local-ipfs".to_string(),
        pinned_at,
    })
}

/// Pin a file via Pinata.
pub fn pin_to_pinata(file_path: &Path, config: &IpfsConfig) -> SealedResult<IpfsRecord> {
    if !file_path.exists() {
        return Err(SealedError::FileNotFound(file_path.display().to_string()));
    }

    let api_key = config.api_key.as_ref().ok_or_else(|| {
        SealedError::IpfsError("Pinata API key required. Set --ipfs-key or SEALED_IPFS_KEY env var.".to_string())
    })?;

    let file_data = std::fs::read(file_path)?;
    let file_name = file_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let url = format!("{}/pinning/pinFileToIPFS", config.api_url);

    info!("Pinning {} to Pinata", file_name);

    let form = reqwest::blocking::multipart::Form::new()
        .part("file", reqwest::blocking::multipart::Part::bytes(file_data)
            .file_name(file_name));

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .map_err(|e| SealedError::IpfsError(format!("Pinata request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(SealedError::IpfsError(format!(
            "Pinata API returned status {}: {}",
            response.status(),
            response.text().unwrap_or_default()
        )));
    }

    let body: serde_json::Value = response.json().map_err(|e| {
        SealedError::IpfsError(format!("Failed to parse Pinata response: {}", e))
    })?;

    let cid = body["IpfsHash"]
        .as_str()
        .ok_or_else(|| SealedError::IpfsError("No CID in Pinata response".to_string()))?
        .to_string();

    let gateway = format!("{}/{}", config.gateway_url, cid);
    let pinned_at = chrono::Utc::now().to_rfc3339();

    info!("Pinned to Pinata: CID={}, Gateway={}", cid, gateway);

    Ok(IpfsRecord {
        cid,
        gateway_url: gateway,
        service: "pinata".to_string(),
        pinned_at,
    })
}

/// Pin a file to IPFS (auto-detects local vs Pinata).
pub fn pin_to_ipfs(file_path: &Path, config: &IpfsConfig) -> SealedResult<IpfsRecord> {
    if config.api_key.is_some() {
        pin_to_pinata(file_path, config)
    } else {
        pin_to_local_ipfs(file_path, config)
    }
}
