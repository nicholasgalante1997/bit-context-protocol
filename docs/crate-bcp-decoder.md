# bcp-decoder

<span class="badge badge-green">Complete</span> <span class="badge badge-blue">Phase 3</span>

> The consumer-facing API. Reads LCP binary payloads and produces typed Rust structs. Provides both synchronous (buffered) and asynchronous (streaming) decode paths. Transparently handles zstd decompression and BLAKE3 content-addressed reference resolution.

## Crate Info

| Field | Value |
|-------|-------|
| Path | `crates/bcp-decoder/` |
| Spec | [SPEC_04](decoder.md), [SPEC_06](spec_06.md), [SPEC_07](spec_07.md) |
| Dependencies | `bcp-wire`, `bcp-types`, `thiserror`, `tokio`, `zstd` |
| Dev Dependencies | `bcp-encoder` (round-trip tests) |

---

## Purpose and Role in the Protocol

The decoder is the inverse of the encoder and the entry point for the consumption side of the LCP pipeline:

```
.lcp binary ──▶ bcp-decoder ──▶ Vec<Block> ──▶ lcp-driver ──▶ model-ready text ──▶ LLM
```

The decoder handles structural parsing (header validation, frame extraction, TLV field decoding, summary extraction) plus two transparency layers:

- **Decompression**: Per-block zstd decompression (when `COMPRESSED` block flag is set) and whole-payload decompression (when `COMPRESSED` header flag is set)
- **Reference resolution**: BLAKE3 hash lookup via a `ContentStore` (when `IS_REFERENCE` block flag is set)

Two decode modes serve different use cases:

- **Synchronous** (`LcpDecoder::decode` / `decode_with_store`): For payloads already in memory. Simple, single-pass, returns a `DecodedPayload` with all blocks.
- **Streaming** (`StreamingDecoder`): For large payloads or network streams. Reads blocks incrementally. Uses `tokio::io::AsyncRead` for async I/O. Proves the RFC's P0 requirement: "The format MUST support streaming / incremental decode without buffering the entire payload."

The decoder is also where the protocol's forward compatibility guarantees are enforced. A v1.0 decoder reading a payload from a v1.1 encoder will:
- Accept unknown block types as `BlockContent::Unknown { type_id, body }`
- Skip unknown TLV field IDs within known block types
- Preserve unknown `Lang` variants as `Lang::Other(u8)`

---

## Synchronous Decoder

Stateless unit struct — all state is local to the decode call.

```rust
pub struct LcpDecoder;

impl LcpDecoder {
    pub fn decode(payload: &[u8]) -> Result<DecodedPayload, DecodeError>;
    pub fn decode_with_store(
        payload: &[u8],
        store: &dyn ContentStore,
    ) -> Result<DecodedPayload, DecodeError>;
}

pub struct DecodedPayload {
    pub header: LcpHeader,
    pub blocks: Vec<Block>,
}
```

### Decode Algorithm

1. **Parse header** (8 bytes): Validates magic (`LCP\0`), version (major must be 1), and reserved byte (must be 0x00).

2. **Whole-payload decompression**: If the header's `COMPRESSED` flag (bit 0) is set, decompress all bytes after the header with zstd (max 256 MiB decompressed).

3. **Iterate block frames**: For each `BlockFrame` in the (possibly decompressed) block stream:

```
                    ┌──────────────┐
                    │  Read Frame  │  BlockFrame::read_from()
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │   Resolve    │  IS_REFERENCE → store.get(hash)
                    │   Reference  │  (32-byte hash → original body)
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │  Decompress  │  COMPRESSED → zstd decompress
                    │   Block      │  (max 16 MiB per block)
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │   Extract    │  HAS_SUMMARY → Summary::decode()
                    │   Summary    │
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │   Decode     │  BlockContent::decode_body()
                    │    Body      │
                    └──────────────┘
```

4. **Validate termination**: END sentinel must be present. No trailing data allowed.

### Round-Trip Guarantee

Both `decode()` and `decode_with_store()` are tested against every encoder feature combination: compression, content addressing, auto-dedup, summaries, and all 11 block types.

```rust
// Without content addressing
let decoded = LcpDecoder::decode(&payload)?;

// With content addressing
let decoded = LcpDecoder::decode_with_store(&payload, store.as_ref())?;
```

---

## Streaming Decoder

The async decoder uses a state machine to yield blocks one at a time.

```rust
pub struct StreamingDecoder<R: AsyncRead + Unpin> {
    reader: R,
    state: StreamState,
    buf: Vec<u8>,
    decompressed_payload: Option<Vec<u8>>,
    decompressed_cursor: usize,
    content_store: Option<Arc<dyn ContentStore>>,
}

pub enum DecoderEvent {
    Header(LcpHeader),
    Block(Block),
}

impl<R: AsyncRead + Unpin> StreamingDecoder<R> {
    pub fn new(reader: R) -> Self;
    pub fn with_content_store(self, store: Arc<dyn ContentStore>) -> Self;
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

- **ReadHeader**: Reads 8 bytes. Validates header. If `COMPRESSED` flag is set, reads all remaining bytes and decompresses into an internal buffer. Transitions to `ReadBlocks`. Yields `DecoderEvent::Header(header)`.
- **ReadBlocks**: Each `next()` call reads one block (from the async reader or from the decompressed buffer). Applies reference resolution and per-block decompression transparently. Yields `DecoderEvent::Block(block)`.
- **Done**: Returns `None` for all subsequent calls.

### Whole-Payload Compression in Streaming Mode

When the header's `COMPRESSED` flag is detected, the streaming decoder buffers the entire remaining stream, decompresses it, and then parses blocks from the decompressed buffer. This is a documented tradeoff: whole-payload compression sacrifices true streaming capability for better compression ratio. Per-block compression preserves streaming.

### Content Store

Attach a content store to resolve `IS_REFERENCE` blocks:

```rust
let store = Arc::new(MemoryContentStore::new());
let mut decoder = StreamingDecoder::new(reader)
    .with_content_store(store);
```

---

## Decompression Module

`decompression.rs` provides zstd decompression with safety limits.

| Item | Description |
|------|-------------|
| `MAX_BLOCK_DECOMPRESSED_SIZE` | 16 MiB — per-block decompression limit |
| `MAX_PAYLOAD_DECOMPRESSED_SIZE` | 256 MiB — whole-payload decompression limit |
| `decompress(data, max_size)` | Zstd decompress with bomb protection |

---

## BlockReader

The internal TLV field deserializer used by every block type's `decode_body()` method.

```rust
pub struct BlockReader<'a> {
    buf: &'a [u8],
    pos: usize,
}
```

Each block type's `decode_body` creates a `BlockReader`, iterates fields with `next_field()`, and matches on `field_id`. Unknown fields are silently skipped for forward compatibility.

---

## Forward Compatibility: Three Layers

### Layer 1: Unknown Block Types
Captured as `BlockContent::Unknown { type_id, body }`. No error raised.

### Layer 2: Unknown Field IDs
Silently skipped via the `_ => {}` match arm. Wire-type-aware skip.

### Layer 3: Unknown Enum Values
`Lang::Other(byte)`, `BlockType::Unknown(id)` — preserved, not rejected.

---

## Validation

| Stage | Check | Error |
|-------|-------|-------|
| Header | Magic = `LCP\0` | `InvalidHeader(InvalidMagic)` |
| Header | Major version = 1 | `InvalidHeader(UnsupportedVersion)` |
| Header | Reserved byte = 0x00 | `InvalidHeader(ReservedNonZero)` |
| Decompression | Valid zstd frame | `DecompressFailed` |
| Decompression | Output <= size limit | `DecompressionBomb` |
| Reference | Store provided | `MissingContentStore` |
| Reference | Hash found in store | `UnresolvedReference` |
| Frame | Body doesn't overflow payload | `Wire(UnexpectedEof)` |
| Body | Required TLV fields present | `Type(MissingRequiredField)` |
| Termination | END sentinel present | `MissingEndSentinel` |
| Termination | No bytes after END | `TrailingData { extra_bytes }` |

---

## Error Types

```rust
pub enum DecodeError {
    InvalidHeader(WireError),
    BlockTooLarge { size: usize, offset: usize },
    MissingField { block_type, field_name, field_id },
    InvalidUtf8 { block_type, field_name },
    MissingEndSentinel,
    TrailingData { extra_bytes: usize },
    DecompressFailed(String),
    DecompressionBomb { actual: usize, limit: usize },
    UnresolvedReference { hash: [u8; 32] },
    MissingContentStore,
    Type(TypeError),
    Wire(WireError),
    Io(std::io::Error),
}
```

---

## Module Map

```
src/
├── lib.rs            → Re-exports LcpDecoder, StreamingDecoder, DecodeError, DecoderEvent
├── decoder.rs        → LcpDecoder (sync), decode_with_store, DecodedPayload (26 tests)
├── streaming.rs      → StreamingDecoder state machine (9 async tests)
├── block_reader.rs   → BlockReader, RawField TLV deserializer (6 tests)
├── decompression.rs  → decompress(), MAX_BLOCK/PAYLOAD_DECOMPRESSED_SIZE (4 tests)
└── error.rs          → DecodeError enum
```

## Build & Test

```bash
cargo build -p bcp-decoder
cargo test -p bcp-decoder
cargo clippy -p bcp-decoder -- -W clippy::pedantic
cargo doc -p bcp-decoder --no-deps
```
