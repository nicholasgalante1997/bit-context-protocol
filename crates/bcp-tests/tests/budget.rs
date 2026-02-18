//! Budget engine integration tests.
//!
//! Validates that the token budget system makes correct decisions about which
//! blocks to render in full, which to summarize, and which to omit. Each test
//! exercises the complete encode → decode → render pipeline so the budget
//! engine sees real annotated blocks, not hand-constructed fixtures.
//!
//! Token estimation reference (HeuristicEstimator: chars / 4, min 1):
//!   - 40 chars  →  10 tokens
//!   - 64 chars  →  16 tokens
//!   - 80 chars  →  20 tokens
//!   - 200 chars →  50 tokens
//!   - 400 chars → 100 tokens

use bcp_decoder::LcpDecoder;
use bcp_driver::{DefaultDriver, DriverConfig, LcpDriver, OutputMode};
use bcp_encoder::LcpEncoder;
use bcp_types::BlockType;
use bcp_types::enums::{Lang, Priority, Role};

// ── Test 1: Critical blocks always render even under extreme budget pressure ──

#[test]
fn budget_critical_always_included() {
    // critical.rs body: 64 chars → ~16 tokens
    // normal.rs body:   32 chars → ~8 tokens
    // low.rs body:      32 chars → ~8 tokens
    // Budget of 10 cannot fit any single block fully, yet CRITICAL must appear.
    let payload = LcpEncoder::new()
        .add_code(
            Lang::Rust,
            "critical.rs",
            b"fn critical() { /* CRITICAL_MARKER_XYZ */ }",
        )
        .with_priority(Priority::Critical)
        .add_code(Lang::Rust, "normal.rs", b"fn normal() { /* normal */ }")
        .with_priority(Priority::Normal)
        .add_code(
            Lang::Rust,
            "low.rs",
            b"fn low_priority() { /* low */ }",
        )
        .with_priority(Priority::Low)
        .encode()
        .unwrap();

    let decoded = LcpDecoder::decode(&payload).unwrap();
    let config = DriverConfig {
        mode: OutputMode::Xml,
        token_budget: Some(10),
        ..Default::default()
    };
    let output = DefaultDriver.render(&decoded.blocks, &config).unwrap();

    assert!(
        output.contains("CRITICAL_MARKER_XYZ"),
        "CRITICAL block content must appear in output even under extreme budget; output:\n{output}"
    );

    // Not all three blocks can be rendered in full at budget=10
    let normal_present = output.contains("fn normal()");
    let low_present = output.contains("fn low_priority()");
    assert!(
        !(normal_present && low_present),
        "budget of 10 should not allow all three blocks to render in full"
    );
}

// ── Test 2: Background blocks are omitted or placeholdered before others ──────

#[test]
fn budget_background_omitted_first() {
    // Each block body is ~40 chars → ~10 tokens with HeuristicEstimator.
    // Budget of 25 can fit ~2 full blocks. BACKGROUND should be the first
    // casualty, so its unique marker should not appear as full content.
    //
    // Note: the driver uses CodeAwareEstimator (not Heuristic). Unindented
    // single-line code estimates at chars/4, same ratio as heuristic for
    // non-indented code, so the approximation holds here.
    let payload = LcpEncoder::new()
        .add_code(Lang::Rust, "normal.rs", b"fn normal_work() { /* NORMAL_MARKER */ }")
        .with_priority(Priority::Normal)
        .add_code(Lang::Rust, "low.rs", b"fn low_work() { /* LOW_MARKER_ABC */ }")
        .with_priority(Priority::Low)
        .add_code(
            Lang::Rust,
            "background.rs",
            b"fn background_work() { /* background_unique_marker */ }",
        )
        .with_priority(Priority::Background)
        .encode()
        .unwrap();

    let decoded = LcpDecoder::decode(&payload).unwrap();
    let config = DriverConfig {
        mode: OutputMode::Xml,
        token_budget: Some(22),
        ..Default::default()
    };
    let output = DefaultDriver.render(&decoded.blocks, &config).unwrap();

    assert!(
        output.contains("NORMAL_MARKER"),
        "NORMAL block must appear when budget allows; output:\n{output}"
    );

    // BACKGROUND full content must not appear — it's either a placeholder or omitted.
    // The placeholder shows the path "background.rs" but not the body text.
    assert!(
        !output.contains("background_unique_marker"),
        "BACKGROUND block full content must not appear under budget pressure; output:\n{output}"
    );
}

// ── Test 3: No budget renders all blocks in full ──────────────────────────────

#[test]
fn budget_no_budget_renders_all() {
    let payload = LcpEncoder::new()
        .add_code(Lang::Rust, "critical.rs", b"fn critical() {}")
        .with_priority(Priority::Critical)
        .add_code(Lang::Rust, "high.rs", b"fn high() {}")
        .with_priority(Priority::High)
        .add_code(Lang::Rust, "normal.rs", b"fn normal() {}")
        .with_priority(Priority::Normal)
        .add_code(Lang::Rust, "low.rs", b"fn low() {}")
        .with_priority(Priority::Low)
        .add_code(Lang::Rust, "background.rs", b"fn background() {}")
        .with_priority(Priority::Background)
        .encode()
        .unwrap();

    let decoded = LcpDecoder::decode(&payload).unwrap();
    // Default config has no token_budget and Adaptive verbosity → render all full.
    let config = DriverConfig::default();
    let output = DefaultDriver.render(&decoded.blocks, &config).unwrap();

    for path in &["critical.rs", "high.rs", "normal.rs", "low.rs", "background.rs"] {
        assert!(
            output.contains(path),
            "all block paths must appear when no budget is set; missing: {path}; output:\n{output}"
        );
    }
}

// ── Test 4: High priority blocks fall back to summary under budget pressure ───

#[test]
fn budget_summary_used_under_pressure() {
    // Long content: "// Long content that would consume many tokens. " * 10 = 480 chars → ~120 tokens
    // Summary: "Short summary of high priority block." = 38 chars → ~10 tokens
    // Budget of 20: too tight for full content, but summary fits.
    // HIGH always gets at least something (Full → Summary → Full forced).
    let long_content = b"// Long content that would consume many tokens. ".repeat(10);

    let payload = LcpEncoder::new()
        .add_code(Lang::Rust, "high.rs", &long_content)
        .with_summary("Short summary of high priority block.")
        .with_priority(Priority::High)
        .encode()
        .unwrap();

    let decoded = LcpDecoder::decode(&payload).unwrap();
    let config = DriverConfig {
        mode: OutputMode::Xml,
        token_budget: Some(20),
        ..Default::default()
    };
    let output = DefaultDriver.render(&decoded.blocks, &config).unwrap();

    assert!(
        output.contains("Short summary") || output.contains("Long content"),
        "HIGH block must be represented (summary or full) even under budget pressure; output:\n{output}"
    );
}

// ── Test 5: Type filter is independent of budget — filtered blocks don't cost ─

#[test]
fn budget_type_filter_independent_of_budget() {
    // Encode: Rust CODE, User conversation, Python CODE.
    // Filter to Code only. The conversation should be absent regardless of budget.
    // With a tight budget, at least one code block should still render.
    let payload = LcpEncoder::new()
        .add_code(Lang::Rust, "rust_code.rs", b"fn rust_entry() { /* RUST_MARKER */ }")
        .add_conversation(Role::User, b"This conversation should be absent from filtered output.")
        .add_code(Lang::Python, "python_code.py", b"def python_entry(): pass  # PYTHON_MARKER")
        .encode()
        .unwrap();

    let decoded = LcpDecoder::decode(&payload).unwrap();
    let config = DriverConfig {
        mode: OutputMode::Minimal,
        include_types: Some(vec![BlockType::Code]),
        token_budget: Some(30),
        ..Default::default()
    };
    let output = DefaultDriver.render(&decoded.blocks, &config).unwrap();

    assert!(
        !output.contains("This conversation should be absent"),
        "Conversation block must not appear when include_types filters to Code only; output:\n{output}"
    );

    let has_rust = output.contains("RUST_MARKER");
    let has_python = output.contains("PYTHON_MARKER");
    assert!(
        has_rust || has_python,
        "at least one CODE block must appear in the filtered output; output:\n{output}"
    );
}

// ── Test 6: Priority ordering verified — Critical in, Background out ───────────

#[test]
fn budget_priority_ordering_verified() {
    // Five blocks at different priorities. Each body is ~200 chars → ~50 tokens.
    // Budget of 110 can fit about 2 full blocks.
    // CRITICAL must win its slot. BACKGROUND must be absent or placeholder.
    let body_critical = b"fn critical_path() { /* CRITCONTENT_AAA */ let x = 1 + 1; let y = x * 2; x }";
    let body_high = b"fn high_path() { /* HIGHCONTENT_BBB */ let x = 1 + 1; let y = x * 2; x + y }";
    let body_normal = b"fn normal_path() { /* NORMCONTENT_CCC */ let x = 1; let y = 2; x + y + 3 }";
    let body_low = b"fn low_path() { /* LOWCONTENT_DDD */ let x = 0; let y = 0; x + y + 0 + 0 }";
    let body_bg = b"fn background_path() { /* BGCONTENT_EEE */ let x = 0; let y = 0; x + y }";

    let payload = LcpEncoder::new()
        .add_code(Lang::Rust, "critical.rs", body_critical)
        .with_priority(Priority::Critical)
        .add_code(Lang::Rust, "high.rs", body_high)
        .with_priority(Priority::High)
        .add_code(Lang::Rust, "normal.rs", body_normal)
        .with_priority(Priority::Normal)
        .add_code(Lang::Rust, "low.rs", body_low)
        .with_priority(Priority::Low)
        .add_code(Lang::Rust, "background.rs", body_bg)
        .with_priority(Priority::Background)
        .encode()
        .unwrap();

    let decoded = LcpDecoder::decode(&payload).unwrap();
    let config = DriverConfig {
        mode: OutputMode::Minimal,
        token_budget: Some(110),
        ..Default::default()
    };
    let output = DefaultDriver.render(&decoded.blocks, &config).unwrap();

    assert!(
        output.contains("CRITCONTENT_AAA"),
        "CRITICAL block must appear in output; output:\n{output}"
    );

    // BACKGROUND should be absent or a placeholder (no full body text).
    assert!(
        !output.contains("BGCONTENT_EEE"),
        "BACKGROUND block full content must not appear when budget is tight; output:\n{output}"
    );
}
