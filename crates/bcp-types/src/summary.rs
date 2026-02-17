use bcp_wire::varint::{decode_varint, encode_varint};

use crate::error::TypeError;

/// Summary sub-block — a compact UTF-8 description prefixed to the body
/// when the block's `HAS_SUMMARY` flag is set.
///
/// The summary is designed for token-budget-aware rendering: when a block
/// is too large to include in full, the renderer can substitute the summary
/// to preserve context without blowing the token budget.
///
/// Wire layout (within the block body, before any TLV fields):
///
/// ```text
/// ┌─────────────────────────────────────────────────────┐
/// │ summary_len  (varint)         — byte length of text │
/// │ summary_text [summary_len]    — UTF-8 bytes         │
/// │ ... remaining TLV fields ...                        │
/// └─────────────────────────────────────────────────────┘
/// ```
///
/// The summary is always the first thing in the body when present. The
/// decoder checks `BlockFlags::has_summary()` before attempting to read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub text: String,
}

/// Maximum varint size in bytes, used for stack-allocated scratch buffers.
const MAX_VARINT_LEN: usize = 10;

impl Summary {
    /// Encode this summary into the front of a body buffer.
    ///
    /// Writes `text.len()` as a varint followed by the raw UTF-8 bytes.
    /// Call this before appending the block's TLV fields.
    pub fn encode(&self, buf: &mut Vec<u8>) {
        let mut scratch = [0u8; MAX_VARINT_LEN];
        let n = encode_varint(self.text.len() as u64, &mut scratch);
        buf.extend_from_slice(&scratch[..n]);
        buf.extend_from_slice(self.text.as_bytes());
    }

    /// Decode a summary from the front of a body buffer.
    ///
    /// Returns `(summary, bytes_consumed)`. The caller should slice the
    /// body past `bytes_consumed` before decoding TLV fields.
    pub fn decode(buf: &[u8]) -> Result<(Self, usize), TypeError> {
        let (len, n) = decode_varint(buf)?;
        let len = len as usize;
        let text_bytes = buf
            .get(n..n + len)
            .ok_or(bcp_wire::WireError::UnexpectedEof { offset: n })?;
        let text = String::from_utf8_lossy(text_bytes).into_owned();
        Ok((Self { text }, n + len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_summary() {
        let summary = Summary {
            text: "This block contains the main entry point.".to_string(),
        };
        let mut buf = Vec::new();
        summary.encode(&mut buf);

        let (decoded, consumed) = Summary::decode(&buf).unwrap();
        assert_eq!(decoded, summary);
        assert_eq!(consumed, buf.len());
    }

    #[test]
    fn roundtrip_empty_summary() {
        let summary = Summary {
            text: String::new(),
        };
        let mut buf = Vec::new();
        summary.encode(&mut buf);

        let (decoded, consumed) = Summary::decode(&buf).unwrap();
        assert_eq!(decoded, summary);
        assert_eq!(consumed, buf.len());
    }

    #[test]
    fn summary_followed_by_other_data() {
        let summary = Summary {
            text: "short".to_string(),
        };
        let mut buf = Vec::new();
        summary.encode(&mut buf);
        buf.extend_from_slice(b"remaining TLV data");

        let (decoded, consumed) = Summary::decode(&buf).unwrap();
        assert_eq!(decoded.text, "short");
        assert_eq!(&buf[consumed..], b"remaining TLV data");
    }
}
