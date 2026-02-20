# Bit Context Protocol (BCP)

> A binary serialization format designed to maximize semantic density within a token-constrained context window.

## What is BCP?

BCP is a block-based binary container format with typed semantic regions (code, conversation, file trees, tool output, metadata) and a driver API for decoding into token-efficient text. Where Protocol Buffers optimizes for machine-to-machine RPC and MessagePack for general serialization, BCP optimizes for LLM consumption.

Current LLM context is wasteful: repeated markdown delimiters, redundant path prefixes, verbose JSON envelopes, and duplicated content across turns. In practice, **30-50% of tokens in a typical agent context window are structural, not semantic**. BCP eliminates this overhead.

## Workspace

This is the Rust proof-of-concept implementation (`bit-context-protocol`), organized as a Cargo workspace:

| Crate | Purpose | Status |
|-------|---------|--------|
| `bcp-wire` | Wire format primitives (varint, header, block frame) | <span class="badge badge-green">Complete</span> |
| `bcp-types` | Block type definitions and field encoding | <span class="badge badge-green">Complete</span> |
| `bcp-encoder` | Builder API for producing BCP payloads | <span class="badge badge-green">Complete</span> |
| `bcp-decoder` | Sync and streaming decode of BCP payloads | <span class="badge badge-green">Complete</span> |
| `bcp-driver` | Renderer (blocks to model-ready text, token budget engine) | <span class="badge badge-green">Complete</span> |
| `bcp-cli` | CLI tool — inspect, validate, encode, decode, stats | <span class="badge badge-green">Complete</span> |

## Data Flow

```
JSON manifest ──▶ bcp encode ──▶ .bcp binary ──▶ bcp decode / bcp inspect / bcp stats
                  (BcpEncoder)   (wire format)    (BcpDecoder + DefaultDriver)
```

Full library pipeline:

```
Tool / Agent ──▶ BcpEncoder ──▶ .bcp binary ──▶ BcpDecoder ──▶ Driver ──▶ LLM
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

## CLI

The `bcp` binary provides five subcommands for working with `.bcp` files without writing Rust:

```bash
# Create a .bcp file from a JSON manifest
bcp encode context.json -o context.bcp

# Check structural validity (exits 0 = valid, 1 = invalid)
bcp validate context.bcp

# Inspect block layout
bcp inspect context.bcp --show-body

# Render as model-ready text
bcp decode context.bcp --mode xml
bcp decode context.bcp --mode markdown --include code,conversation
bcp decode context.bcp --mode minimal --budget 2000

# Token and size statistics
bcp stats context.bcp
```

See [bcp-cli](crate-bcp-cli.md) for the full manifest format and flag reference.
