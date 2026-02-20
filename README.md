# Bit Context Protocol

Reference implementation of the **Bit Context Protocol (BCP)** binary format — a compact, typed serialization format for structured LLM context. Where traditional approaches waste 30-50% of tokens on markdown fences, JSON envelopes, and repeated path prefixes, BCP packs the same semantic content into a binary representation that decodes into token-efficient text.

Based on [BCP RFC Draft v0.1.0](./RFC.txt) (February 2026).

## Status

**RFC Phase 1 complete.** The wire format, type system, encoder, decoder, driver/renderer, CLI tool, and conformance test suite are all implemented. 305 tests passing across the workspace.

| Crate | Purpose | Status |
|-------|---------|--------|
| `bcp-wire` | Varint encoding, file header, block frame envelope | Complete (44 tests) |
| `bcp-types` | 11 semantic block types, TLV field encoding, shared enums | Complete (53 tests) |
| `bcp-encoder` | Builder API with compression, content addressing, dedup | Complete (49 tests) |
| `bcp-decoder` | Sync + async streaming decoder with forward compatibility | Complete (6 tests) |
| `bcp-driver` | XML/Markdown/Minimal render modes, token budget engine | Complete (27 tests) |
| `bcp-cli` | inspect, validate, encode, decode, stats commands | Complete |
| `bcp-tests` | Golden fixtures, snapshot conformance, roundtrip, benchmarks | Complete (108 tests) |

## Data Flow

```
Tool / Agent ──▶ bcp-encoder ──▶ .bcp binary ──▶ bcp-decoder ──▶ Vec<Block> ──▶ bcp-driver ──▶ LLM
```

## Quick Start

### Prerequisites

- [mise](https://mise.jdx.dev/) (manages Rust 1.93.0 toolchain)

### Build & Test

```bash
mise install          # Install pinned Rust 1.93.0
mise run build        # Build all crates
mise run test         # Run all 305 tests
mise run ci           # Full pipeline: fmt check → clippy → test
```

### Encode a Payload

```rust
use bcp_encoder::BcpEncoder;
use bcp_types::enums::{Lang, Role, Status, Priority};

let payload = BcpEncoder::new()
    .add_code(Lang::Rust, "src/main.rs", b"fn main() {}")
    .with_summary("Entry point.")?
    .with_priority(Priority::High)?
    .add_conversation(Role::User, b"Fix the timeout bug.")
    .add_tool_result("ripgrep", Status::Ok, b"3 matches found.")
    .encode()?;
```

### Decode a Payload

```rust
use bcp_decoder::BcpDecoder;

let decoded = BcpDecoder::decode(&payload)?;
for block in &decoded.blocks {
    println!("{:?}", block.content);
}
```

### Render to Model-Ready Text

```rust
use bcp_driver::{DefaultDriver, BcpDriver, DriverConfig, OutputMode, Verbosity};

let driver = DefaultDriver;
let config = DriverConfig {
    mode: OutputMode::Xml,
    verbosity: Verbosity::Adaptive,
    token_budget: Some(8000),
    ..Default::default()
};
let text = driver.render(&decoded.blocks, &config)?;
```

### CLI

```bash
bcp inspect payload.bcp               # Block summary table
bcp validate payload.bcp              # Structural correctness check
bcp decode payload.bcp --mode xml     # Render as XML-tagged text
bcp decode payload.bcp --mode minimal # Render as minimal delimiters
bcp stats payload.bcp                 # Size and token efficiency stats
bcp encode manifest.json -o out.bcp   # Create .bcp from JSON manifest
```

### Streaming Decode

```rust
use bcp_decoder::{StreamingDecoder, DecoderEvent};

let mut decoder = StreamingDecoder::new(reader);
while let Some(event) = decoder.next().await {
    match event? {
        DecoderEvent::Header(h) => println!("v{}.{}", h.version_major, h.version_minor),
        DecoderEvent::Block(b) => println!("{:?}", b.content),
    }
}
```

## Workspace

```
bit-context-protocol/
├── crates/
│   ├── bcp-wire/       Wire primitives (varint, header, block frame)
│   ├── bcp-types/      Block type structs, TLV fields, enums
│   ├── bcp-encoder/    Builder API → binary payload
│   ├── bcp-decoder/    Binary payload → typed structs (sync + async)
│   ├── bcp-driver/     Typed structs → token-efficient text
│   ├── bcp-cli/        Command-line tool for .bcp files
│   └── bcp-tests/      Integration tests, golden fixtures, benchmarks
├── docs/               Docsify documentation site
├── RFC.txt             BCP RFC Draft v0.1.0
├── mise.toml           Tool versions + task runner
└── Cargo.toml          Workspace root
```

## Available Tasks

```
mise run build          Build all crates
mise run build:release  Release build
mise run test           Run all tests
mise run test:wire      Test bcp-wire only
mise run test:types     Test bcp-types only
mise run test:encoder   Test bcp-encoder only
mise run test:decoder   Test bcp-decoder only
mise run clippy         Pedantic clippy lints
mise run fmt            Format all source files
mise run fmt:check      Check formatting
mise run check          Fast type-check (no codegen)
mise run doc            Generate rustdoc
mise run doc:serve      Serve rustdoc on :8080
mise run docsite:serve  Serve docsify docs on :4040
mise run ci             Full CI pipeline
mise run clean          Remove build artifacts
```

## Documentation

Serve the docs site locally:

```bash
mise run docsite:serve
```

Then open `http://localhost:4040`. Covers architecture, wire format specification, per-crate reference pages, block type reference, error catalog, and test suite documentation.

## Block Types

| ID | Type | Description |
|----|------|-------------|
| `0x01` | CODE | Source code with language and path |
| `0x02` | CONVERSATION | Chat turn with role |
| `0x03` | FILE_TREE | Directory structure with nested entries |
| `0x04` | TOOL_RESULT | Tool/MCP output with status |
| `0x05` | DOCUMENT | Prose content (markdown/plain/html) |
| `0x06` | STRUCTURED_DATA | JSON, YAML, TOML, CSV, XML |
| `0x07` | DIFF | Code changes with hunks |
| `0x08` | ANNOTATION | Metadata overlay (priority/summary/tag) |
| `0x09` | EMBEDDING_REF | Vector store reference |
| `0x0A` | IMAGE | Image data with alt text |
| `0xFE` | EXTENSION | User-defined block (namespace + type) |
| `0xFF` | END | Stream sentinel |

## Features

- **Zstd compression** — per-block or whole-payload, with 256-byte threshold and bomb protection
- **BLAKE3 content addressing** — deduplicate identical blocks across payloads
- **Token budget engine** — priority-based degradation (full → summary → placeholder → omit)
- **Forward compatibility** — unknown block types, fields, and enum values preserved, not rejected
- **Three render modes** — XML-tagged (Claude-optimized), Markdown (universal), Minimal (max efficiency)
- **Streaming decode** — async incremental parsing via `StreamingDecoder`

## License

See [RFC.txt](./RFC.txt) for specification terms.
