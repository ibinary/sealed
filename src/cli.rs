use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "sealed-ch", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Seal a file to produce ownership proof artifacts.
    Seal {
        #[arg(value_name = "PATH")]
        input: PathBuf,

        #[arg(short, long)]
        output: Option<PathBuf>,

        #[arg(long, default_value = "20")]
        edge_width: u32,

        #[arg(short, long)]
        key: Option<PathBuf>,

        #[arg(long)]
        ipfs: bool,

        #[arg(long, default_value = "http://127.0.0.1:5001")]
        ipfs_url: String,

        #[arg(long)]
        ipfs_key: Option<String>,

        #[arg(long, default_value = "5")]
        frame_interval: u64,

        #[arg(long)]
        sample_frames: Option<usize>,

        #[arg(long)]
        timestamp: bool,
    },

    /// Verify a suspect image against a sealed record.
    Verify {
        #[arg(value_name = "SUSPECT")]
        suspect: PathBuf,

        #[arg(value_name = "SEALED_DIR")]
        sealed_dir: PathBuf,

        #[arg(short, long)]
        public_key: Option<PathBuf>,
    },

    /// Generate an Ed25519 signing keypair.
    Keygen {
        #[arg(short, long, default_value = ".")]
        output: PathBuf,

        #[arg(short, long)]
        password: bool,
    },

    /// Start the demo web server.
    Serve {
        #[arg(short, long, default_value = "8000")]
        port: u16,

        #[arg(long, default_value = "static")]
        static_dir: PathBuf,

        #[arg(long, default_value = "uploads")]
        uploads_dir: PathBuf,

        #[arg(short, long)]
        key: Option<PathBuf>,
    },

    /// Poll for OTS Bitcoin confirmation (internal, spawned automatically).
    #[command(hide = true)]
    OtsUpgrade {
        #[arg(long)]
        hash: String,

        #[arg(long)]
        output_dir: PathBuf,

        #[arg(long)]
        ipfs_url: Option<String>,

        #[arg(long)]
        ipfs_key: Option<String>,
    },

    /// Pin a sealed record to IPFS.
    IpfsPin {
        #[arg(value_name = "SEALED_DIR")]
        sealed_dir: PathBuf,

        #[arg(long, default_value = "http://127.0.0.1:5001")]
        ipfs_url: String,

        #[arg(long)]
        ipfs_key: Option<String>,
    },
}
