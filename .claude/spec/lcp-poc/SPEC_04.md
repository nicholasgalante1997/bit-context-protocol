# SPEC_04 — Decoder

**Crate**: `lcp-decoder`
**Phase**: 2 (Decode & Render)
**Prerequisites**: SPEC_01, SPEC_02, SPEC_03
**Dependencies**: `lcp-wire`, `lcp-types`

---

## Context

The decoder is the inverse of the encoder: it reads a binary LCP payload and
produces a sequence of typed `Block` structs. It provides both a synchronous
buffered API (for payloads already in memory) and an asynchronous streaming
API (for reading from files or network streams without buffering the entire
payload). The decoder validates structural integrity but is permissive with
unknown block types (forward compatibility).

---

## Requirements

### 1. Synchronous Decoder

```rust
/// Synchronous LCP decoder — parses a complete in-memory payload.
///
/// Decoding proceeds in three steps:
///   1. Validate and parse the 8-byte file header.
///   2. Iterate block frames: for each frame, parse the block type,
///      flags, and body length, then deserialize the body into the
///      corresponding BlockContent variant.
///   3. Stop when an END sentinel block (type=0xFF) is encountered.
///
/// Unknown block types are captured as `BlockContent::Unknown` and
/// do not cause errors (forward compatibility per RFC §3, P1 Schema
/// Evolution).
pub struct LcpDecoder;

impl LcpDecoder {
    /// Decode a complete LCP payload from a byte slice.
    ///
    /// Returns a `DecodedPayload` containing the header and an ordered
    /// Vec of blocks (excluding the END sentinel).
    pub fn decode(payload: &[u8]) -> Result<DecodedPayload, DecodeError> {
        // Implementation
    }
}

/// The result of decoding an LCP payload.
pub struct DecodedPayload {
    /// The parsed file header.
    pub header: LcpHeader,

    /// Ordered sequence of blocks (END sentinel is consumed, not included).
    pub blocks: Vec<Block>,
}
```

### 2. Block Body Deserialization

The decoder delegates body parsing to a `BlockReader` that handles the
TLV field extraction defined in SPEC_02.

```rust
/// Deserializes a block body from TLV-encoded bytes into typed fields.
///
/// The reader processes fields in any order — field IDs may appear
/// in any sequence (robustness). Unknown field IDs are skipped
/// without error (forward compatibility).
struct BlockReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> BlockReader<'a> {
    fn new(buf: &'a [u8]) -> Self { /* ... */ }

    /// Read the next field. Returns None when the buffer is exhausted.
    fn next_field(&mut self) -> Result<Option<RawField<'a>>, DecodeError> {
        // Implementation
    }
}

/// A raw TLV field before type-specific interpretation.
struct RawField<'a> {
    field_id: u64,
    wire_type: FieldWireType,
    data: &'a [u8],
}
```

Each block type implements a `decode_body` function:

```rust
impl CodeBlock {
    /// Parse a CODE block body from TLV-encoded bytes.
    ///
    /// Required fields: lang (1), path (2), content (3).
    /// Optional fields: line_start (4), line_end (5).
    ///
    /// Errors if required fields are missing.
    pub fn decode_body(body: &[u8]) -> Result<Self, DecodeError> {
        // Implementation
    }
}

// Similar decode_body for all other block types...
```

### 3. Summary Extraction

When a block's `HAS_SUMMARY` flag is set, the body begins with a
length-prefixed summary before the main TLV fields:

```rust
/// Extract the summary sub-block from the beginning of a block body.
///
/// Wire layout when HAS_SUMMARY flag is set:
///   [varint] summary_len
///   [bytes]  summary_text (UTF-8, summary_len bytes)
///   [bytes]  remaining_body (main TLV fields)
///
/// Returns (summary_text, remaining_body_slice).
fn extract_summary(body: &[u8]) -> Result<(String, &[u8]), DecodeError> {
    // Implementation
}
```

### 4. Streaming Decoder (Async)

```rust
use tokio::io::AsyncRead;
use tokio_stream::Stream;

/// Asynchronous streaming decoder — yields blocks one at a time
/// without buffering the entire payload.
///
/// This is the primary API for large payloads or network streams.
/// The decoder reads the header first, then yields blocks as they
/// are fully received.
pub struct StreamingDecoder;

impl StreamingDecoder {
    /// Create a streaming decoder from an async reader.
    ///
    /// The returned Stream yields blocks in wire order. The END
    /// sentinel terminates the stream (it is not yielded).
    ///
    /// Backpressure is handled naturally: the stream only reads
    /// the next block when polled.
    pub fn decode_stream(
        reader: impl AsyncRead + Unpin,
    ) -> impl Stream<Item = Result<Block, DecodeError>> {
        // Implementation
    }

    /// Variant that also returns the parsed header before blocks.
    pub fn decode_stream_with_header(
        reader: impl AsyncRead + Unpin,
    ) -> impl Stream<Item = Result<DecoderEvent, DecodeError>> {
        // Implementation
    }
}

/// Events emitted by the streaming decoder.
pub enum DecoderEvent {
    /// The file header has been parsed.
    Header(LcpHeader),
    /// A block has been fully decoded.
    Block(Block),
}
```

### 5. Validation

The decoder performs the following validation:

```rust
/// Validation checks during decode:
///
/// 1. Header validation:
///    - Magic number matches LCP_MAGIC
///    - Version major is 1 (reject unknown major versions)
///    - Reserved byte is 0x00
///
/// 2. Block frame validation:
///    - content_len does not exceed remaining payload bytes
///    - block_type is within expected range (unknown types are OK)
///    - block_flags reserved bits are 0
///
/// 3. Body validation:
///    - Required fields are present for known block types
///    - String fields are valid UTF-8
///    - Enum values are within defined ranges (unknown preserved)
///
/// 4. Stream termination:
///    - Payload ends with an END sentinel block
///    - Warning (not error) if additional bytes follow the END block
```

### 6. Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("invalid header: {0}")]
    InvalidHeader(#[from] WireError),

    #[error("block body too large: {size} bytes at offset {offset}")]
    BlockTooLarge { size: usize, offset: usize },

    #[error("required field {field_name} (id={field_id}) missing in {block_type} block")]
    MissingField {
        block_type: &'static str,
        field_name: &'static str,
        field_id: u64,
    },

    #[error("invalid UTF-8 in field {field_name} of {block_type} block")]
    InvalidUtf8 {
        block_type: &'static str,
        field_name: &'static str,
    },

    #[error("payload does not end with END sentinel")]
    MissingEndSentinel,

    #[error("unexpected data after END sentinel ({extra_bytes} bytes)")]
    TrailingData { extra_bytes: usize },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

---

## File Structure

```
crates/lcp-decoder/
├── Cargo.toml
└── src/
    ├── lib.rs            # Crate root: pub use LcpDecoder, StreamingDecoder
    ├── decoder.rs        # LcpDecoder (sync)
    ├── streaming.rs      # StreamingDecoder (async)
    ├── block_reader.rs   # BlockReader TLV deserialization
    ├── decompression.rs  # Zstd decompression wrapper (stub in Phase 2)
    └── error.rs          # DecodeError
```

---

## Acceptance Criteria

- [ ] `LcpDecoder::decode` parses payloads produced by `LcpEncoder::encode`
- [ ] Round-trip: encode N blocks → decode → compare produces identical `Vec<Block>`
- [ ] Unknown block types are captured as `BlockContent::Unknown` (not errors)
- [ ] Missing required fields produce `DecodeError::MissingField`
- [ ] Invalid UTF-8 in string fields produces `DecodeError::InvalidUtf8`
- [ ] Missing END sentinel produces `DecodeError::MissingEndSentinel`
- [ ] Trailing data after END produces `DecodeError::TrailingData` (warning, not fatal)
- [ ] Streaming decoder produces identical blocks to sync decoder for same input
- [ ] Streaming decoder correctly handles backpressure (reads only when polled)
- [ ] Summary sub-blocks are correctly extracted when `HAS_SUMMARY` flag is set
- [ ] Optional fields that are absent in wire data result in `None` in structs

---

## Verification

```bash
cargo build -p lcp-decoder
cargo test -p lcp-decoder
cargo clippy -p lcp-decoder -- -W clippy::pedantic
cargo doc -p lcp-decoder --no-deps

# Round-trip integration test (requires lcp-encoder)
cargo test -p lcp-decoder --test roundtrip
```
