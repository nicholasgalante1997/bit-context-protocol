# Decoder

<span class="badge badge-green">SPEC_04</span> <span class="badge badge-mauve">bcp-decoder</span>

> Reads binary LCP payloads and produces typed `Block` structs. Provides both synchronous (buffered) and asynchronous (streaming) APIs. Validates structural integrity while remaining permissive with unknown block types.

## Overview

```
crates/bcp-decoder/
├── Cargo.toml
└── src/
    ├── lib.rs            # Crate root: pub use LcpDecoder, StreamingDecoder
    ├── decoder.rs        # LcpDecoder (synchronous)
    ├── streaming.rs      # StreamingDecoder (async, tokio)
    ├── block_reader.rs   # BlockReader TLV field deserializer
    ├── decompression.rs  # Zstd decompression (Phase 3 stub)
    └── error.rs          # DecodeError
```

**Dependencies**: `bcp-wire`, `bcp-types`, `thiserror`, `tokio`
**Dev dependencies**: `bcp-encoder` (round-trip tests)

---

## Synchronous Decoder

Parses a complete in-memory payload. Stateless unit struct.

```rust
pub struct LcpDecoder;

impl LcpDecoder {
    pub fn decode(payload: &[u8]) -> Result<DecodedPayload, DecodeError>;
}

pub struct DecodedPayload {
    pub header: LcpHeader,
    pub blocks: Vec<Block>,  // END sentinel consumed, not included
}
```

**Decoding steps**:

1. Parse and validate 8-byte header (magic, version, reserved)
2. Iterate block frames starting at offset 8
3. For each frame:
   - Read block type, flags, content length
   - If `HAS_SUMMARY` flag set: extract length-prefixed summary from body start
   - Deserialize remaining body via `BlockContent::decode_body()`
   - Unknown block types become `BlockContent::Unknown { type_id, body }`
4. Stop when END sentinel (type `0xFF`) is encountered
5. Verify no trailing data after END
6. Return `DecodedPayload`

---

## Streaming Decoder

Async decoder that yields blocks one at a time without buffering the entire payload. Uses a state machine internally.

```rust
pub struct StreamingDecoder<R: AsyncRead + Unpin> { /* ... */ }

impl<R: AsyncRead + Unpin> StreamingDecoder<R> {
    pub fn new(reader: R) -> Self;
    pub async fn next(&mut self) -> Option<Result<DecoderEvent, DecodeError>>;
}

pub enum DecoderEvent {
    Header(LcpHeader),  // Emitted once, first
    Block(Block),        // Emitted per block
}
```

### State Machine

```
ReadHeader ──▶ ReadBlocks ──▶ Done
              (loop per block)
```

- **ReadHeader**: Reads 8 bytes, validates header, yields `DecoderEvent::Header`, transitions to `ReadBlocks`
- **ReadBlocks**: Reads one block frame per `next()` call. If type is `0xFF`, transitions to `Done`. Otherwise yields `DecoderEvent::Block`
- **Done**: Returns `None` (stream exhausted)

Backpressure is natural: the stream only reads the next block when polled. A reusable `Vec<u8>` buffer is shared across all block reads.

---

## BlockReader

Internal TLV field deserializer. Processes fields in any order and skips unknown field IDs for forward compatibility.

```rust
pub struct BlockReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> BlockReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self;
    pub fn next_field(&mut self) -> Result<Option<RawField<'a>>, DecodeError>;
    pub fn position(&self) -> usize;
    pub fn remaining(&self) -> usize;
}

pub struct RawField<'a> {
    pub field_id: u64,
    pub wire_type: FieldWireType,
    pub data: &'a [u8],  // Raw payload (caller interprets)
}
```

Each block type's `decode_body` uses `BlockReader` to iterate fields, matching on `field_id` and ignoring unknown IDs.

---

## Summary Extraction

When `BlockFlags::HAS_SUMMARY` is set, the body is structured as:

```
[varint] summary_len
[bytes]  summary_text (UTF-8)
[bytes]  remaining body (TLV fields)
```

The decoder calls `Summary::decode(body)` to extract the summary text and byte count, then passes the remaining slice to `BlockContent::decode_body()`.

---

## Forward Compatibility

Three layers of tolerance enable schema evolution:

1. **Unknown block types**: Captured as `BlockContent::Unknown { type_id, body }` — no error
2. **Unknown field IDs**: Silently skipped by `BlockReader` via wire-type-aware `skip_field()`
3. **Unknown enum values**: Preserved by variants like `Lang::Other(u8)` and `BlockType::Unknown(u8)`

---

## Validation

| Check | Stage | Error |
|-------|-------|-------|
| Magic number `LCP\0` | Header | `InvalidHeader(InvalidMagic)` |
| Version major = 1 | Header | `InvalidHeader(UnsupportedVersion)` |
| Reserved byte = 0x00 | Header | `InvalidHeader(ReservedNonZero)` |
| Body length within payload | Frame | `Wire(UnexpectedEof)` |
| Required fields present | Body | `MissingField` |
| String fields are UTF-8 | Body | `InvalidUtf8` |
| END sentinel present | Termination | `MissingEndSentinel` |
| No bytes after END | Termination | `TrailingData` |

---

## Error Types

```rust
pub enum DecodeError {
    InvalidHeader(WireError),
    BlockTooLarge { size: usize, offset: usize },
    MissingField { block_type: &'static str, field_name: &'static str, field_id: u64 },
    InvalidUtf8 { block_type: &'static str, field_name: &'static str },
    MissingEndSentinel,
    TrailingData { extra_bytes: usize },
    Type(TypeError),    // From bcp-types
    Wire(WireError),    // From bcp-wire
    Io(std::io::Error),
}
```

---

## Verification

```bash
cargo build -p bcp-decoder
cargo test -p bcp-decoder
cargo clippy -p bcp-decoder -- -W clippy::pedantic
```
