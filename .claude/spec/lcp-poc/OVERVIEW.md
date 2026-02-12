# LCP Proof of Concept — System Overview

## Project Identity

| Field       | Value                                                  |
|-------------|--------------------------------------------------------|
| Name        | `lcp-core` — LLM Context Pack Reference Implementation |
| Language    | Rust (2024 edition)                                    |
| RFC         | LCP RFC Draft v0.1.0 (February 2026)                   |
| Scope       | Proof of concept: encode, decode, render, CLI          |
| Repository  | `bit-context-protocol`                                 |

---

## System Overview

```
┌──────────────────────────────────────────────────────────────────────┐
│                        LCP Proof of Concept                         │
│                                                                     │
│  ┌─────────────┐    ┌──────────────┐    ┌───────────────────────┐   │
│  │  Encoder    │    │  Wire Format │    │  Decoder / Driver     │   │
│  │             │    │              │    │                       │   │
│  │  Rust API   │──▶│  .lcp file   │──▶│  Binary ─▶ Blocks     │   │
│  │  (builder   │    │  (binary     │    │  Blocks ─▶ Text       │   │
│  │   pattern)  │    │   payload)   │    │  (XML/MD/Minimal)     │   │
│  └─────────────┘    └──────────────┘    └───────────────────────┘   │
│        │                   │                       │                 │
│        │                   │                       │                 │
│        ▼                   ▼                       ▼                 │
│  ┌─────────────┐    ┌──────────────┐    ┌───────────────────────┐   │
│  │  Compression│    │  Content     │    │  Token Budget Engine  │   │
│  │  (zstd)     │    │  Addressing  │    │  (priority ranking,   │   │
│  │             │    │  (BLAKE3)    │    │   summary fallback)   │   │
│  └─────────────┘    └──────────────┘    └───────────────────────┘   │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  CLI Tool (`lcp`)                                            │   │
│  │  inspect · validate · encode · decode · stats                │   │
│  └──────────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────────┘
```

---

## Goals

1. **Validate the wire format** — Prove that the LCP binary format specified in the RFC
   can be implemented with correct encode/decode round-trips across all 11 block types.

2. **Demonstrate token savings** — Show measurable token reduction (target: 30-50%
   structural overhead savings) compared to equivalent raw markdown context.

3. **Prove streaming viability** — Implement incremental decode without buffering the
   entire payload, confirming the format supports streaming consumption.

4. **Exercise the driver model** — Implement at least three output format modes
   (XML-tagged, Markdown, Minimal) and demonstrate adaptive budget-aware rendering.

5. **Establish Rust idioms** — Define the crate structure, error handling, and API
   patterns that a production `lcp-core` crate would use.

6. **Build a usable CLI** — Deliver a command-line tool for inspecting, validating,
   and converting LCP payloads to aid further development and debugging.

---

## Current State Analysis

| Component                     | Status | Notes                                          |
|-------------------------------|--------|-------------------------------------------------|
| RFC specification             | ✅     | Complete draft v0.1.0 with all block types      |
| Rust project scaffold         | 🔄     | `Cargo.toml` exists, `src/main.rs` is stub      |
| Wire format primitives        | ❌     | Varint (LEB128), header, block frame unbuilt     |
| Block type definitions        | ❌     | 11 block types specified but not implemented     |
| Encoder API                   | ❌     | Builder pattern specified in RFC, not coded      |
| Decoder                       | ❌     | Binary → struct deserialization not started       |
| Driver / Renderer             | ❌     | Text output modes not started                    |
| Compression (zstd)            | ❌     | Per-block and whole-payload compression           |
| Content addressing (BLAKE3)   | ❌     | Hash-based dedup not started                     |
| Token budget engine           | ❌     | Priority ranking and summary fallback            |
| CLI tool                      | ❌     | Inspect, validate, encode, decode commands       |
| Test suite                    | ❌     | Golden files, round-trip, conformance tests      |

---

## Technology Stack

| Tool / Crate        | Version   | Purpose                                         |
|----------------------|-----------|-------------------------------------------------|
| Rust                 | 2024 ed.  | Primary language, `#![warn(clippy::pedantic)]`  |
| `zstd`               | 0.13.x    | Zstandard compression (per-block, whole-payload) |
| `blake3`             | 1.x       | Content-addressed hashing (32-byte digests)      |
| `clap`               | 4.x       | CLI argument parsing with derive macros          |
| `thiserror`          | 2.x       | Typed error definitions for the library crate    |
| `anyhow`             | 1.x       | Error handling in the CLI binary                 |
| `tokio`              | 1.x       | Async runtime for streaming decode               |
| `tokio-stream`       | 0.1.x     | `Stream` trait for `decode_stream`               |
| `bytes`              | 1.x       | Efficient byte buffer management                 |
| `insta`              | 1.x       | Snapshot testing for golden file conformance      |

---

## Component Architecture

```
lcp-core/
├── Cargo.toml                    # Workspace root
├── RFC.txt                       # Specification document
│
├── crates/
│   ├── lcp-wire/                 # Wire format primitives
│   │   └── src/
│   │       ├── lib.rs            # Crate root, re-exports
│   │       ├── varint.rs         # LEB128 encode/decode
│   │       ├── header.rs         # 8-byte file header
│   │       ├── block_frame.rs    # Block envelope (type, flags, length)
│   │       └── error.rs          # Wire-level errors
│   │
│   ├── lcp-types/                # Block type definitions
│   │   └── src/
│   │       ├── lib.rs            # Crate root, re-exports
│   │       ├── block_type.rs     # BlockType enum (0x01..0xFF)
│   │       ├── fields.rs         # Field ID constants per block type
│   │       ├── code.rs           # CODE block (0x01)
│   │       ├── conversation.rs   # CONVERSATION block (0x02)
│   │       ├── file_tree.rs      # FILE_TREE block (0x03)
│   │       ├── tool_result.rs    # TOOL_RESULT block (0x04)
│   │       ├── document.rs       # DOCUMENT block (0x05)
│   │       ├── structured_data.rs# STRUCTURED_DATA block (0x06)
│   │       ├── diff.rs           # DIFF block (0x07)
│   │       ├── annotation.rs     # ANNOTATION block (0x08)
│   │       ├── embedding_ref.rs  # EMBEDDING_REF block (0x09)
│   │       ├── image.rs          # IMAGE block (0x0A)
│   │       ├── extension.rs      # EXTENSION block (0xFE)
│   │       ├── end.rs            # END sentinel (0xFF)
│   │       ├── enums.rs          # Lang, Role, Status, Priority, etc.
│   │       └── error.rs          # Type-level errors
│   │
│   ├── lcp-encoder/              # Encoder (structs → binary)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── encoder.rs        # LcpEncoder builder
│   │       ├── block_writer.rs   # Serialize individual blocks
│   │       ├── compression.rs    # Zstd compression wrapper
│   │       ├── content_store.rs  # BLAKE3 content addressing
│   │       └── error.rs
│   │
│   ├── lcp-decoder/              # Decoder (binary → structs)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── decoder.rs        # LcpDecoder (sync)
│   │       ├── streaming.rs      # Streaming decode (async)
│   │       ├── block_reader.rs   # Deserialize individual blocks
│   │       ├── decompression.rs  # Zstd decompression wrapper
│   │       └── error.rs
│   │
│   ├── lcp-driver/               # Driver / Renderer (structs → text)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── driver.rs         # LcpDriver trait + config
│   │       ├── render_xml.rs     # XML-tagged output mode
│   │       ├── render_markdown.rs# Markdown output mode
│   │       ├── render_minimal.rs # Minimal output mode
│   │       ├── budget.rs         # Token budget engine
│   │       └── error.rs
│   │
│   └── lcp-cli/                  # CLI binary
│       └── src/
│           ├── main.rs           # Entry point, clap setup
│           ├── cmd_inspect.rs    # `lcp inspect <file>`
│           ├── cmd_validate.rs   # `lcp validate <file>`
│           ├── cmd_encode.rs     # `lcp encode <input>`
│           ├── cmd_decode.rs     # `lcp decode <file>`
│           └── cmd_stats.rs      # `lcp stats <file>`
│
├── tests/
│   ├── golden/                   # Golden file fixtures (.lcp + expected output)
│   │   ├── simple_code.lcp
│   │   ├── simple_code.xml.expected
│   │   ├── simple_code.md.expected
│   │   ├── simple_code.min.expected
│   │   ├── conversation.lcp
│   │   ├── mixed_blocks.lcp
│   │   └── compressed.lcp
│   ├── roundtrip.rs              # Encode → decode → verify identity
│   ├── conformance.rs            # Golden file snapshot tests
│   └── budget.rs                 # Token budget behavior tests
│
└── benches/
    ├── encode.rs                 # Encoding throughput benchmarks
    ├── decode.rs                 # Decoding throughput benchmarks
    └── token_savings.rs          # Token count comparison vs. markdown
```

---

## Data Flow

### Encode Path

```
  Tool / Agent                  Encoder                       Disk / Wire
       │                          │                               │
       │  .add_code(lang, path,   │                               │
       │   content)               │                               │
       ├─────────────────────────▶│                               │
       │                          │  Serialize block fields       │
       │  .with_summary("...")    │  (varint field IDs +          │
       ├─────────────────────────▶│   length-prefixed values)     │
       │                          │                               │
       │  .with_priority(High)    │                               │
       ├─────────────────────────▶│                               │
       │                          │                               │
       │  .encode()               │                               │
       ├─────────────────────────▶│                               │
       │                          │  1. Write 8-byte header       │
       │                          │  2. For each block:           │
       │                          │     a. Write block_type       │
       │                          │     b. Write block_flags      │
       │                          │     c. Compress body (opt.)   │
       │                          │     d. Write content_len      │
       │                          │     e. Write body bytes       │
       │                          │  3. Write END sentinel        │
       │                          ├──────────────────────────────▶│
       │                          │          .lcp payload         │
```

### Decode Path

```
  Disk / Wire             Decoder              Driver               LLM
       │                    │                    │                    │
       │  Read bytes        │                    │                    │
       ├───────────────────▶│                    │                    │
       │                    │  1. Parse header   │                    │
       │                    │  2. For each block:│                    │
       │                    │     a. Read type   │                    │
       │                    │     b. Read flags  │                    │
       │                    │     c. Read length │                    │
       │                    │     d. Decompress  │                    │
       │                    │     e. Parse fields│                    │
       │                    │                    │                    │
       │                    │  Vec<Block>        │                    │
       │                    ├───────────────────▶│                    │
       │                    │                    │  1. Scan pass:     │
       │                    │                    │     estimate tokens│
       │                    │                    │  2. Budget pass:   │
       │                    │                    │     rank by        │
       │                    │                    │     priority       │
       │                    │                    │  3. Render pass:   │
       │                    │                    │     emit text per  │
       │                    │                    │     output mode    │
       │                    │                    │                    │
       │                    │                    │  Model-ready text  │
       │                    │                    ├───────────────────▶│
```

---

## Success Criteria

### Phase Completion Gates

- [ ] **Phase 1 — Foundation**: All 11 block types can be serialized to bytes and
  deserialized back with bit-exact round-trip fidelity. Header and varint encoding
  pass exhaustive edge-case tests (0, 1, max u32, max u64 values).

- [ ] **Phase 2 — Render**: The driver produces correct XML-tagged, Markdown, and
  Minimal output for all block types. Output matches golden file snapshots.

- [ ] **Phase 3 — Advanced Features**: Zstd compression reduces payload size by
  ≥20% on representative inputs. BLAKE3 content addressing deduplicates identical
  blocks. Token budget engine correctly prioritizes and summarizes blocks.

- [ ] **Phase 4 — Tooling**: The `lcp` CLI can inspect, validate, encode, and decode
  LCP files. All commands produce correct output and exit codes.

### Quality Metrics

- [ ] **Round-trip fidelity**: 100% of encoded payloads decode to identical block
  structures (tested across all block types and field combinations).

- [ ] **Token savings**: ≥30% structural overhead reduction in Minimal mode vs.
  equivalent raw markdown (measured on at least 5 representative payloads).

- [ ] **Streaming correctness**: Streaming decode produces identical output to
  buffered decode for all golden file inputs.

- [ ] **Test coverage**: ≥90% line coverage across `lcp-wire`, `lcp-types`,
  `lcp-encoder`, and `lcp-decoder` crates.

- [ ] **Clippy clean**: Zero warnings with `#![warn(clippy::pedantic)]` on all crates.

- [ ] **Documentation**: All public API items have rustdoc with offset/size annotations
  following the verbose commenting style (field offset, byte size, wire type).
