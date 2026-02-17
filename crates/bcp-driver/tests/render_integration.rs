//! Integration test: encode → decode → render
//!
//! Verifies the full pipeline from encoder to driver output. The encoder
//! produces a binary payload, the decoder extracts typed blocks, and the
//! driver renders them into model-ready text in all three output modes.

use bcp_decoder::LcpDecoder;
use bcp_driver::{DefaultDriver, DriverConfig, LcpDriver, OutputMode};
use bcp_encoder::LcpEncoder;
use bcp_types::enums::{Lang, Role, Status};

/// Build a representative payload with multiple block types, encode it,
/// decode it, then render in all three modes and verify key properties.
#[test]
fn full_pipeline_encode_decode_render() {
    let payload = LcpEncoder::new()
        .add_code(Lang::Rust, "src/main.rs", b"fn main() {\n    println!(\"hello\");\n}")
        .with_summary("Entry point: prints hello.")
        .add_tool_result("ripgrep", Status::Ok, b"3 matches for 'Config' across 2 files.")
        .add_conversation(Role::User, b"Fix the connection timeout bug.")
        .add_conversation(Role::Assistant, b"I'll examine the pool config.")
        .encode()
        .expect("encoding should succeed");

    let decoded = LcpDecoder::decode(&payload).expect("decoding should succeed");
    let driver = DefaultDriver;

    // ── XML mode ──────────────────────────────────────────────────
    let xml_config = DriverConfig {
        mode: OutputMode::Xml,
        target_model: None,
        include_types: None,
    };
    let xml = driver
        .render(&decoded.blocks, &xml_config)
        .expect("XML render should succeed");

    assert!(xml.starts_with("<context>"), "XML must start with <context>");
    assert!(xml.ends_with("</context>"), "XML must end with </context>");
    // Summary replaces content for the code block
    assert!(xml.contains("summary=\"true\""), "summary attr present");
    assert!(xml.contains("Entry point: prints hello."), "summary text present");
    assert!(!xml.contains("println!"), "full content should not appear when summary exists");
    assert!(xml.contains("<tool name=\"ripgrep\" status=\"ok\">"));
    assert!(xml.contains("<turn role=\"user\">Fix the connection timeout bug.</turn>"));
    assert!(xml.contains("<turn role=\"assistant\">"));

    // ── Markdown mode ─────────────────────────────────────────────
    let md_config = DriverConfig {
        mode: OutputMode::Markdown,
        target_model: None,
        include_types: None,
    };
    let md = driver
        .render(&decoded.blocks, &md_config)
        .expect("Markdown render should succeed");

    assert!(md.contains("## src/main.rs (summary)"));
    assert!(md.contains("### Tool: ripgrep (ok)"));
    assert!(md.contains("**User**: Fix the connection timeout bug."));
    assert!(md.contains("**Assistant**: I'll examine the pool config."));

    // ── Minimal mode ──────────────────────────────────────────────
    let min_config = DriverConfig {
        mode: OutputMode::Minimal,
        target_model: None,
        include_types: None,
    };
    let min = driver
        .render(&decoded.blocks, &min_config)
        .expect("Minimal render should succeed");

    assert!(min.contains("--- src/main.rs [rust] (summary) ---"));
    assert!(min.contains("--- ripgrep [ok] ---"));
    assert!(min.contains("[user] Fix the connection timeout bug."));
    assert!(min.contains("[assistant] I'll examine the pool config."));
}

/// Verify that include_types filtering works through the full pipeline.
#[test]
fn pipeline_with_type_filter() {
    let payload = LcpEncoder::new()
        .add_code(Lang::Rust, "src/main.rs", b"fn main() {}")
        .add_conversation(Role::User, b"Hello")
        .encode()
        .expect("encoding should succeed");

    let decoded = LcpDecoder::decode(&payload).expect("decoding should succeed");
    let driver = DefaultDriver;

    let config = DriverConfig {
        mode: OutputMode::Minimal,
        target_model: None,
        include_types: Some(vec![bcp_types::BlockType::Conversation]),
    };

    let result = driver
        .render(&decoded.blocks, &config)
        .expect("render should succeed");

    assert!(result.contains("[user] Hello"));
    assert!(!result.contains("main.rs"), "code block should be filtered out");
}
