# bcp-decoder

<span class="badge badge-green">Complete</span> <span class="badge badge-blue">Phase 2</span>

> The consumer-facing API. Reads LCP binary payloads and produces typed Rust structs. Provides both synchronous (buffered) and asynchronous (streaming) decode paths.

## Crate Info

| Field | Value |
|-------|-------|
| Path | `crates/bcp-decoder/` |
| Spec | [SPEC_04](decoder.md) |
| Dependencies | `bcp-wire`, `bcp-types`, `thiserror`, `tokio` |
| Dev Dependencies | `bcp-encoder` (round-trip tests) |

---

## Purpose and Role in the Protocol

The decoder is the inverse of the encoder and the entry point for the consumption side of the LCP pipeline. It sits between the binary payload and the driver/renderer that produces model-ready text:

```
.lcp binary ──▶ bcp-decoder ──▶ Vec<Block> ──▶ lcp-driver ──▶ model-ready text ──▶ LLM
```

The RFC (Section 5.1) describes the driver as "not a simple deserializer — it is an opinionated renderer." The decoder is the deserializer. It handles the structural parsing (header validation, frame extraction, TLV field decoding, summary extraction) and produces typed `Block` structs that the driver can then rank, budget, and render.

Two decode modes serve different use cases:

- **Synchronous** (`LcpDecoder::decode`): For payloads already in memory. Simple, single-pass, returns a `DecodedPayload` with all blocks. Used when the payload is small or was loaded from disk.
- **Streaming** (`StreamingDecoder`): For large payloads or network streams. Reads blocks incrementally without buffering the entire payload. Uses `tokio::io::AsyncRead` for async I/O. This is the API that proves the RFC's P0 requirement: "The format MUST support streaming / incremental decode without buffering the entire payload."

The decoder is also where the protocol's forward compatibility guarantees are enforced. A v1.0 decoder reading a payload from a v1.1 encoder will:
- Accept unknown block types as `BlockContent::Unknown { type_id, body }`
- Skip unknown TLV field IDs within known block types
- Preserve unknown `Lang` variants as `Lang::Other(u8)`

This means the protocol can evolve without breaking existing decoders — new block types and fields can be added without a major version bump.

---

## Synchronous Decoder

Stateless unit struct — all state is local to the `decode()` call.

```rust
pub struct LcpDecoder;

impl LcpDecoder {
    pub fn decode(payload: &[u8]) -> Result<DecodedPayload, DecodeError>;
}

pub struct DecodedPayload {
    pub header: LcpHeader,
    pub blocks: Vec<Block>,
}
```

### Decode Algorithm

1. **Parse header** (8 bytes): Validates magic (`LCP\0`), version (major must be 1), and reserved byte (must be 0x00). Errors immediately on invalid headers.

2. **Iterate block frames**: Starting at byte offset 8, calls `BlockFrame::read_from` in a loop. For each frame:
   - If block type is `0xFF` (END): stop iteration
   - Extract block type and flags from the frame
   - If `HAS_SUMMARY` flag is set: call `Summary::decode` on the body to extract the summary and get the remaining bytes
   - Call `BlockContent::decode_body` on the remaining body bytes, passing the block type for dispatch
   - Construct a `Block { block_type, flags, summary, content }` and push to the result list

3. **Validate termination**: The END sentinel must be present (otherwise `MissingEndSentinel`). If bytes remain after the END sentinel, return `TrailingData` with the count.

4. **Return** `DecodedPayload { header, blocks }`

### Round-Trip Guarantee

The sync decoder is tested against every payload the encoder can produce. The dev dependency on `bcp-encoder` enables tests like:

```rust
let payload = LcpEncoder::new()
    .add_code(Lang::Rust, "main.rs", b"fn main() {}")
    .with_summary("Entry point.")
    .encode()?;

let decoded = LcpDecoder::decode(&payload)?;
assert_eq!(decoded.blocks.len(), 1);
// Verify all fields match...
```

This round-trip testing covers all 11 block types, optional fields, summaries, priority annotations, and edge cases like empty bodies and large payloads.

---

## Streaming Decoder

The async decoder uses a state machine to yield blocks one at a time. This is the critical API for the RFC's streaming decode requirement.

```rust
pub struct StreamingDecoder<R: AsyncRead + Unpin> {
    reader: R,
    state: DecoderState,
    buf: Vec<u8>,  // Reused across block reads
}

pub enum DecoderEvent {
    Header(LcpHeader),
    Block(Block),
}

impl<R: AsyncRead + Unpin> StreamingDecoder<R> {
    pub fn new(reader: R) -> Self;
    pub async fn next(&mut self) -> Option<Result<DecoderEvent, DecodeError>>;
}
```

### State Machine

```
ReadHeader ──▶ ReadBlocks ──▶ Done
                  │    ▲
                  └────┘
              (one block per next() call)
```

- **ReadHeader**: Reads exactly 8 bytes via `AsyncRead`. Validates the header. Transitions to `ReadBlocks`. Yields `DecoderEvent::Header(header)`.
- **ReadBlocks**: Each `next()` call reads one block:
  1. Read block_type varint (byte-by-byte via async read)
  2. If `0xFF`: read the remaining END frame bytes, transition to `Done`, return `None`
  3. Read flags byte (1 byte)
  4. Read content_len varint
  5. Read exactly `content_len` bytes into `self.buf`
  6. Extract summary if `HAS_SUMMARY` flag set
  7. Decode body into `BlockContent`
  8. Yield `DecoderEvent::Block(block)`
- **Done**: Returns `None` for all subsequent calls

### Backpressure

The streaming decoder only reads the next block when `next()` is called. This means:
- A consumer processing blocks slowly won't cause unbounded memory growth
- The async reader (file, socket, etc.) is only polled when data is needed
- Large payloads can be processed with constant memory overhead (one block at a time)

### Buffer Reuse

The internal `buf: Vec<u8>` is reused across all block reads. For each block, it's resized to `content_len` (which may grow the allocation but never shrinks it), filled via `read_exact`, and then the contents are decoded. This avoids per-block allocation overhead.

### Varint Reading

The streaming decoder reads varints byte-by-byte (since `AsyncRead` doesn't guarantee buffered access):

```rust
async fn read_varint(&mut self) -> Result<u64, DecodeError> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    loop {
        let byte = self.read_u8().await?;
        result |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 { return Ok(result); }
        shift += 7;
        if shift >= 70 { return Err(/* VarintTooLong */); }
    }
}
```

---

## BlockReader

The internal TLV field deserializer used by every block type's `decode_body()` method.

```rust
pub struct BlockReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

pub struct RawField<'a> {
    pub field_id: u64,
    pub wire_type: FieldWireType,
    pub data: &'a [u8],
}
```

### How Block Types Use BlockReader

Each block type's `decode_body` creates a `BlockReader`, iterates fields with `next_field()`, and matches on `field_id`:

```rust
// Simplified pattern used by every block type
let mut reader = BlockReader::new(body);
let mut lang = None;
let mut path = None;

while let Some(field) = reader.next_field()? {
    match field.field_id {
        1 => lang = Some(decode_varint_from(field.data)?),
        2 => path = Some(String::from_utf8_lossy(field.data)),
        _ => { /* unknown field — silently skip */ }
    }
}
```

The key property: **fields can appear in any order**, and **unknown fields are silently skipped**. This is what makes forward compatibility work at the field level.

---

## Forward Compatibility: Three Layers

The decoder implements the RFC's P1 Schema Evolution requirement through three complementary mechanisms:

### Layer 1: Unknown Block Types

When `BlockContent::decode_body` encounters a `BlockType::Unknown(id)`, it captures the raw body bytes:

```rust
BlockContent::Unknown { type_id: id, body: raw_bytes.to_vec() }
```

No error is raised. The block appears in the decoded output with its full body preserved, so it can be round-tripped or inspected by tooling that understands the type.

### Layer 2: Unknown Field IDs

When `BlockReader::next_field` returns a field with an ID that the block type doesn't recognize, the block's `decode_body` falls into the `_ => {}` match arm and the field is skipped. The skip is wire-type-aware:

- Varint: consume one varint value
- Bytes/Nested: read length varint, skip that many bytes

### Layer 3: Unknown Enum Values

`Lang::from_wire_byte` returns `Lang::Other(byte)` for unrecognized values instead of an error. `BlockType::from_wire_id` returns `BlockType::Unknown(id)`. This means a newer encoder can add `Lang::Zig = 0x12` and an older decoder will preserve it as `Lang::Other(0x12)`.

---

## Validation

The decoder validates at multiple levels:

| Stage | Check | Error |
|-------|-------|-------|
| Header | Magic = `LCP\0` | `InvalidHeader(InvalidMagic)` |
| Header | Major version = 1 | `InvalidHeader(UnsupportedVersion)` |
| Header | Reserved byte = 0x00 | `InvalidHeader(ReservedNonZero)` |
| Frame | Body doesn't overflow payload | `Wire(UnexpectedEof)` |
| Body | Required TLV fields present | `MissingField { block_type, field_name, field_id }` |
| Body | String fields are valid UTF-8 | `InvalidUtf8 { block_type, field_name }` |
| Termination | END sentinel present | `MissingEndSentinel` |
| Termination | No bytes after END | `TrailingData { extra_bytes }` |

### TrailingData: Warning, Not Fatal

Per the spec, `TrailingData` indicates that the payload decoded successfully but extra bytes exist after the END sentinel. This could mean:
- A buggy encoder that wrote extra data
- An index trailer (future feature)
- Corruption

The decoder returns this as an error, but callers can choose to treat it as a warning if the decoded blocks are otherwise valid.

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
    Type(TypeError),
    Wire(WireError),
    Io(std::io::Error),
}
```

All variants use `&'static str` for block type and field names to avoid allocation in error paths. The `field_id` is included so developers debugging binary payloads can correlate errors with the TLV field layout tables in the spec.

---

## Phase 3 Stub

**`decompression.rs`** (SPEC_06): Currently a comment-only file. When implemented, it will:
- Detect `BlockFlags::COMPRESSED` and decompress individual block bodies with zstd
- Detect `HeaderFlags::COMPRESSED` and decompress the entire payload after the header
- Include decompression bomb protection via a `max_size` parameter

---

## Module Map

```
src/
├── lib.rs            → Re-exports LcpDecoder, StreamingDecoder, DecodeError, DecoderEvent
├── decoder.rs        → LcpDecoder (sync), DecodedPayload (47 tests)
├── streaming.rs      → StreamingDecoder state machine (8 async tests)
├── block_reader.rs   → BlockReader, RawField TLV deserializer (6 tests)
├── decompression.rs  → Phase 3 stub (comment only)
└── error.rs          → DecodeError enum
```

## Build & Test

```bash
cargo build -p bcp-decoder
cargo test -p bcp-decoder
cargo clippy -p bcp-decoder -- -W clippy::pedantic
cargo doc -p bcp-decoder --no-deps
```
