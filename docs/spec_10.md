# bcp-tests (Golden File Test Suite)

<span class="badge badge-green">Complete</span> <span class="badge badge-blue">Phase 4</span>

> The conformance backbone of the BCP PoC. Pre-built binary fixtures, insta snapshot tests for all three render modes, round-trip byte-integrity tests, budget behavior tests, a token savings benchmark, and criterion throughput benchmarks — all in one dedicated test crate.

## Crate Info

| Field | Value |
|-------|-------|
| Path | `crates/bcp-tests/` |
| Spec | SPEC_10 |
| Dependencies | `bcp-encoder`, `bcp-decoder`, `bcp-driver`, `bcp-types`, `insta`, `criterion`, `hex`, `blake3` |
| Generator | `cargo run --bin generate_golden -p bcp-tests` |

---

## Purpose and Role in the Protocol

`bcp-tests` is the integration testing harness for the entire BCP stack. It exercises every crate end-to-end — encoding, wire format, decoding, rendering, and token estimation — from a single test runner. Rather than testing crates in isolation, it verifies the *pipeline*: that bytes flow correctly from `LcpEncoder` through `LcpDecoder` through `DefaultDriver` and produce expected textual output.

The crate has three distinct responsibilities:

```text
┌────────────────────────────────────────────────────────────────┐
│                        bcp-tests                               │
│                                                                │
│  ┌─────────────────┐  ┌──────────────────┐  ┌──────────────┐  │
│  │  Golden Fixture  │  │  Snapshot Tests  │  │  Benchmarks  │  │
│  │  Generator       │  │  (insta)         │  │  (criterion) │  │
│  │                  │  │                  │  │              │  │
│  │  Binary that     │  │  Read .lcp file  │  │  Throughput  │  │
│  │  writes .lcp     │  │  Decode + Render │  │  encode/     │  │
│  │  fixtures to     │  │  Compare vs      │  │  decode/     │  │
│  │  tests/golden/   │  │  .snap files     │  │  estimate    │  │
│  └─────────────────┘  └──────────────────┘  └──────────────┘  │
│                                                                │
│  ┌─────────────────┐  ┌──────────────────┐  ┌──────────────┐  │
│  │  Round-Trip      │  │  Budget Tests    │  │  Token       │  │
│  │  Tests           │  │                  │  │  Savings     │  │
│  │                  │  │  Priority        │  │  Tests       │  │
│  │  Encode→Decode   │  │  ordering,       │  │              │  │
│  │  →Encode         │  │  degradation     │  │  ≥30%        │  │
│  │  byte-identical  │  │  under budget    │  │  reduction   │  │
│  └─────────────────┘  └──────────────────┘  └──────────────┘  │
└────────────────────────────────────────────────────────────────┘
```

---

## Golden File Fixtures

Fixtures live in `tests/golden/`. Each fixture directory contains a `manifest.json` (human-readable description) and a `payload.lcp` (binary, committed). The generator binary writes them; the snapshot tests read them.

```text
tests/golden/
├── simple_code/               Single Rust CODE block
├── conversation/              USER + ASSISTANT turns
├── mixed_blocks/              CODE + CONVERSATION + TOOL_RESULT + FILE_TREE
├── with_summaries/            CODE + TOOL_RESULT with summary sub-blocks
├── compressed_blocks/         Per-block zstd compression
├── compressed_payload/        Whole-payload zstd compression
├── content_addressed/         Two identical CODE blocks deduplicated via BLAKE3
│   └── content_store.json     Hash → content mapping
├── budget_constrained/        CRITICAL + NORMAL + BACKGROUND priorities
├── all_block_types/           One of each of the 11 semantic block types
└── edge_cases/
    ├── empty_content/         CODE block with zero-length content field
    ├── large_varint/          CODE block with 16 KiB content (3-byte LEB128 varint)
    ├── unknown_block_type/    Handcrafted: type_id=0x42, body=b"hello"
    └── trailing_data/         Valid payload + 4 extra bytes after END sentinel
```

### Regenerating Fixtures

Fixtures are committed to the repository and only need to be regenerated when the wire format changes:

```bash
cargo run --bin generate_golden -p bcp-tests
git diff tests/golden/   # review the binary diff
```

The generator uses `LcpEncoder` for semantic fixtures and hand-crafts bytes for `unknown_block_type` and `trailing_data` using the wire-layer `BlockFrame` API.

---

## Test Files

### `tests/roundtrip.rs` — 16 tests

Verifies encode → decode → encode produces byte-identical output for all 11 block types, plus semantic equivalence for compressed payloads.

```text
┌─────────────────────────────┬──────────────────────────────────────────┐
│ Test                        │ Block type / invariant tested            │
├─────────────────────────────┼──────────────────────────────────────────┤
│ roundtrip_code_block        │ CODE (no line range)                     │
│ roundtrip_code_range        │ CODE with line_start + line_end          │
│ roundtrip_conversation      │ CONVERSATION (no tool_call_id)           │
│ roundtrip_conversation_tool │ CONVERSATION with tool_call_id           │
│ roundtrip_file_tree         │ FILE_TREE with nested directory entries  │
│ roundtrip_tool_result       │ TOOL_RESULT (Status::Ok)                 │
│ roundtrip_document          │ DOCUMENT (FormatHint::Markdown)          │
│ roundtrip_structured_data   │ STRUCTURED_DATA (DataFormat::Json)       │
│ roundtrip_diff              │ DIFF with one DiffHunk                   │
│ roundtrip_annotation        │ ANNOTATION block (add_annotation)        │
│ roundtrip_embedding_ref     │ EMBEDDING_REF with 32-byte source_hash   │
│ roundtrip_image             │ IMAGE (MediaType::Png)                   │
│ roundtrip_extension         │ EXTENSION with namespace + type_name     │
│ roundtrip_block_with_summary│ CODE + HAS_SUMMARY flag set              │
│ roundtrip_compressed_blocks │ Per-block zstd: semantic equivalence     │
│ roundtrip_compressed_payload│ Whole-payload zstd: semantic equivalence │
└─────────────────────────────┴──────────────────────────────────────────┘
```

The `encode_from_blocks` helper reconstructs an `LcpEncoder` payload from a `&[Block]` slice by pattern-matching all `BlockContent` variants. Byte-identical assertions hold for uncompressed payloads; compressed payloads compare decoded content only (the `COMPRESSED` flag is a storage hint, not semantic content).

### `tests/conformance.rs` — 27 tests

Snapshot tests using `insta`. Each test reads a golden `.lcp` file, decodes it, renders it in one output mode, and compares against a committed `.snap` file.

```text
┌────────────────────────────┬──────┬────────────┬─────────┐
│ Fixture                    │ XML  │ Markdown   │ Minimal │
├────────────────────────────┼──────┼────────────┼─────────┤
│ simple_code                │  ✓   │     ✓      │    ✓    │
│ conversation               │  ✓   │     ✓      │    ✓    │
│ mixed_blocks               │  ✓   │     ✓      │    ✓    │
│ with_summaries             │  ✓   │     ✓      │    ✓    │
│ compressed_blocks          │  ✓   │     ✓      │    ✓    │
│ compressed_payload         │  ✓   │     ✓      │    ✓    │
│ content_addressed          │  ✓   │     ✓      │    ✓    │
│ budget_constrained (500t)  │  ✓   │            │    ✓    │
│ budget_constrained (200t)  │  ✓   │            │         │
│ all_block_types            │  ✓   │     ✓      │    ✓    │
└────────────────────────────┴──────┴────────────┴─────────┘
```

**Insta snapshot workflow:**

```bash
# First run: generates .snap.new pending files, tests fail
cargo test -p bcp-tests --test conformance

# Review and approve each snapshot interactively
cargo insta review

# Subsequent runs compare against committed .snap files
cargo test -p bcp-tests --test conformance
```

Snapshot files live in `tests/snapshots/` and are committed alongside the `.lcp` fixtures. Any renderer change will produce a clear diff.

### `tests/budget.rs` — 6 tests

Verifies the token budget engine's priority-based degradation rules.

```text
┌────────────────────────────────────┬────────────────────────────────────────┐
│ Test                               │ Invariant verified                     │
├────────────────────────────────────┼────────────────────────────────────────┤
│ budget_critical_always_included    │ CRITICAL → Full, even over budget      │
│ budget_background_omitted_first    │ BACKGROUND omitted before NORMAL/LOW   │
│ budget_no_budget_renders_all       │ token_budget: None → all blocks Full   │
│ budget_summary_used_under_pressure │ HIGH falls back to Summary, never Omit │
│ budget_type_filter_independent     │ include_types filtering before budget  │
│ budget_priority_ordering_verified  │ CRITICAL present, BACKGROUND absent    │
└────────────────────────────────────┴────────────────────────────────────────┘
```

### `tests/edge_cases.rs` — 6 tests

```text
┌──────────────────────────────────────┬────────────────────────────────────────┐
│ Test                                 │ Invariant verified                     │
├──────────────────────────────────────┼────────────────────────────────────────┤
│ unknown_block_type_preserved         │ type_id=0x42 → BlockContent::Unknown   │
│ unknown_block_type_reencodes_identical│ Unknown body bytes round-trip intact  │
│ empty_content_valid                  │ CODE with b"" is valid                 │
│ empty_content_golden                 │ Golden fixture decodes to 1 block      │
│ large_varint_roundtrip               │ 16 KiB content → 3-byte LEB128 varint │
│ trailing_data_warning                │ Ok or TrailingData { extra_bytes: 4 }  │
└──────────────────────────────────────┴────────────────────────────────────────┘
```

### `tests/token_savings.rs` — 3 tests

Benchmarks the core value proposition: LCP Minimal mode uses ≥30% fewer tokens than equivalent raw markdown for the same semantic content.

```text
┌─────────────────────────────────┬────────────────────────────────────────┐
│ Test                            │ Assertion                              │
├─────────────────────────────────┼────────────────────────────────────────┤
│ token_savings_vs_markdown       │ HeuristicEstimator: ≥30% savings       │
│ code_aware_estimator_savings    │ CodeAwareEstimator: ≥25% savings       │
│ xml_mode_vs_markdown            │ XML mode vs markdown: ≥5% savings      │
└─────────────────────────────────┴────────────────────────────────────────┘
```

Actual results with a representative payload (5 code files, 2 turns, 1 tool result, 1 file tree):

```
Markdown tokens: 991
Minimal tokens:  672
Savings:         32.2%
```

---

## Criterion Benchmarks

Three benchmark binaries measure throughput and latency across the BCP pipeline.

### `benches/encode.rs`

```text
┌────────────────────────────┬────────────────────────────────────────────┐
│ Benchmark                  │ What it measures                           │
├────────────────────────────┼────────────────────────────────────────────┤
│ encode_small               │ Single 43-byte CODE block                  │
│ encode_medium              │ 4 heterogeneous blocks, ~4 KB content      │
│ encode_compression/no_comp │ Two CODE blocks, no compression            │
│ encode_compression/per_block│ Two CODE blocks, per-block zstd           │
│ encode_compression/whole   │ Two CODE blocks, whole-payload zstd        │
│ encode_throughput/1kb      │ Throughput (MB/s) for 1 KB payload         │
│ encode_throughput/10kb     │ Throughput (MB/s) for 10 KB payload        │
│ encode_throughput/100kb    │ Throughput (MB/s) for 100 KB payload       │
└────────────────────────────┴────────────────────────────────────────────┘
```

### `benches/decode.rs`

```text
┌───────────────────────────────┬────────────────────────────────────────┐
│ Benchmark                     │ What it measures                       │
├───────────────────────────────┼────────────────────────────────────────┤
│ decode_small                  │ Single CODE block                      │
│ decode_medium                 │ 4 blocks, ~4 KB content                │
│ decode_compression/uncomp     │ Uncompressed 2-block payload           │
│ decode_compression/per_block  │ Per-block zstd 2-block payload         │
│ decode_compression/whole      │ Whole-payload zstd 2-block payload     │
│ decode_throughput/1kb         │ Throughput (MB/s) for 1 KB payload     │
│ decode_throughput/10kb        │ Throughput (MB/s) for 10 KB payload    │
│ decode_throughput/100kb       │ Throughput (MB/s) for 100 KB payload   │
└───────────────────────────────┴────────────────────────────────────────┘
```

### `benches/token_savings.rs`

```text
┌───────────────────────────┬────────────────────────────────────────────┐
│ Benchmark                 │ What it measures                           │
├───────────────────────────┼────────────────────────────────────────────┤
│ estimate_heuristic        │ HeuristicEstimator on rendered output      │
│ estimate_code_aware       │ CodeAwareEstimator on rendered output      │
│ full_pipeline             │ Encode → Decode → Render → Estimate        │
└───────────────────────────┴────────────────────────────────────────────┘
```

---

## Module Map

```text
crates/bcp-tests/
├── Cargo.toml                  Package manifest; criterion bench targets
├── src/
│   └── bin/
│       └── generate_golden.rs  Fixture generator binary (14 fixtures, 25 files)
├── tests/
│   ├── golden/                 Committed binary fixtures + manifests
│   │   ├── simple_code/
│   │   ├── conversation/
│   │   ├── mixed_blocks/
│   │   ├── with_summaries/
│   │   ├── compressed_blocks/
│   │   ├── compressed_payload/
│   │   ├── content_addressed/
│   │   ├── budget_constrained/
│   │   ├── all_block_types/
│   │   └── edge_cases/
│   │       ├── empty_content/
│   │       ├── large_varint/
│   │       ├── unknown_block_type/
│   │       └── trailing_data/
│   ├── snapshots/              Committed insta snapshot files (.snap)
│   ├── roundtrip.rs            16 tests — byte-identical encode→decode→encode
│   ├── conformance.rs          27 tests — golden file snapshot tests
│   ├── budget.rs               6 tests — budget engine behavior
│   ├── edge_cases.rs           6 tests — forward compat, empty fields, trailing data
│   └── token_savings.rs        3 tests — ≥30% structural overhead reduction
└── benches/
    ├── encode.rs               8 criterion benchmarks — encoding throughput
    ├── decode.rs               8 criterion benchmarks — decoding throughput
    └── token_savings.rs        3 criterion benchmarks — estimation speed
```

---

## Build & Test

```bash
# Generate golden fixture files (run when wire format changes)
cargo run --bin generate_golden -p bcp-tests

# Run all integration tests
cargo test -p bcp-tests

# Run all workspace tests
cargo test --workspace

# First-time snapshot review (after conformance tests generate .snap.new files)
INSTA_UPDATE=always cargo test -p bcp-tests --test conformance
# — or interactive review:
cargo insta review

# Run a specific test file
cargo test -p bcp-tests --test roundtrip
cargo test -p bcp-tests --test budget
cargo test -p bcp-tests --test edge_cases
cargo test -p bcp-tests --test token_savings -- --nocapture

# Run criterion benchmarks (takes ~minutes)
cargo bench -p bcp-tests --bench encode
cargo bench -p bcp-tests --bench decode
cargo bench -p bcp-tests --bench token_savings

# Full workspace validation
cargo clippy --workspace -- -W clippy::pedantic
cargo doc --workspace --no-deps
```
