# LLM Context Pack (LCP)

> A binary serialization format designed to maximize semantic density within a token-constrained context window.

## What is LCP?

LCP is a block-based binary container format with typed semantic regions (code, conversation, file trees, tool output, metadata) and a driver API for decoding into token-efficient text. Where Protocol Buffers optimizes for machine-to-machine RPC and MessagePack for general serialization, LCP optimizes for LLM consumption.

Current LLM context is wasteful: repeated markdown delimiters, redundant path prefixes, verbose JSON envelopes, and duplicated content across turns. In practice, **30-50% of tokens in a typical agent context window are structural, not semantic**. LCP eliminates this overhead.

## Workspace

This is the Rust proof-of-concept implementation (`bit-context-protocol`), organized as a Cargo workspace:

| Crate | Purpose | Status |
|-------|---------|--------|
| `bcp-wire` | Wire format primitives (varint, header, block frame) | <span class="badge badge-green">Complete</span> |
| `bcp-types` | Block type definitions and field encoding | <span class="badge badge-green">Complete</span> |
| `bcp-encoder` | Builder API for producing LCP payloads | <span class="badge badge-green">Complete</span> |
| `bcp-decoder` | Sync and streaming decode of LCP payloads | <span class="badge badge-green">Complete</span> |
| `lcp-driver` | Renderer (blocks to model-ready text) | <span class="badge badge-yellow">Planned</span> |
| `lcp-cli` | CLI tool for inspect/validate/encode/decode | <span class="badge badge-yellow">Planned</span> |

## Data Flow

```
Tool / Agent ──▶ LcpEncoder ──▶ .lcp binary ──▶ LcpDecoder ──▶ Driver ──▶ LLM
                 (builder)      (wire format)    (binary→blocks) (blocks→text)
```

## Quick Start

```bash
# Build all crates
cargo build --workspace

# Run all tests
cargo test --workspace

# Clippy (pedantic)
cargo clippy --workspace -- -W clippy::pedantic
```
