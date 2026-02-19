use crate::error::WireError;
use crate::varint::{decode_varint, encode_varint};

/// Per-block flags bitfield.
///
/// Bit layout:
///   bit 0 = has summary sub-block appended after the body
///   bit 1 = body is compressed with zstd
///   bit 2 = body is a BLAKE3 hash reference, not inline data
///   bits 3-7 = reserved
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockFlags(u8);

impl BlockFlags {
    pub const NONE: Self = Self(0);
    pub const HAS_SUMMARY: Self = Self(0b0000_0001);
    pub const COMPRESSED: Self = Self(0b0000_0010);
    pub const IS_REFERENCE: Self = Self(0b0000_0100);

    pub fn from_raw(raw: u8) -> Self {
        Self(raw)
    }

    pub fn raw(self) -> u8 {
        self.0
    }

    pub fn has_summary(self) -> bool {
        self.0 & Self::HAS_SUMMARY.0 != 0
    }

    pub fn is_compressed(self) -> bool {
        self.0 & Self::COMPRESSED.0 != 0
    }

    pub fn is_reference(self) -> bool {
        self.0 & Self::IS_REFERENCE.0 != 0
    }
}

/// Known block type IDs.
///
/// These are the semantic type tags that appear on the wire.
/// The `bcp-types` crate defines the full typed structs for each.
pub mod block_type {
    pub const CODE: u8 = 0x01;
    pub const CONVERSATION: u8 = 0x02;
    pub const FILE_TREE: u8 = 0x03;
    pub const TOOL_RESULT: u8 = 0x04;
    pub const DOCUMENT: u8 = 0x05;
    pub const STRUCTURED_DATA: u8 = 0x06;
    pub const DIFF: u8 = 0x07;
    pub const ANNOTATION: u8 = 0x08;
    pub const EMBEDDING_REF: u8 = 0x09;
    pub const IMAGE: u8 = 0x0A;
    pub const EXTENSION: u8 = 0xFE;
    pub const END: u8 = 0xFF;
}

/// Block frame — the wire envelope wrapping every block's body.
///
/// ```text
/// ┌──────────────────────────────────────────────────┐
/// │ block_type   (varint, 1-2 bytes typically)       │
/// │ block_flags  (uint8, 1 byte)                     │
/// │ content_len  (varint, 1-5 bytes typically)       │
/// │ body         [content_len bytes]                  │
/// └──────────────────────────────────────────────────┘
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockFrame {
    /// The semantic block type (CODE=0x01, CONVERSATION=0x02, etc.).
    pub block_type: u8,

    /// Per-block flags.
    pub flags: BlockFlags,

    /// The raw body bytes (content_len bytes from the wire).
    pub body: Vec<u8>,
}

/// Maximum varint size in bytes, used for buffer sizing.
const MAX_VARINT_LEN: usize = 10;

impl BlockFrame {
    /// Write this block frame to the provided writer.
    ///
    /// Wire layout written:
    ///   1. block_type as varint
    ///   2. block_flags as single u8
    ///   3. body.len() as varint (content_len)
    ///   4. body bytes
    ///
    /// # Returns
    ///
    /// Total number of bytes written.
    pub fn write_to(&self, w: &mut impl std::io::Write) -> Result<usize, WireError> {
        let mut bytes_written = 0;
        let mut varint_buf = [0u8; MAX_VARINT_LEN];

        // 1. Block type as varint
        let n = encode_varint(u64::from(self.block_type), &mut varint_buf);
        w.write_all(&varint_buf[..n])?;
        bytes_written += n;

        // 2. Flags as a single raw byte
        w.write_all(&[self.flags.raw()])?;
        bytes_written += 1;

        // 3. Content length as varint
        let n = encode_varint(self.body.len() as u64, &mut varint_buf);
        w.write_all(&varint_buf[..n])?;
        bytes_written += n;

        // 4. Body bytes
        w.write_all(&self.body)?;
        bytes_written += self.body.len();

        Ok(bytes_written)
    }

    /// Read a block frame from the provided byte slice.
    ///
    /// # Returns
    ///
    /// `Some((frame, bytes_consumed))` for normal blocks, or
    /// `None` if the block type is END (0xFF), signaling the
    /// end of the block stream.
    ///
    /// # Errors
    ///
    /// - [`WireError::UnexpectedEof`] if the slice is too short.
    /// - [`WireError::VarintTooLong`] if a varint is malformed.
    pub fn read_from(buf: &[u8]) -> Result<Option<(Self, usize)>, WireError> {
        let mut cursor = 0;

        // 1. Block type (varint)
        let (block_type_raw, n) = decode_varint(
            buf.get(cursor..)
                .ok_or(WireError::UnexpectedEof { offset: cursor })?,
        )?;
        cursor += n;

        let block_type = block_type_raw as u8;

        // END sentinel: signal that the stream is done
        if block_type == block_type::END {
            return Ok(None);
        }

        // 2. Flags (single byte)
        let flags_byte = *buf
            .get(cursor)
            .ok_or(WireError::UnexpectedEof { offset: cursor })?;
        cursor += 1;
        let flags = BlockFlags::from_raw(flags_byte);

        // 3. Content length (varint)
        let (content_len, n) = decode_varint(
            buf.get(cursor..)
                .ok_or(WireError::UnexpectedEof { offset: cursor })?,
        )?;
        cursor += n;

        // Check for overflow before converting to usize
        let content_len_usize = usize::try_from(content_len).map_err(|_| {
            WireError::UnexpectedEof {
                offset: cursor, // position after content_len varint
            }
        })?;

        // 4. Body bytes
        let body_end = match cursor.checked_add(content_len_usize) {
            Some(end) => end,
            None => {
                return Err(WireError::UnexpectedEof {
                    offset: buf.len(),
                })
            }
        };
        if buf.len() < body_end {
            return Err(WireError::UnexpectedEof { offset: buf.len() });
        }
        let body = buf[cursor..body_end].to_vec();
        cursor = body_end;

        Ok(Some((
            Self {
                block_type,
                flags,
                body,
            },
            cursor,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: write a frame to a Vec and return the bytes.
    fn write_frame(frame: &BlockFrame) -> Vec<u8> {
        let mut buf = Vec::new();
        frame.write_to(&mut buf).unwrap();
        buf
    }

    #[test]
    fn roundtrip_code_block() {
        let frame = BlockFrame {
            block_type: block_type::CODE,
            flags: BlockFlags::NONE,
            body: b"fn main() {}".to_vec(),
        };
        let bytes = write_frame(&frame);
        let (parsed, consumed) = BlockFrame::read_from(&bytes).unwrap().unwrap();
        assert_eq!(parsed, frame);
        assert_eq!(consumed, bytes.len());
    }

    #[test]
    fn roundtrip_with_flags() {
        let frame = BlockFrame {
            block_type: block_type::TOOL_RESULT,
            flags: BlockFlags::from_raw(
                BlockFlags::HAS_SUMMARY.raw() | BlockFlags::COMPRESSED.raw(),
            ),
            body: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };
        let bytes = write_frame(&frame);
        let (parsed, _) = BlockFrame::read_from(&bytes).unwrap().unwrap();
        assert!(parsed.flags.has_summary());
        assert!(parsed.flags.is_compressed());
        assert!(!parsed.flags.is_reference());
        assert_eq!(parsed.body, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn roundtrip_empty_body() {
        let frame = BlockFrame {
            block_type: block_type::ANNOTATION,
            flags: BlockFlags::NONE,
            body: vec![],
        };
        let bytes = write_frame(&frame);
        let (parsed, consumed) = BlockFrame::read_from(&bytes).unwrap().unwrap();
        assert_eq!(parsed, frame);
        assert_eq!(consumed, bytes.len());
    }

    #[test]
    fn roundtrip_large_body() {
        // 10KB body to test multi-byte content_len varint
        let frame = BlockFrame {
            block_type: block_type::CODE,
            flags: BlockFlags::NONE,
            body: vec![0xAB; 10_000],
        };
        let bytes = write_frame(&frame);
        let (parsed, consumed) = BlockFrame::read_from(&bytes).unwrap().unwrap();
        assert_eq!(parsed.body.len(), 10_000);
        assert_eq!(consumed, bytes.len());
    }

    #[test]
    fn end_block_returns_none() {
        // Manually write an END block: type=0xFF
        let mut buf = Vec::new();
        let mut varint_buf = [0u8; 10];
        let n = encode_varint(u64::from(block_type::END), &mut varint_buf);
        buf.extend_from_slice(&varint_buf[..n]);

        let result = BlockFrame::read_from(&buf).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn read_truncated_body() {
        // Write a frame claiming 100 bytes of body but only provide 5
        let frame = BlockFrame {
            block_type: block_type::CODE,
            flags: BlockFlags::NONE,
            body: vec![0xFF; 100],
        };
        let full_bytes = write_frame(&frame);

        // Chop it short: keep the header but only 5 bytes of body
        let truncated = &full_bytes[..full_bytes.len() - 95];
        let result = BlockFrame::read_from(truncated);
        assert!(matches!(result, Err(WireError::UnexpectedEof { .. })));
    }

    #[test]
    fn multiple_frames_sequential() {
        // Write two frames back-to-back, then read them both
        let frame1 = BlockFrame {
            block_type: block_type::CODE,
            flags: BlockFlags::NONE,
            body: b"first".to_vec(),
        };
        let frame2 = BlockFrame {
            block_type: block_type::CONVERSATION,
            flags: BlockFlags::NONE,
            body: b"second".to_vec(),
        };

        let mut buf = Vec::new();
        frame1.write_to(&mut buf).unwrap();
        frame2.write_to(&mut buf).unwrap();

        // Read first frame
        let (parsed1, consumed1) = BlockFrame::read_from(&buf).unwrap().unwrap();
        assert_eq!(parsed1, frame1);

        // Read second frame starting where the first ended
        let (parsed2, consumed2) = BlockFrame::read_from(&buf[consumed1..]).unwrap().unwrap();
        assert_eq!(parsed2, frame2);
        assert_eq!(consumed1 + consumed2, buf.len());
    }

    #[test]
    fn all_block_types_roundtrip() {
        let types = [
            block_type::CODE,
            block_type::CONVERSATION,
            block_type::FILE_TREE,
            block_type::TOOL_RESULT,
            block_type::DOCUMENT,
            block_type::STRUCTURED_DATA,
            block_type::DIFF,
            block_type::ANNOTATION,
            block_type::EMBEDDING_REF,
            block_type::IMAGE,
            block_type::EXTENSION,
        ];
        for &bt in &types {
            let frame = BlockFrame {
                block_type: bt,
                flags: BlockFlags::NONE,
                body: vec![bt], // body is just the type byte for identification
            };
            let bytes = write_frame(&frame);
            let (parsed, _) = BlockFrame::read_from(&bytes).unwrap().unwrap();
            assert_eq!(parsed.block_type, bt, "failed for block type {bt:#04X}");
            assert_eq!(parsed.body, vec![bt]);
        }
    }
}
