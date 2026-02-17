# Wire Format Primitives

<span class="badge badge-green">SPEC_01</span> <span class="badge badge-mauve">bcp-wire</span>

> The lowest-level building blocks of the LCP binary format. Every other crate depends on these primitives.

## Overview

The `bcp-wire` crate implements three primitives: **varint encoding** (LEB128), a fixed **file header**, and a **block frame** envelope. The design follows Protocol Buffers conventions: little-endian byte order, unsigned LEB128 for variable-length integers, and length-prefixed payloads.

```
crates/bcp-wire/
├── Cargo.toml
└── src/
    ├── lib.rs            # Crate root, re-exports
    ├── varint.rs         # LEB128 encode/decode
    ├── header.rs         # 8-byte file header
    ├── block_frame.rs    # Block envelope (type, flags, length, body)
    └── error.rs          # WireError
```

**Dependencies**: `thiserror = "2"` (only dependency).

---

## Varint Encoding (LEB128)

Unsigned LEB128 encodes integers using 7 data bits per byte, with the MSB as a continuation flag. Identical to Protocol Buffers varint format.

### Wire Format

Each byte: `[C|D6|D5|D4|D3|D2|D1|D0]` where `C` = continuation bit, `D0-D6` = data bits.

| Value | Encoded | Bytes |
|-------|---------|-------|
| `0` | `[0x00]` | 1 |
| `127` | `[0x7F]` | 1 |
| `128` | `[0x80, 0x01]` | 2 |
| `300` | `[0xAC, 0x02]` | 2 |
| `16384` | `[0x80, 0x80, 0x01]` | 3 |
| `u32::MAX` | 5 bytes | 5 |
| `u64::MAX` | 10 bytes | 10 |

### API

```rust
// Encode a u64 into buf. Returns bytes written (1-10).
// Panics if buf is shorter than the required length.
// A 10-byte buffer is always sufficient.
pub fn encode_varint(value: u64, buf: &mut [u8]) -> usize;

// Decode from a byte slice. Returns (value, bytes_consumed).
// Errors: VarintTooLong (>10 bytes), UnexpectedEof (truncated input).
pub fn decode_varint(buf: &[u8]) -> Result<(u64, usize), WireError>;
```

### Algorithms

**Encode**: Extract lowest 7 bits. If remaining value > 0, set MSB (continuation). Right-shift by 7. Repeat until value is 0.

**Decode**: Read byte, extract lower 7 bits, shift into result at current offset. If MSB set, continue. If MSB clear, return. Error after 10 bytes.

### Implementation Notes

- The decoder handles trailing bytes correctly: it only consumes the varint portion and returns the byte count, so callers can advance their cursor
- Empty input returns `UnexpectedEof { offset: 0 }`
- 11+ continuation bytes returns `VarintTooLong`

---

## File Header

Every LCP payload begins with a fixed 8-byte header.

### Wire Layout

```
Offset  Size     Description
──────  ───────  ──────────────────────────────────
0x00    4 bytes  Magic number: "LCP\0" (0x4C, 0x43, 0x50, 0x00)
0x04    1 byte   Version major (current: 1)
0x05    1 byte   Version minor (current: 0)
0x06    1 byte   Flags bitfield
0x07    1 byte   Reserved (MUST be 0x00)
```

### HeaderFlags

| Bit | Name | Description |
|-----|------|-------------|
| 0 | `COMPRESSED` | Whole-payload zstd compression |
| 1 | `HAS_INDEX` | Index trailer appended after END block |
| 2-7 | Reserved | MUST be 0 |

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
    pub fn new(flags: HeaderFlags) -> Self;           // Current version, given flags
    pub fn write_to(&self, buf: &mut [u8]) -> Result<(), WireError>;
    pub fn read_from(buf: &[u8]) -> Result<Self, WireError>;
}
```

### Validation Order

`read_from` validates in this order for the most useful error messages:

1. Buffer length >= 8 bytes (`UnexpectedEof`)
2. Magic number matches `LCP\0` (`InvalidMagic`)
3. Major version is 1 (`UnsupportedVersion`)
4. Reserved byte is 0x00 (`ReservedNonZero`)

### Implementation Notes

- Magic is compared as raw bytes (`buf[0..4] != LCP_MAGIC`), not as a `u32`, to avoid endianness concerns
- The `u32::from_le_bytes` conversion is only used in the `InvalidMagic` error for readable hex display
- `HeaderFlags` implements `Default` (all bits zero)

---

## Block Frame

The block frame is the envelope wrapping every block's body. Blocks are concatenated sequentially after the header.

### Wire Layout

```
┌──────────────────────────────────────────────────┐
│ block_type   (varint, 1-2 bytes typically)       │
│ block_flags  (uint8, 1 byte)                     │
│ content_len  (varint, 1-5 bytes typically)       │
│ body         [content_len bytes]                  │
└──────────────────────────────────────────────────┘
```

### BlockFlags

| Bit | Name | Description |
|-----|------|-------------|
| 0 | `HAS_SUMMARY` | Summary sub-block appended after body |
| 1 | `COMPRESSED` | Body is zstd compressed (per-block) |
| 2 | `IS_REFERENCE` | Body is a BLAKE3 hash, not inline data |
| 3-7 | Reserved | Must be 0 |

### Block Type IDs

All known block types are defined as constants in `block_frame::block_type`:

| ID | Constant | Semantic |
|----|----------|----------|
| `0x01` | `CODE` | Source code |
| `0x02` | `CONVERSATION` | Chat turn |
| `0x03` | `FILE_TREE` | Directory structure |
| `0x04` | `TOOL_RESULT` | Tool/MCP output |
| `0x05` | `DOCUMENT` | Prose/markdown |
| `0x06` | `STRUCTURED_DATA` | JSON/YAML/TOML/CSV |
| `0x07` | `DIFF` | Code changes |
| `0x08` | `ANNOTATION` | Metadata overlay |
| `0x09` | `EMBEDDING_REF` | Vector reference |
| `0x0A` | `IMAGE` | Image data |
| `0xFE` | `EXTENSION` | User-defined |
| `0xFF` | `END` | Stream sentinel |

### API

```rust
pub struct BlockFlags(u8);
impl BlockFlags {
    pub const NONE: Self;
    pub const HAS_SUMMARY: Self;
    pub const COMPRESSED: Self;
    pub const IS_REFERENCE: Self;
    pub fn from_raw(raw: u8) -> Self;
    pub fn raw(self) -> u8;
    pub fn has_summary(self) -> bool;
    pub fn is_compressed(self) -> bool;
    pub fn is_reference(self) -> bool;
}

pub struct BlockFrame {
    pub block_type: u8,
    pub flags: BlockFlags,
    pub body: Vec<u8>,
}

impl BlockFrame {
    // Write frame to a writer. Returns total bytes written.
    pub fn write_to(&self, w: &mut impl Write) -> Result<usize, WireError>;

    // Read from byte slice. Returns None for END (0xFF) sentinel.
    // Returns Some((frame, bytes_consumed)) for normal blocks.
    pub fn read_from(buf: &[u8]) -> Result<Option<(Self, usize)>, WireError>;
}
```

### END Sentinel

The END block (type `0xFF`) signals the end of the block stream. `read_from` returns `Ok(None)` when it encounters an END block. The END block has no flags, length, or body on the wire — only the type varint is read before returning.

### Implementation Notes

- `write_to` accepts `impl Write`, so it works with `Vec<u8>`, files, or any writer
- `read_from` returns `(BlockFrame, usize)` so callers know how many bytes were consumed and can read the next frame from `&buf[consumed..]`
- Multiple frames can be read sequentially from a contiguous buffer by advancing the slice offset

---

## Error Types

All wire-level errors are in `WireError`:

```rust
pub enum WireError {
    VarintTooLong,                              // >10 bytes without termination
    UnexpectedEof { offset: usize },            // Input ended prematurely
    InvalidMagic { found: u32 },                // Wrong magic number
    UnsupportedVersion { major: u8, minor: u8 },// Unknown version
    ReservedNonZero { offset: usize, value: u8 },// Non-zero reserved field
    Io(std::io::Error),                         // Wrapped I/O error
}
```

- Every variant carries enough context for debugging binary payloads (byte offset, found value)
- `Io` uses `#[from]` for automatic conversion from `std::io::Error` via `?`
- Display formatting uses hex for binary values (e.g. `{found:#010X}` produces `0x00001234`)

---

## Verification

```bash
cargo build -p bcp-wire
cargo test -p bcp-wire
cargo clippy -p bcp-wire -- -W clippy::pedantic
```
