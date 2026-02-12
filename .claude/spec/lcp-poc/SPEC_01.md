# SPEC_01 — Wire Format Primitives

**Crate**: `lcp-wire`
**Phase**: 1 (Foundation)
**Prerequisites**: None
**Dependencies**: None

---

## Context

The LCP wire format is built on three primitives: **varint encoding** (LEB128),
a fixed **file header**, and a **block frame** envelope. Every other layer of the
system depends on these primitives being correct and well-tested. This spec
defines the `lcp-wire` crate which owns these lowest-level building blocks.

The design follows Protocol Buffers conventions for familiarity: little-endian
byte order, unsigned LEB128 for variable-length integers, and length-prefixed
payloads. Unlike protobuf, LCP uses a fixed 8-byte header instead of a
`.proto` schema, and block types are semantic (CODE, CONVERSATION) rather than
structural (message, field).

---

## Requirements

### 1. Varint Encoding (LEB128)

Implement unsigned LEB128 (Little-Endian Base 128) encoding and decoding.
Each byte uses 7 bits for data and 1 continuation bit (MSB). The encoding
is identical to Protocol Buffers' varint format.

```rust
/// Encode a u64 value as an unsigned LEB128 varint into the provided buffer.
///
/// Returns the number of bytes written (1-10 for u64 range).
///
/// Wire format example:
///   Value 0      → [0x00]                    (1 byte)
///   Value 1      → [0x01]                    (1 byte)
///   Value 127    → [0x7F]                    (1 byte)
///   Value 128    → [0x80, 0x01]              (2 bytes)
///   Value 300    → [0xAC, 0x02]              (2 bytes)
///   Value 16383  → [0xFF, 0x7F]              (2 bytes)
///   Value 16384  → [0x80, 0x80, 0x01]        (3 bytes)
pub fn encode_varint(value: u64, buf: &mut [u8]) -> usize {
    // Implementation
}

/// Decode an unsigned LEB128 varint from the provided byte slice.
///
/// Returns (decoded_value, bytes_consumed) on success.
///
/// Errors:
///   - `WireError::VarintTooLong` if more than 10 bytes are consumed
///     without finding a terminating byte (MSB clear).
///   - `WireError::UnexpectedEof` if the slice ends mid-varint.
pub fn decode_varint(buf: &[u8]) -> Result<(u64, usize), WireError> {
    // Implementation
}
```

**Encoding algorithm**:
1. Take the lowest 7 bits of the value.
2. If remaining value > 0, set the MSB (continuation bit) and emit the byte.
3. Right-shift the value by 7 bits and repeat.
4. When remaining value is 0 after taking 7 bits, emit final byte without MSB.

**Decoding algorithm**:
1. Read a byte. Extract the lower 7 bits and shift them into the result
   at the current bit offset.
2. If the MSB is set, increment the byte counter and bit offset by 7, repeat.
3. If the MSB is clear, return the accumulated result and bytes consumed.
4. If more than 10 bytes are consumed, return `VarintTooLong` error.

### 2. File Header

Every LCP payload begins with a fixed 8-byte header.

```rust
/// LCP file header — first 8 bytes of every payload.
///
/// ┌────────┬─────────┬────────────────────────────────────────────┐
/// │ Offset │ Size    │ Description                                │
/// ├────────┼─────────┼────────────────────────────────────────────┤
/// │ 0x00   │ 4 bytes │ Magic number: 0x4C435000 ("LCP\0")         │
/// │ 0x04   │ 1 byte  │ Format version major (current: 1)          │
/// │ 0x05   │ 1 byte  │ Format version minor (current: 0)          │
/// │ 0x06   │ 1 byte  │ Flags bitfield:                            │
/// │        │         │   bit 0 = payload is compressed (zstd)     │
/// │        │         │   bit 1 = has index trailer                │
/// │        │         │   bits 2-7 = reserved (MUST be 0)          │
/// │ 0x07   │ 1 byte  │ Reserved (MUST be 0x00)                    │
/// └────────┴─────────┴────────────────────────────────────────────┘
///
/// Total header size: 8 bytes (constant).
pub struct LcpHeader {
    /// Format version major. Current: 1.
    /// Offset: 0x04, Size: 1 byte.
    pub version_major: u8,

    /// Format version minor. Current: 0.
    /// Offset: 0x05, Size: 1 byte.
    pub version_minor: u8,

    /// Flags bitfield.
    /// Offset: 0x06, Size: 1 byte.
    pub flags: HeaderFlags,
}

/// Header flags bitfield.
///
/// Bit layout:
///   bit 0 = compressed (whole-payload zstd compression)
///   bit 1 = has_index  (index trailer appended after END block)
///   bits 2-7 = reserved (MUST be 0)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderFlags(u8);

impl HeaderFlags {
    pub const COMPRESSED: Self = Self(0b0000_0001);
    pub const HAS_INDEX: Self  = Self(0b0000_0010);

    pub fn is_compressed(self) -> bool { self.0 & 0x01 != 0 }
    pub fn has_index(self) -> bool     { self.0 & 0x02 != 0 }
    pub fn raw(self) -> u8             { self.0 }
}
```

**Constants**:

```rust
/// Magic number: ASCII "LCP\0" = 0x4C, 0x43, 0x50, 0x00.
/// Written as a little-endian u32: 0x0050_434C.
pub const LCP_MAGIC: [u8; 4] = [0x4C, 0x43, 0x50, 0x00];

/// Total header size in bytes (fixed).
pub const HEADER_SIZE: usize = 8;

/// Current format version.
pub const VERSION_MAJOR: u8 = 1;
pub const VERSION_MINOR: u8 = 0;
```

**Serialization**:

```rust
impl LcpHeader {
    /// Write the header to the first 8 bytes of the provided buffer.
    ///
    /// Layout (8 bytes total):
    ///   [0..4] = LCP_MAGIC
    ///   [4]    = version_major
    ///   [5]    = version_minor
    ///   [6]    = flags.raw()
    ///   [7]    = 0x00 (reserved)
    pub fn write_to(&self, buf: &mut [u8]) -> Result<(), WireError> {
        // Implementation
    }

    /// Parse a header from the first 8 bytes of the provided buffer.
    ///
    /// Validates:
    ///   - Magic number matches LCP_MAGIC
    ///   - Version major is supported (currently only 1)
    ///   - Reserved byte is 0x00
    pub fn read_from(buf: &[u8]) -> Result<Self, WireError> {
        // Implementation
    }
}
```

### 3. Block Frame

The block frame is the envelope around every block's body. It consists of
a type tag, flags byte, and length-prefixed body.

```rust
/// Block frame — the envelope wrapping every block's body.
///
/// Wire layout:
///   ┌──────────────────────────────────────────────────┐
///   │ block_type   (varint, 1-2 bytes typically)       │
///   │ block_flags  (uint8, 1 byte)                     │
///   │ content_len  (varint, 1-5 bytes typically)       │
///   │ body         [content_len bytes]                  │
///   └──────────────────────────────────────────────────┘
///
/// block_flags bitfield:
///   bit 0 = has summary sub-block
///   bit 1 = body is compressed (zstd, per-block)
///   bit 2 = body is a reference (BLAKE3 hash, not inline data)
///   bits 3-7 = reserved
pub struct BlockFrame {
    /// The semantic type of this block (CODE=0x01, CONVERSATION=0x02, etc.).
    pub block_type: u8,

    /// Per-block flags.
    pub flags: BlockFlags,

    /// The raw body bytes (after decompression if applicable).
    pub body: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockFlags(u8);

impl BlockFlags {
    pub const HAS_SUMMARY: Self  = Self(0b0000_0001);
    pub const COMPRESSED: Self   = Self(0b0000_0010);
    pub const IS_REFERENCE: Self = Self(0b0000_0100);

    pub fn has_summary(self) -> bool  { self.0 & 0x01 != 0 }
    pub fn is_compressed(self) -> bool { self.0 & 0x02 != 0 }
    pub fn is_reference(self) -> bool  { self.0 & 0x04 != 0 }
    pub fn raw(self) -> u8            { self.0 }
}
```

**Serialization**:

```rust
impl BlockFrame {
    /// Write this block frame to the provided writer.
    ///
    /// Wire bytes written:
    ///   1. block_type as varint
    ///   2. block_flags as single u8
    ///   3. body.len() as varint (content_len)
    ///   4. body bytes
    pub fn write_to(&self, w: &mut impl std::io::Write) -> Result<usize, WireError> {
        // Implementation
    }

    /// Read a block frame from the provided reader.
    ///
    /// Returns None if the block type is END (0xFF), signaling
    /// the end of the block stream.
    pub fn read_from(r: &mut impl std::io::Read) -> Result<Option<Self>, WireError> {
        // Implementation
    }
}
```

### 4. Error Types

```rust
/// Wire-level errors for the `lcp-wire` crate.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    /// Varint encoding exceeded 10 bytes without terminating.
    #[error("varint too long: exceeded 10-byte limit")]
    VarintTooLong,

    /// Input ended before a complete varint or header could be read.
    #[error("unexpected end of input at offset {offset}")]
    UnexpectedEof { offset: usize },

    /// Magic number did not match "LCP\0".
    #[error("invalid magic number: expected 0x4C435000, got {found:#010X}")]
    InvalidMagic { found: u32 },

    /// Unsupported format version.
    #[error("unsupported version {major}.{minor}")]
    UnsupportedVersion { major: u8, minor: u8 },

    /// Reserved field was non-zero.
    #[error("reserved field at offset {offset} was {value:#04X}, expected 0x00")]
    ReservedNonZero { offset: usize, value: u8 },

    /// I/O error during read or write.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

---

## File Structure

```
crates/lcp-wire/
├── Cargo.toml
└── src/
    ├── lib.rs            # Crate root: pub mod + re-exports
    ├── varint.rs          # encode_varint, decode_varint
    ├── header.rs          # LcpHeader, HeaderFlags, LCP_MAGIC
    ├── block_frame.rs     # BlockFrame, BlockFlags
    └── error.rs           # WireError
```

---

## Acceptance Criteria

- [ ] `encode_varint(0, &mut buf)` writes `[0x00]` and returns 1
- [ ] `encode_varint(300, &mut buf)` writes `[0xAC, 0x02]` and returns 2
- [ ] `decode_varint(&[0xAC, 0x02])` returns `(300, 2)`
- [ ] Round-trip: `decode_varint(encode_varint(v))` == `v` for all test values
- [ ] `decode_varint` on 11 continuation bytes returns `VarintTooLong`
- [ ] `LcpHeader::write_to` + `LcpHeader::read_from` round-trips correctly
- [ ] `LcpHeader::read_from` rejects wrong magic number with `InvalidMagic`
- [ ] `LcpHeader::read_from` rejects version 2.0 with `UnsupportedVersion`
- [ ] `BlockFrame::write_to` + `BlockFrame::read_from` round-trips for all block types
- [ ] `BlockFrame::read_from` returns `None` for END block (type 0xFF)
- [ ] All public items have rustdoc with wire layout annotations

---

## Verification

```bash
# Build the crate
cargo build -p lcp-wire

# Run tests
cargo test -p lcp-wire

# Clippy pedantic
cargo clippy -p lcp-wire -- -W clippy::pedantic

# Check documentation builds
cargo doc -p lcp-wire --no-deps
```
