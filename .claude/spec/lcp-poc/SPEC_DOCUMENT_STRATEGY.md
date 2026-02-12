# LCP PoC — Spec Document Strategy

## Dependency Graph

```
                            Phase 1: Foundation
                    ┌──────────────────────────────────┐
                    │                                  │
                    │   SPEC_01          SPEC_02       │
                    │   Wire Format      Block Types   │
                    │   Primitives       & Fields      │
                    │       │               │          │
                    │       └───────┬───────┘          │
                    │               │                  │
                    │               ▼                  │
                    │          SPEC_03                 │
                    │          Encoder API             │
                    │               │                  │
                    └───────────────┼──────────────────┘
                                    │
                            Phase 2: Decode & Render
                    ┌───────────────┼──────────────────┐
                    │               ▼                  │
                    │          SPEC_04                 │
                    │          Decoder                 │
                    │               │                  │
                    │               ▼                  │
                    │          SPEC_05                 │
                    │          Driver / Renderer       │
                    │                                  │
                    └───────────────┬──────────────────┘
                                    │
                            Phase 3: Advanced Features
                    ┌───────────────┼──────────────────┐
                    │               │                  │
                    │     ┌─────────┼─────────┐        │
                    │     ▼         ▼         ▼        │
                    │  SPEC_06   SPEC_07   SPEC_08     │
                    │  Compress  Content   Token        │
                    │  (zstd)   Address   Budget        │
                    │  (BLAKE3)  Engine                 │
                    │                                  │
                    └───────────────┬──────────────────┘
                                    │
                            Phase 4: Tooling
                    ┌───────────────┼──────────────────┐
                    │               ▼                  │
                    │     ┌─────────┴─────────┐        │
                    │     ▼                   ▼        │
                    │  SPEC_09            SPEC_10      │
                    │  CLI Tool           Golden File  │
                    │                     Test Suite   │
                    │                                  │
                    └──────────────────────────────────┘
```

---

## Phase 1: Foundation

**Goal**: Establish the binary wire format and encoding path.

| Spec    | Focus                          | Deliverables                                        |
|---------|--------------------------------|-----------------------------------------------------|
| SPEC_01 | Wire format primitives         | `lcp-wire` crate: varint, header, block frame       |
| SPEC_02 | Block type definitions         | `lcp-types` crate: all 11 block types, enums        |
| SPEC_03 | Encoder API                    | `lcp-encoder` crate: builder pattern, serialization |

**Exit Criteria**:
- [ ] Varint round-trips correctly for values 0, 1, 127, 128, 16383, 2^32-1, 2^63-1
- [ ] File header writes/reads magic `0x4C435000`, version 1.0, flags byte
- [ ] All 11 block types serialize to bytes and pass manual hex inspection
- [ ] `LcpEncoder` builder compiles and produces valid `.lcp` payloads
- [ ] `cargo test -p lcp-wire -p lcp-types -p lcp-encoder` passes with zero failures
- [ ] `cargo clippy -p lcp-wire -p lcp-types -p lcp-encoder -- -W clippy::pedantic` emits zero warnings

---

## Phase 2: Decode & Render

**Goal**: Read LCP payloads back and render them as model-ready text.

| Spec    | Focus                          | Deliverables                                        |
|---------|--------------------------------|-----------------------------------------------------|
| SPEC_04 | Decoder                        | `lcp-decoder` crate: sync + async streaming decode  |
| SPEC_05 | Driver / Renderer              | `lcp-driver` crate: XML, Markdown, Minimal modes    |

**Exit Criteria**:
- [ ] Decoder parses all valid payloads produced by SPEC_03's encoder
- [ ] Encode → decode round-trip preserves all block fields exactly
- [ ] Streaming decode produces byte-identical output to buffered decode
- [ ] XML-tagged mode output matches expected format from RFC §12.3
- [ ] Markdown and Minimal modes produce syntactically valid output
- [ ] `cargo test -p lcp-decoder -p lcp-driver` passes with zero failures

---

## Phase 3: Advanced Features

**Goal**: Add compression, content addressing, and token budget intelligence.

| Spec    | Focus                          | Deliverables                                        |
|---------|--------------------------------|-----------------------------------------------------|
| SPEC_06 | Compression                    | Zstd per-block + whole-payload, flags integration   |
| SPEC_07 | Content addressing             | BLAKE3 hashing, reference blocks, dedup             |
| SPEC_08 | Token budget engine            | Priority ranking, summary fallback, two-pass decode |

**Exit Criteria**:
- [ ] Compressed payloads are ≥20% smaller than uncompressed on representative data
- [ ] Compressed payloads decode to identical block structures
- [ ] BLAKE3 reference blocks resolve correctly against a content store
- [ ] Duplicate blocks in a payload are deduplicated to a single stored copy
- [ ] Token budget engine emits summaries for low-priority blocks when budget is tight
- [ ] Budget engine includes full content for CRITICAL blocks even when over budget
- [ ] `cargo test -p lcp-encoder -p lcp-decoder -p lcp-driver` passes (all three crates affected)

---

## Phase 4: Tooling

**Goal**: Deliver a usable CLI and comprehensive test suite.

| Spec    | Focus                          | Deliverables                                        |
|---------|--------------------------------|-----------------------------------------------------|
| SPEC_09 | CLI tool                       | `lcp-cli` binary: inspect, validate, encode, decode |
| SPEC_10 | Golden file test suite         | Fixtures, snapshot tests, conformance validation    |

**Exit Criteria**:
- [ ] `lcp inspect <file>` prints a human-readable summary of blocks
- [ ] `lcp validate <file>` exits 0 for valid files, non-zero with diagnostic for invalid
- [ ] `lcp encode <input>` produces a valid `.lcp` file from JSON/TOML input
- [ ] `lcp decode <file>` renders to stdout in the specified output mode
- [ ] `lcp stats <file>` reports block counts, sizes, and compression ratios
- [ ] Golden file tests cover: simple code, conversation, mixed blocks, compressed,
  content-addressed, budget-constrained scenarios
- [ ] `cargo test --workspace` passes with zero failures
- [ ] `cargo clippy --workspace -- -W clippy::pedantic` emits zero warnings

---

## Execution Strategy

```
Week 1          Week 2          Week 3          Week 4
────────────────────────────────────────────────────────────────

SPEC_01 ████████
Wire Primitives ▏

SPEC_02 ████████████████
Block Types     ▏

        SPEC_03 ████████████████
        Encoder         ▏
                                ▏
                SPEC_04 ████████████████
                Decoder         ▏
                                ▏
                        SPEC_05 ████████████████
                        Driver          ▏
                                        ▏
                ┌─ SPEC_06 ─────────────┤  (parallel with SPEC_05)
                ├─ SPEC_07 ─────────────┤  (parallel with SPEC_05)
                └─ SPEC_08 ─────────────┤  (after SPEC_05)
                                        ▏
                                SPEC_09 ████████
                                CLI     ▏
                                        ▏
                                SPEC_10 ████████
                                Tests   ▏

────────────────────────────────────────────────────────────────
         Phase 1         Phase 2      Phase 3      Phase 4
```

**Parallelization notes**:
- SPEC_01 and SPEC_02 can start in parallel (no dependency between them)
- SPEC_06 (compression) and SPEC_07 (content addressing) can run in parallel
- SPEC_08 (token budget) depends on SPEC_05 (driver) being complete
- SPEC_09 and SPEC_10 can run in parallel once all prior specs are done

---

## Risk Mitigation

| Risk                                        | Likelihood | Impact | Mitigation                                                        |
|---------------------------------------------|------------|--------|-------------------------------------------------------------------|
| Varint edge cases cause silent data loss     | Medium     | High   | Exhaustive property-based tests for LEB128; compare against known-good impl |
| Zstd compression ratio disappointing for small blocks | Medium | Low | Fall back to uncompressed for blocks under threshold (e.g., 256 bytes) |
| Streaming decode buffer management complex   | High       | Medium | Start with sync-only decode; add async streaming after core is solid |
| Token estimation inaccurate without tokenizer| Medium     | Medium | Use character-count heuristic (÷4 for English text); make tokenizer pluggable |
| Block type field encoding ambiguity          | Low        | High   | Define exact wire format for each block type in SPEC_02 with hex examples |
| BLAKE3 content store adds I/O latency        | Low        | Low    | PoC uses in-memory `HashMap<[u8; 32], Vec<u8>>`; defer disk store |
| Scope creep into production features         | High       | Medium | Strict PoC boundary: no encryption, no WASM, no nested blocks     |

---

## Rollback Plan

### Phase 1 Rollback
- Delete `crates/lcp-wire`, `crates/lcp-types`, `crates/lcp-encoder` directories
- Revert `Cargo.toml` workspace members list
- No external state is created; rollback is a clean `git revert`

### Phase 2 Rollback
- Delete `crates/lcp-decoder`, `crates/lcp-driver` directories
- Revert workspace members; Phase 1 crates remain functional independently

### Phase 3 Rollback
- Remove compression and content-addressing code from encoder/decoder
- Revert the `zstd` and `blake3` dependency additions
- Core encode/decode path continues to work without these features

### Phase 4 Rollback
- Delete `crates/lcp-cli` and `tests/golden/` directory
- Library crates remain fully functional without the CLI
- Individual crate tests remain in their respective `src/` directories
