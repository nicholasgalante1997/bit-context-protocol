# LCP PoC — Design Decisions

This document records non-obvious technical decisions made during the PoC
design. Each decision includes the options considered, trade-offs, and the
chosen approach with rationale.

---

## DD-01: Varint Encoding — LEB128 vs. VLQ vs. Fixed Width

### Question

How should variable-length integers be encoded on the wire?

### Options

#### Option A: Unsigned LEB128 (Protocol Buffers convention)

```rust
// Encoding: 7 bits per byte, MSB is continuation flag
// Value 300 → [0xAC, 0x02]
fn encode_varint(mut value: u64, buf: &mut [u8]) -> usize {
    let mut i = 0;
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        buf[i] = if value > 0 { byte | 0x80 } else { byte };
        i += 1;
        if value == 0 { break; }
    }
    i
}
```

- Pros: Industry standard (protobuf, DWARF, WebAssembly). Battle-tested. ⭐⭐⭐⭐⭐
- Cons: Maximum 10 bytes for u64. Not self-synchronizing. ⭐⭐⭐

#### Option B: Variable-Length Quantity (VLQ, MIDI convention)

```rust
// Encoding: same bit layout as LEB128 but big-endian byte order
// Value 300 → [0x82, 0x2C]
```

- Pros: Self-synchronizing (can scan backwards). ⭐⭐⭐
- Cons: Uncommon in modern formats. Less library support. ⭐⭐

#### Option C: Fixed 4-byte or 8-byte integers

```rust
// All integers are u32 little-endian (4 bytes) or u64 (8 bytes)
// Value 300 → [0x2C, 0x01, 0x00, 0x00]
```

- Pros: Simplest to implement. Constant-time access. ⭐⭐⭐⭐
- Cons: Wastes bytes for small values (block types, field IDs are 1-2 bytes). ⭐

### Decision: **Option A — Unsigned LEB128**

**Rationale**: LEB128 is the clear winner for a format that prioritizes
compactness. Block type IDs (0x01-0xFF) and field IDs (1-5 typically) encode
as single bytes. Content lengths for typical blocks (< 16KB) encode as 1-2
bytes. The protobuf convention ensures developers are familiar with the
encoding and can verify correctness against the protobuf specification.

### Implementation Rules

- Use `u64` as the Rust type for all varint values (decode returns `u64`).
- Reject varints exceeding 10 bytes (maximum for u64).
- The encode function takes `&mut [u8]` (caller provides buffer) for zero-allocation encoding.
- The decode function returns `(value, bytes_consumed)` for cursor advancement.

---

## DD-02: Error Handling — thiserror vs. Custom Enum vs. anyhow Everywhere

### Question

What error handling strategy should the crate library vs. CLI use?

### Options

#### Option A: `thiserror` for library crates, `anyhow` for CLI

```rust
// Library crate (lcp-wire):
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("varint too long")]
    VarintTooLong,
    #[error("invalid magic: {found:#X}")]
    InvalidMagic { found: u32 },
}

// CLI (lcp-cli):
fn main() -> anyhow::Result<()> {
    let payload = std::fs::read(&args.file)?; // auto-wraps IO error
    let decoded = LcpDecoder::decode(&payload)?; // auto-wraps DecodeError
    Ok(())
}
```

- Pros: Clean separation. Library errors are specific and matchable. CLI errors auto-convert. ⭐⭐⭐⭐⭐
- Cons: Two error crates as dependencies. ⭐⭐⭐⭐

#### Option B: `thiserror` everywhere

```rust
// Every layer has its own error enum. CLI wraps them manually.
fn main() -> Result<(), CliError> {
    let payload = std::fs::read(&args.file).map_err(CliError::Io)?;
    let decoded = LcpDecoder::decode(&payload).map_err(CliError::Decode)?;
    Ok(())
}
```

- Pros: Precise error types at every level. ⭐⭐⭐⭐
- Cons: Boilerplate in CLI. Manual wrapping at every boundary. ⭐⭐

#### Option C: `anyhow` everywhere

```rust
// All functions return anyhow::Result
fn decode(payload: &[u8]) -> anyhow::Result<DecodedPayload> {
    // Callers cannot match on specific error variants
}
```

- Pros: Minimal boilerplate. Fast development. ⭐⭐⭐⭐
- Cons: Library consumers cannot match on specific errors. Bad API design for a reusable crate. ⭐

### Decision: **Option A — thiserror for libraries, anyhow for CLI**

**Rationale**: The `lcp-*` library crates are intended to be reusable. Consumers
need to match on specific errors (e.g., to distinguish `InvalidMagic` from
`VarintTooLong`). `thiserror` derives provide this with zero boilerplate.
The CLI binary is the application boundary where error specificity matters
less — `anyhow` gives clean error messages with source chain for free.

### Implementation Rules

- Each library crate has its own error enum in `error.rs`.
- Error enums use `#[derive(Debug, thiserror::Error)]`.
- Errors include `#[from]` for natural conversion from lower-level errors.
- The CLI uses `anyhow::Result<()>` for `main()` and all command functions.
- Never use `.unwrap()` in library code. Use `.expect()` only for invariants.

---

## DD-03: Buffer Management — Vec<u8> vs. bytes::Bytes vs. &[u8]

### Question

How should the encoder and decoder manage byte buffers?

### Options

#### Option A: `Vec<u8>` for owned, `&[u8]` for borrowed

```rust
// Encoder produces owned bytes:
pub fn encode(&self) -> Result<Vec<u8>, EncodeError> { /* ... */ }

// Decoder borrows from input:
pub fn decode(payload: &[u8]) -> Result<DecodedPayload, DecodeError> { /* ... */ }
```

- Pros: Idiomatic Rust. No extra dependencies. Clear ownership semantics. ⭐⭐⭐⭐⭐
- Cons: Decoder must copy strings out of the input slice. ⭐⭐⭐

#### Option B: `bytes::Bytes` for zero-copy shared ownership

```rust
// Decoder returns Bytes that reference-count the input:
pub fn decode(payload: Bytes) -> Result<DecodedPayload, DecodeError> { /* ... */ }

// Block content is Bytes (zero-copy slice of original buffer):
pub struct CodeBlock {
    pub content: Bytes, // zero-copy reference into payload
}
```

- Pros: Zero-copy for large payloads. Familiar from tokio ecosystem. ⭐⭐⭐⭐
- Cons: Adds dependency. Lifetime tied to input buffer (can't drop input). ⭐⭐⭐

#### Option C: Arena allocation

```rust
// All decoded data lives in an arena. Lifetime tied to arena.
pub fn decode<'a>(payload: &[u8], arena: &'a Arena) -> Result<DecodedPayload<'a>, DecodeError> { /* ... */ }
```

- Pros: Fast allocation. Batch deallocation. ⭐⭐⭐
- Cons: Complex lifetimes. Unusual API for consumers. ⭐

### Decision: **Option A — Vec<u8> for owned, &[u8] for borrowed**

**Rationale**: The PoC prioritizes simplicity and correctness over zero-copy
performance. `Vec<u8>` and `&[u8]` are idiomatic, require no extra
dependencies, and have clear ownership semantics. The string copies during
decode are negligible for the payload sizes expected in a PoC. If profiling
shows this is a bottleneck, migrating to `bytes::Bytes` is straightforward.

Note: We still include `bytes` as a dependency for the `bytes::Buf` / `bytes::BufMut`
traits, which simplify the streaming decoder. We just don't use `Bytes` as
the primary buffer type in the public API.

### Implementation Rules

- Encoder's `encode()` returns `Vec<u8>`.
- Decoder's `decode()` takes `&[u8]` and returns owned structs.
- Block content fields are `Vec<u8>` (owned copies from the input).
- String fields are `String` (owned, decoded from UTF-8).
- The streaming decoder may use `bytes::BytesMut` internally.

---

## DD-04: Endianness — Little-Endian vs. Big-Endian vs. Native

### Question

What byte order should LCP use for multi-byte integers?

### Options

#### Option A: Little-endian (matching protobuf and most modern CPUs)

```rust
// Write u32 as little-endian:
buf[0] = (value & 0xFF) as u8;
buf[1] = ((value >> 8) & 0xFF) as u8;
buf[2] = ((value >> 16) & 0xFF) as u8;
buf[3] = ((value >> 24) & 0xFF) as u8;
```

- Pros: Matches x86/ARM native order. No byte-swapping on common platforms. Matches protobuf. ⭐⭐⭐⭐⭐
- Cons: Not "network byte order" (big-endian). ⭐⭐⭐⭐

#### Option B: Big-endian (network byte order)

- Pros: Traditional for network protocols. ⭐⭐⭐
- Cons: Requires byte-swapping on x86/ARM. Against modern convention. ⭐⭐

### Decision: **Option A — Little-endian**

**Rationale**: The RFC explicitly specifies little-endian (§4.1). Additionally,
varint encoding (LEB128) is inherently little-endian. All modern consumer
CPUs (x86, ARM) are little-endian, so this avoids byte-swapping overhead.

### Implementation Rules

- The file header magic number is written as raw bytes `[0x4C, 0x43, 0x50, 0x00]`,
  not as a u32 (byte order independent).
- All other multi-byte integers use little-endian encoding.
- Use `u32::to_le_bytes()` / `u32::from_le_bytes()` for fixed-width integers.
- Varints are inherently little-endian (LSB first).

---

## DD-05: Workspace Structure — Monolithic Crate vs. Multi-Crate Workspace

### Question

Should the PoC be a single crate or a Cargo workspace with multiple crates?

### Options

#### Option A: Multi-crate workspace (5 library crates + 1 binary)

```toml
[workspace]
members = [
    "crates/lcp-wire",
    "crates/lcp-types",
    "crates/lcp-encoder",
    "crates/lcp-decoder",
    "crates/lcp-driver",
    "crates/lcp-cli",
]
```

- Pros: Clear separation of concerns. Independent compilation. Consumers can depend on only what they need. ⭐⭐⭐⭐⭐
- Cons: More `Cargo.toml` files. Inter-crate dependency management. ⭐⭐⭐

#### Option B: Single crate with modules

```
src/
├── lib.rs
├── wire/
├── types/
├── encoder/
├── decoder/
├── driver/
└── main.rs
```

- Pros: Simpler setup. Single `Cargo.toml`. Faster initial development. ⭐⭐⭐⭐
- Cons: Consumers get everything or nothing. Longer compile times. Less clear boundaries. ⭐⭐

#### Option C: Two crates (library + binary)

```toml
[workspace]
members = ["lcp-core", "lcp-cli"]
```

- Pros: Balanced. Clean library/binary split. ⭐⭐⭐⭐
- Cons: Less granular than multi-crate. ⭐⭐⭐

### Decision: **Option A — Multi-crate workspace**

**Rationale**: Even for a PoC, the multi-crate workspace establishes the
correct architecture that a production implementation would use. It enforces
clean dependency boundaries (e.g., `lcp-types` cannot accidentally depend on
`lcp-driver`). The `lcp-wire` and `lcp-types` crates can be published
independently for use by other tools. The additional `Cargo.toml` files are
trivial boilerplate.

### Implementation Rules

- Workspace root `Cargo.toml` defines shared dependencies with `[workspace.dependencies]`.
- Each crate re-exports its public API from `lib.rs`.
- Inter-crate dependencies use `{ workspace = true }` syntax.
- The `lcp-cli` binary depends on all library crates.
- Shared test utilities go in a `tests/` directory at workspace root.

---

## DD-06: Compression Library — zstd (C wrapper) vs. zstd-rs (pure Rust) vs. lz4

### Question

Which compression library should the PoC use?

### Options

#### Option A: `zstd` crate (wrapper around the C reference implementation)

```toml
[dependencies]
zstd = "0.13"
```

```rust
let compressed = zstd::encode_all(data.as_slice(), 3)?;
let decompressed = zstd::decode_all(compressed.as_slice())?;
```

- Pros: Reference implementation. Best ratio-to-speed. RFC specifies zstd. Widely used. ⭐⭐⭐⭐⭐
- Cons: Requires C compiler for building. Larger binary. ⭐⭐⭐

#### Option B: `zstd-rs` / `ruzstd` (pure Rust decompressor)

- Pros: Pure Rust. No C toolchain needed. WASM-compatible. ⭐⭐⭐⭐
- Cons: Decompress-only (no encoder). Slower. Less tested. ⭐⭐

#### Option C: `lz4_flex` (LZ4 in pure Rust)

- Pros: Very fast. Pure Rust. Small. ⭐⭐⭐⭐
- Cons: Worse compression ratio than zstd. RFC specifies zstd, not lz4. ⭐⭐

### Decision: **Option A — `zstd` crate (C wrapper)**

**Rationale**: The RFC explicitly specifies Zstandard compression (§4.6).
The C wrapper provides both encoding and decoding with the reference
implementation's quality. The C toolchain requirement is acceptable for
a Rust PoC (every Rust developer has a C compiler). WASM compatibility
is a future concern, not a PoC concern.

### Implementation Rules

- Compression level: 3 (default, good balance of speed and ratio).
- Compression threshold: 256 bytes minimum body size.
- If compressed size >= uncompressed size, store uncompressed (clear the flag).
- Decompression bomb protection: reject outputs exceeding 10x the compressed size.

---

## DD-07: Output Renderer — Trait Dispatch vs. Enum Match vs. Strategy Pattern

### Question

How should the driver dispatch to different output mode renderers?

### Options

#### Option A: Trait objects (`Box<dyn Renderer>`)

```rust
trait Renderer {
    fn render_code(&self, block: &CodeBlock) -> String;
    fn render_conversation(&self, block: &ConversationBlock) -> String;
    // ... per block type
}

struct XmlRenderer;
impl Renderer for XmlRenderer { /* ... */ }
```

- Pros: Extensible. New modes added without modifying existing code. ⭐⭐⭐⭐
- Cons: Virtual dispatch overhead. More complex for 3 renderers. ⭐⭐⭐

#### Option B: Enum match in a single function

```rust
fn render_block(block: &Block, mode: OutputMode) -> String {
    match mode {
        OutputMode::Xml => render_xml(block),
        OutputMode::Markdown => render_markdown(block),
        OutputMode::Minimal => render_minimal(block),
    }
}
```

- Pros: Simple. All rendering logic visible in one place. No dynamic dispatch. ⭐⭐⭐⭐⭐
- Cons: Adding a mode requires touching the match. Less extensible. ⭐⭐⭐

#### Option C: Separate structs implementing a shared trait (static dispatch)

```rust
struct DefaultDriver<R: Renderer> {
    renderer: R,
}
```

- Pros: Zero-cost abstraction. Monomorphized. ⭐⭐⭐⭐
- Cons: Generic proliferation. Mode selected at compile time, not runtime. ⭐⭐

### Decision: **Option A — Trait objects**

**Rationale**: The driver needs to select the rendering mode at runtime (from
CLI flags or `DriverConfig`). Trait objects provide clean extensibility:
adding a "Raw" mode later requires only a new struct, not modifying
existing renderers. The virtual dispatch overhead is negligible compared
to the string formatting cost. Each renderer lives in its own file for
clear separation.

### Implementation Rules

- `Renderer` trait has one method per block type + a `render_block` dispatcher.
- Each renderer struct lives in its own file (`render_xml.rs`, etc.).
- `DefaultDriver` holds a `Box<dyn Renderer>` selected from `OutputMode`.
- Renderers are stateless (no fields) — `&self` is just for trait dispatch.
- Summary rendering is a variant of each block's render method (pass a flag).
