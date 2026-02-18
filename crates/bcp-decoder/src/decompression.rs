use std::io::Cursor;

use crate::error::DecodeError;

/// Maximum decompressed size for a single block body (16 MiB).
///
/// Matches the encoder's `MAX_BLOCK_BODY_SIZE`. If a per-block
/// decompression exceeds this limit, the decoder returns
/// [`DecodeError::DecompressionBomb`].
pub const MAX_BLOCK_DECOMPRESSED_SIZE: usize = 16 * 1024 * 1024;

/// Maximum decompressed size for whole-payload decompression (256 MiB).
///
/// Whole payloads can contain many blocks, so the limit is higher
/// than the per-block limit. If the decompressed payload exceeds
/// this, the decoder returns [`DecodeError::DecompressionBomb`].
pub const MAX_PAYLOAD_DECOMPRESSED_SIZE: usize = 256 * 1024 * 1024;

/// Decompress a zstd-compressed byte slice with a safety limit.
///
/// Returns the decompressed bytes, or an error if:
/// - The input is not valid zstd data ([`DecodeError::DecompressFailed`]).
/// - The decompressed output exceeds `max_size`
///   ([`DecodeError::DecompressionBomb`]).
///
/// # Arguments
///
/// - `data` — the zstd-compressed input bytes.
/// - `max_size` — upper bound on decompressed output (bomb protection).
pub fn decompress(data: &[u8], max_size: usize) -> Result<Vec<u8>, DecodeError> {
    let decompressed = zstd::decode_all(Cursor::new(data))
        .map_err(|e| DecodeError::DecompressFailed(e.to_string()))?;
    if decompressed.len() > max_size {
        return Err(DecodeError::DecompressionBomb {
            actual: decompressed.len(),
            limit: max_size,
        });
    }
    Ok(decompressed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compress_test_data(data: &[u8]) -> Vec<u8> {
        zstd::encode_all(Cursor::new(data), 3).unwrap()
    }

    #[test]
    fn decompress_roundtrip() {
        let original = "fn main() { println!(\"hello\"); }\n".repeat(50);
        let compressed = compress_test_data(original.as_bytes());
        let result = decompress(&compressed, MAX_BLOCK_DECOMPRESSED_SIZE).unwrap();
        assert_eq!(result, original.as_bytes());
    }

    #[test]
    fn decompress_rejects_invalid_data() {
        let garbage = b"this is not zstd data";
        let result = decompress(garbage, MAX_BLOCK_DECOMPRESSED_SIZE);
        assert!(matches!(result, Err(DecodeError::DecompressFailed(_))));
    }

    #[test]
    fn decompress_rejects_bomb() {
        let data = "x".repeat(10_000);
        let compressed = compress_test_data(data.as_bytes());
        let result = decompress(&compressed, 100);
        assert!(matches!(result, Err(DecodeError::DecompressionBomb { .. })));
    }

    #[test]
    fn constants_are_correct() {
        assert_eq!(MAX_BLOCK_DECOMPRESSED_SIZE, 16 * 1024 * 1024);
        assert_eq!(MAX_PAYLOAD_DECOMPRESSED_SIZE, 256 * 1024 * 1024);
    }
}
