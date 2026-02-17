# bcp-encoder

<span class="badge badge-green">Complete</span> <span class="badge badge-blue">Phase 3</span>

> The producer-facing API. Tools, agents, and MCP servers use this crate to construct LCP binary payloads from structured Rust types. Supports per-block zstd compression, whole-payload compression, and BLAKE3 content-addressed deduplication.

## Crate Info

| Field | Value |
|-------|-------|
| Path | `crates/bcp-encoder/` |
| Spec | [SPEC_03](encoder.md), [SPEC_06](spec_06.md), [SPEC_07](spec_07.md) |
| Dependencies | `bcp-wire`, `bcp-types`, `blake3`, `thiserror`, `zstd` |
| Dev Dependencies | `bcp-decoder` (round-trip and cross-cutting tests) |

---

## Purpose and Role in the Protocol

The encoder sits at the beginning of the LCP data flow. The RFC (Section 5.6) specifies a builder API that allows any tool — an AI coding assistant, an MCP server, a CLI utility — to produce LCP payloads without understanding the binary wire format details. The encoder handles:

1. **Type-safe block construction**: Each `add_*` method accepts strongly-typed Rust arguments (not raw bytes), preventing malformed blocks at compile time
2. **TLV body serialization**: Delegates to `BlockContent::encode_body()` (from `bcp-types`) to convert struct fields into wire-format TLV bytes
3. **Summary attachment**: The `with_summary` modifier prepends a length-prefixed summary to the block body and sets the `HAS_SUMMARY` flag
4. **Priority annotation**: The `with_priority` convenience method appends a separate ANNOTATION block targeting the previous block
5. **Zstd compression** (RFC §4.6): Per-block compression for individual large blocks, or whole-payload compression for the entire block stream
6. **BLAKE3 content addressing** (RFC §4.7): Block body deduplication via content-addressed hash references, reducing redundant data across repeated blocks
7. **Payload assembly**: The `encode()` method writes the 8-byte header, processes all blocks through the content-addressing and compression pipeline, and appends the END sentinel

The encoder is the first half of the "compression opportunity" described in RFC Section 2.3. Where a typical AI agent context window wastes 30-50% of tokens on structural overhead (markdown fences, JSON envelopes, repeated path prefixes), the encoder packs the same semantic content into a compact binary representation. With zstd compression, realistic Rust source files achieve >= 20% size reduction; with content addressing, duplicate blocks are reduced to 32 bytes each.

---

## Usage

### Basic encoding

```rust
use bcp_encoder::LcpEncoder;
use bcp_types::enums::{Lang, Role, Status, Priority};

let payload = LcpEncoder::new()
    .add_code(Lang::Rust, "src/main.rs", content.as_bytes())
    .with_summary("Entry point: CLI setup and server startup.")
    .with_priority(Priority::High)
    .add_conversation(Role::User, b"Fix the timeout bug.")
    .add_conversation(Role::Assistant, b"I'll examine the pool config...")
    .add_tool_result("ripgrep", Status::Ok, b"3 matches found.")
    .encode()?;
// payload is a Vec<u8> ready for storage or transmission
```

### With compression

```rust
use bcp_encoder::LcpEncoder;
use bcp_types::enums::Lang;

// Per-block compression (individual large blocks)
let payload = LcpEncoder::new()
    .add_code(Lang::Rust, "lib.rs", large_file.as_bytes())
    .with_compression()   // compress this block if >= 256 bytes
    .encode()?;

// Whole-payload compression (compress everything after header)
let payload = LcpEncoder::new()
    .add_code(Lang::Rust, "a.rs", file_a.as_bytes())
    .add_code(Lang::Rust, "b.rs", file_b.as_bytes())
    .compress_payload()
    .encode()?;
```

### With content addressing

```rust
use std::sync::Arc;
use bcp_encoder::{LcpEncoder, MemoryContentStore};
use bcp_types::enums::Lang;

let store = Arc::new(MemoryContentStore::new());

// Explicit content addressing
let payload = LcpEncoder::new()
    .set_content_store(store.clone())
    .add_code(Lang::Rust, "lib.rs", content.as_bytes())
    .with_content_addressing()  // replace body with 32-byte hash
    .encode()?;

// Auto-dedup (duplicate blocks become references automatically)
let payload = LcpEncoder::new()
    .set_content_store(store.clone())
    .auto_dedup()
    .add_code(Lang::Rust, "a.rs", content.as_bytes())  // inline (first)
    .add_code(Lang::Rust, "a.rs", content.as_bytes())  // reference (dup)
    .encode()?;
```

---

## LcpEncoder

### Internal Structure

```rust
pub struct LcpEncoder {
    blocks: Vec<PendingBlock>,
    flags: HeaderFlags,
    compress_payload: bool,
    compress_all_blocks: bool,
    content_store: Option<Arc<dyn ContentStore>>,
    auto_dedup: bool,
}

struct PendingBlock {
    block_type: u8,
    content: BlockContent,
    summary: Option<String>,
    compress: bool,
    content_address: bool,
}
```

### Block Addition Methods

All 12 methods follow the same pattern: construct a `BlockContent` variant, wrap it in a `PendingBlock`, push it onto the internal list, return `&mut Self` for chaining.

| Method | Block Type | Parameters |
|--------|-----------|------------|
| `add_code` | CODE (0x01) | `lang: Lang`, `path: &str`, `content: &[u8]` |
| `add_code_range` | CODE (0x01) | + `line_start: u32`, `line_end: u32` |
| `add_conversation` | CONVERSATION (0x02) | `role: Role`, `content: &[u8]` |
| `add_conversation_tool` | CONVERSATION (0x02) | + `tool_call_id: &str` |
| `add_file_tree` | FILE_TREE (0x03) | `root: &str`, `entries: Vec<FileEntry>` |
| `add_tool_result` | TOOL_RESULT (0x04) | `name: &str`, `status: Status`, `content: &[u8]` |
| `add_document` | DOCUMENT (0x05) | `title: &str`, `content: &[u8]`, `format_hint: FormatHint` |
| `add_structured_data` | STRUCTURED_DATA (0x06) | `format: DataFormat`, `content: &[u8]` |
| `add_diff` | DIFF (0x07) | `path: &str`, `hunks: Vec<DiffHunk>` |
| `add_annotation` | ANNOTATION (0x08) | `target_block_id: u32`, `kind: AnnotationKind`, `value: &[u8]` |
| `add_image` | IMAGE (0x0A) | `media_type: MediaType`, `alt_text: &str`, `data: &[u8]` |
| `add_extension` | EXTENSION (0xFE) | `namespace: &str`, `type_name: &str`, `content: &[u8]` |

### Modifier Methods

Modifiers act on the most recently added block and return `&mut Self` for chaining.

#### `with_summary(summary: &str)`

Sets `pending.summary = Some(summary.to_string())` on the last block. During `encode()`, this causes:
1. The summary text to be encoded as a length-prefixed UTF-8 string at the **front** of the block body
2. The `HAS_SUMMARY` flag to be set on the `BlockFrame`
3. The TLV field data to follow immediately after the summary bytes

#### `with_priority(priority: Priority)`

Appends a new ANNOTATION block targeting the most recently added block by its zero-based index.

#### `with_compression()`

Marks the last block for per-block zstd compression. During `encode()`, the block body is compressed if it exceeds 256 bytes and compression yields a size reduction. The `COMPRESSED` flag (bit 1) is set on the block frame.

#### `with_content_addressing()`

Marks the last block for content addressing. During `encode()`, the block body is hashed with BLAKE3, stored in the content store, and replaced with the 32-byte hash. The `IS_REFERENCE` flag (bit 2) is set.

### Encoder-Level Methods

| Method | Effect |
|--------|--------|
| `compress_blocks()` | Enable per-block compression for all blocks |
| `compress_payload()` | Enable whole-payload zstd compression |
| `set_content_store(Arc<dyn ContentStore>)` | Configure the BLAKE3 hash store |
| `auto_dedup()` | Auto-detect and content-address duplicate bodies |

### encode()

The serialization method. Processes accumulated blocks through a multi-stage pipeline and produces a self-contained binary payload.

**Encode Pipeline** (per block):

```
                        ┌─────────────┐
                        │  Serialize  │  encode_body() + summary
                        └──────┬──────┘
                               │
                        ┌──────▼──────┐
                        │  Content    │  BLAKE3 hash → store → 32-byte ref
                        │  Address    │  (if requested, sets IS_REFERENCE)
                        └──────┬──────┘
                               │
                        ┌──────▼──────┐
                        │  Per-block  │  zstd compress if >= 256 bytes
                        │  Compress   │  (skipped for refs & whole-payload)
                        └──────┬──────┘
                               │
                        ┌──────▼──────┐
                        │ Write Frame │  BlockFrame::write_to()
                        └─────────────┘
```

**After all blocks + END sentinel:**

If `compress_payload` is set, everything after the 8-byte header is compressed as a single zstd frame. The header's `COMPRESSED` flag (bit 0) is set. If compression yields no savings, the payload is stored uncompressed.

**Key invariants:**
- Content addressing runs before compression (a 32-byte hash is below the 256-byte threshold)
- Whole-payload compression takes precedence over per-block (compressing within a compressed stream wastes bytes)
- No-savings guard: both compression modes silently fall back to uncompressed when zstd doesn't help

---

## Compression Module

`compression.rs` provides zstd compression and decompression utilities.

| Item | Description |
|------|-------------|
| `COMPRESSION_THRESHOLD` | 256 bytes — minimum body size before compression is attempted |
| `compress(data: &[u8]) -> Option<Vec<u8>>` | Returns `Some(compressed)` if smaller, `None` if no savings |
| `decompress(data: &[u8], max_size: usize) -> Result<Vec<u8>, CompressionError>` | Decompression with bomb protection |

Default zstd compression level: 3 (good balance of speed and ratio for code/text).

---

## Content Store

### ContentStore Trait (in `bcp-types`)

```rust
pub trait ContentStore: Send + Sync {
    fn get(&self, hash: &[u8; 32]) -> Option<Vec<u8>>;
    fn put(&self, content: &[u8]) -> [u8; 32];
    fn contains(&self, hash: &[u8; 32]) -> bool;
}
```

### MemoryContentStore (in `bcp-encoder`)

In-memory implementation backed by `RwLock<HashMap<[u8; 32], Vec<u8>>>`. Suitable for PoC and testing. Uses BLAKE3 for hashing.

```rust
let store = MemoryContentStore::new();
let hash = store.put(b"fn main() {}");  // BLAKE3 hash
assert!(store.contains(&hash));
assert_eq!(store.len(), 1);
assert_eq!(store.total_bytes(), 12);
```

---

## BlockWriter

The internal TLV field serializer that `BlockContent::encode_body()` delegates to.

```rust
pub struct BlockWriter {
    buf: Vec<u8>,
}

impl BlockWriter {
    pub fn new() -> Self;
    pub fn write_varint_field(&mut self, field_id: u64, value: u64);
    pub fn write_bytes_field(&mut self, field_id: u64, value: &[u8]);
    pub fn write_nested_field(&mut self, field_id: u64, nested: &[u8]);
    pub fn finish(self) -> Vec<u8>;
}
```

---

## Error Types

```rust
pub enum CompressionError {
    CompressFailed(String),
    DecompressFailed(String),
    DecompressionBomb { actual: usize, limit: usize },
}

pub enum EncodeError {
    EmptyPayload,
    BlockTooLarge { size: usize, limit: usize },
    InvalidSummaryTarget,
    MissingContentStore,
    Compression(CompressionError),
    Wire(WireError),
    Io(std::io::Error),
}
```

---

## Module Map

```
src/
├── lib.rs            → Re-exports LcpEncoder, MemoryContentStore, EncodeError, CompressionError
├── encoder.rs        → LcpEncoder builder, PendingBlock, encode() pipeline (49 tests)
├── block_writer.rs   → BlockWriter TLV field serializer (5 tests)
├── compression.rs    → COMPRESSION_THRESHOLD, compress(), decompress() (7 tests)
├── content_store.rs  → MemoryContentStore (9 tests)
└── error.rs          → CompressionError, EncodeError
```

## Build & Test

```bash
cargo build -p bcp-encoder
cargo test -p bcp-encoder
cargo clippy -p bcp-encoder -- -W clippy::pedantic
cargo doc -p bcp-encoder --no-deps
```
