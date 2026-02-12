# SPEC_10 — Golden File Test Suite

**Location**: `tests/`
**Phase**: 4 (Tooling)
**Prerequisites**: SPEC_01 through SPEC_08
**Dependencies**: All crates, `insta`

---

## Context

The golden file test suite is the conformance backbone of the PoC. It provides
pre-built `.lcp` fixture files with known contents and expected rendered
output in all three modes (XML, Markdown, Minimal). Tests use `insta` for
snapshot testing: expected output is stored as files and compared against
actual output. Any drift produces a clear diff and a test failure.

Beyond snapshot tests, the suite includes round-trip tests (encode → decode →
compare), budget behavior tests, and compression conformance tests.

---

## Requirements

### 1. Golden File Fixtures

Each fixture consists of:
- A `.json` manifest describing the blocks to encode
- The resulting `.lcp` binary (generated and committed)
- Expected rendered output for each mode: `.xml.expected`, `.md.expected`, `.min.expected`

```
tests/golden/
├── simple_code/
│   ├── manifest.json
│   ├── payload.lcp
│   ├── output.xml.expected
│   ├── output.md.expected
│   └── output.min.expected
│
├── conversation/
│   ├── manifest.json
│   ├── payload.lcp
│   ├── output.xml.expected
│   ├── output.md.expected
│   └── output.min.expected
│
├── mixed_blocks/
│   ├── manifest.json          # CODE + CONVERSATION + TOOL_RESULT + FILE_TREE
│   ├── payload.lcp
│   ├── output.xml.expected
│   ├── output.md.expected
│   └── output.min.expected
│
├── with_summaries/
│   ├── manifest.json          # Blocks with summary sub-blocks
│   ├── payload.lcp
│   ├── output.xml.expected
│   ├── output.md.expected
│   └── output.min.expected
│
├── compressed_blocks/
│   ├── manifest.json          # Per-block zstd compression
│   ├── payload.lcp
│   ├── output.xml.expected    # Same as uncompressed (transparent)
│   ├── output.md.expected
│   └── output.min.expected
│
├── compressed_payload/
│   ├── manifest.json          # Whole-payload zstd compression
│   ├── payload.lcp
│   ├── output.xml.expected
│   ├── output.md.expected
│   └── output.min.expected
│
├── content_addressed/
│   ├── manifest.json          # Blocks with BLAKE3 references
│   ├── payload.lcp
│   ├── content_store.json     # Hash→content mapping
│   ├── output.xml.expected
│   ├── output.md.expected
│   └── output.min.expected
│
├── budget_constrained/
│   ├── manifest.json          # Mixed priorities, tight budget
│   ├── payload.lcp
│   ├── output.xml.budget500.expected
│   ├── output.xml.budget200.expected
│   └── output.min.budget500.expected
│
├── all_block_types/
│   ├── manifest.json          # One of each block type
│   ├── payload.lcp
│   ├── output.xml.expected
│   ├── output.md.expected
│   └── output.min.expected
│
└── edge_cases/
    ├── empty_content/
    │   ├── manifest.json      # Blocks with empty content fields
    │   └── payload.lcp
    ├── large_varint/
    │   ├── manifest.json      # Block with content_len > 16KB
    │   └── payload.lcp
    ├── unknown_block_type/
    │   └── payload.lcp        # Handcrafted: block type 0x42
    └── trailing_data/
        └── payload.lcp        # Valid payload + extra bytes after END
```

### 2. Snapshot Conformance Tests

```rust
use insta::assert_snapshot;
use lcp_decoder::LcpDecoder;
use lcp_driver::{DefaultDriver, DriverConfig, LcpDriver, OutputMode};

/// Test that each golden file produces expected output in each mode.
///
/// For each fixture directory:
///   1. Read the payload.lcp file
///   2. Decode it with LcpDecoder
///   3. Render with DefaultDriver in each mode
///   4. Compare against the .expected file using insta
#[test]
fn golden_simple_code_xml() {
    let payload = std::fs::read("tests/golden/simple_code/payload.lcp").unwrap();
    let decoded = LcpDecoder::decode(&payload).unwrap();
    let config = DriverConfig {
        mode: OutputMode::Xml,
        ..Default::default()
    };
    let output = DefaultDriver.render(&decoded.blocks, &config).unwrap();
    assert_snapshot!("simple_code_xml", output);
}

// Repeat for each fixture × each mode...
```

### 3. Round-Trip Tests

```rust
/// Encode → decode → encode → compare bytes.
///
/// For each block type, construct a block with representative field
/// values, encode it, decode it, re-encode it, and verify the two
/// payloads are byte-identical.
#[test]
fn roundtrip_code_block() {
    let original = LcpEncoder::new()
        .add_code(Lang::Rust, "src/main.rs", b"fn main() {}")
        .encode()
        .unwrap();

    let decoded = LcpDecoder::decode(&original).unwrap();

    // Re-encode from decoded blocks
    let re_encoded = encode_from_blocks(&decoded.blocks).unwrap();

    assert_eq!(original, re_encoded);
}

/// Round-trip with compression: compressed payload should decode
/// to identical blocks as uncompressed payload.
#[test]
fn roundtrip_compressed() {
    let mut encoder = LcpEncoder::new();
    encoder.add_code(Lang::Rust, "src/main.rs", &large_rust_content());
    encoder.compress_blocks();

    let compressed = encoder.encode().unwrap();
    let decoded = LcpDecoder::decode(&compressed).unwrap();

    // Compare against uncompressed encode → decode
    let mut uncompressed_encoder = LcpEncoder::new();
    uncompressed_encoder.add_code(Lang::Rust, "src/main.rs", &large_rust_content());
    let uncompressed = uncompressed_encoder.encode().unwrap();
    let uncompressed_decoded = LcpDecoder::decode(&uncompressed).unwrap();

    assert_eq!(decoded.blocks, uncompressed_decoded.blocks);
}
```

### 4. Budget Behavior Tests

```rust
/// Test that the budget engine makes correct decisions.
#[test]
fn budget_critical_always_included() {
    // Encode 3 blocks: one CRITICAL, one NORMAL, one LOW
    // Set budget to fit only 1 block
    // Verify CRITICAL block is rendered fully
    // Verify others are summarized or omitted
}

#[test]
fn budget_background_omitted_first() {
    // Encode 3 blocks: NORMAL, LOW, BACKGROUND
    // Set budget to fit 2 blocks
    // Verify BACKGROUND is omitted first
}

#[test]
fn budget_no_budget_renders_all() {
    // Encode 5 blocks with various priorities
    // Set no budget (token_budget = None)
    // Verify all blocks are rendered fully
}
```

### 5. Edge Case Tests

```rust
/// Unknown block types are preserved, not errors.
#[test]
fn unknown_block_type_preserved() {
    let payload = std::fs::read("tests/golden/edge_cases/unknown_block_type/payload.lcp").unwrap();
    let decoded = LcpDecoder::decode(&payload).unwrap();
    assert!(decoded.blocks.iter().any(|b| matches!(&b.content, BlockContent::Unknown { type_id: 0x42, .. })));
}

/// Empty content fields are valid.
#[test]
fn empty_content_valid() {
    let payload = LcpEncoder::new()
        .add_code(Lang::Rust, "empty.rs", b"")
        .encode()
        .unwrap();
    let decoded = LcpDecoder::decode(&payload).unwrap();
    assert_eq!(decoded.blocks.len(), 1);
}

/// Trailing data after END sentinel produces warning, not error.
#[test]
fn trailing_data_warning() {
    let payload = std::fs::read("tests/golden/edge_cases/trailing_data/payload.lcp").unwrap();
    let result = LcpDecoder::decode(&payload);
    // Should succeed (warning-level) or produce TrailingData error depending on strictness
    assert!(result.is_ok() || matches!(result, Err(DecodeError::TrailingData { .. })));
}
```

### 6. Token Savings Benchmark Test

```rust
/// Compare token counts between LCP Minimal mode and raw markdown.
///
/// This is the core value proposition test: LCP should use fewer
/// tokens for the same semantic content.
#[test]
fn token_savings_vs_markdown() {
    let estimator = HeuristicEstimator;

    // Build a representative context: 5 code files, 2 conversation turns,
    // 1 tool result, 1 file tree
    let payload = build_representative_payload();
    let decoded = LcpDecoder::decode(&payload).unwrap();

    // Render in Minimal mode
    let minimal_config = DriverConfig {
        mode: OutputMode::Minimal,
        ..Default::default()
    };
    let minimal_output = DefaultDriver.render(&decoded.blocks, &minimal_config).unwrap();
    let minimal_tokens = estimator.estimate(&minimal_output);

    // Build equivalent raw markdown
    let markdown_equivalent = build_equivalent_markdown(&decoded.blocks);
    let markdown_tokens = estimator.estimate(&markdown_equivalent);

    let savings_pct = (1.0 - minimal_tokens as f64 / markdown_tokens as f64) * 100.0;

    println!("Markdown tokens: {markdown_tokens}");
    println!("Minimal tokens:  {minimal_tokens}");
    println!("Savings:         {savings_pct:.1}%");

    // Target: ≥30% structural overhead reduction
    assert!(savings_pct >= 30.0, "Expected ≥30% savings, got {savings_pct:.1}%");
}
```

---

## File Structure

```
tests/
├── golden/                     # Golden file fixtures (described above)
├── roundtrip.rs                # Encode → decode → compare tests
├── conformance.rs              # Golden file snapshot tests
├── budget.rs                   # Token budget behavior tests
├── edge_cases.rs               # Edge case and error handling tests
└── token_savings.rs            # Token count comparison benchmark

benches/
├── encode.rs                   # Encoding throughput (criterion)
├── decode.rs                   # Decoding throughput
└── token_savings.rs            # Token savings benchmark
```

---

## Acceptance Criteria

- [ ] All golden file snapshot tests pass (`cargo insta test`)
- [ ] Round-trip tests pass for all 11 block types
- [ ] Compressed round-trip tests pass (per-block and whole-payload)
- [ ] Content-addressed round-trip tests pass
- [ ] Budget behavior tests verify correct priority ordering
- [ ] Unknown block type test passes (forward compatibility)
- [ ] Empty content test passes
- [ ] Token savings test achieves ≥30% reduction vs. markdown
- [ ] All edge case tests pass
- [ ] `cargo test --workspace` exits with code 0
- [ ] Golden files are committed to the repository

---

## Verification

```bash
# Run all tests
cargo test --workspace

# Run snapshot tests specifically
cargo insta test

# Review any snapshot changes
cargo insta review

# Run benchmarks
cargo bench --bench encode
cargo bench --bench decode
cargo bench --bench token_savings

# Full workspace validation
cargo clippy --workspace -- -W clippy::pedantic
cargo doc --workspace --no-deps
```
