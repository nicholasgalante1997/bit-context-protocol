//! Conformance tests: golden fixture files decoded and rendered to insta snapshots.
//!
//! Each test reads a pre-built binary `.lcp` fixture from `tests/golden/`,
//! decodes it with [`LcpDecoder`], and renders the result with [`DefaultDriver`]
//! in one of the three output modes (XML, Markdown, Minimal). The rendered
//! string is compared against an insta snapshot stored in `tests/snapshots/`.
//!
//! # Why golden files?
//!
//! The generator binary (`src/bin/generate_golden.rs`) writes deterministic
//! payloads once and commits them. The conformance suite then verifies that the
//! decoder + driver pipeline produces identical human-readable output across
//! all commits. A diff in a snapshot signals either a deliberate format change
//! (accept via `cargo insta review`) or an accidental regression.
//!
//! # First run
//!
//! Snapshots do not exist on a fresh checkout. Run with `INSTA_UPDATE=always`
//! to write the initial `.snap` files:
//!
//! ```bash
//! INSTA_UPDATE=always cargo test -p bcp-tests --test conformance
//! ```
//!
//! Subsequent runs compare against the written snapshots and fail on any diff.

use std::path::Path;

use bcp_decoder::LcpDecoder;
use bcp_driver::{DefaultDriver, DriverConfig, LcpDriver, OutputMode};
use bcp_encoder::MemoryContentStore;
use bcp_types::ContentStore;
use insta::assert_snapshot;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Read a golden fixture payload from `tests/golden/<fixture>/payload.lcp`.
fn golden_payload(fixture: &str) -> Vec<u8> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir
        .join("tests/golden")
        .join(fixture)
        .join("payload.lcp");
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("failed to read golden fixture {}: {e}", path.display()))
}

/// Decode a payload and render it with the given output mode using default config.
fn render(payload: &[u8], mode: OutputMode) -> String {
    let decoded = LcpDecoder::decode(payload)
        .unwrap_or_else(|e| panic!("decode failed for mode {mode:?}: {e}"));
    let config = DriverConfig {
        mode,
        ..DriverConfig::default()
    };
    DefaultDriver
        .render(&decoded.blocks, &config)
        .unwrap_or_else(|e| panic!("render failed for mode {mode:?}: {e}"))
}

// ── simple_code ───────────────────────────────────────────────────────────────

#[test]
fn simple_code_xml() {
    let payload = golden_payload("simple_code");
    let output = render(&payload, OutputMode::Xml);
    assert_snapshot!("simple_code_xml", output);
}

#[test]
fn simple_code_markdown() {
    let payload = golden_payload("simple_code");
    let output = render(&payload, OutputMode::Markdown);
    assert_snapshot!("simple_code_markdown", output);
}

#[test]
fn simple_code_minimal() {
    let payload = golden_payload("simple_code");
    let output = render(&payload, OutputMode::Minimal);
    assert_snapshot!("simple_code_minimal", output);
}

// ── conversation ──────────────────────────────────────────────────────────────

#[test]
fn conversation_xml() {
    let payload = golden_payload("conversation");
    let output = render(&payload, OutputMode::Xml);
    assert_snapshot!("conversation_xml", output);
}

#[test]
fn conversation_markdown() {
    let payload = golden_payload("conversation");
    let output = render(&payload, OutputMode::Markdown);
    assert_snapshot!("conversation_markdown", output);
}

#[test]
fn conversation_minimal() {
    let payload = golden_payload("conversation");
    let output = render(&payload, OutputMode::Minimal);
    assert_snapshot!("conversation_minimal", output);
}

// ── mixed_blocks ──────────────────────────────────────────────────────────────

#[test]
fn mixed_blocks_xml() {
    let payload = golden_payload("mixed_blocks");
    let output = render(&payload, OutputMode::Xml);
    assert_snapshot!("mixed_blocks_xml", output);
}

#[test]
fn mixed_blocks_markdown() {
    let payload = golden_payload("mixed_blocks");
    let output = render(&payload, OutputMode::Markdown);
    assert_snapshot!("mixed_blocks_markdown", output);
}

#[test]
fn mixed_blocks_minimal() {
    let payload = golden_payload("mixed_blocks");
    let output = render(&payload, OutputMode::Minimal);
    assert_snapshot!("mixed_blocks_minimal", output);
}

// ── with_summaries ────────────────────────────────────────────────────────────

#[test]
fn with_summaries_xml() {
    let payload = golden_payload("with_summaries");
    let output = render(&payload, OutputMode::Xml);
    assert_snapshot!("with_summaries_xml", output);
}

#[test]
fn with_summaries_markdown() {
    let payload = golden_payload("with_summaries");
    let output = render(&payload, OutputMode::Markdown);
    assert_snapshot!("with_summaries_markdown", output);
}

#[test]
fn with_summaries_minimal() {
    let payload = golden_payload("with_summaries");
    let output = render(&payload, OutputMode::Minimal);
    assert_snapshot!("with_summaries_minimal", output);
}

// ── compressed_blocks ─────────────────────────────────────────────────────────
//
// Per-block zstd compression is transparent to the decoder. The rendered
// output is semantically identical to an uncompressed equivalent payload.

#[test]
fn compressed_blocks_xml() {
    let payload = golden_payload("compressed_blocks");
    let output = render(&payload, OutputMode::Xml);
    assert_snapshot!("compressed_blocks_xml", output);
}

#[test]
fn compressed_blocks_markdown() {
    let payload = golden_payload("compressed_blocks");
    let output = render(&payload, OutputMode::Markdown);
    assert_snapshot!("compressed_blocks_markdown", output);
}

#[test]
fn compressed_blocks_minimal() {
    let payload = golden_payload("compressed_blocks");
    let output = render(&payload, OutputMode::Minimal);
    assert_snapshot!("compressed_blocks_minimal", output);
}

// ── compressed_payload ────────────────────────────────────────────────────────
//
// Whole-payload zstd compression is handled in the header and is transparent
// to the block-level decoder. Rendering is identical to the uncompressed form.

#[test]
fn compressed_payload_xml() {
    let payload = golden_payload("compressed_payload");
    let output = render(&payload, OutputMode::Xml);
    assert_snapshot!("compressed_payload_xml", output);
}

#[test]
fn compressed_payload_markdown() {
    let payload = golden_payload("compressed_payload");
    let output = render(&payload, OutputMode::Markdown);
    assert_snapshot!("compressed_payload_markdown", output);
}

#[test]
fn compressed_payload_minimal() {
    let payload = golden_payload("compressed_payload");
    let output = render(&payload, OutputMode::Minimal);
    assert_snapshot!("compressed_payload_minimal", output);
}

// ── content_addressed ─────────────────────────────────────────────────────────
//
// The fixture contains two CODE blocks that share identical content, stored
// once in the content store via BLAKE3 deduplication. The decoder requires a
// populated `ContentStore` to resolve the hash references.
//
// The shared content is `b"fn shared() -> u32 { 42 }"` — we pre-populate a
// `MemoryContentStore` with this known value rather than parsing the JSON file.

fn decode_content_addressed() -> Vec<bcp_types::block::Block> {
    let payload = golden_payload("content_addressed");
    let store = MemoryContentStore::new();
    store.put(b"fn shared() -> u32 { 42 }");
    LcpDecoder::decode_with_store(&payload, &store)
        .expect("content_addressed decode failed")
        .blocks
}

#[test]
fn content_addressed_xml() {
    let blocks = decode_content_addressed();
    let config = DriverConfig {
        mode: OutputMode::Xml,
        ..DriverConfig::default()
    };
    let output = DefaultDriver
        .render(&blocks, &config)
        .expect("render failed");
    assert_snapshot!("content_addressed_xml", output);
}

#[test]
fn content_addressed_markdown() {
    let blocks = decode_content_addressed();
    let config = DriverConfig {
        mode: OutputMode::Markdown,
        ..DriverConfig::default()
    };
    let output = DefaultDriver
        .render(&blocks, &config)
        .expect("render failed");
    assert_snapshot!("content_addressed_markdown", output);
}

#[test]
fn content_addressed_minimal() {
    let blocks = decode_content_addressed();
    let config = DriverConfig {
        mode: OutputMode::Minimal,
        ..DriverConfig::default()
    };
    let output = DefaultDriver
        .render(&blocks, &config)
        .expect("render failed");
    assert_snapshot!("content_addressed_minimal", output);
}

// ── budget_constrained ────────────────────────────────────────────────────────
//
// The fixture has three blocks with CRITICAL, NORMAL, and BACKGROUND priorities.
// Rendering with a token budget exercises the budget engine's degradation chain.

fn budget_render(mode: OutputMode, token_budget: u32) -> String {
    let payload = golden_payload("budget_constrained");
    let decoded = LcpDecoder::decode(&payload).expect("budget_constrained decode failed");
    let config = DriverConfig {
        mode,
        token_budget: Some(token_budget),
        ..DriverConfig::default()
    };
    DefaultDriver
        .render(&decoded.blocks, &config)
        .expect("budget render failed")
}

#[test]
fn budget_constrained_xml_500() {
    let output = budget_render(OutputMode::Xml, 500);
    assert_snapshot!("budget_constrained_xml_500", output);
}

#[test]
fn budget_constrained_xml_200() {
    let output = budget_render(OutputMode::Xml, 200);
    assert_snapshot!("budget_constrained_xml_200", output);
}

#[test]
fn budget_constrained_minimal_500() {
    let output = budget_render(OutputMode::Minimal, 500);
    assert_snapshot!("budget_constrained_minimal_500", output);
}

// ── all_block_types ───────────────────────────────────────────────────────────
//
// The fixture contains one block of each of the 11 semantic block types.
// The Image block carries binary PNG data that the renderer cannot emit as
// UTF-8 text, so it is excluded via `include_types`. All other text-renderable
// block types are exercised. This snapshot verifies that every text renderer
// handles every text-renderable block type.

use bcp_types::BlockType;

fn all_block_types_text_only(mode: OutputMode) -> String {
    let payload = golden_payload("all_block_types");
    let decoded = LcpDecoder::decode(&payload).expect("all_block_types decode failed");
    let config = DriverConfig {
        mode,
        include_types: Some(vec![
            BlockType::Code,
            BlockType::Conversation,
            BlockType::FileTree,
            BlockType::ToolResult,
            BlockType::Document,
            BlockType::StructuredData,
            BlockType::Diff,
            BlockType::EmbeddingRef,
            BlockType::Extension,
        ]),
        ..DriverConfig::default()
    };
    DefaultDriver
        .render(&decoded.blocks, &config)
        .expect("all_block_types render failed")
}

#[test]
fn all_block_types_xml() {
    let output = all_block_types_text_only(OutputMode::Xml);
    assert_snapshot!("all_block_types_xml", output);
}

#[test]
fn all_block_types_markdown() {
    let output = all_block_types_text_only(OutputMode::Markdown);
    assert_snapshot!("all_block_types_markdown", output);
}

#[test]
fn all_block_types_minimal() {
    let output = all_block_types_text_only(OutputMode::Minimal);
    assert_snapshot!("all_block_types_minimal", output);
}
