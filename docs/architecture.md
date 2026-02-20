# Architecture

## System Overview

```
┌──────────────────────────────────────────────────────────────────────┐
│                        BCP Proof of Concept                         │
│                                                                     │
│  ┌─────────────┐    ┌──────────────┐    ┌───────────────────────┐   │
│  │  Encoder    │    │  Wire Format │    │  Decoder / Driver     │   │
│  │             │    │              │    │                       │   │
│  │  Rust API   │──▶│  .bcp file   │──▶│  Binary ─▶ Blocks     │   │
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
│  │  CLI Tool (bcp)                                              │   │
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
      bcp-driver              (depends on bcp-wire + bcp-types)
          │
          ▼
       bcp-cli                (depends on all crates)
```

## Implementation Phases

| Phase | Specs | Crates | Goal | Status |
|-------|-------|--------|------|--------|
| **1. Foundation** | SPEC_01, 02, 03 | `bcp-wire`, `bcp-types`, `bcp-encoder` | Binary wire format + encoding | Complete |
| **2. Decode & Render** | SPEC_04, 05 | `bcp-decoder`, `bcp-driver` | Read payloads, render as text | Complete |
| **3. Advanced** | SPEC_06, 07 | `bcp-encoder`, `bcp-decoder` | Zstd compression, BLAKE3 content addressing | Complete |
| **3b. Budget Engine** | SPEC_08 | `bcp-driver` | Token budget engine, priority ranking, summary fallback | Complete |
| **4. Tooling** | SPEC_09 | `bcp-cli` | CLI binary (inspect, validate, encode, decode, stats) | Complete |
| **4b. Test Suite** | SPEC_10 | `bcp-tests` | Golden files, snapshot tests, roundtrip tests, budget tests, criterion benchmarks | Complete |

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
| `criterion` | 0.5.x | Criterion benchmark harness |
