/// Semantic block type identifiers.
///
/// Each variant maps to the wire byte value defined in RFC §4.4 and
/// mirrored by the `bcp_wire::block_frame::block_type` constants.
/// Unknown values are captured by `Unknown(u8)` for forward compatibility —
/// a newer encoder may produce block types this version doesn't recognize,
/// and we preserve them rather than discarding.
///
/// ```text
/// ┌──────┬──────────────────┬──────────────────────────────────┐
/// │ Wire │ Variant          │ Description                      │
/// ├──────┼──────────────────┼──────────────────────────────────┤
/// │ 0x01 │ Code             │ Source code with language/path    │
/// │ 0x02 │ Conversation     │ Chat turn with role              │
/// │ 0x03 │ FileTree         │ Directory structure              │
/// │ 0x04 │ ToolResult       │ Tool/MCP output                  │
/// │ 0x05 │ Document         │ Prose/markdown content           │
/// │ 0x06 │ StructuredData   │ JSON/YAML/TOML/CSV data          │
/// │ 0x07 │ Diff             │ Code changes with hunks          │
/// │ 0x08 │ Annotation       │ Metadata overlay                 │
/// │ 0x09 │ EmbeddingRef     │ Vector reference                 │
/// │ 0x0A │ Image            │ Image reference or embed         │
/// │ 0xFE │ Extension        │ User-defined block               │
/// │ 0xFF │ End              │ End-of-stream sentinel           │
/// └──────┴──────────────────┴──────────────────────────────────┘
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockType {
    Code,
    Conversation,
    FileTree,
    ToolResult,
    Document,
    StructuredData,
    Diff,
    Annotation,
    EmbeddingRef,
    Image,
    Extension,
    End,
    /// Forward-compatible catch-all for block types this version
    /// doesn't recognize. The raw wire byte is preserved so it can
    /// be re-encoded without loss.
    Unknown(u8),
}

impl BlockType {
    /// Return the single-byte wire ID for this block type.
    ///
    /// For known variants this is the constant from RFC §4.4.
    /// For `Unknown(id)`, returns the captured byte as-is.
    pub fn wire_id(&self) -> u8 {
        match self {
            Self::Code => 0x01,
            Self::Conversation => 0x02,
            Self::FileTree => 0x03,
            Self::ToolResult => 0x04,
            Self::Document => 0x05,
            Self::StructuredData => 0x06,
            Self::Diff => 0x07,
            Self::Annotation => 0x08,
            Self::EmbeddingRef => 0x09,
            Self::Image => 0x0A,
            Self::Extension => 0xFE,
            Self::End => 0xFF,
            Self::Unknown(id) => *id,
        }
    }

    /// Parse a wire byte into a [`BlockType`].
    ///
    /// Known values map to their named variant. Anything else becomes
    /// `Unknown(id)`, preserving the raw value for round-tripping.
    pub fn from_wire_id(id: u8) -> Self {
        match id {
            0x01 => Self::Code,
            0x02 => Self::Conversation,
            0x03 => Self::FileTree,
            0x04 => Self::ToolResult,
            0x05 => Self::Document,
            0x06 => Self::StructuredData,
            0x07 => Self::Diff,
            0x08 => Self::Annotation,
            0x09 => Self::EmbeddingRef,
            0x0A => Self::Image,
            0xFE => Self::Extension,
            0xFF => Self::End,
            other => Self::Unknown(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_known_variants_roundtrip() {
        let variants = [
            (BlockType::Code, 0x01),
            (BlockType::Conversation, 0x02),
            (BlockType::FileTree, 0x03),
            (BlockType::ToolResult, 0x04),
            (BlockType::Document, 0x05),
            (BlockType::StructuredData, 0x06),
            (BlockType::Diff, 0x07),
            (BlockType::Annotation, 0x08),
            (BlockType::EmbeddingRef, 0x09),
            (BlockType::Image, 0x0A),
            (BlockType::Extension, 0xFE),
            (BlockType::End, 0xFF),
        ];

        for (variant, wire) in variants {
            assert_eq!(variant.wire_id(), wire, "wire_id mismatch for {variant:?}");
            assert_eq!(
                BlockType::from_wire_id(wire),
                variant,
                "from_wire_id mismatch for {wire:#04X}"
            );
        }
    }

    #[test]
    fn unknown_value_preserved() {
        let unknown = BlockType::from_wire_id(0x42);
        assert_eq!(unknown, BlockType::Unknown(0x42));
        assert_eq!(unknown.wire_id(), 0x42);
    }

    #[test]
    fn unknown_zero_preserved() {
        let unknown = BlockType::from_wire_id(0x00);
        assert_eq!(unknown, BlockType::Unknown(0x00));
        assert_eq!(unknown.wire_id(), 0x00);
    }
}
