use crate::error::TypeError;

/// END sentinel — marks the end of the block stream.
///
/// The END block has no fields and no body. Its presence on the wire is
/// signaled solely by the block type byte (0xFF) in the block frame
/// header. The `bcp_wire::BlockFrame::read_from` method returns `None`
/// when it encounters an END block, terminating the read loop.
///
/// Unlike other block types that carry TLV-encoded fields, `EndBlock`
/// is a zero-field struct. Its `encode_body` always returns an empty
/// vec and its `decode_body` succeeds only on empty input.
///
/// Wire layout:
///
/// ```text
/// ┌──────────────────────────────────────┐
/// │ block_type = 0xFF (varint, 2 bytes)  │
/// │ (no flags, no content_len, no body)  │
/// └──────────────────────────────────────┘
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndBlock;

impl EndBlock {
    /// Serialize the END block body — always empty.
    pub fn encode_body(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Deserialize an END block from a body buffer.
    ///
    /// Succeeds on empty input. Non-empty input is accepted but ignored,
    /// since a future spec revision could attach trailing metadata to
    /// the END sentinel.
    pub fn decode_body(_buf: &[u8]) -> Result<Self, TypeError> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_produces_empty_body() {
        let block = EndBlock;
        assert!(block.encode_body().is_empty());
    }

    #[test]
    fn decode_empty_body() {
        let block = EndBlock::decode_body(&[]).unwrap();
        assert_eq!(block, EndBlock);
    }

    #[test]
    fn decode_ignores_trailing_bytes() {
        let block = EndBlock::decode_body(&[0xFF, 0x00]).unwrap();
        assert_eq!(block, EndBlock);
    }
}
