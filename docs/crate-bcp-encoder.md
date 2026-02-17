# bcp-encoder

<span class="badge badge-green">Complete</span> <span class="badge badge-blue">Phase 1</span>

> The producer-facing API. Tools, agents, and MCP servers use this crate to construct LCP binary payloads from structured Rust types.

## Crate Info

| Field | Value |
|-------|-------|
| Path | `crates/bcp-encoder/` |
| Spec | [SPEC_03](encoder.md) |
| Dependencies | `bcp-wire`, `bcp-types`, `thiserror` |
| Dependents | `bcp-decoder` (dev, for round-trip tests) |

---

## Purpose and Role in the Protocol

The encoder sits at the beginning of the LCP data flow. The RFC (Section 5.6) specifies a builder API that allows any tool — an AI coding assistant, an MCP server, a CLI utility — to produce LCP payloads without understanding the binary wire format details. The encoder handles:

1. **Type-safe block construction**: Each `add_*` method accepts strongly-typed Rust arguments (not raw bytes), preventing malformed blocks at compile time
2. **TLV body serialization**: Delegates to `BlockContent::encode_body()` (from `bcp-types`) to convert struct fields into wire-format TLV bytes
3. **Summary attachment**: The `with_summary` modifier prepends a length-prefixed summary to the block body and sets the `HAS_SUMMARY` flag
4. **Priority annotation**: The `with_priority` convenience method appends a separate ANNOTATION block targeting the previous block
5. **Payload assembly**: The `encode()` method writes the 8-byte header, serializes all accumulated blocks as `BlockFrame`s, and appends the END sentinel

The encoder is the first half of the "compression opportunity" described in RFC Section 2.3. Where a typical AI agent context window wastes 30-50% of tokens on structural overhead (markdown fences, JSON envelopes, repeated path prefixes), the encoder packs the same semantic content into a compact binary representation. The decoder and driver later expand this back into a token-efficient text format — but the encoder is where the structural overhead is first eliminated.

---

## Usage

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

This mirrors the pseudocode from RFC Section 12.1 almost exactly. The builder pattern means blocks are accumulated in memory as `PendingBlock` structs, then serialized in a single pass when `encode()` is called.

---

## LcpEncoder

### Internal Structure

```rust
pub struct LcpEncoder {
    blocks: Vec<PendingBlock>,  // Accumulated blocks awaiting serialization
    flags: HeaderFlags,         // Header-level flags (default: NONE)
}

struct PendingBlock {
    block_type: u8,             // Wire type ID (0x01 for CODE, etc.)
    content: BlockContent,      // Typed content from bcp-types
    summary: Option<String>,    // Set by with_summary()
    compress: bool,             // Phase 3 stub, always false
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

Modifiers act on the most recently added block and return `&mut Self` for chaining. Both panic if called with no blocks added.

#### `with_summary(summary: &str)`

Sets `pending.summary = Some(summary.to_string())` on the last block. During `encode()`, this causes:
1. The summary text to be encoded as a length-prefixed UTF-8 string at the **front** of the block body
2. The `HAS_SUMMARY` flag to be set on the `BlockFrame`
3. The TLV field data to follow immediately after the summary bytes

This is the mechanism that enables the token budget engine to show a compact summary instead of full content when context space is limited.

#### `with_priority(priority: Priority)`

This does **not** modify the target block. Instead, it appends a new ANNOTATION block:

```rust
pub fn with_priority(&mut self, priority: Priority) -> &mut Self {
    let target_index = self.blocks.len() - 1;
    self.push_block(
        block_type::ANNOTATION,
        BlockContent::Annotation(AnnotationBlock {
            target_block_id: target_index as u32,
            kind: AnnotationKind::Priority,
            value: vec![priority.to_wire_byte()],
        }),
    );
    self
}
```

The ANNOTATION block references the target by its zero-based index in the block stream. The driver reads these annotations during the budget allocation pass to determine which blocks to include, summarize, or omit.

### encode()

The serialization method. Consumes the accumulated blocks and produces a self-contained binary payload.

**Flow**:

1. **Validate**: Return `EncodeError::EmptyPayload` if no blocks added
2. **Pre-allocate**: `8 (header) + blocks.len() * 256 (estimate) + 3 (END)`
3. **Write header**: `LcpHeader::new(self.flags).write_to(&mut output[..8])`
4. **For each PendingBlock**:
   - Call `BlockContent::encode_body()` to get TLV bytes
   - If summary is set: encode `Summary { text }` first, then append TLV bytes
   - Validate total body size <= 16 MiB
   - Set `HAS_SUMMARY` flag if summary is present
   - Wrap in `BlockFrame { block_type, flags, body }`
   - Call `BlockFrame::write_to(&mut output)` to append the frame
5. **Write END sentinel**: `BlockFrame { type: 0xFF, flags: NONE, body: [] }`
6. **Return**: `Ok(output)`

The output is a complete LCP payload ready for storage on disk, transmission over the network, or consumption by `bcp-decoder`.

---

## BlockWriter

The internal TLV field serializer that `BlockContent::encode_body()` delegates to. Each block type constructs a `BlockWriter`, writes its fields, and calls `finish()` to get the byte buffer.

```rust
pub struct BlockWriter {
    buf: Vec<u8>,
}

impl BlockWriter {
    pub fn new() -> Self;
    pub fn with_capacity(capacity: usize) -> Self;
    pub fn write_varint_field(&mut self, field_id: u64, value: u64);
    pub fn write_bytes_field(&mut self, field_id: u64, value: &[u8]);
    pub fn write_nested_field(&mut self, field_id: u64, nested: &[u8]);
    pub fn finish(self) -> Vec<u8>;
}
```

Wire format per method:
- `write_varint_field`: `field_id (varint) | 0 (varint) | value (varint)`
- `write_bytes_field`: `field_id (varint) | 1 (varint) | length (varint) | bytes`
- `write_nested_field`: `field_id (varint) | 2 (varint) | length (varint) | nested_bytes`

---

## Error Types

```rust
pub enum EncodeError {
    EmptyPayload,                               // No blocks added before encode()
    BlockTooLarge { size: usize, limit: usize }, // Body exceeds 16 MiB (MAX_BLOCK_BODY_SIZE)
    InvalidSummaryTarget,                       // Defined but unused; panics used instead
    Wire(WireError),                            // From bcp-wire (header/frame write failure)
    Io(std::io::Error),                         // I/O failure during write
}
```

---

## Phase 3 Stubs

Two modules exist as placeholders for features that will be activated in later specs:

- **`compression.rs`** (SPEC_06): Will wrap zstd compression with a 256-byte minimum threshold. The `PendingBlock.compress` flag and `BlockFlags::COMPRESSED` / `HeaderFlags::COMPRESSED` are already defined in the wire format.
- **`content_store.rs`** (SPEC_07): Will implement BLAKE3 content addressing. When enabled, the encoder hashes block bodies, stores them in a `ContentStore`, and writes the 32-byte hash as the body with `BlockFlags::IS_REFERENCE` set.

Both are currently comment-only files with no functional code.

---

## Module Map

```
src/
├── lib.rs            → Re-exports LcpEncoder, EncodeError
├── encoder.rs        → LcpEncoder builder, PendingBlock, encode() (18 tests)
├── block_writer.rs   → BlockWriter TLV field serializer
├── compression.rs    → Phase 3 stub (comment only)
├── content_store.rs  → Phase 3 stub (comment only)
└── error.rs          → EncodeError enum
```

## Build & Test

```bash
cargo build -p bcp-encoder
cargo test -p bcp-encoder
cargo clippy -p bcp-encoder -- -W clippy::pedantic
cargo doc -p bcp-encoder --no-deps
```
