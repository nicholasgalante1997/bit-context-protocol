/// Maximum number of bytes a u64 varint can occupy.
/// ceil(64 / 7) = 10 bytes.
const MAX_VARINT_BYTES: usize = 10;

/// Encode a `u64` value as an unsigned LEB128 varint into the provided buffer.
///
/// # Returns
///
/// The number of bytes written (1–10).
///
/// # Panics
///
/// Panics if `buf` is shorter than the required encoding length.
/// A 10-byte buffer is always sufficient for any `u64`.
///
/// # Wire format examples
///
/// | Value   | Encoded bytes        | Length |
/// |---------|----------------------|--------|
/// | 0       | `[0x00]`             | 1      |
/// | 1       | `[0x01]`             | 1      |
/// | 127     | `[0x7F]`             | 1      |
/// | 128     | `[0x80, 0x01]`       | 2      |
/// | 300     | `[0xAC, 0x02]`       | 2      |
/// | 16383   | `[0xFF, 0x7F]`       | 2      |
/// | 16384   | `[0x80, 0x80, 0x01]` | 3      |
pub fn encode_varint(mut value: u64, buf: &mut [u8]) -> usize {
    let mut i = 0;
    loop {
        // Take the lowest 7 bits
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;

        if value > 0 {
            // More bytes to come: set the continuation bit
            byte |= 0x80;
        }

        buf[i] = byte;
        i += 1;

        if value == 0 {
            break;
        }
    }
    i
}

use crate::error::WireError;

/// Decode an unsigned LEB128 varint from the provided byte slice.
///
/// # Returns
///
/// `(decoded_value, bytes_consumed)` on success.
///
/// # Errors
///
/// - [`WireError::VarintTooLong`] if more than 10 bytes are consumed
///   without finding a terminating byte.
/// - [`WireError::UnexpectedEof`] if the slice ends mid-varint.
pub fn decode_varint(buf: &[u8]) -> Result<(u64, usize), WireError> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;

    for (i, &byte) in buf.iter().enumerate() {
        if i >= MAX_VARINT_BYTES {
            return Err(WireError::VarintTooLong);
        }

        // Extract the 7 data bits and shift them into position
        let data = u64::from(byte & 0x7F);
        result |= data << shift;
        shift += 7;

        // If MSB is clear, this is the last byte
        if byte & 0x80 == 0 {
            return Ok((result, i + 1));
        }
    }

    // We ran out of input bytes while MSB was still set
    Err(WireError::UnexpectedEof { offset: buf.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: encode a value and return just the used bytes
    fn encode(value: u64) -> Vec<u8> {
        let mut buf = [0u8; MAX_VARINT_BYTES];
        let len = encode_varint(value, &mut buf);
        buf[..len].to_vec()
    }

    #[test]
    fn encode_zero() {
        assert_eq!(encode(0), vec![0x00]);
    }

    #[test]
    fn encode_one() {
        assert_eq!(encode(1), vec![0x01]);
    }

    #[test]
    fn encode_127() {
        // Largest single-byte value (7 bits all set)
        assert_eq!(encode(127), vec![0x7F]);
    }

    #[test]
    fn encode_128() {
        // First value requiring 2 bytes
        assert_eq!(encode(128), vec![0x80, 0x01]);
    }

    #[test]
    fn encode_300() {
        // The protobuf spec example value
        assert_eq!(encode(300), vec![0xAC, 0x02]);
    }

    #[test]
    fn encode_16383() {
        // Largest 2-byte value (14 bits all set)
        assert_eq!(encode(16383), vec![0xFF, 0x7F]);
    }

    #[test]
    fn encode_16384() {
        // First 3-byte value
        assert_eq!(encode(16384), vec![0x80, 0x80, 0x01]);
    }

    #[test]
    fn encode_u32_max() {
        let bytes = encode(u64::from(u32::MAX));
        assert_eq!(bytes.len(), 5);
    }

    #[test]
    fn encode_u64_max() {
        let bytes = encode(u64::MAX);
        assert_eq!(bytes.len(), MAX_VARINT_BYTES);
    }

    #[test]
    fn roundtrip_boundary_values() {
        let values = [
            0,
            1,
            127,
            128,
            255,
            256,
            16383,
            16384,
            u64::from(u32::MAX),
            u64::MAX,
        ];
        for &value in &values {
            let encoded = encode(value);
            let (decoded, consumed) = decode_varint(&encoded).unwrap();
            assert_eq!(decoded, value, "roundtrip failed for {value}");
            assert_eq!(consumed, encoded.len());
        }
    }

    #[test]
    fn decode_with_trailing_bytes() {
        // Decoder should only consume the varint, leaving trailing data alone
        let buf = [0xAC, 0x02, 0xFF, 0xFF];
        let (value, consumed) = decode_varint(&buf).unwrap();
        assert_eq!(value, 300);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn decode_empty_input() {
        let result = decode_varint(&[]);
        assert!(matches!(
            result,
            Err(WireError::UnexpectedEof { offset: 0 })
        ));
    }

    #[test]
    fn decode_truncated_varint() {
        // 0x80 has continuation bit set but there's no next byte
        let result = decode_varint(&[0x80]);
        assert!(matches!(result, Err(WireError::UnexpectedEof { .. })));
    }

    #[test]
    fn decode_too_long() {
        // 11 bytes all with continuation bit set
        let buf = [0x80; 11];
        let result = decode_varint(&buf);
        assert!(matches!(result, Err(WireError::VarintTooLong)));
    }
}
