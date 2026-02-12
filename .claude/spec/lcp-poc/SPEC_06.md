# SPEC_06 — Compression (Zstd)

**Crate**: `lcp-encoder`, `lcp-decoder` (modifications)
**Phase**: 3 (Advanced Features)
**Prerequisites**: SPEC_01, SPEC_02, SPEC_03, SPEC_04
**Dependencies**: `lcp-wire`, `lcp-types`, `zstd`

---

## Context

LCP supports two compression modes per RFC §4.6:

1. **Per-block compression**: Individual block bodies are compressed with zstd.
   The `COMPRESSED` bit (bit 1) in `block_flags` indicates this. The
   `content_len` field reflects the *compressed* size; the decoder decompresses
   the body after reading it.

2. **Whole-payload compression**: The entire block stream (everything after the
   8-byte header) is compressed as a single zstd frame. The `COMPRESSED` bit
   (bit 0) in the file header's `flags` field indicates this. The decoder
   decompresses the stream before parsing block frames.

Zstd is chosen for its excellent ratio-to-speed tradeoff (RFC §4.6). For the
PoC, we use the `zstd` crate which wraps the reference C implementation.

---

## Requirements

### 1. Per-Block Compression (Encoder Side)

```rust
impl LcpEncoder {
    /// Enable per-block compression for the last added block.
    ///
    /// When set, the block body will be compressed with zstd before
    /// writing to the payload. The COMPRESSED flag (bit 1) will be
    /// set in the block's flags byte.
    ///
    /// Compression is skipped if the compressed body is not smaller
    /// than the uncompressed body (compression is never harmful).
    pub fn with_compression(&mut self) -> &mut Self { /* ... */ }

    /// Enable per-block compression for all blocks in this encoder.
    ///
    /// Each block's body is independently compressed. Blocks where
    /// compression does not reduce size are stored uncompressed.
    pub fn compress_blocks(&mut self) -> &mut Self { /* ... */ }
}
```

### 2. Whole-Payload Compression (Encoder Side)

```rust
impl LcpEncoder {
    /// Enable whole-payload compression.
    ///
    /// The entire block stream (all block frames + END sentinel)
    /// is compressed as a single zstd frame after serialization.
    /// The COMPRESSED flag (bit 0) is set in the file header.
    ///
    /// This is mutually exclusive with per-block compression:
    /// if both are requested, whole-payload compression wins and
    /// per-block compression flags are cleared.
    pub fn compress_payload(&mut self) -> &mut Self { /* ... */ }
}
```

### 3. Compression Wrapper

```rust
/// Zstd compression utilities for LCP.
///
/// Default compression level: 3 (good balance of speed and ratio).
/// The PoC does not use custom dictionaries (future optimization per
/// RFC §9.3).
pub mod compression {
    /// Compress a byte slice with zstd.
    ///
    /// Returns the compressed bytes. If compression does not reduce
    /// size, returns None (caller should store uncompressed).
    pub fn compress(data: &[u8]) -> Option<Vec<u8>> {
        let compressed = zstd::encode_all(data, 3).ok()?;
        if compressed.len() < data.len() {
            Some(compressed)
        } else {
            None
        }
    }

    /// Decompress a zstd-compressed byte slice.
    ///
    /// The `max_size` parameter provides an upper bound on the
    /// decompressed output to prevent decompression bombs.
    pub fn decompress(data: &[u8], max_size: usize) -> Result<Vec<u8>, CompressionError> {
        // Implementation
    }
}
```

### 4. Per-Block Decompression (Decoder Side)

```rust
/// During block frame reading, the decoder checks the COMPRESSED
/// flag (bit 1) in block_flags. If set, the body bytes are
/// decompressed before being passed to the block's decode_body.
///
/// Decompression flow:
///   1. Read block_type, block_flags, content_len
///   2. Read content_len bytes (compressed body)
///   3. If flags.is_compressed():
///      a. Decompress body bytes with zstd
///      b. Use decompressed bytes for decode_body
///   4. Else: use raw bytes for decode_body
```

### 5. Whole-Payload Decompression (Decoder Side)

```rust
/// During payload decoding, the decoder checks the COMPRESSED
/// flag (bit 0) in the file header's flags. If set, all bytes
/// after the 8-byte header are decompressed as a single zstd
/// frame before block frame parsing begins.
///
/// Decompression flow:
///   1. Parse 8-byte header
///   2. If header.flags.is_compressed():
///      a. Decompress remaining bytes with zstd
///      b. Parse block frames from decompressed bytes
///   3. Else: parse block frames from raw bytes
```

### 6. Compression Threshold

```rust
/// Minimum block body size (in bytes) before per-block compression
/// is attempted. Blocks smaller than this threshold are always
/// stored uncompressed, as zstd overhead outweighs savings for
/// very small inputs.
///
/// Default: 256 bytes.
pub const COMPRESSION_THRESHOLD: usize = 256;
```

### 7. Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum CompressionError {
    #[error("zstd compression failed: {0}")]
    CompressFailed(String),

    #[error("zstd decompression failed: {0}")]
    DecompressFailed(String),

    #[error("decompressed size {actual} exceeds limit {limit}")]
    DecompressionBomb { actual: usize, limit: usize },
}
```

---

## File Structure

Changes to existing crates:

```
crates/lcp-encoder/src/compression.rs   # Full implementation (was stub)
crates/lcp-decoder/src/decompression.rs # Full implementation (was stub)
```

---

## Acceptance Criteria

- [ ] Per-block compressed payload decodes to identical blocks as uncompressed
- [ ] Whole-payload compressed payload decodes to identical blocks as uncompressed
- [ ] Blocks under `COMPRESSION_THRESHOLD` bytes are not compressed
- [ ] Compression that increases size is skipped (body stored uncompressed)
- [ ] Header `COMPRESSED` flag is set only for whole-payload compression
- [ ] Block `COMPRESSED` flag is set only for per-block compression
- [ ] Decompression bomb protection rejects outputs exceeding `max_size`
- [ ] Round-trip: encode(compressed) → decode → compare is identical to uncompressed
- [ ] Compression ratio is ≥20% on a 50-line Rust source file block

---

## Verification

```bash
cargo test -p lcp-encoder -p lcp-decoder -- compression
cargo clippy -p lcp-encoder -p lcp-decoder -- -W clippy::pedantic

# Compression ratio benchmark
cargo test -p lcp-encoder -- compression_ratio --nocapture
```
