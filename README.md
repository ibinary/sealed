# Sealed
**Simple Media Ownership, Copyright, and License Protection Utility** — MIT License

Sealed is an open-source utility that employs a novel process to protect original creator or copyright holder media. Sealed now supports IMAGE(S), VIDEO(S), and TEXT(S) via .PDFs. Sealed 3.x will add AUDIO(S). The goal is to help protect original content creators and copyright holders from the growing incursions on their art, that may be proven manually, automatically, or legally in any process the copyright holder may engage in. Sealed is open-source and offers no accountability or liability for your use of this code or service.

The process provides a proof-based model of ownership that is resistant to AI scraping, GPT refactoring, outpainting, decryption, or other processes that may use or alter source images. The code for Sealed is open-source, under the MIT License, and may be included in as many applets, applications, and services as possible with attribution and a link to this repository.

## What's New in 2.0

- **SHA-256 + BLAKE3** cryptographic hashes over raw decoded pixel data
- **Ed25519 digital signatures** — cryptographically proves *who* sealed it
- **IPFS pinning** — hash record and signed record pinned for identity + temporal proof
- **OpenTimestamps** — `--timestamp` submits hash to the Bitcoin blockchain for independent temporal proof, with automatic background polling for confirmation
- **Verification command** — `sealed-ch verify` checks any suspect image against a sealed record
- **Perceptual hashing** (aHash + dHash + pHash) — three independent algorithms detect visually similar derivatives
- **Block-DCT tile hashing** — sub-region crop detection even when whole-image perceptual hashes fail
- **Password-encrypted keys** — AES-256-GCM + Argon2 key encryption
- **Deterministic processing** — same input always produces the same sealed output
- **Modular Rust library** — use as a CLI tool *or* integrate as a Rust crate
- **Built-in demo web UI** — `sealed-ch serve` for browser-based sealing and verification

## Process

Sealed invokes a process where media can be measured, cropped, shared, much like "edges" on paintings used for anti-forgery and insurance process. Cryptographic hashes and perceptual fingerprints are generated to .json and .txt files to be secured personally, or on a service like IPFS or blockchain or any preferred secure store.

1. Copyright IMAGE(S), VIDEO(S) or TEXT(S) are uploaded to Sealed.ch OR local terminal application OR self-directed use of the open-source code — https://github.com/ibinary/sealed — integrated for custom solutions. AUDIO(S) will be part of Sealed 3.x.
2. IMAGE(S) is **cryptographically hashed** (SHA-256 + BLAKE3) over raw decoded pixel data to fingerprint the original.
3. IMAGE(S) is cropped, producing a separate file of frames or "edges."
4. **Perceptual hashes** (aHash, dHash, pHash) are computed across all artifacts for fuzzy matching.
5. A **block-DCT tile index** is generated for sub-region crop detection.
6. Post crop IMAGE(S) are HASHED. Post crop EDGE(S) are HASHED.
7. If a signing key is provided, the hash record is **digitally signed** with Ed25519.
8. .ZIP file is produced with: original IMAGE(S), cropped IMAGE(S), edges IMAGE(S), share IMAGE(S), and HASH in .TXT and .JSON formats.
9. Post crop original "share" IMAGE(S) are available for immediate distribution.
10. VIDEO(S) follow the IMAGE(S) path after pre-processing to reduce the VIDEO(S) to a single XOR frame (IMAGE).
11. TEXT(S) follow the IMAGE(S) path after pre-processing to reduce the .PDF TEXT(S) to a single XOR frame (IMAGE).
12. Optionally, the hash record and signed record are **pinned to IPFS** for immutable public proof.
13. Optionally, the SHA-256 hash is submitted to **OpenTimestamps** for Bitcoin blockchain timestamping. A background process automatically polls for Bitcoin confirmation and upgrades the proof.

![sealed-process](https://github.com/ibinary/sealed/assets/86942/868fc0a0-7617-4e36-8e77-2234c8e044da)

## Post Process

If a shared IMAGE(S), VIDEO(S) or TEXT(S) is repurposed, the original owner has the sealed .ZIP to prove it's theirs.

- .ZIP contains .txt and .json hash files that can be stored locally, or imported into any database or monitoring tool.
- `sealed-ch verify` compares any suspect image against the sealed record — EXACT MATCH, PERCEPTUALLY SIMILAR, or NO MATCH.
- Tile matching catches crops and sub-regions that regular hashing would miss.

## Output Structure

```
sealed/<filename>-<uuid>/
  original.png          # The original image
  frame.png             # Edge frame only (border pixels)
  cropped.png           # Interior only (without edges)
  share.png             # Share-ready version (tightly cropped interior pixels only)
  recombined.png        # Recombined from frame + cropped (should match original)
  hashes.json           # All cryptographic + perceptual hashes (machine-readable)
  hashes.txt            # Human-readable hash summary
  signed_record.json    # Ed25519-signed hash record (if key provided)
  ipfs_record.json      # IPFS CID and gateway URL for hashes (if pinned)
  ipfs_signed_record.json # IPFS CID for signed record (if key + IPFS)
  timestamp.ots         # OpenTimestamps proof (if --timestamp)
  timestamp_record.json # Timestamp submission metadata (if --timestamp)
  <filename>.zip        # Archive of all above
```

## Quick Start

```bash
# Build from source
cargo build --release

# Generate a signing keypair (password-encrypted recommended)
sealed-ch keygen --output ./keys --password

# Seal an image
sealed-ch seal photo.png --key ./keys/sealed.key

# Seal with IPFS + Bitcoin timestamp
sealed-ch seal photo.png --key ./keys/sealed.key --ipfs --timestamp

# Verify a suspect image
sealed-ch verify suspect.png ./sealed/photo-abc123/ --public-key ./keys/sealed.pub
```

## Library Usage (Rust Crate)

```rust
use sealed::image_processing::{seal_image, SealConfig};
use sealed::hashing::compute_hash_record;
use sealed::signing::SealedKeyPair;
use sealed::verification::verify_image;

let img = image::open("photo.png")?;
let config = SealConfig::default();
let artifacts = seal_image(&img, &config)?;

let keypair = SealedKeyPair::generate();
let envelope = keypair.sign(&serde_json::to_string(&artifacts.original_hashes)?);
assert!(envelope.verify().is_ok());

let result = verify_image(Path::new("suspect.png"), Path::new("./sealed/photo-abc/"), Some(Path::new("./keys/sealed.pub")))?;
println!("{}", result.verdict);
```

## Dependencies

**Linux:** Install these tools on most Linux distributions using the package manager.

Ubuntu and other Debian-based distributions, use apt:
```bash
sudo apt update && sudo apt install ffmpeg poppler-utils
```

Fedora, CentOS, or other Red Hat-based distributions, use dnf:
```bash
sudo dnf install ffmpeg poppler-utils
```

**macOS:** Use Homebrew:
```bash
brew install ffmpeg poppler
```

**Windows:** Install ffmpeg and poppler and add to PATH.

## Architecture

```
src/
  lib.rs                # Library entry point (all modules re-exported)
  main.rs               # CLI entry point
  cli.rs                # clap v4 command definitions
  errors.rs             # Error types (thiserror)
  hashing.rs            # SHA-256, BLAKE3, aHash, dHash, pHash, comparison
  signing.rs            # Ed25519 keypair generation, signing, encryption
  timestamp.rs          # OpenTimestamps Bitcoin blockchain timestamping + auto-upgrade
  image_processing.rs   # Edge extraction, cropping, artifact generation
  tile_hashing.rs       # Block-DCT sub-region crop detection
  video.rs              # Video frame extraction + XOR compositing
  pdf.rs                # PDF to image conversion + processing
  archive.rs            # ZIP archive creation
  ipfs.rs               # IPFS pinning (local node + Pinata)
  verification.rs       # Suspect image verification against sealed records
  web_server.rs         # Built-in demo web UI
tests/
  integration.rs        # End-to-end seal/verify tests
  hashing.rs            # Hash algorithm tests
  signing.rs            # Signature tests
  image_processing.rs   # Artifact generation tests
  tile_hashing.rs       # Crop detection tests
static/
  index.html            # Demo web UI
```

## History

The idea for Sealed was prompted by a chance conversation at [Musée d'Orsay](https://www.musee-orsay.fr/en) in 2010. We asked about the insurance process for paintings in the gallery, and learned about scanning or photographing "edges" as a prime defense against forgery. As the content industry has changed with the move from analog to digital (no (print) negatives) and more recently scraped for use in corpus for AI used in GPT, a need has grown to have a simple, secure, open-source method to secure copyright.

"The Son of Man" (French: Le fils de l'homme) — [Wikipedia](https://en.wikipedia.org/wiki/The_Son_of_Man) — a 1964 painting by the Belgian surrealist painter René Magritte was chosen for Sealed.ch homepage, as a reflection of the use of this process in the popular 1999 movie "The Thomas Crown Affair" — [Wikipedia](https://en.wikipedia.org/wiki/The_Thomas_Crown_Affair_(1999_film)).

## Contact

We hope others can leverage this process, code into their products, services and applications to ensure protection for creators and copyright holders, who are appreciated for their work, but often not respected in terms of attribution or compensation. If you have any suggestions, enhancements, updates, forks, all are warmly welcomed at sealed-ch@pm.me

Jake Kitchen — jakekitchen@proton.me / Ken Nickerson — kenn@ibinary.com
Sealed was privately funded by iBinary LLC. Follow Sealed on Twitter: https://twitter.com/sealedch
