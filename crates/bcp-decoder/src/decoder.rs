use bcp_types::block::{Block, BlockContent};
use bcp_types::block_type::BlockType;
use bcp_types::content_store::ContentStore;
use bcp_types::summary::Summary;
use bcp_wire::block_frame::BlockFrame;
use bcp_wire::header::{HEADER_SIZE, BcpHeader};

use crate::decompression::{self, MAX_BLOCK_DECOMPRESSED_SIZE, MAX_PAYLOAD_DECOMPRESSED_SIZE};
use crate::error::DecodeError;

/// The result of decoding a BCP payload.
///
/// Contains the parsed file header and an ordered sequence of typed
/// blocks. The END sentinel is consumed during decoding and is not
/// included in the `blocks` vector.
///
/// ```text
/// ┌──────────────────────────────────────────────────┐
/// │ DecodedPayload                                   │
/// │   header: BcpHeader  ← version, flags            │
/// │   blocks: Vec<Block> ← ordered content blocks    │
/// └──────────────────────────────────────────────────┘
/// ```
pub struct DecodedPayload {
    /// The parsed file header (magic validated, version checked).
    pub header: BcpHeader,

    /// Ordered sequence of blocks, excluding the END sentinel.
    ///
    /// Block ordering matches the wire order. Annotation blocks
    /// appear at whatever position the encoder placed them, with
    /// `target_block_id` referencing earlier blocks by index.
    pub blocks: Vec<Block>,
}

/// Synchronous BCP decoder — parses a complete in-memory payload.
///
/// The decoder reads an entire BCP payload from a byte slice and
/// produces a [`DecodedPayload`] containing the header and all typed
/// blocks. It is the inverse of
/// `BcpEncoder::encode` from the `bcp-encoder` crate.
///
/// Decoding proceeds in four steps:
///
///   1. **Header**: Validate and parse the 8-byte file header (magic
///      number, version, flags, reserved byte).
///   2. **Whole-payload decompression**: If the header's `COMPRESSED`
///      flag (bit 0) is set, decompress all bytes after the header
///      with zstd before parsing block frames.
///   3. **Block frames**: Iterate block frames by reading `BlockFrame`
///      envelopes. For each frame:
///      - If `COMPRESSED` (bit 1): decompress the body with zstd.
///      - If `IS_REFERENCE` (bit 2): resolve the 32-byte BLAKE3 hash
///        against the content store to recover the original body.
///      - Extract the summary sub-block if `HAS_SUMMARY` (bit 0) is set.
///      - Deserialize the body into the corresponding `BlockContent`.
///   4. **Termination**: Stop when an END sentinel (type=0xFF) is
///      encountered. Detect and report trailing data after the sentinel.
///
/// Unknown block types are captured as `BlockContent::Unknown` and do
/// not cause errors — this is the forward compatibility guarantee from
/// RFC §3, P1 Schema Evolution.
///
/// # Example
///
/// ```rust
/// use bcp_encoder::BcpEncoder;
/// use bcp_decoder::BcpDecoder;
/// use bcp_types::enums::{Lang, Role};
///
/// let payload = BcpEncoder::new()
///     .add_code(Lang::Rust, "main.rs", b"fn main() {}")
///     .add_conversation(Role::User, b"hello")
///     .encode()
///     .unwrap();
///
/// let decoded = BcpDecoder::decode(&payload).unwrap();
/// assert_eq!(decoded.blocks.len(), 2);
/// ```
pub struct BcpDecoder;

impl BcpDecoder {
    /// Decode a complete BCP payload from a byte slice.
    ///
    /// This is the standard entry point for payloads that do not contain
    /// content-addressed (reference) blocks. If the payload contains
    /// blocks with the `IS_REFERENCE` flag, use
    /// [`decode_with_store`](Self::decode_with_store) instead.
    ///
    /// Handles whole-payload and per-block zstd decompression
    /// transparently.
    ///
    /// # Errors
    ///
    /// - [`DecodeError::InvalidHeader`] if the magic, version, or reserved
    ///   byte is wrong.
    /// - [`DecodeError::Wire`] if a block frame is malformed.
    /// - [`DecodeError::Type`] if a block body fails TLV deserialization.
    /// - [`DecodeError::DecompressFailed`] if zstd decompression fails.
    /// - [`DecodeError::DecompressionBomb`] if decompressed size exceeds
    ///   the safety limit.
    /// - [`DecodeError::MissingContentStore`] if a reference block is
    ///   encountered (use `decode_with_store` instead).
    /// - [`DecodeError::MissingEndSentinel`] if the payload ends without an
    ///   END block.
    /// - [`DecodeError::TrailingData`] if extra bytes follow the END
    ///   sentinel.
    pub fn decode(payload: &[u8]) -> Result<DecodedPayload, DecodeError> {
        Self::decode_inner(payload, None)
    }

    /// Decode a payload that may contain content-addressed blocks.
    ///
    /// Same as [`decode`](Self::decode), but accepts a [`ContentStore`]
    /// for resolving `IS_REFERENCE` blocks. When a block's body is a
    /// 32-byte BLAKE3 hash, the decoder looks it up in the store to
    /// retrieve the original body bytes.
    ///
    /// # Errors
    ///
    /// All errors from [`decode`](Self::decode), plus:
    /// - [`DecodeError::UnresolvedReference`] if a hash is not found in
    ///   the content store.
    pub fn decode_with_store(
        payload: &[u8],
        store: &dyn ContentStore,
    ) -> Result<DecodedPayload, DecodeError> {
        Self::decode_inner(payload, Some(store))
    }

    /// Shared decode implementation.
    fn decode_inner(
        payload: &[u8],
        store: Option<&dyn ContentStore>,
    ) -> Result<DecodedPayload, DecodeError> {
        // 1. Parse the 8-byte header.
        let header = BcpHeader::read_from(payload).map_err(DecodeError::InvalidHeader)?;

        // 2. Whole-payload decompression.
        let block_data: std::borrow::Cow<'_, [u8]> = if header.flags.is_compressed() {
            let compressed = &payload[HEADER_SIZE..];
            let decompressed =
                decompression::decompress(compressed, MAX_PAYLOAD_DECOMPRESSED_SIZE)?;
            std::borrow::Cow::Owned(decompressed)
        } else {
            std::borrow::Cow::Borrowed(&payload[HEADER_SIZE..])
        };

        let mut cursor = 0;
        let mut blocks = Vec::new();
        let mut found_end = false;

        // 3. Read block frames until END sentinel or EOF.
        while cursor < block_data.len() {
            let remaining = &block_data[cursor..];

            if let Some((frame, consumed)) = BlockFrame::read_from(remaining)? {
                let block = Self::decode_block_frame(&frame, store)?;
                blocks.push(block);
                cursor += consumed;
            } else {
                // END sentinel encountered. BlockFrame::read_from returns
                // None for type=0xFF. Account for the END frame bytes:
                // varint(0xFF) = [0xFF, 0x01] + flags(0x00) + content_len(0x00) = 4 bytes.
                // But we need to calculate the actual size consumed by the
                // END sentinel's varint encoding.
                found_end = true;
                cursor += Self::end_sentinel_size(remaining)?;
                break;
            }
        }

        // 4. Validate termination.
        if !found_end {
            return Err(DecodeError::MissingEndSentinel);
        }

        if cursor < block_data.len() {
            return Err(DecodeError::TrailingData {
                extra_bytes: block_data.len() - cursor,
            });
        }

        Ok(DecodedPayload { header, blocks })
    }

    /// Decode a single block from a `BlockFrame`.
    ///
    /// Processing pipeline:
    ///   1. If `IS_REFERENCE`: resolve the 32-byte hash via content store.
    ///   2. If `COMPRESSED`: decompress the body with zstd.
    ///   3. If `HAS_SUMMARY`: extract the summary from the front of the body.
    ///   4. Deserialize the TLV body into a `BlockContent` variant.
    fn decode_block_frame(
        frame: &BlockFrame,
        store: Option<&dyn ContentStore>,
    ) -> Result<Block, DecodeError> {
        let block_type = BlockType::from_wire_id(frame.block_type);

        // Stage 1: Resolve content-addressed references.
        let resolved_body = if frame.flags.is_reference() {
            let store = store.ok_or(DecodeError::MissingContentStore)?;
            if frame.body.len() != 32 {
                return Err(DecodeError::Wire(bcp_wire::WireError::UnexpectedEof {
                    offset: frame.body.len(),
                }));
            }
            let hash: [u8; 32] = frame.body[..32].try_into().expect("length already checked");
            store
                .get(&hash)
                .ok_or(DecodeError::UnresolvedReference { hash })?
        } else {
            frame.body.clone()
        };

        // Stage 2: Decompress if needed.
        let decompressed_body = if frame.flags.is_compressed() {
            decompression::decompress(&resolved_body, MAX_BLOCK_DECOMPRESSED_SIZE)?
        } else {
            resolved_body
        };

        // Stage 3 & 4: Summary extraction + TLV body decode.
        let mut body = decompressed_body.as_slice();
        let mut summary = None;

        if frame.flags.has_summary() {
            let (sum, consumed) = Summary::decode(body)?;
            summary = Some(sum);
            body = &body[consumed..];
        }

        let content = BlockContent::decode_body(&block_type, body)?;

        Ok(Block {
            block_type,
            flags: frame.flags,
            summary,
            content,
        })
    }

    /// Calculate the byte size of the END sentinel in the wire format.
    ///
    /// The END sentinel is:
    ///   - `block_type` = 0xFF, encoded as varint → `[0xFF, 0x01]` (2 bytes)
    ///   - `flags` = 0x00 (1 byte)
    ///   - `content_len` = 0, encoded as varint → `[0x00]` (1 byte)
    ///
    /// Total: 4 bytes. However, we compute this from the wire rather
    /// than hardcoding, in case future encoders use a different varint
    /// encoding width.
    fn end_sentinel_size(buf: &[u8]) -> Result<usize, DecodeError> {
        // Read the block_type varint (0xFF)
        let (_, type_len) = bcp_wire::varint::decode_varint(buf)?;
        let mut size = type_len;

        // flags byte
        size += 1;

        // content_len varint (should be 0)
        let flags_and_len = &buf[size..];
        if flags_and_len.is_empty() {
            return Err(DecodeError::Wire(bcp_wire::WireError::UnexpectedEof {
                offset: size,
            }));
        }
        let (_, len_size) = bcp_wire::varint::decode_varint(flags_and_len)?;
        size += len_size;

        Ok(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcp_encoder::BcpEncoder;
    use bcp_types::diff::DiffHunk;
    use bcp_types::enums::{
        AnnotationKind, DataFormat, FormatHint, Lang, MediaType, Priority, Role, Status,
    };
    use bcp_types::file_tree::{FileEntry, FileEntryKind};
    use bcp_wire::block_frame::{BlockFlags, BlockFrame};

    // ── Round-trip helpers ────────────────────────────────────────────────

    /// Encode with `BcpEncoder`, decode with `BcpDecoder`, return blocks.
    fn roundtrip(encoder: &BcpEncoder) -> DecodedPayload {
        let payload = encoder.encode().unwrap();
        BcpDecoder::decode(&payload).unwrap()
    }

    // ── Acceptance criteria tests ─────────────────────────────────────────

    #[test]
    fn decode_parses_encoder_output() {
        let payload = BcpEncoder::new()
            .add_code(Lang::Rust, "main.rs", b"fn main() {}")
            .encode()
            .unwrap();

        let decoded = BcpDecoder::decode(&payload).unwrap();
        assert_eq!(decoded.blocks.len(), 1);
        assert_eq!(decoded.header.version_major, 1);
        assert_eq!(decoded.header.version_minor, 0);
    }

    #[test]
    fn roundtrip_single_code_block() {
        let decoded =
            roundtrip(BcpEncoder::new().add_code(Lang::Rust, "lib.rs", b"pub fn hello() {}"));

        assert_eq!(decoded.blocks.len(), 1);
        let block = &decoded.blocks[0];
        assert_eq!(block.block_type, BlockType::Code);
        assert!(block.summary.is_none());

        match &block.content {
            BlockContent::Code(code) => {
                assert_eq!(code.lang, Lang::Rust);
                assert_eq!(code.path, "lib.rs");
                assert_eq!(code.content, b"pub fn hello() {}");
                assert!(code.line_range.is_none());
            }
            other => panic!("expected Code, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_multiple_block_types() {
        let decoded = roundtrip(
            BcpEncoder::new()
                .add_code(Lang::Python, "app.py", b"print('hi')")
                .add_conversation(Role::User, b"What is this?")
                .add_conversation(Role::Assistant, b"A greeting script.")
                .add_tool_result("pytest", Status::Ok, b"1 passed")
                .add_document("README", b"# Hello", FormatHint::Markdown),
        );

        assert_eq!(decoded.blocks.len(), 5);

        // Verify type ordering matches encoder order
        let types: Vec<_> = decoded
            .blocks
            .iter()
            .map(|b| b.block_type.clone())
            .collect();
        assert_eq!(
            types,
            vec![
                BlockType::Code,
                BlockType::Conversation,
                BlockType::Conversation,
                BlockType::ToolResult,
                BlockType::Document,
            ]
        );
    }

    #[test]
    fn roundtrip_with_summary() {
        let decoded = roundtrip(
            BcpEncoder::new()
                .add_code(Lang::Rust, "main.rs", b"fn main() {}")
                .with_summary("Application entry point."),
        );

        assert_eq!(decoded.blocks.len(), 1);
        let block = &decoded.blocks[0];
        assert!(block.flags.has_summary());
        assert_eq!(
            block.summary.as_ref().unwrap().text,
            "Application entry point."
        );

        // The content should still decode correctly
        match &block.content {
            BlockContent::Code(code) => {
                assert_eq!(code.path, "main.rs");
            }
            other => panic!("expected Code, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_with_priority_annotation() {
        let decoded = roundtrip(
            BcpEncoder::new()
                .add_code(Lang::Rust, "lib.rs", b"// code")
                .with_priority(Priority::High),
        );

        // Encoder produces CODE + ANNOTATION blocks
        assert_eq!(decoded.blocks.len(), 2);
        assert_eq!(decoded.blocks[0].block_type, BlockType::Code);
        assert_eq!(decoded.blocks[1].block_type, BlockType::Annotation);

        match &decoded.blocks[1].content {
            BlockContent::Annotation(ann) => {
                assert_eq!(ann.target_block_id, 0);
                assert_eq!(ann.kind, AnnotationKind::Priority);
                assert_eq!(ann.value, vec![Priority::High.to_wire_byte()]);
            }
            other => panic!("expected Annotation, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_all_block_types() {
        let decoded = roundtrip(
            BcpEncoder::new()
                .add_code(Lang::Rust, "main.rs", b"fn main() {}")
                .add_conversation(Role::User, b"hello")
                .add_file_tree(
                    "/project",
                    vec![FileEntry {
                        name: "lib.rs".to_string(),
                        kind: FileEntryKind::File,
                        size: 100,
                        children: vec![],
                    }],
                )
                .add_tool_result("rg", Status::Ok, b"3 matches")
                .add_document("README", b"# Title", FormatHint::Markdown)
                .add_structured_data(DataFormat::Json, b"{\"key\": \"val\"}")
                .add_diff(
                    "src/lib.rs",
                    vec![DiffHunk {
                        old_start: 1,
                        new_start: 1,
                        lines: b"+new line\n".to_vec(),
                    }],
                )
                .add_annotation(0, AnnotationKind::Tag, b"important")
                .add_image(MediaType::Png, "screenshot", b"\x89PNG")
                .add_extension("myco", "custom", b"data"),
        );

        assert_eq!(decoded.blocks.len(), 10);
        let types: Vec<_> = decoded
            .blocks
            .iter()
            .map(|b| b.block_type.clone())
            .collect();
        assert_eq!(
            types,
            vec![
                BlockType::Code,
                BlockType::Conversation,
                BlockType::FileTree,
                BlockType::ToolResult,
                BlockType::Document,
                BlockType::StructuredData,
                BlockType::Diff,
                BlockType::Annotation,
                BlockType::Image,
                BlockType::Extension,
            ]
        );
    }

    #[test]
    fn roundtrip_code_with_line_range() {
        let decoded = roundtrip(BcpEncoder::new().add_code_range(
            Lang::Rust,
            "lib.rs",
            b"fn foo() {}",
            10,
            20,
        ));

        match &decoded.blocks[0].content {
            BlockContent::Code(code) => {
                assert_eq!(code.line_range, Some((10, 20)));
            }
            other => panic!("expected Code, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_conversation_with_tool_call_id() {
        let decoded =
            roundtrip(BcpEncoder::new().add_conversation_tool(Role::Tool, b"result", "call_abc"));

        match &decoded.blocks[0].content {
            BlockContent::Conversation(conv) => {
                assert_eq!(conv.tool_call_id.as_deref(), Some("call_abc"));
            }
            other => panic!("expected Conversation, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_preserves_all_field_values() {
        // Comprehensive field-level round-trip for complex blocks.
        let decoded = roundtrip(
            BcpEncoder::new()
                .add_file_tree(
                    "/project/src",
                    vec![
                        FileEntry {
                            name: "main.rs".to_string(),
                            kind: FileEntryKind::File,
                            size: 512,
                            children: vec![],
                        },
                        FileEntry {
                            name: "lib".to_string(),
                            kind: FileEntryKind::Directory,
                            size: 0,
                            children: vec![FileEntry {
                                name: "utils.rs".to_string(),
                                kind: FileEntryKind::File,
                                size: 128,
                                children: vec![],
                            }],
                        },
                    ],
                )
                .add_diff(
                    "Cargo.toml",
                    vec![
                        DiffHunk {
                            old_start: 5,
                            new_start: 5,
                            lines: b"+tokio = \"1\"\n".to_vec(),
                        },
                        DiffHunk {
                            old_start: 20,
                            new_start: 21,
                            lines: b"-old_dep = \"0.1\"\n+new_dep = \"0.2\"\n".to_vec(),
                        },
                    ],
                ),
        );

        assert_eq!(decoded.blocks.len(), 2);

        // Verify FileTree fields
        match &decoded.blocks[0].content {
            BlockContent::FileTree(tree) => {
                assert_eq!(tree.root_path, "/project/src");
                assert_eq!(tree.entries.len(), 2);
                assert_eq!(tree.entries[0].name, "main.rs");
                assert_eq!(tree.entries[0].size, 512);
                assert_eq!(tree.entries[1].name, "lib");
                assert_eq!(tree.entries[1].children.len(), 1);
                assert_eq!(tree.entries[1].children[0].name, "utils.rs");
            }
            other => panic!("expected FileTree, got {other:?}"),
        }

        // Verify Diff fields
        match &decoded.blocks[1].content {
            BlockContent::Diff(diff) => {
                assert_eq!(diff.path, "Cargo.toml");
                assert_eq!(diff.hunks.len(), 2);
                assert_eq!(diff.hunks[0].old_start, 5);
                assert_eq!(diff.hunks[1].old_start, 20);
                assert_eq!(diff.hunks[1].new_start, 21);
            }
            other => panic!("expected Diff, got {other:?}"),
        }
    }

    // ── Validation tests ──────────────────────────────────────────────────

    #[test]
    fn rejects_bad_magic() {
        let mut payload = BcpEncoder::new()
            .add_conversation(Role::User, b"hi")
            .encode()
            .unwrap();

        // Corrupt the magic bytes
        payload[0] = b'X';
        let result = BcpDecoder::decode(&payload);
        assert!(matches!(result, Err(DecodeError::InvalidHeader(_))));
    }

    #[test]
    fn rejects_truncated_header() {
        let result = BcpDecoder::decode(&[0x4C, 0x43, 0x50, 0x00]);
        assert!(matches!(result, Err(DecodeError::InvalidHeader(_))));
    }

    #[test]
    fn rejects_missing_end_sentinel() {
        let payload = BcpEncoder::new()
            .add_conversation(Role::User, b"hi")
            .encode()
            .unwrap();

        // Strip the last 4 bytes (the END sentinel)
        let truncated = &payload[..payload.len() - 4];
        let result = BcpDecoder::decode(truncated);
        assert!(matches!(result, Err(DecodeError::MissingEndSentinel)));
    }

    #[test]
    fn detects_trailing_data() {
        let mut payload = BcpEncoder::new()
            .add_conversation(Role::User, b"hi")
            .encode()
            .unwrap();

        // Append garbage after the END sentinel
        payload.extend_from_slice(b"trailing garbage");
        let result = BcpDecoder::decode(&payload);
        assert!(matches!(
            result,
            Err(DecodeError::TrailingData { extra_bytes: 16 })
        ));
    }

    #[test]
    fn unknown_block_type_captured_not_rejected() {
        // Manually construct a payload with an unknown block type (0x42).
        // We'll build: header + unknown frame + END sentinel.
        use bcp_wire::header::HeaderFlags;

        let mut payload = vec![0u8; HEADER_SIZE];
        let header = BcpHeader::new(HeaderFlags::NONE);
        header.write_to(&mut payload).unwrap();

        // Unknown block frame: type=0x42, flags=0x00, content_len=5, body=b"hello"
        let frame = BlockFrame {
            block_type: 0x42,
            flags: BlockFlags::NONE,
            body: b"hello".to_vec(),
        };
        frame.write_to(&mut payload).unwrap();

        // END sentinel
        let end = BlockFrame {
            block_type: 0xFF,
            flags: BlockFlags::NONE,
            body: Vec::new(),
        };
        end.write_to(&mut payload).unwrap();

        let decoded = BcpDecoder::decode(&payload).unwrap();
        assert_eq!(decoded.blocks.len(), 1);
        assert_eq!(decoded.blocks[0].block_type, BlockType::Unknown(0x42));

        match &decoded.blocks[0].content {
            BlockContent::Unknown { type_id, body } => {
                assert_eq!(*type_id, 0x42);
                assert_eq!(body, b"hello");
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn optional_fields_absent_result_in_none() {
        let decoded = roundtrip(
            BcpEncoder::new()
                .add_code(Lang::Rust, "x.rs", b"let x = 1;")
                .add_conversation(Role::User, b"msg"),
        );

        // Code: line_range should be None
        match &decoded.blocks[0].content {
            BlockContent::Code(code) => assert!(code.line_range.is_none()),
            other => panic!("expected Code, got {other:?}"),
        }

        // Conversation: tool_call_id should be None
        match &decoded.blocks[1].content {
            BlockContent::Conversation(conv) => assert!(conv.tool_call_id.is_none()),
            other => panic!("expected Conversation, got {other:?}"),
        }
    }

    #[test]
    fn summary_extraction_with_body() {
        let decoded = roundtrip(
            BcpEncoder::new()
                .add_document(
                    "Guide",
                    b"# Getting Started\n\nWelcome!",
                    FormatHint::Markdown,
                )
                .with_summary("Onboarding guide for new contributors."),
        );

        let block = &decoded.blocks[0];
        assert!(block.flags.has_summary());
        assert_eq!(
            block.summary.as_ref().unwrap().text,
            "Onboarding guide for new contributors."
        );

        match &block.content {
            BlockContent::Document(doc) => {
                assert_eq!(doc.title, "Guide");
                assert_eq!(doc.content, b"# Getting Started\n\nWelcome!");
                assert_eq!(doc.format_hint, FormatHint::Markdown);
            }
            other => panic!("expected Document, got {other:?}"),
        }
    }

    #[test]
    fn rfc_example_roundtrip() {
        let decoded = roundtrip(
            BcpEncoder::new()
                .add_code(Lang::Rust, "src/main.rs", b"fn main() { todo!() }")
                .with_summary("Entry point: CLI setup and server startup.")
                .with_priority(Priority::High)
                .add_conversation(Role::User, b"Fix the timeout bug.")
                .add_conversation(Role::Assistant, b"I'll examine the pool config...")
                .add_tool_result("ripgrep", Status::Ok, b"3 matches found."),
        );

        assert_eq!(decoded.blocks.len(), 5);

        // Block 0: CODE with summary
        assert_eq!(decoded.blocks[0].block_type, BlockType::Code);
        assert_eq!(
            decoded.blocks[0].summary.as_ref().unwrap().text,
            "Entry point: CLI setup and server startup."
        );

        // Block 1: ANNOTATION (priority)
        assert_eq!(decoded.blocks[1].block_type, BlockType::Annotation);

        // Block 2-3: CONVERSATION
        assert_eq!(decoded.blocks[2].block_type, BlockType::Conversation);
        assert_eq!(decoded.blocks[3].block_type, BlockType::Conversation);

        // Block 4: TOOL_RESULT
        assert_eq!(decoded.blocks[4].block_type, BlockType::ToolResult);
    }

    #[test]
    fn empty_body_blocks() {
        // Extension with empty content
        let decoded = roundtrip(BcpEncoder::new().add_extension("ns", "type", b""));

        match &decoded.blocks[0].content {
            BlockContent::Extension(ext) => {
                assert_eq!(ext.namespace, "ns");
                assert_eq!(ext.type_name, "type");
                assert!(ext.content.is_empty());
            }
            other => panic!("expected Extension, got {other:?}"),
        }
    }

    // ── Per-block compression roundtrip tests ───────────────────────────

    #[test]
    fn roundtrip_per_block_compression() {
        let big_content = "fn main() { println!(\"hello world\"); }\n".repeat(50);
        let payload = BcpEncoder::new()
            .add_code(Lang::Rust, "main.rs", big_content.as_bytes())
            .with_compression()
            .encode()
            .unwrap();

        // Verify the block is actually compressed on the wire
        let frame_buf = &payload[HEADER_SIZE..];
        let (frame, _) = BlockFrame::read_from(frame_buf).unwrap().unwrap();
        assert!(frame.flags.is_compressed());

        // Decode should transparently decompress
        let decoded = BcpDecoder::decode(&payload).unwrap();
        assert_eq!(decoded.blocks.len(), 1);
        match &decoded.blocks[0].content {
            BlockContent::Code(code) => {
                assert_eq!(code.path, "main.rs");
                assert_eq!(code.content, big_content.as_bytes());
            }
            other => panic!("expected Code, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_per_block_compression_with_summary() {
        let big_content = "pub fn process() -> Result<(), Error> { Ok(()) }\n".repeat(50);
        let payload = BcpEncoder::new()
            .add_code(Lang::Rust, "lib.rs", big_content.as_bytes())
            .with_summary("Main processing function.")
            .with_compression()
            .encode()
            .unwrap();

        let decoded = BcpDecoder::decode(&payload).unwrap();
        let block = &decoded.blocks[0];
        assert!(block.flags.has_summary());
        assert!(block.flags.is_compressed());
        assert_eq!(
            block.summary.as_ref().unwrap().text,
            "Main processing function."
        );
        match &block.content {
            BlockContent::Code(code) => assert_eq!(code.content, big_content.as_bytes()),
            other => panic!("expected Code, got {other:?}"),
        }
    }

    // ── Whole-payload compression roundtrip tests ───────────────────────

    #[test]
    fn roundtrip_whole_payload_compression() {
        let big_content = "use std::io;\n".repeat(100);
        let payload = BcpEncoder::new()
            .add_code(Lang::Rust, "a.rs", big_content.as_bytes())
            .add_code(Lang::Rust, "b.rs", big_content.as_bytes())
            .compress_payload()
            .encode()
            .unwrap();

        let decoded = BcpDecoder::decode(&payload).unwrap();
        assert_eq!(decoded.blocks.len(), 2);
        assert!(decoded.header.flags.is_compressed());

        for block in &decoded.blocks {
            match &block.content {
                BlockContent::Code(code) => {
                    assert_eq!(code.content, big_content.as_bytes());
                }
                other => panic!("expected Code, got {other:?}"),
            }
        }
    }

    // ── Content addressing roundtrip tests ──────────────────────────────

    #[test]
    fn roundtrip_content_addressing() {
        use bcp_encoder::MemoryContentStore;
        use std::sync::Arc;

        let store = Arc::new(MemoryContentStore::new());
        let payload = BcpEncoder::new()
            .set_content_store(store.clone())
            .add_code(Lang::Rust, "main.rs", b"fn main() {}")
            .with_content_addressing()
            .encode()
            .unwrap();

        // Verify it's a reference on the wire
        let frame_buf = &payload[HEADER_SIZE..];
        let (frame, _) = BlockFrame::read_from(frame_buf).unwrap().unwrap();
        assert!(frame.flags.is_reference());
        assert_eq!(frame.body.len(), 32);

        // decode() without store should fail
        let result = BcpDecoder::decode(&payload);
        assert!(matches!(result, Err(DecodeError::MissingContentStore)));

        // decode_with_store should succeed
        let decoded = BcpDecoder::decode_with_store(&payload, store.as_ref()).unwrap();
        assert_eq!(decoded.blocks.len(), 1);
        match &decoded.blocks[0].content {
            BlockContent::Code(code) => {
                assert_eq!(code.path, "main.rs");
                assert_eq!(code.content, b"fn main() {}");
            }
            other => panic!("expected Code, got {other:?}"),
        }
    }

    #[test]
    fn roundtrip_auto_dedup() {
        use bcp_encoder::MemoryContentStore;
        use std::sync::Arc;

        let store = Arc::new(MemoryContentStore::new());
        let payload = BcpEncoder::new()
            .set_content_store(store.clone())
            .auto_dedup()
            .add_code(Lang::Rust, "main.rs", b"fn main() {}")
            .add_code(Lang::Rust, "main.rs", b"fn main() {}") // duplicate
            .encode()
            .unwrap();

        let decoded = BcpDecoder::decode_with_store(&payload, store.as_ref()).unwrap();
        assert_eq!(decoded.blocks.len(), 2);

        // Both should decode to the same content
        for block in &decoded.blocks {
            match &block.content {
                BlockContent::Code(code) => {
                    assert_eq!(code.content, b"fn main() {}");
                }
                other => panic!("expected Code, got {other:?}"),
            }
        }
    }

    #[test]
    fn unresolved_reference_errors() {
        use bcp_encoder::MemoryContentStore;
        use std::sync::Arc;

        let encode_store = Arc::new(MemoryContentStore::new());
        let payload = BcpEncoder::new()
            .set_content_store(encode_store)
            .add_code(Lang::Rust, "main.rs", b"fn main() {}")
            .with_content_addressing()
            .encode()
            .unwrap();

        // Decode with a fresh (empty) store — hash won't be found
        let decode_store = MemoryContentStore::new();
        let result = BcpDecoder::decode_with_store(&payload, &decode_store);
        assert!(matches!(
            result,
            Err(DecodeError::UnresolvedReference { .. })
        ));
    }

    // ── Combined compression + content addressing ───────────────────────

    #[test]
    fn roundtrip_refs_with_whole_payload_compression() {
        use bcp_encoder::MemoryContentStore;
        use std::sync::Arc;

        let store = Arc::new(MemoryContentStore::new());
        let big_content = "fn process() -> bool { true }\n".repeat(50);
        let payload = BcpEncoder::new()
            .set_content_store(store.clone())
            .compress_payload()
            .add_code(Lang::Rust, "main.rs", big_content.as_bytes())
            .with_content_addressing()
            .add_conversation(Role::User, b"Review this code")
            .encode()
            .unwrap();

        let decoded = BcpDecoder::decode_with_store(&payload, store.as_ref()).unwrap();
        assert_eq!(decoded.blocks.len(), 2);

        match &decoded.blocks[0].content {
            BlockContent::Code(code) => {
                assert_eq!(code.content, big_content.as_bytes());
            }
            other => panic!("expected Code, got {other:?}"),
        }
        match &decoded.blocks[1].content {
            BlockContent::Conversation(conv) => {
                assert_eq!(conv.content, b"Review this code");
            }
            other => panic!("expected Conversation, got {other:?}"),
        }
    }
}
