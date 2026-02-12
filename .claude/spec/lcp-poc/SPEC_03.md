# SPEC_03 — Encoder API

**Crate**: `lcp-encoder`
**Phase**: 1 (Foundation)
**Prerequisites**: SPEC_01, SPEC_02
**Dependencies**: `lcp-wire`, `lcp-types`

---

## Context

The encoder is the tool-facing API that allows agents, MCP servers, and other
producers to construct LCP payloads. It uses a builder pattern matching the
RFC §5.6 specification: methods like `add_code`, `add_conversation`, etc. with
chainable `with_summary` and `with_priority` modifiers. The encoder takes
structured Rust types and serializes them into a valid LCP binary payload.

---

## Requirements

### 1. Builder API

```rust
/// LCP encoder — constructs a binary payload from structured blocks.
///
/// Usage follows the builder pattern from RFC §5.6:
///
/// ```rust
/// let payload = LcpEncoder::new()
///     .add_code(Lang::Rust, "src/main.rs", content.as_bytes())
///     .with_summary("Entry point: CLI setup and server startup.")
///     .with_priority(Priority::High)
///     .add_conversation(Role::User, b"Fix the timeout bug.")
///     .add_conversation(Role::Assistant, b"I'll examine the pool config...")
///     .add_tool_result("ripgrep", Status::Ok, b"3 matches found.")
///     .encode()?;
/// ```
///
/// The encoder accumulates blocks in order and serializes them when
/// `.encode()` is called. The output is a complete LCP payload:
/// header + block frames + END sentinel.
pub struct LcpEncoder {
    blocks: Vec<PendingBlock>,
    flags: HeaderFlags,
}
```

### 2. Block Addition Methods

Each method appends a block to the internal list and returns `&mut Self`
for chaining.

```rust
impl LcpEncoder {
    /// Create a new encoder with default settings (version 1.0, no flags).
    pub fn new() -> Self { /* ... */ }

    /// Add a CODE block.
    ///
    /// Arguments:
    ///   - `lang`: Programming language enum
    ///   - `path`: File path (UTF-8 string)
    ///   - `content`: Raw source code bytes
    pub fn add_code(
        &mut self,
        lang: Lang,
        path: &str,
        content: &[u8],
    ) -> &mut Self { /* ... */ }

    /// Add a CODE block with an optional line range.
    pub fn add_code_range(
        &mut self,
        lang: Lang,
        path: &str,
        content: &[u8],
        line_start: u32,
        line_end: u32,
    ) -> &mut Self { /* ... */ }

    /// Add a CONVERSATION block.
    pub fn add_conversation(
        &mut self,
        role: Role,
        content: &[u8],
    ) -> &mut Self { /* ... */ }

    /// Add a CONVERSATION block with a tool call ID.
    pub fn add_conversation_tool(
        &mut self,
        role: Role,
        content: &[u8],
        tool_call_id: &str,
    ) -> &mut Self { /* ... */ }

    /// Add a FILE_TREE block.
    pub fn add_file_tree(
        &mut self,
        root: &str,
        entries: Vec<FileEntry>,
    ) -> &mut Self { /* ... */ }

    /// Add a TOOL_RESULT block.
    pub fn add_tool_result(
        &mut self,
        name: &str,
        status: Status,
        content: &[u8],
    ) -> &mut Self { /* ... */ }

    /// Add a DOCUMENT block.
    pub fn add_document(
        &mut self,
        title: &str,
        content: &[u8],
        format_hint: FormatHint,
    ) -> &mut Self { /* ... */ }

    /// Add a STRUCTURED_DATA block.
    pub fn add_structured_data(
        &mut self,
        format: DataFormat,
        content: &[u8],
    ) -> &mut Self { /* ... */ }

    /// Add a DIFF block.
    pub fn add_diff(
        &mut self,
        path: &str,
        hunks: Vec<DiffHunk>,
    ) -> &mut Self { /* ... */ }

    /// Add an ANNOTATION block.
    pub fn add_annotation(
        &mut self,
        target_block_id: u32,
        kind: AnnotationKind,
        value: &[u8],
    ) -> &mut Self { /* ... */ }

    /// Add an IMAGE block.
    pub fn add_image(
        &mut self,
        media_type: MediaType,
        alt_text: &str,
        data: &[u8],
    ) -> &mut Self { /* ... */ }

    /// Add an EXTENSION block.
    pub fn add_extension(
        &mut self,
        namespace: &str,
        type_name: &str,
        content: &[u8],
    ) -> &mut Self { /* ... */ }
}
```

### 3. Modifier Methods

Modifiers apply to the most recently added block.

```rust
impl LcpEncoder {
    /// Attach a summary to the last added block.
    ///
    /// Sets the HAS_SUMMARY flag on the block and prepends the summary
    /// sub-block to the body during serialization.
    ///
    /// Panics if no blocks have been added yet.
    pub fn with_summary(&mut self, summary: &str) -> &mut Self { /* ... */ }

    /// Attach a priority annotation to the last added block.
    ///
    /// This is a convenience method that appends an ANNOTATION block
    /// targeting the last added block with kind=Priority.
    pub fn with_priority(&mut self, priority: Priority) -> &mut Self { /* ... */ }
}
```

### 4. Encode Method

```rust
impl LcpEncoder {
    /// Serialize all accumulated blocks into a complete LCP payload.
    ///
    /// Output layout:
    ///   [8 bytes]    File header (magic, version, flags, reserved)
    ///   [N bytes]    Block 0 frame (type + flags + len + body)
    ///   [N bytes]    Block 1 frame ...
    ///   ...
    ///   [2-3 bytes]  END sentinel (type=0xFF, flags=0x00, len=0)
    ///
    /// The payload is a self-contained byte sequence ready for storage
    /// or transmission.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> { /* ... */ }
}
```

### 5. Internal Pending Block

```rust
/// Internal representation of a block awaiting serialization.
///
/// Captures the block type, any modifier state (summary, compression),
/// and the typed content needed to serialize the body.
struct PendingBlock {
    block_type: BlockType,
    content: BlockContent,
    summary: Option<String>,
    compress: bool,
}
```

### 6. Block Body Serialization

The encoder must serialize each block's fields into a TLV-encoded body.
The serialization logic lives in a `BlockWriter` that handles the field
encoding convention defined in SPEC_02.

```rust
/// Serializes typed block fields into a TLV-encoded byte body.
///
/// Wire format per field:
///   field_id (varint) | wire_type (varint) | payload
///
/// For Varint fields:
///   field_id | 0 | value (varint)
///
/// For Bytes fields:
///   field_id | 1 | length (varint) | bytes [length]
///
/// For Nested fields:
///   field_id | 2 | length (varint) | nested_fields [length]
struct BlockWriter {
    buf: Vec<u8>,
}

impl BlockWriter {
    fn new() -> Self { /* ... */ }

    /// Write a varint field.
    fn write_varint_field(&mut self, field_id: u64, value: u64) { /* ... */ }

    /// Write a bytes field (strings are bytes with UTF-8 content).
    fn write_bytes_field(&mut self, field_id: u64, value: &[u8]) { /* ... */ }

    /// Write a nested field (serialized sub-fields as bytes).
    fn write_nested_field(&mut self, field_id: u64, nested: &[u8]) { /* ... */ }

    /// Consume and return the accumulated bytes.
    fn finish(self) -> Vec<u8> { /* ... */ }
}
```

### 7. Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    #[error("no blocks have been added to the encoder")]
    EmptyPayload,

    #[error("block body exceeds maximum size ({size} bytes, limit {limit})")]
    BlockTooLarge { size: usize, limit: usize },

    #[error("summary references a block that does not exist")]
    InvalidSummaryTarget,

    #[error(transparent)]
    Wire(#[from] WireError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

---

## File Structure

```
crates/lcp-encoder/
├── Cargo.toml
└── src/
    ├── lib.rs            # Crate root: pub use LcpEncoder
    ├── encoder.rs        # LcpEncoder struct + builder methods
    ├── block_writer.rs   # BlockWriter TLV serialization
    ├── compression.rs    # Zstd compression wrapper (stub in Phase 1)
    ├── content_store.rs  # BLAKE3 content store (stub in Phase 1)
    └── error.rs          # EncodeError
```

---

## Acceptance Criteria

- [ ] `LcpEncoder::new().add_code(...).encode()` produces valid bytes
  starting with `LCP_MAGIC`
- [ ] Builder methods are chainable: `encoder.add_code(...).with_summary(...).add_conversation(...)`
- [ ] `with_summary` sets the `HAS_SUMMARY` flag on the preceding block
- [ ] `with_priority` appends an ANNOTATION block targeting the correct block index
- [ ] Empty encoder (no blocks added) returns `EncodeError::EmptyPayload`
- [ ] Payload ends with an END sentinel block (type=0xFF, flags=0x00, len=0)
- [ ] All 11 block types can be added and encoded without error
- [ ] Encoded payload byte length matches expected calculation (header + sum of block frames + END)
- [ ] Optional fields (line_range, tool_call_id, schema) are omitted from wire when `None`

---

## Verification

```bash
cargo build -p lcp-encoder
cargo test -p lcp-encoder
cargo clippy -p lcp-encoder -- -W clippy::pedantic
cargo doc -p lcp-encoder --no-deps
```
