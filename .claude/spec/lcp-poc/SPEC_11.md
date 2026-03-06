# SPEC_11 — Tiktoken Benchmark Harness

**Location**: `crates/bcp-bench-real/`
**Phase**: 5 (Validation)
**Prerequisites**: SPEC_01 through SPEC_10 (all Phase 1–4 complete)
**Dependencies**: `bcp-encoder`, `bcp-decoder`, `bcp-driver`, `bcp-types`, `tiktoken-rs`, `serde`, `serde_json`, `criterion`

---

## Context

The RFC §6 token efficiency analysis contains theoretical estimates ("~67% overhead
reduction for code blocks", "~80% for tool results"). These numbers were derived from
character-count heuristics (`HeuristicEstimator`, `CodeAwareEstimator`) applied to
synthetic payloads. The existing `bcp-tests` benchmarks (SPEC_10) validate throughput
and heuristic estimation, but they do not answer the core question: **how many real
BPE tokens does BCP save compared to the markdown context that tools actually produce?**

This spec introduces a separate crate (`bcp-bench-real`) that:

1. Captures real-world context payloads from Claude Code sessions
2. Encodes them as BCP, renders in all three modes
3. Counts tokens using `tiktoken-rs` (`cl100k_base` — the standard BPE tokenizer
   used by GPT-4 and close enough to Claude's tokenizer for comparative benchmarks)
4. Constructs two "equivalent markdown" baselines (naive and realistic)
5. Produces a token savings table that replaces the RFC §6 estimates with measured data

This is a separate crate from `bcp-tests` because the concerns are different:
`bcp-tests` validates conformance and correctness; `bcp-bench-real` validates the
value proposition against real tokenizer output.

---

## Requirements

### 1. Crate Setup

A new workspace member `crates/bcp-bench-real/` with the following structure.
No `lib.rs` — this crate is purely a benchmark and binary harness.

```toml
[package]
name = "bcp-bench-real"
version = "0.1.0"
edition = "2024"
publish = false

[[bin]]
name = "bench_tokens"
path = "src/bin/bench_tokens.rs"

[[bin]]
name = "capture_session"
path = "src/bin/capture_session.rs"

[[bench]]
name = "tiktoken"
harness = false

[dependencies]
bcp-encoder = { path = "../bcp-encoder" }
bcp-decoder = { path = "../bcp-decoder" }
bcp-driver  = { path = "../bcp-driver" }
bcp-types   = { path = "../bcp-types" }
tiktoken-rs = "0.6"
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
anyhow      = { workspace = true }
criterion   = { workspace = true }
```

Add `"crates/bcp-bench-real"` to the workspace `members` array in the root `Cargo.toml`.

Add `tiktoken-rs = "0.6"` and `serde_json = "1"` to `[workspace.dependencies]`.

### 2. Session Fixture Format

Real context payloads are captured as JSON fixture files. Each fixture represents
a single agent session's context window — the blocks that would be sent to the model.

```
crates/bcp-bench-real/
└── fixtures/
    ├── real_session_small.json     # ~5 files, 2 turns, 1 tool result
    ├── real_session_medium.json    # ~15 files, 5 turns, 3 tool results
    ├── real_session_large.json     # ~30 files, 10 turns, 8 tool results
    └── schema.json                 # JSON schema for fixture format
```

The fixture JSON schema:

```json
{
  "session_id": "string (optional, for provenance)",
  "captured_at": "ISO 8601 timestamp",
  "description": "Human-readable description of the session",
  "blocks": [
    {
      "type": "code",
      "language": "rust",
      "path": "src/main.rs",
      "content": "fn main() { ... }",
      "summary": "Entry point with CLI arg parsing."
    },
    {
      "type": "conversation",
      "role": "user",
      "content": "Fix the timeout bug in the connection pool."
    },
    {
      "type": "conversation",
      "role": "assistant",
      "content": "I'll examine the pool configuration..."
    },
    {
      "type": "tool_result",
      "tool_name": "ripgrep",
      "status": "ok",
      "content": "src/pool.rs:42: pub timeout: Duration,\nsrc/pool.rs:87: .timeout(config.timeout)"
    },
    {
      "type": "file_tree",
      "root_path": "src/",
      "entries": [
        { "name": "main.rs", "kind": "file", "size": 1234 },
        { "name": "pool.rs", "kind": "file", "size": 5678 },
        { "name": "config/", "kind": "dir", "children": [
          { "name": "mod.rs", "kind": "file", "size": 890 }
        ]}
      ]
    },
    {
      "type": "document",
      "title": "README.md",
      "content": "# Project\n\nA connection pool library...",
      "format_hint": "markdown"
    },
    {
      "type": "structured_data",
      "format": "json",
      "content": "{\"timeout_ms\": 5000, \"max_connections\": 10}"
    },
    {
      "type": "diff",
      "path": "src/pool.rs",
      "hunks": [
        {
          "old_start": 42,
          "new_start": 42,
          "lines": [
            { "kind": "context", "content": "    pub timeout: Duration," },
            { "kind": "remove",  "content": "    pub max_retries: u32," },
            { "kind": "add",     "content": "    pub max_retries: u32," },
            { "kind": "add",     "content": "    pub retry_backoff: Duration," }
          ]
        }
      ]
    }
  ]
}
```

### 3. Session Capture Binary

`src/bin/capture_session.rs` — a helper that constructs fixture JSON from raw inputs.
Two capture modes:

**Mode A: From directory scan.** Point it at a project directory and it builds a
realistic session fixture by reading source files, constructing a file tree, and
inserting synthetic conversation turns.

```
cargo run -p bcp-bench-real --bin capture_session -- \
    --mode dir-scan \
    --path ./crates/bcp-encoder/src \
    --max-files 15 \
    --output fixtures/real_session_medium.json
```

**Mode B: From transcript JSON.** Accept a JSON file matching the schema above,
validate it, and normalize paths.

```
cargo run -p bcp-bench-real --bin capture_session -- \
    --mode transcript \
    --input raw_transcript.json \
    --output fixtures/real_session_large.json
```

### 4. Fixture-to-BCP Encoder

A shared module `src/fixture.rs` that reads a session fixture JSON and produces
a `Vec<u8>` BCP payload using `BcpEncoder`.

```rust
use anyhow::Result;
use bcp_encoder::BcpEncoder;
use bcp_types::enums::{Lang, Role, Status, DataFormat, FormatHint};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
pub struct SessionFixture {
    pub description: String,
    pub blocks: Vec<FixtureBlock>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FixtureBlock {
    Code {
        language: String,
        path: String,
        content: String,
        summary: Option<String>,
    },
    Conversation {
        role: String,
        content: String,
    },
    ToolResult {
        tool_name: String,
        status: String,
        content: String,
        summary: Option<String>,
    },
    FileTree {
        root_path: String,
        entries: Vec<serde_json::Value>,
    },
    Document {
        title: String,
        content: String,
        format_hint: Option<String>,
    },
    StructuredData {
        format: String,
        content: String,
    },
    Diff {
        path: String,
        hunks: Vec<serde_json::Value>,
    },
}

/// Encode a fixture JSON file into a BCP payload.
pub fn encode_fixture(path: &Path) -> Result<Vec<u8>> {
    let json = std::fs::read_to_string(path)?;
    let fixture: SessionFixture = serde_json::from_str(&json)?;
    let mut encoder = BcpEncoder::new();

    for block in &fixture.blocks {
        match block {
            FixtureBlock::Code { language, path, content, summary } => {
                let lang = parse_lang(language);
                encoder.add_code(lang, path, content.as_bytes());
                if let Some(s) = summary {
                    encoder.with_summary(s)?;
                }
            }
            FixtureBlock::Conversation { role, content } => {
                let r = parse_role(role);
                encoder.add_conversation(r, content.as_bytes());
            }
            FixtureBlock::ToolResult { tool_name, status, content, summary } => {
                let s = parse_status(status);
                encoder.add_tool_result(tool_name, s, content.as_bytes());
                if let Some(sm) = summary {
                    encoder.with_summary(sm)?;
                }
            }
            // ... remaining block types
            _ => { /* skip unsupported types in PoC */ }
        }
    }

    Ok(encoder.encode()?)
}
```

### 5. Equivalent Markdown Builders

The markdown baseline construction is critical to the integrity of the benchmark.
Two builders produce different baselines, representing the range of real-world
context formatting.

**Builder A: Naive Markdown** — the most common formatting pattern. Triple-backtick
code fences with language tags, `### Role:` headers for conversation, bullet-list
tool results.

```rust
/// Build naive markdown equivalent — triple backticks, ### headers.
///
/// This represents how most tools dump context into a prompt:
///   ```rust
///   // src/main.rs
///   fn main() { ... }
///   ```
///
///   ### User:
///   Fix the timeout bug.
///
///   ### Tool Result (ripgrep):
///   ```
///   src/pool.rs:42: pub timeout: Duration,
///   ```
pub fn build_naive_markdown(blocks: &[Block]) -> String {
    let mut out = String::new();
    for block in blocks {
        match &block.content {
            BlockContent::Code(c) => {
                let lang = format!("{:?}", c.language).to_lowercase();
                let path = std::str::from_utf8(&c.path).unwrap_or("");
                out.push_str(&format!("```{lang}\n// {path}\n"));
                out.push_str(std::str::from_utf8(&c.content).unwrap_or(""));
                out.push_str("\n```\n\n");
            }
            BlockContent::Conversation(c) => {
                let role = format!("{:?}", c.role);
                out.push_str(&format!("### {role}:\n\n"));
                out.push_str(std::str::from_utf8(&c.content).unwrap_or(""));
                out.push_str("\n\n");
            }
            BlockContent::ToolResult(t) => {
                let name = std::str::from_utf8(&t.tool_name).unwrap_or("");
                let status = format!("{:?}", t.status);
                out.push_str(&format!("### Tool Result ({name}) [{status}]:\n\n```\n"));
                out.push_str(std::str::from_utf8(&t.content).unwrap_or(""));
                out.push_str("\n```\n\n");
            }
            BlockContent::Document(d) => {
                let title = std::str::from_utf8(&d.title).unwrap_or("");
                out.push_str(&format!("## {title}\n\n"));
                out.push_str(std::str::from_utf8(&d.content).unwrap_or(""));
                out.push_str("\n\n");
            }
            // ... remaining block types with typical markdown formatting
            _ => {}
        }
    }
    out
}
```

**Builder B: Realistic Agent Markdown** — models how Claude Code actually formats
context, with XML-style tags, system prompt wrappers, and JSON tool-call envelopes.

```rust
/// Build realistic agent markdown — mimics Claude Code's actual context format.
///
/// This is the fairer comparison because it represents what models actually
/// receive today. Includes:
///   - <source> tags around code files
///   - <tool_result> JSON envelopes
///   - <conversation> wrappers with role attributes
///   - Full file paths repeated in multiple positions
pub fn build_realistic_markdown(blocks: &[Block]) -> String {
    let mut out = String::new();
    out.push_str("<context>\n");

    for (i, block) in blocks.iter().enumerate() {
        match &block.content {
            BlockContent::Code(c) => {
                let path = std::str::from_utf8(&c.path).unwrap_or("");
                let lang = format!("{:?}", c.language).to_lowercase();
                // Claude Code style: full path + language + fenced content
                out.push_str(&format!(
                    "<source path=\"{path}\" language=\"{lang}\">\n```{lang}\n"
                ));
                out.push_str(std::str::from_utf8(&c.content).unwrap_or(""));
                out.push_str("\n```\n</source>\n\n");
            }
            BlockContent::ToolResult(t) => {
                let name = std::str::from_utf8(&t.tool_name).unwrap_or("");
                // JSON-RPC style envelope
                out.push_str(&format!(
                    "<tool_result>\n{{\n  \"tool\": \"{name}\",\n  \"status\": \"{:?}\",\n  \"output\": ",
                    t.status
                ));
                // Escape content as JSON string
                let content = std::str::from_utf8(&t.content).unwrap_or("");
                out.push_str(&format!("{}\n}}\n</tool_result>\n\n",
                    serde_json::to_string(content).unwrap_or_default()));
            }
            BlockContent::Conversation(c) => {
                let role = format!("{:?}", c.role).to_lowercase();
                out.push_str(&format!("<message role=\"{role}\">\n"));
                out.push_str(std::str::from_utf8(&c.content).unwrap_or(""));
                out.push_str("\n</message>\n\n");
            }
            _ => {
                // Other block types: generic wrapper
                out.push_str(&format!("<block index=\"{i}\">\n"));
                // ... serialize content as-is
                out.push_str("</block>\n\n");
            }
        }
    }

    out.push_str("</context>\n");
    out
}
```

### 6. Token Counting with tiktoken-rs

All token counting uses `tiktoken-rs::cl100k_base` (GPT-4 / Claude-comparable BPE).
The tokenizer is initialized once and reused.

```rust
use tiktoken_rs::cl100k_base;

pub struct TokenCounter {
    bpe: tiktoken_rs::CoreBPE,
}

impl TokenCounter {
    pub fn new() -> anyhow::Result<Self> {
        let bpe = cl100k_base()?;
        Ok(Self { bpe })
    }

    /// Count tokens in a string using cl100k_base BPE.
    pub fn count(&self, text: &str) -> usize {
        self.bpe.encode_with_special_tokens(text).len()
    }
}
```

### 7. Benchmark Binary

`src/bin/bench_tokens.rs` — the main output artifact. Reads a fixture, encodes it,
renders in all modes, builds both markdown baselines, counts tokens, and prints a
results table.

```rust
use anyhow::Result;
use bcp_bench_real::{
    fixture::encode_fixture,
    markdown::{build_naive_markdown, build_realistic_markdown},
    token_counter::TokenCounter,
};
use bcp_decoder::BcpDecoder;
use bcp_driver::{DefaultDriver, DriverConfig, BcpDriver, OutputMode};

fn main() -> Result<()> {
    let fixture_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "fixtures/real_session_medium.json".into());

    let payload = encode_fixture(fixture_path.as_ref())?;
    let decoded = BcpDecoder::decode(&payload)?;
    let counter = TokenCounter::new()?;

    // BCP rendered outputs
    let mut results: Vec<(&str, usize)> = Vec::new();

    for (label, mode) in [
        ("BCP XML",     OutputMode::Xml),
        ("BCP Markdown", OutputMode::Markdown),
        ("BCP Minimal", OutputMode::Minimal),
    ] {
        let config = DriverConfig {
            mode,
            ..Default::default()
        };
        let rendered = DefaultDriver.render(&decoded.blocks, &config)?;
        let tokens = counter.count(&rendered);
        results.push((label, tokens));
    }

    // Markdown baselines
    let naive_md = build_naive_markdown(&decoded.blocks);
    let naive_tokens = counter.count(&naive_md);
    results.push(("Raw MD (naive)", naive_tokens));

    let realistic_md = build_realistic_markdown(&decoded.blocks);
    let realistic_tokens = counter.count(&realistic_md);
    results.push(("Raw MD (agent)", realistic_tokens));

    // Print results table
    println!();
    println!("╔══════════════════════╦═════════╦═══════════════════╦═══════════════════╗");
    println!("║ Format               ║ Tokens  ║ vs Naive MD       ║ vs Agent MD       ║");
    println!("╠══════════════════════╬═════════╬═══════════════════╬═══════════════════╣");

    for (label, tokens) in &results {
        let vs_naive = if *label != "Raw MD (naive)" && *label != "Raw MD (agent)" {
            let pct = (1.0 - *tokens as f64 / naive_tokens as f64) * 100.0;
            format!("{pct:+.1}%")
        } else {
            "—".into()
        };
        let vs_realistic = if *label != "Raw MD (naive)" && *label != "Raw MD (agent)" {
            let pct = (1.0 - *tokens as f64 / realistic_tokens as f64) * 100.0;
            format!("{pct:+.1}%")
        } else {
            "—".into()
        };
        println!("║ {label:<20} ║ {tokens:>7} ║ {vs_naive:>17} ║ {vs_realistic:>17} ║");
    }

    println!("╚══════════════════════╩═════════╩═══════════════════╩═══════════════════╝");
    println!();

    // Per-block-type breakdown
    print_per_block_breakdown(&decoded.blocks, &counter)?;

    // Wire size stats
    println!("BCP wire size:     {} bytes", payload.len());
    println!("Naive MD size:     {} bytes", naive_md.len());
    println!("Agent MD size:     {} bytes", realistic_md.len());
    println!(
        "Wire compression:  {:.1}% vs naive MD bytes",
        (1.0 - payload.len() as f64 / naive_md.len() as f64) * 100.0
    );

    Ok(())
}

/// Break down token overhead per block type.
///
/// For each block type present in the payload, render just that block in
/// Minimal mode vs naive markdown and report the structural overhead tokens.
fn print_per_block_breakdown(blocks: &[Block], counter: &TokenCounter) -> Result<()> {
    println!("\nPer-Block-Type Overhead Breakdown:");
    println!("┌──────────────────┬───────────┬───────────┬─────────┐");
    println!("│ Block Type       │ BCP Min.  │ Naive MD  │ Savings │");
    println!("├──────────────────┼───────────┼───────────┼─────────┤");

    // Group blocks by type, render each group, compare
    // ...

    println!("└──────────────────┴───────────┴───────────┴─────────┘");
    Ok(())
}
```

### 8. Criterion Benchmark

`benches/tiktoken.rs` — runs the full pipeline under Criterion for reproducible,
statistically rigorous measurements. Three benchmark groups:

```rust
use criterion::{Criterion, criterion_group, criterion_main, BenchmarkId};

/// Group 1: Token counting throughput.
/// How fast is tiktoken tokenization for BCP-sized payloads?
fn bench_token_counting(c: &mut Criterion) {
    let counter = TokenCounter::new().unwrap();
    let payload = encode_fixture("fixtures/real_session_medium.json").unwrap();
    let decoded = BcpDecoder::decode(&payload).unwrap();
    let rendered = DefaultDriver
        .render(&decoded.blocks, &DriverConfig::default())
        .unwrap();

    c.bench_function("tiktoken_count_medium", |b| {
        b.iter(|| counter.count(&rendered));
    });
}

/// Group 2: End-to-end pipeline.
/// Encode fixture → decode → render → count tokens.
fn bench_full_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_pipeline");

    for fixture in ["real_session_small", "real_session_medium", "real_session_large"] {
        let path = format!("fixtures/{fixture}.json");
        group.bench_with_input(
            BenchmarkId::from_parameter(fixture),
            &path,
            |b, path| {
                b.iter(|| {
                    let payload = encode_fixture(path.as_ref()).unwrap();
                    let decoded = BcpDecoder::decode(&payload).unwrap();
                    let config = DriverConfig {
                        mode: OutputMode::Minimal,
                        ..Default::default()
                    };
                    let rendered = DefaultDriver.render(&decoded.blocks, &config).unwrap();
                    TokenCounter::new().unwrap().count(&rendered)
                });
            },
        );
    }

    group.finish();
}

/// Group 3: Token savings across modes.
/// For each output mode, measure BCP tokens vs markdown tokens.
fn bench_savings_by_mode(c: &mut Criterion) {
    let counter = TokenCounter::new().unwrap();
    let payload = encode_fixture("fixtures/real_session_medium.json").unwrap();
    let decoded = BcpDecoder::decode(&payload).unwrap();
    let naive_md = build_naive_markdown(&decoded.blocks);

    let mut group = c.benchmark_group("savings_by_mode");

    for (label, mode) in [
        ("xml",      OutputMode::Xml),
        ("markdown", OutputMode::Markdown),
        ("minimal",  OutputMode::Minimal),
    ] {
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &mode,
            |b, mode| {
                let config = DriverConfig {
                    mode: *mode,
                    ..Default::default()
                };
                let rendered = DefaultDriver.render(&decoded.blocks, &config).unwrap();
                b.iter(|| counter.count(&rendered));
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_token_counting, bench_full_pipeline, bench_savings_by_mode);
criterion_main!(benches);
```

### 9. RFC §6 Update Data

The benchmark binary produces a machine-parseable summary that can directly
replace the estimates in RFC §6:

```rust
/// Print RFC-ready table to stdout.
fn print_rfc_table(results: &BenchResults) {
    println!("\nRFC §6 Replacement Data (cl100k_base tokenizer):");
    println!();
    println!("   +----------------------------+-----------+-----------+---------+");
    println!("   | Context Pattern            | Markdown  | BCP Min.  | Savings |");
    println!("   |                            | Tokens    | Tokens    |         |");
    println!("   +----------------------------+-----------+-----------+---------+");

    for row in &results.per_type {
        println!(
            "   | {:<26} | {:>9} | {:>9} | {:>6.0}% |",
            row.label, row.md_tokens, row.bcp_tokens, row.savings_pct
        );
    }

    println!("   +----------------------------+-----------+-----------+---------+");
    println!(
        "   | TOTAL                      | {:>9} | {:>9} | {:>6.1}% |",
        results.total_md, results.total_bcp, results.total_savings_pct
    );
    println!("   +----------------------------+-----------+-----------+---------+");
}
```

---

## File Structure

```
crates/bcp-bench-real/
├── Cargo.toml
├── src/
│   ├── fixture.rs              # JSON fixture → BcpEncoder → Vec<u8>
│   ├── markdown.rs             # Two markdown baseline builders
│   ├── token_counter.rs        # tiktoken-rs wrapper
│   ├── lib.rs                  # Re-exports: fixture, markdown, token_counter
│   └── bin/
│       ├── bench_tokens.rs     # Main benchmark binary (table output)
│       └── capture_session.rs  # Fixture generation helper
├── fixtures/
│   ├── schema.json             # JSON schema for fixture format
│   ├── real_session_small.json
│   ├── real_session_medium.json
│   └── real_session_large.json
└── benches/
    └── tiktoken.rs             # Criterion benchmarks
```

---

## Acceptance Criteria

- [ ] `bcp-bench-real` compiles as a workspace member with `cargo build -p bcp-bench-real`
- [ ] At least 3 fixture files committed (small, medium, large)
- [ ] `cargo run -p bcp-bench-real --bin bench_tokens` prints a complete token savings table
- [ ] Token counts use `cl100k_base` BPE, not character heuristics
- [ ] Two markdown baselines produced: naive (backtick fences) and realistic (XML+JSON envelopes)
- [ ] Per-block-type breakdown table is printed
- [ ] BCP Minimal mode achieves measurably fewer tokens than both markdown baselines
- [ ] Criterion benchmarks run: `cargo bench -p bcp-bench-real`
- [ ] RFC §6 replacement data is printed in ASCII table format matching RFC style
- [ ] `capture_session --mode dir-scan` generates a valid fixture from a real directory
- [ ] All fixture JSON validates against the schema
- [ ] `cargo clippy -p bcp-bench-real -- -W clippy::pedantic` emits zero warnings

---

## Verification

```bash
# Build the crate
cargo build -p bcp-bench-real

# Generate a fixture from this repo's own source
cargo run -p bcp-bench-real --bin capture_session -- \
    --mode dir-scan \
    --path ./crates/bcp-encoder/src \
    --max-files 10 \
    --output crates/bcp-bench-real/fixtures/real_session_medium.json

# Run the token benchmark
cargo run -p bcp-bench-real --bin bench_tokens -- \
    crates/bcp-bench-real/fixtures/real_session_medium.json

# Run Criterion benchmarks
cargo bench -p bcp-bench-real

# Lint check
cargo clippy -p bcp-bench-real -- -W clippy::pedantic
```

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| `tiktoken-rs` BPE data download fails in CI | Medium | Medium | Vendor the `cl100k_base` data file in fixtures/; or gate tiktoken tests behind a feature flag |
| Naive markdown baseline is too naive, overstating savings | High | High | Build two baselines (naive + realistic); report both; let reader choose |
| Fixture files are too synthetic to be representative | Medium | High | Use `capture_session --mode dir-scan` against real projects; include actual source files from this repo |
| `cl100k_base` diverges from Claude's tokenizer | Low | Low | cl100k is the standard benchmark tokenizer; note in output that Claude-specific numbers may differ by ±5% |
| Large fixtures slow down `cargo test` | Medium | Low | Criterion benches are opt-in (`cargo bench`), not part of `cargo test`; binary benchmarks are run manually |

---

## Relationship to Existing Crates

```
                              SPEC_11 (this spec)
                        ┌─────────────────────────────┐
                        │    bcp-bench-real            │
                        │                             │
                        │  fixtures/*.json            │
                        │      │                      │
                        │      ▼                      │
                        │  fixture.rs (encode)        │
                        │      │                      │
                        │      ▼                      │
                        │  bcp-encoder ──▶ .bcp       │
                        │      │                      │
                        │      ▼                      │
                        │  bcp-decoder ──▶ Vec<Block> │
                        │      │         │            │
                        │      ▼         ▼            │
                        │  bcp-driver   markdown.rs   │
                        │  (3 modes)   (2 baselines)  │
                        │      │         │            │
                        │      ▼         ▼            │
                        │  token_counter.rs           │
                        │  (tiktoken-rs cl100k_base)  │
                        │      │                      │
                        │      ▼                      │
                        │  Results Table              │
                        │  (RFC §6 replacement data)  │
                        └─────────────────────────────┘
```

This crate is read-only with respect to all other crates — it consumes their
public APIs but does not modify them. It can be added or removed from the
workspace without affecting any production code.
