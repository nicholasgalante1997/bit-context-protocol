//! Integration test: encode → decode → render
//!
//! Verifies the full pipeline from encoder to driver output. The encoder
//! produces a binary payload, the decoder extracts typed blocks, and the
//! driver renders them into model-ready text in all three output modes.

use bcp_decoder::LcpDecoder;
use bcp_driver::{DefaultDriver, DriverConfig, LcpDriver, OutputMode, Verbosity};
use bcp_encoder::LcpEncoder;
use bcp_types::enums::{Lang, Priority, Role, Status};

/// Build a representative payload with multiple block types, encode it,
/// decode it, then render in all three modes and verify key properties.
///
/// Default config (Adaptive + no budget) renders all blocks with full
/// content — summaries are ignored.
#[test]
fn full_pipeline_encode_decode_render() {
    let payload = LcpEncoder::new()
        .add_code(
            Lang::Rust,
            "src/main.rs",
            b"fn main() {\n    println!(\"hello\");\n}",
        )
        .with_summary("Entry point: prints hello.")
        .add_tool_result(
            "ripgrep",
            Status::Ok,
            b"3 matches for 'Config' across 2 files.",
        )
        .add_conversation(Role::User, b"Fix the connection timeout bug.")
        .add_conversation(Role::Assistant, b"I'll examine the pool config.")
        .encode()
        .expect("encoding should succeed");

    let decoded = LcpDecoder::decode(&payload).expect("decoding should succeed");
    let driver = DefaultDriver;

    // ── XML mode (full content, no budget) ──────────────────────────
    let xml_config = DriverConfig {
        mode: OutputMode::Xml,
        ..DriverConfig::default()
    };
    let xml = driver
        .render(&decoded.blocks, &xml_config)
        .expect("XML render should succeed");

    assert!(
        xml.starts_with("<context>"),
        "XML must start with <context>"
    );
    assert!(xml.ends_with("</context>"), "XML must end with </context>");
    // Full content rendered (Adaptive + no budget = all Full)
    assert!(
        xml.contains("println!"),
        "full content should appear in Adaptive mode without budget"
    );
    assert!(xml.contains("<tool name=\"ripgrep\" status=\"ok\">"));
    assert!(xml.contains("<turn role=\"user\">Fix the connection timeout bug.</turn>"));
    assert!(xml.contains("<turn role=\"assistant\">"));

    // ── Markdown mode ─────────────────────────────────────────────
    let md_config = DriverConfig {
        mode: OutputMode::Markdown,
        ..DriverConfig::default()
    };
    let md = driver
        .render(&decoded.blocks, &md_config)
        .expect("Markdown render should succeed");

    assert!(md.contains("## src/main.rs"));
    assert!(md.contains("### Tool: ripgrep (ok)"));
    assert!(md.contains("**User**: Fix the connection timeout bug."));
    assert!(md.contains("**Assistant**: I'll examine the pool config."));

    // ── Minimal mode ──────────────────────────────────────────────
    let min_config = DriverConfig {
        mode: OutputMode::Minimal,
        ..DriverConfig::default()
    };
    let min = driver
        .render(&decoded.blocks, &min_config)
        .expect("Minimal render should succeed");

    assert!(min.contains("--- src/main.rs [rust] ---"));
    assert!(min.contains("--- ripgrep [ok] ---"));
    assert!(min.contains("[user] Fix the connection timeout bug."));
    assert!(min.contains("[assistant] I'll examine the pool config."));
}

/// Verify Summary verbosity renders summaries where available.
#[test]
fn pipeline_with_summary_verbosity() {
    let payload = LcpEncoder::new()
        .add_code(
            Lang::Rust,
            "src/main.rs",
            b"fn main() {\n    println!(\"hello\");\n}",
        )
        .with_summary("Entry point: prints hello.")
        .add_tool_result(
            "ripgrep",
            Status::Ok,
            b"3 matches for 'Config' across 2 files.",
        )
        .encode()
        .expect("encoding should succeed");

    let decoded = LcpDecoder::decode(&payload).expect("decoding should succeed");
    let driver = DefaultDriver;

    let config = DriverConfig {
        mode: OutputMode::Xml,
        verbosity: Verbosity::Summary,
        ..DriverConfig::default()
    };
    let xml = driver
        .render(&decoded.blocks, &config)
        .expect("render should succeed");

    // Code block has summary → summary replaces content
    assert!(
        xml.contains("Entry point: prints hello."),
        "summary text should appear"
    );
    assert!(
        !xml.contains("println!"),
        "full content should not appear when using Summary verbosity"
    );
    // Tool result has no summary → renders full content
    assert!(
        xml.contains("3 matches for"),
        "tool result without summary renders full content"
    );
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
        include_types: Some(vec![bcp_types::BlockType::Conversation]),
        ..DriverConfig::default()
    };

    let result = driver
        .render(&decoded.blocks, &config)
        .expect("render should succeed");

    assert!(result.contains("[user] Hello"));
    assert!(
        !result.contains("main.rs"),
        "code block should be filtered out"
    );
}

/// Roundtrip with budget: encode blocks with priority annotations, decode,
/// render with a tight token budget, and verify budget decisions take effect.
///
/// The budget is large enough for the Critical block's full content and the
/// Normal block's summary, but not for both blocks' full content.
#[test]
fn roundtrip_with_budget() {
    let big_content = "x".repeat(800); // ~200 tokens (heuristic: chars/4)
    let payload = LcpEncoder::new()
        // Block 0: Normal priority (default), has summary
        .add_code(Lang::Rust, "src/lib.rs", big_content.as_bytes())
        .with_summary("Library exports and module declarations.")
        // Block 2: Critical priority (annotation at block 1 targets block 0... but
        // with_priority targets the last non-annotation block)
        .add_code(Lang::Rust, "src/main.rs", big_content.as_bytes())
        .with_priority(Priority::Critical)
        .encode()
        .expect("encoding should succeed");

    let decoded = LcpDecoder::decode(&payload).expect("decoding should succeed");
    let driver = DefaultDriver;

    // Budget of 250: enough for one full block (~200 tokens) + one summary (~10),
    // but not enough for two full blocks (~400)
    let config = DriverConfig {
        mode: OutputMode::Xml,
        token_budget: Some(250),
        verbosity: Verbosity::Adaptive,
        ..DriverConfig::default()
    };
    let xml = driver
        .render(&decoded.blocks, &config)
        .expect("render should succeed");

    // Critical block (src/main.rs) must render full content
    assert!(
        xml.contains("path=\"src/main.rs\""),
        "Critical block should be present"
    );
    // Normal block with summary should degrade to summary
    assert!(
        xml.contains("Library exports and module declarations."),
        "Normal block should render its summary under tight budget"
    );
}

/// Verify that Adaptive mode without a budget produces identical output
/// to Full mode for the same blocks.
#[test]
fn adaptive_mode_without_budget_matches_full() {
    let payload = LcpEncoder::new()
        .add_code(
            Lang::Rust,
            "src/main.rs",
            b"fn main() {\n    println!(\"hello\");\n}",
        )
        .with_summary("Entry point.")
        .add_tool_result("ripgrep", Status::Ok, b"3 matches found.")
        .encode()
        .expect("encoding should succeed");

    let decoded = LcpDecoder::decode(&payload).expect("decoding should succeed");
    let driver = DefaultDriver;

    let adaptive_config = DriverConfig {
        mode: OutputMode::Xml,
        verbosity: Verbosity::Adaptive,
        ..DriverConfig::default()
    };
    let full_config = DriverConfig {
        mode: OutputMode::Xml,
        verbosity: Verbosity::Full,
        ..DriverConfig::default()
    };

    let adaptive_result = driver
        .render(&decoded.blocks, &adaptive_config)
        .expect("adaptive render should succeed");
    let full_result = driver
        .render(&decoded.blocks, &full_config)
        .expect("full render should succeed");

    assert_eq!(
        adaptive_result, full_result,
        "Adaptive without budget should match Full"
    );
}

/// Multiple priorities with tight budget: verify the full degradation chain.
#[test]
fn mixed_priorities_budget_allocation() {
    let big = "y".repeat(400); // ~100 tokens each
    let payload = LcpEncoder::new()
        // Block 0: Background priority
        .add_code(Lang::Python, "bg.py", big.as_bytes())
        .with_priority(Priority::Background)
        // Block 2: Normal priority (default), has summary
        .add_code(Lang::Rust, "normal.rs", big.as_bytes())
        .with_summary("Normal block summary.")
        // Block 4: High priority
        .add_code(Lang::Go, "high.go", big.as_bytes())
        .with_priority(Priority::High)
        // Block 6: Critical priority
        .add_code(Lang::Java, "critical.java", big.as_bytes())
        .with_priority(Priority::Critical)
        .encode()
        .expect("encoding should succeed");

    let decoded = LcpDecoder::decode(&payload).expect("decoding should succeed");
    let driver = DefaultDriver;

    // Budget of 250: enough for Critical (~133tok code) + High (~133tok) but
    // Normal and Background must degrade. Using CodeAwareEstimator (chars/3 for code).
    let config = DriverConfig {
        mode: OutputMode::Minimal,
        token_budget: Some(250),
        verbosity: Verbosity::Adaptive,
        ..DriverConfig::default()
    };
    let result = driver
        .render(&decoded.blocks, &config)
        .expect("render should succeed");

    // Critical block always renders full
    assert!(
        result.contains("--- critical.java [java] ---"),
        "Critical block must render: {result}"
    );
    // High block should render (full or summary, depends on budget consumed)
    assert!(
        result.contains("high.go"),
        "High block must be present: {result}"
    );
    // Background block should be placeholder or omitted (never full)
    assert!(
        !result.contains("--- bg.py [python] ---"),
        "Background block should not render full: {result}"
    );
}
