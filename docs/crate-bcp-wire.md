# bcp-wire

<span class="badge badge-green">Complete</span> <span class="badge badge-blue">Phase 1</span>

> The foundation of the entire LCP protocol. Every byte that enters or exits the system passes through primitives defined in this crate.

## Crate Info

| Field | Value |
|-------|-------|
| Path | `crates/bcp-wire/` |
| Spec | [SPEC_01](wire-primitives.md) |
| Dependencies | `thiserror` |
| Dependents | `bcp-types`, `bcp-encoder`, `bcp-decoder` |

---

## Purpose and Role in the Protocol

The LCP RFC (Section 4) defines a binary wire format built on three primitives: **varint encoding**, a **file header**, and a **block frame envelope**. `bcp-wire` is the Rust implementation of these primitives. It is the lowest layer of the entire crate stack and has zero internal dependencies beyond `thiserror` for error types.

Every other crate in the workspace depends on `bcp-wire`:

- **bcp-types** uses the varint functions and `BlockFlags` to encode/decode TLV field payloads inside block bodies
- **bcp-encoder** uses `LcpHeader::write_to` and `BlockFrame::write_to` to produce the binary output
- **bcp-decoder** uses `LcpHeader::read_from` and `BlockFrame::read_from` to parse binary input

If the wire format is wrong, nothing else works. This is why `bcp-wire` has exhaustive tests covering edge cases (varint overflow, truncated input, boundary values, sequential frame reads) and is the first spec implemented in the project.

---

## Why These Primitives Exist

The LCP format is designed for a fundamentally different target than Protocol Buffers or MessagePack: **maximizing semantic density within a token-constrained context window**. The wire primitives reflect this:

- **Varint encoding (LEB128)** provides compact integer representation. Block type IDs, field IDs, and content lengths are all varints, meaning small values (which are the common case) use only 1 byte. This follows protobuf conventions so the format is familiar to systems programmers, but the use case is novel: these varints tag *semantic context blocks* (code, conversation, file trees) rather than generic message fields.

- **The 8-byte file header** is deliberately fixed-size for fast validation. A receiver can check the magic number (`LCP\0`) and version in a single 8-byte read before committing to parse the rest of the payload. The flags byte enables whole-payload compression and an index trailer, both of which are critical for the token budget engine that decides which blocks to expand vs. summarize.

- **The block frame envelope** wraps every semantic block with its type tag, per-block flags, and length. This length-prefixed design enables streaming decode — the decoder reads the frame header, knows exactly how many bytes the body occupies, and can process or skip it without buffering the entire payload. The `BlockFlags` bitfield controls per-block features: summary sub-blocks (for budget-aware rendering), per-block zstd compression, and BLAKE3 content-addressed references for deduplication.

---

## Varint Encoding (LEB128)

Unsigned LEB128 uses 7 data bits per byte with the MSB as a continuation flag. This is the same encoding used by Protocol Buffers, WebAssembly, and DWARF debug info.

### Wire Examples

| Value | Encoded | Bytes | Notes |
|-------|---------|-------|-------|
| `0` | `[0x00]` | 1 | Minimum: single zero byte |
| `127` | `[0x7F]` | 1 | Largest single-byte value (7 bits set) |
| `128` | `[0x80, 0x01]` | 2 | First two-byte value |
| `300` | `[0xAC, 0x02]` | 2 | Protobuf spec example |
| `16383` | `[0xFF, 0x7F]` | 2 | Largest two-byte value |
| `16384` | `[0x80, 0x80, 0x01]` | 3 | First three-byte value |
| `u32::MAX` | 5 bytes | 5 | — |
| `u64::MAX` | 10 bytes | 10 | Maximum: `ceil(64/7)` |

### API

```rust
/// Encode a u64 as LEB128. Returns bytes written (1-10).
/// Panics if buf is shorter than required. A 10-byte buffer always suffices.
pub fn encode_varint(value: u64, buf: &mut [u8]) -> usize;

/// Decode LEB128 from a byte slice. Returns (value, bytes_consumed).
pub fn decode_varint(buf: &[u8]) -> Result<(u64, usize), WireError>;
```

### Key Behaviors

- **Trailing bytes are left alone**: `decode_varint(&[0xAC, 0x02, 0xFF, 0xFF])` returns `(300, 2)`. The decoder only consumes the varint portion. This is essential for sequential frame parsing, where the block type varint is immediately followed by the flags byte.
- **Empty input**: Returns `WireError::UnexpectedEof { offset: 0 }`.
- **Overflow protection**: More than 10 continuation bytes returns `WireError::VarintTooLong`. This prevents malicious inputs from causing unbounded reads.
- **Round-trip fidelity**: `decode_varint(encode_varint(v))` is tested for all boundary values including 0, 1, 127, 128, 255, 16383, 16384, `u32::MAX`, and `u64::MAX`.

---

## File Header

Every LCP payload begins with a fixed 8-byte header. The fixed size means the decoder can validate the payload with a single read before committing to parse blocks.

### Wire Layout

```
Offset  Size     Field
──────  ───────  ──────────────────────────────────────────────
0x00    4 bytes  Magic: "LCP\0" (0x4C, 0x43, 0x50, 0x00)
0x04    1 byte   Version major (current: 1)
0x05    1 byte   Version minor (current: 0)
0x06    1 byte   Flags bitfield
0x07    1 byte   Reserved (MUST be 0x00)
```

### HeaderFlags Bitfield

| Bit | Constant | Meaning |
|-----|----------|---------|
| 0 | `COMPRESSED` | Entire payload (after header) is zstd compressed |
| 1 | `HAS_INDEX` | An index trailer is appended after the END sentinel |
| 2-7 | — | Reserved, must be 0 |

### API

```rust
pub const LCP_MAGIC: [u8; 4] = [0x4C, 0x43, 0x50, 0x00];
pub const HEADER_SIZE: usize = 8;
pub const VERSION_MAJOR: u8 = 1;
pub const VERSION_MINOR: u8 = 0;

pub struct HeaderFlags(u8);
impl HeaderFlags {
    pub const NONE: Self;
    pub const COMPRESSED: Self;
    pub const HAS_INDEX: Self;
    pub fn from_raw(raw: u8) -> Self;
    pub fn raw(self) -> u8;
    pub fn is_compressed(self) -> bool;
    pub fn has_index(self) -> bool;
}

pub struct LcpHeader {
    pub version_major: u8,
    pub version_minor: u8,
    pub flags: HeaderFlags,
}

impl LcpHeader {
    pub fn new(flags: HeaderFlags) -> Self;
    pub fn write_to(&self, buf: &mut [u8]) -> Result<(), WireError>;
    pub fn read_from(buf: &[u8]) -> Result<Self, WireError>;
}
```

### Validation Order

`read_from` validates in a deliberate sequence that produces the most useful error for each failure mode:

1. **Buffer length** >= 8 → `UnexpectedEof` (not even enough bytes for a header)
2. **Magic number** matches `LCP\0` → `InvalidMagic` (not an LCP file at all)
3. **Major version** is 1 → `UnsupportedVersion` (LCP file, but from the future)
4. **Reserved byte** is 0x00 → `ReservedNonZero` (LCP v1 file, but corrupted or incompatible)

### Implementation Notes

- Magic is stored as `[u8; 4]` and compared as raw bytes, not as a `u32`. This sidesteps endianness entirely. The `u32::from_le_bytes` conversion appears only in the error message for developer readability.
- `HeaderFlags` is a newtype (`struct HeaderFlags(u8)`) with `Copy` semantics. Since it wraps a single byte, copying is trivially cheap and the type provides safety over raw `u8` manipulation.
- `HeaderFlags` implements `Default` (all bits zero), which `LcpEncoder` uses to construct the standard no-flags header.

---

## Block Frame

The block frame is the envelope wrapping every block's body. After the 8-byte header, an LCP payload is a concatenated sequence of block frames terminated by an END sentinel.

### Wire Layout

```
┌──────────────────────────────────────────────────┐
│ block_type   (varint, 1-2 bytes typically)       │
│ block_flags  (uint8, 1 byte)                     │
│ content_len  (varint, 1-5 bytes typically)       │
│ body         [content_len bytes]                  │
└──────────────────────────────────────────────────┘
```

The block type is a varint (not a raw byte) because extension types like `0xFE` require 2 bytes in LEB128 encoding. The flags byte is always exactly 1 byte since only 3 bits are defined and the rest are reserved.

### BlockFlags Bitfield

| Bit | Constant | Meaning | Used By |
|-----|----------|---------|---------|
| 0 | `HAS_SUMMARY` | Body starts with a length-prefixed summary | Token budget engine |
| 1 | `COMPRESSED` | Body is zstd compressed (per-block) | SPEC_06 (not yet implemented) |
| 2 | `IS_REFERENCE` | Body is a 32-byte BLAKE3 hash, not inline data | SPEC_07 (not yet implemented) |
| 3-7 | — | Reserved, must be 0 | — |

### Block Type Constants

All 11 semantic types plus the END sentinel are defined as `u8` constants in the `block_type` module:

```rust
pub mod block_type {
    pub const CODE: u8 = 0x01;
    pub const CONVERSATION: u8 = 0x02;
    pub const FILE_TREE: u8 = 0x03;
    pub const TOOL_RESULT: u8 = 0x04;
    pub const DOCUMENT: u8 = 0x05;
    pub const STRUCTURED_DATA: u8 = 0x06;
    pub const DIFF: u8 = 0x07;
    pub const ANNOTATION: u8 = 0x08;
    pub const EMBEDDING_REF: u8 = 0x09;
    pub const IMAGE: u8 = 0x0A;
    pub const EXTENSION: u8 = 0xFE;
    pub const END: u8 = 0xFF;
}
```

These are intentionally small integers, not strings. Per RFC Section 8.2, this design choice enables a future where model providers can map block type IDs directly to special tokens in a learned binary vocabulary.

### API

```rust
pub struct BlockFrame {
    pub block_type: u8,
    pub flags: BlockFlags,
    pub body: Vec<u8>,
}

impl BlockFrame {
    /// Write frame to a writer. Returns total bytes written.
    /// Accepts impl Write: works with Vec<u8>, files, or network streams.
    pub fn write_to(&self, w: &mut impl Write) -> Result<usize, WireError>;

    /// Read from byte slice. Returns None for END (0xFF) sentinel.
    /// Returns Some((frame, bytes_consumed)) for normal blocks.
    pub fn read_from(buf: &[u8]) -> Result<Option<(Self, usize)>, WireError>;
}
```

### Key Behaviors

- **END sentinel**: When `read_from` encounters block type `0xFF`, it returns `Ok(None)` immediately. The END block has no flags, length, or body — only the type varint is consumed. This signals the decoder to stop iterating.
- **Sequential reading**: `read_from` returns the number of bytes consumed alongside the frame, so callers can advance their slice: `BlockFrame::read_from(&buf[consumed..])`. This pattern is used by the sync decoder to walk through all frames in a payload.
- **Writer-agnostic**: `write_to` accepts `impl Write`, meaning the encoder can write to a `Vec<u8>` (in-memory), a `File` (disk), or a `TcpStream` (network) with the same code path.
- **Empty bodies are valid**: A block with an empty body (content_len = 0) round-trips correctly. This is used by the END sentinel and could be used by ANNOTATION blocks with no payload.
- **Large bodies**: Tested with 10KB bodies to exercise multi-byte content_len varints. The encoder enforces a 16 MiB limit per block body.

---

## Error Types

`WireError` is the foundation error type. Every crate in the workspace either uses it directly or wraps it.

```rust
pub enum WireError {
    VarintTooLong,
    UnexpectedEof { offset: usize },
    InvalidMagic { found: u32 },
    UnsupportedVersion { major: u8, minor: u8 },
    ReservedNonZero { offset: usize, value: u8 },
    Io(#[from] std::io::Error),
}
```

Every variant carries diagnostic context:

- `UnexpectedEof` includes the byte offset where the read failed — critical for debugging truncated binary payloads
- `InvalidMagic` includes the found value as a `u32` formatted in hex (`{found:#010X}` → `0x00001234`)
- `UnsupportedVersion` includes both major and minor so the error message shows exactly what was found
- `Io` wraps `std::io::Error` with `#[from]` for automatic `?` conversion in any function returning `Result<T, WireError>`

### Error Propagation

`WireError` propagates upward through the crate stack:

```
WireError → TypeError (bcp-types, wrapped as TypeError::Wire)
WireError → EncodeError (bcp-encoder, wrapped as EncodeError::Wire)
WireError → DecodeError (bcp-decoder, wrapped as DecodeError::Wire and DecodeError::InvalidHeader)
```

All wrapping uses `#[from]` for transparent `?` operator conversion.

---

## Module Map

```
src/
├── lib.rs          → #![warn(clippy::pedantic)], pub mod + re-exports
├── varint.rs       → encode_varint, decode_varint (14 tests)
├── header.rs       → LcpHeader, HeaderFlags, constants (8 tests)
├── block_frame.rs  → BlockFrame, BlockFlags, block_type module (8 tests)
└── error.rs        → WireError enum (thiserror derived)
```

## Build & Test

```bash
cargo build -p bcp-wire
cargo test -p bcp-wire
cargo clippy -p bcp-wire -- -W clippy::pedantic
cargo doc -p bcp-wire --no-deps
```
