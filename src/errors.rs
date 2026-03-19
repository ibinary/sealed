use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum SealedError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Image processing error: {0}")]
    Image(#[from] image::ImageError),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("ZIP archive error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Signature error: {0}")]
    Signature(#[from] ed25519_dalek::SignatureError),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("External tool failed: {tool} - {message}")]
    ExternalTool { tool: String, message: String },

    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    #[error("IPFS error: {0}")]
    IpfsError(String),

    #[error("Timestamp error: {0}")]
    TimestampError(String),

    #[error("Key error: {0}")]
    KeyError(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
}

pub type SealedResult<T> = Result<T, SealedError>;
