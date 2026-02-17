use std::io::Cursor;

use crate::error::CompressionError;

/// Minimum block body size (in bytes) before per-block compression
/// is attempted.
///
/// Blocks smaller than this threshold are always stored uncompressed
/// because zstd framing overhead (~13 bytes for the frame header)
/// outweighs any savings on very small inputs.
///
/// Default: 256 bytes.
pub const COMPRESSION_THRESHOLD: usize = 256;

/// Default zstd compression level (1–22 scale).
///
/// Level 3 provides a good balance of speed and compression ratio
/// for typical code/text context blocks (RFC §4.6). Higher levels
/// yield diminishing returns for the latency cost.
const DEFAULT_COMPRESSION_LEVEL: i32 = 3;

/// Compress a byte slice with zstd.
///
/// Returns `Some(compressed)` if compression reduced the size, or
/// `None` if the compressed output is >= the input size. This
/// ensures compression is never harmful — the caller should store
/// the block uncompressed when `None` is returned.
///
/// Uses the default compression level (3).
///
/// # Example
///
/// ```rust
/// use bcp_encoder::compression::compress;
///
/// let data = "fn main() { }\n".repeat(100);
/// match compress(data.as_bytes()) {
///     Some(compressed) => assert!(compressed.len() < data.len()),
///     None => { /* data was incompressible */ }
/// }
/// ```
pub fn compress(data: &[u8]) -> Option<Vec<u8>> {
    let compressed = zstd::encode_all(Cursor::new(data), DEFAULT_COMPRESSION_LEVEL).ok()?;
    if compressed.len() < data.len() {
        Some(compressed)
    } else {
        None
    }
}

/// Decompress a zstd-compressed byte slice.
///
/// The `max_size` parameter provides an upper bound on the
/// decompressed output to prevent decompression bombs — if the
/// decompressed data exceeds this limit, an error is returned
/// without completing decompression.
///
/// # Errors
///
/// - [`CompressionError::DecompressFailed`] if zstd cannot decode
///   the input (invalid frame, truncated data, etc.).
/// - [`CompressionError::DecompressionBomb`] if the decompressed
///   size exceeds `max_size`.
pub fn decompress(data: &[u8], max_size: usize) -> Result<Vec<u8>, CompressionError> {
    let decompressed = zstd::decode_all(Cursor::new(data))
        .map_err(|e| CompressionError::DecompressFailed(e.to_string()))?;
    if decompressed.len() > max_size {
        return Err(CompressionError::DecompressionBomb {
            actual: decompressed.len(),
            limit: max_size,
        });
    }
    Ok(decompressed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_returns_none_for_small_incompressible_data() {
        let data = b"abc123";
        assert!(compress(data).is_none());
    }

    #[test]
    fn compress_reduces_repetitive_data() {
        let data = "fn main() { }\n".repeat(100);
        let result = compress(data.as_bytes());
        assert!(result.is_some());
        let compressed = result.unwrap();
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn compress_decompress_roundtrip() {
        let data = "pub fn hello() -> &'static str { \"world\" }\n".repeat(50);
        let compressed = compress(data.as_bytes()).expect("should compress");
        let decompressed =
            decompress(&compressed, 1024 * 1024).expect("should decompress");
        assert_eq!(decompressed, data.as_bytes());
    }

    #[test]
    fn decompress_rejects_bomb() {
        let data = "x".repeat(10_000);
        let compressed = compress(data.as_bytes()).expect("should compress");
        let result = decompress(&compressed, 100);
        assert!(matches!(
            result,
            Err(CompressionError::DecompressionBomb { .. })
        ));
    }

    #[test]
    fn decompress_rejects_invalid_data() {
        let garbage = b"this is not zstd data";
        let result = decompress(garbage, 1024 * 1024);
        assert!(matches!(
            result,
            Err(CompressionError::DecompressFailed(_))
        ));
    }

    #[test]
    fn compression_threshold_is_256() {
        assert_eq!(COMPRESSION_THRESHOLD, 256);
    }
}
