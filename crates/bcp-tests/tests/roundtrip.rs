//! Roundtrip integration tests for the BCP encode → decode → encode pipeline.
//!
//! Each test encodes a payload with [`BcpEncoder`], decodes it with
//! [`BcpDecoder`], then re-encodes the decoded blocks using the
//! [`encode_from_blocks`] helper and asserts the output is byte-identical
//! to the original.
//!
//! The byte-identical invariant holds because:
//!   - The encoder's block ordering, field TLV encoding, and framing are
//!     deterministic for a fixed sequence of builder calls.
//!   - [`encode_from_blocks`] mirrors the original builder calls by
//!     pattern-matching on [`BlockContent`] variants and calling the
//!     corresponding `add_*` method, including re-attaching any
//!     [`Summary`] via [`with_summary`].
//!
//! Compression tests use semantic equality (`decoded.blocks == uncompressed_decoded.blocks`)
//! rather than byte comparison, because compressed blocks decode to the same
//! semantic content as their uncompressed equivalents but the original
//! per-block compression flags are not preserved through a full decode/re-encode
//! cycle without re-specifying them.

use bcp_decoder::BcpDecoder;
use bcp_encoder::{EncodeError, BcpEncoder};
use bcp_types::block::{Block, BlockContent};
use bcp_types::diff::DiffHunk;
use bcp_types::enums::{
    AnnotationKind, DataFormat, FormatHint, Lang, MediaType, Role, Status,
};
use bcp_types::file_tree::{FileEntry, FileEntryKind};

// ── encode_from_blocks helper ────────────────────────────────────────────────

/// Reconstruct a BCP payload from a slice of decoded [`Block`] values.
///
/// Iterates the blocks in order, pattern-matches on [`BlockContent`] to
/// call the appropriate `add_*` method on a fresh [`BcpEncoder`], and
/// re-attaches any [`Summary`] via `with_summary`. Unknown and End
/// variants are skipped — they are not re-encoded as semantic content.
///
/// This is the inverse of `BcpDecoder::decode` for known block types,
/// and the output is byte-identical to the original encoded payload
/// provided the same blocks are supplied in the same order.
pub fn encode_from_blocks(blocks: &[Block]) -> Result<Vec<u8>, EncodeError> {
    let mut encoder = BcpEncoder::new();

    for block in blocks {
        match &block.content {
            BlockContent::Code(code) => {
                match code.line_range {
                    Some((start, end)) => {
                        encoder.add_code_range(code.lang, &code.path, &code.content, start, end);
                    }
                    None => {
                        encoder.add_code(code.lang, &code.path, &code.content);
                    }
                }
            }
            BlockContent::Conversation(conv) => {
                match &conv.tool_call_id {
                    Some(id) => {
                        encoder.add_conversation_tool(conv.role, &conv.content, id);
                    }
                    None => {
                        encoder.add_conversation(conv.role, &conv.content);
                    }
                }
            }
            BlockContent::FileTree(tree) => {
                encoder.add_file_tree(&tree.root_path, tree.entries.clone());
            }
            BlockContent::ToolResult(tool) => {
                encoder.add_tool_result(&tool.tool_name, tool.status, &tool.content);
            }
            BlockContent::Document(doc) => {
                encoder.add_document(&doc.title, &doc.content, doc.format_hint);
            }
            BlockContent::StructuredData(sd) => {
                encoder.add_structured_data(sd.format, &sd.content);
            }
            BlockContent::Diff(diff) => {
                encoder.add_diff(&diff.path, diff.hunks.clone());
            }
            BlockContent::Annotation(ann) => {
                encoder.add_annotation(ann.target_block_id, ann.kind, &ann.value);
            }
            BlockContent::EmbeddingRef(emb) => {
                encoder.add_embedding_ref(&emb.vector_id, &emb.source_hash, &emb.model);
            }
            BlockContent::Image(img) => {
                encoder.add_image(img.media_type, &img.alt_text, &img.data);
            }
            BlockContent::Extension(ext) => {
                encoder.add_extension(&ext.namespace, &ext.type_name, &ext.content);
            }
            BlockContent::End | BlockContent::Unknown { .. } => continue,
        }

        if let Some(summary) = &block.summary {
            encoder.with_summary(&summary.text).unwrap();
        }
    }

    encoder.encode()
}

// ── Roundtrip tests — byte-identical ────────────────────────────────────────

#[test]
fn roundtrip_code_block() {
    let original = BcpEncoder::new()
        .add_code(Lang::Rust, "src/main.rs", b"fn main() { println!(\"hello\"); }")
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

#[test]
fn roundtrip_code_range() {
    let original = BcpEncoder::new()
        .add_code_range(Lang::Rust, "src/lib.rs", b"pub fn handler() {}", 10, 20)
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

#[test]
fn roundtrip_conversation() {
    let original = BcpEncoder::new()
        .add_conversation(Role::User, b"What does this function do?")
        .add_conversation(
            Role::Assistant,
            b"It initializes the connection pool with default settings.",
        )
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

#[test]
fn roundtrip_conversation_tool() {
    let original = BcpEncoder::new()
        .add_conversation_tool(Role::Tool, b"3 matches found in src/", "call_abc123")
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

#[test]
fn roundtrip_file_tree() {
    let entries = vec![
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
    ];

    let original = BcpEncoder::new()
        .add_file_tree("/project/src", entries)
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

#[test]
fn roundtrip_tool_result() {
    let original = BcpEncoder::new()
        .add_tool_result("cargo test", Status::Ok, b"test result: ok. 5 passed; 0 failed")
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

#[test]
fn roundtrip_document() {
    let original = BcpEncoder::new()
        .add_document(
            "Architecture Overview",
            b"# Architecture\n\nThis system uses a layered approach.",
            FormatHint::Markdown,
        )
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

#[test]
fn roundtrip_structured_data() {
    let original = BcpEncoder::new()
        .add_structured_data(DataFormat::Json, b"{\"name\":\"bcp\",\"version\":1}")
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

#[test]
fn roundtrip_diff() {
    let hunks = vec![DiffHunk {
        old_start: 5,
        new_start: 5,
        lines: b"-    old_value: u32,\n+    new_value: u64,\n".to_vec(),
    }];

    let original = BcpEncoder::new()
        .add_diff("src/types.rs", hunks)
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

#[test]
fn roundtrip_annotation() {
    let original = BcpEncoder::new()
        .add_code(Lang::Rust, "src/hot_path.rs", b"#[inline(always)] fn compute() -> u64 { 42 }")
        .add_annotation(0, AnnotationKind::Tag, b"hot-path")
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

#[test]
fn roundtrip_embedding_ref() {
    let vector_id = b"vec-embedding-001";
    let source_hash = vec![0xAB; 32];
    let model = "text-embedding-3-small";

    let original = BcpEncoder::new()
        .add_embedding_ref(vector_id, &source_hash, model)
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

#[test]
fn roundtrip_image() {
    // Minimal 1×1 PNG (67 bytes).
    let tiny_png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
        0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41,
        0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
        0x00, 0x00, 0x02, 0x00, 0x01, 0xE2, 0x21, 0xBC,
        0x33, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
        0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    let original = BcpEncoder::new()
        .add_image(MediaType::Png, "Application screenshot", tiny_png)
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

#[test]
fn roundtrip_extension() {
    let original = BcpEncoder::new()
        .add_extension("com.example.tools", "lsp_diagnostic", b"{\"severity\":\"error\"}")
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

// ── Roundtrip with summary — byte-identical ──────────────────────────────────

#[test]
fn roundtrip_block_with_summary() {
    let original = BcpEncoder::new()
        .add_code(Lang::Rust, "src/pool.rs", b"pub struct ConnectionPool { max: usize }")
        .with_summary("Connection pool with configurable max connections.").unwrap()
        .add_conversation(Role::User, b"How does the pool handle overflow?")
        .encode()
        .unwrap();

    let decoded = BcpDecoder::decode(&original).unwrap();
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(re_encoded, original);
}

// ── Compression roundtrips — semantic equality ───────────────────────────────

/// Extract the semantic content from decoded blocks, stripping wire-layer flags.
///
/// Compression flags (`COMPRESSED`, `IS_REFERENCE`) are a wire concern — they
/// describe how bytes were transported, not what the block contains. Two payloads
/// that differ only in per-block compression produce decoded blocks with identical
/// `content` and `summary` but different `flags`. Semantic comparison strips flags
/// so that compressed and uncompressed payloads compare equal after decoding.
fn semantic_contents(blocks: &[Block]) -> Vec<(&BlockContent, Option<&str>)> {
    blocks
        .iter()
        .map(|b| (&b.content, b.summary.as_ref().map(|s| s.text.as_str())))
        .collect()
}

#[test]
fn roundtrip_compressed_blocks() {
    // Use content long enough to exceed the compression threshold so the
    // COMPRESSED flag is actually set on the wire.
    let long_content = "fn placeholder() -> u64 { 0 }\n".repeat(50);

    let compressed = BcpEncoder::new()
        .add_code(Lang::Rust, "src/generated.rs", long_content.as_bytes())
        .with_compression().unwrap()
        .add_conversation(Role::User, b"Summarize this module.")
        .encode()
        .unwrap();

    let uncompressed = BcpEncoder::new()
        .add_code(Lang::Rust, "src/generated.rs", long_content.as_bytes())
        .add_conversation(Role::User, b"Summarize this module.")
        .encode()
        .unwrap();

    let compressed_decoded = BcpDecoder::decode(&compressed).unwrap();
    let uncompressed_decoded = BcpDecoder::decode(&uncompressed).unwrap();

    // The decoder transparently decompresses — both payloads decode to the same
    // semantic content. We compare content and summaries only, not wire-layer flags,
    // since the COMPRESSED flag is preserved on the decoded Block struct.
    assert_eq!(
        semantic_contents(&compressed_decoded.blocks),
        semantic_contents(&uncompressed_decoded.blocks),
    );
}

#[test]
fn roundtrip_compressed_payload() {
    let long_content = "pub use std::collections::HashMap;\n".repeat(50);

    let compressed = BcpEncoder::new()
        .add_code(Lang::Rust, "src/imports.rs", long_content.as_bytes())
        .add_document("Changelog", b"# Changelog\n\n## v1.0.0\nInitial release.", FormatHint::Markdown)
        .compress_payload()
        .encode()
        .unwrap();

    let uncompressed = BcpEncoder::new()
        .add_code(Lang::Rust, "src/imports.rs", long_content.as_bytes())
        .add_document("Changelog", b"# Changelog\n\n## v1.0.0\nInitial release.", FormatHint::Markdown)
        .encode()
        .unwrap();

    let compressed_decoded = BcpDecoder::decode(&compressed).unwrap();
    let uncompressed_decoded = BcpDecoder::decode(&uncompressed).unwrap();

    // Whole-payload decompression is transparent — decoded block content is
    // semantically identical to the uncompressed equivalent.
    assert_eq!(
        semantic_contents(&compressed_decoded.blocks),
        semantic_contents(&uncompressed_decoded.blocks),
    );
}
