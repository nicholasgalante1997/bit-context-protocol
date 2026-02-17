# Bit Context Protocol

Reference implementation of the **LLM Context Pack (LCP)** binary format — a compact, typed serialization format for structured LLM context. Where traditional approaches waste 30-50% of tokens on markdown fences, JSON envelopes, and repeated path prefixes, LCP packs the same semantic content into a binary representation that can be decoded and rendered into token-efficient text.

Based on [LCP RFC Draft v0.1.0](./RFC.txt) (February 2026).

## Status

Phase 2 of 4 complete. The wire format, type system, encoder, and decoder are implemented with full round-trip coverage across all 11 block types. Streaming async decode is functional. Forward compatibility (unknown block types, unknown fields, unknown enum values) is enforced.

| Crate | Purpose | Status |
|-------|---------|--------|
| `bcp-wire` | Varint encoding, file header, block frame envelope | Complete |
| `bcp-types` | 11 semantic block types, TLV field encoding, shared enums | Complete |
| `bcp-encoder` | Builder API for producing LCP binary payloads | Complete |
| `bcp-decoder` | Sync + async streaming decoder with forward compatibility | Complete |

Remaining: driver/renderer (SPEC_05), compression (SPEC_06), content addressing (SPEC_07), token budget engine (SPEC_08), CLI tool (SPEC_09), conformance tests (SPEC_10).

## Data Flow

```
Tool / Agent ──▶ bcp-encoder ──▶ .lcp binary ──▶ bcp-decoder ──▶ Vec<Block> ──▶ driver ──▶ LLM
```

## Quick Start

### Prerequisites

- [mise](https://mise.jdx.dev/) (manages Rust toolchain)

### Build & Test

```bash
mise install          # Install pinned Rust 1.93.0
mise run build        # Build all crates
mise run test         # Run all 141 tests
mise run ci           # Full pipeline: fmt check → clippy → test
```

### Encode a Payload

```rust
use bcp_encoder::LcpEncoder;
use bcp_types::enums::{Lang, Role, Status, Priority};

let payload = LcpEncoder::new()
    .add_code(Lang::Rust, "src/main.rs", b"fn main() {}")
    .with_summary("Entry point.")
    .with_priority(Priority::High)
    .add_conversation(Role::User, b"Fix the timeout bug.")
    .add_tool_result("ripgrep", Status::Ok, b"3 matches found.")
    .encode()?;
```

### Decode a Payload

```rust
use bcp_decoder::LcpDecoder;

let decoded = LcpDecoder::decode(&payload)?;
for block in &decoded.blocks {
    println!("{:?}", block.content);
}
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
│   └── bcp-decoder/    Binary payload → typed structs (sync + async)
├── docs/               Docsify documentation site
├── RFC.txt             LCP RFC Draft v0.1.0
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
mise run doc:serve      Serve docsify docs on :3000
mise run ci             Full CI pipeline
mise run clean          Remove build artifacts
```

## Documentation

Serve the docs site locally:

```bash
mise run doc:serve
```

Then open `http://localhost:3000`. Covers architecture, spec documentation for SPEC_01 through SPEC_04, per-crate reference pages, block type reference, and error catalog.

## Block Types

| ID | Type | Description |
|----|------|-------------|
| `0x01` | CODE | Source code with language and path |
| `0x02` | CONVERSATION | Chat turn with role |
| `0x03` | FILE_TREE | Directory structure with nested entries |
| `0x04` | TOOL_RESULT | Tool/MCP output with status |
| `0x05` | DOCUMENT | Prose content (markdown/plain/html) |
| `0x06` | STRUCTURED_DATA | JSON, YAML, TOML, CSV |
| `0x07` | DIFF | Code changes with hunks |
| `0x08` | ANNOTATION | Metadata overlay (priority/summary/tag) |
| `0x09` | EMBEDDING_REF | Vector store reference |
| `0x0A` | IMAGE | Image data with alt text |
| `0xFE` | EXTENSION | User-defined block (namespace + type) |
| `0xFF` | END | Stream sentinel |

## License

See [RFC.txt](./RFC.txt) for specification terms.
