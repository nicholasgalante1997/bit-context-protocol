# Architecture

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
│        ▼                   ▼                       ▼                 │
│  ┌─────────────┐    ┌──────────────┐    ┌───────────────────────┐   │
│  │  Compression│    │  Content     │    │  Token Budget Engine  │   │
│  │  (zstd)     │    │  Addressing  │    │  (priority ranking,   │   │
│  │             │    │  (BLAKE3)    │    │   summary fallback)   │   │
│  └─────────────┘    └──────────────┘    └───────────────────────┘   │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  CLI Tool (lcp)                                              │   │
│  │  inspect · validate · encode · decode · stats                │   │
│  └──────────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────────┘
```

## Crate Dependency Graph

```
bcp-wire          (no dependencies)
   │
   ▼
bcp-types         (depends on bcp-wire)
   │
   ├──────────────┐
   ▼              ▼
bcp-encoder    bcp-decoder    (both depend on bcp-wire + bcp-types)
   │              │
   └──────┬───────┘
          ▼
      lcp-driver              (depends on bcp-types, bcp-decoder)
          │
          ▼
       lcp-cli                (depends on all crates)
```

## Implementation Phases

| Phase | Specs | Crates | Goal |
|-------|-------|--------|------|
| **1. Foundation** | SPEC_01, 02, 03 | `bcp-wire`, `bcp-types`, `bcp-encoder` | Binary wire format + encoding |
| **2. Decode & Render** | SPEC_04, 05 | `bcp-decoder`, `lcp-driver` | Read payloads, render as text |
| **3. Advanced** | SPEC_06, 07, 08 | Modifications to existing crates | Compression, dedup, budget engine |
| **4. Tooling** | SPEC_09, 10 | `lcp-cli`, `tests/` | CLI binary + golden file tests |

## Technology Stack

| Dependency | Version | Purpose |
|------------|---------|---------|
| `thiserror` | 2.x | Typed error definitions (library crates) |
| `anyhow` | 1.x | Error handling (CLI binary) |
| `zstd` | 0.13.x | Zstandard compression |
| `blake3` | 1.x | Content-addressed hashing |
| `clap` | 4.x | CLI argument parsing |
| `tokio` | 1.x | Async runtime (streaming decode) |
| `insta` | 1.x | Snapshot testing |
