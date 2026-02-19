# Encoder API

<span class="badge badge-green">SPEC_03</span> <span class="badge badge-mauve">bcp-encoder</span>

> Builder-pattern API for constructing BCP binary payloads. Tools, agents, and MCP servers use this to produce `.bcp` files.

## Overview

```
crates/bcp-encoder/
├── Cargo.toml
└── src/
    ├── lib.rs            # Crate root: pub use BcpEncoder, EncodeError
    ├── encoder.rs        # BcpEncoder builder struct
    ├── block_writer.rs   # BlockWriter TLV field serializer
    ├── compression.rs    # Zstd compression (Phase 3 stub)
    ├── content_store.rs  # BLAKE3 content store (Phase 3 stub)
    └── error.rs          # EncodeError
```

**Dependencies**: `bcp-wire`, `bcp-types`, `thiserror`

---

## Usage

```rust
use bcp_encoder::BcpEncoder;
use bcp_types::{Lang, Role, Status, Priority};

let payload = BcpEncoder::new()
    .add_code(Lang::Rust, "src/main.rs", content.as_bytes())
    .with_summary("Entry point: CLI setup and server startup.")
    .with_priority(Priority::High)
    .add_conversation(Role::User, b"Fix the timeout bug.")
    .add_conversation(Role::Assistant, b"I'll examine the pool config...")
    .add_tool_result("ripgrep", Status::Ok, b"3 matches found.")
    .encode()?;
```

---

## BcpEncoder

### Constructor

```rust
pub fn new() -> Self;  // Empty block list, default flags (v1.0, no compression)
```

### Block Addition Methods

All return `&mut Self` for chaining. Each appends a block to the internal list.

| Method | Block Type | Key Parameters |
|--------|-----------|----------------|
| `add_code` | CODE | `lang`, `path`, `content` |
| `add_code_range` | CODE | + `line_start`, `line_end` |
| `add_conversation` | CONVERSATION | `role`, `content` |
| `add_conversation_tool` | CONVERSATION | + `tool_call_id` |
| `add_file_tree` | FILE_TREE | `root`, `entries: Vec<FileEntry>` |
| `add_tool_result` | TOOL_RESULT | `name`, `status`, `content` |
| `add_document` | DOCUMENT | `title`, `content`, `format_hint` |
| `add_structured_data` | STRUCTURED_DATA | `format`, `content` |
| `add_diff` | DIFF | `path`, `hunks: Vec<DiffHunk>` |
| `add_annotation` | ANNOTATION | `target_block_id`, `kind`, `value` |
| `add_image` | IMAGE | `media_type`, `alt_text`, `data` |
| `add_extension` | EXTENSION | `namespace`, `type_name`, `content` |

### Modifier Methods

Act on the **most recently added block**. Panic if no blocks have been added.

```rust
// Attach a summary to the last block.
// Sets HAS_SUMMARY flag, prepends summary to body during serialization.
pub fn with_summary(&mut self, summary: &str) -> &mut Self;

// Attach a priority annotation to the last block.
// Appends a new ANNOTATION block targeting the previous block's index.
pub fn with_priority(&mut self, priority: Priority) -> &mut Self;
```

**Note**: `with_priority` does **not** modify the target block. It appends a separate ANNOTATION block with `kind=Priority` and `value=[priority.to_wire_byte()]`.

### Encode

```rust
pub fn encode(&self) -> Result<Vec<u8>, EncodeError>;
```

**Output layout**:
```
[8 bytes]    File header (magic, version, flags, reserved)
[N bytes]    Block 0 frame (type + flags + len + body)
[N bytes]    Block 1 frame ...
...
[2-4 bytes]  END sentinel (type=0xFF, flags=0x00, len=0)
```

**Serialization flow**:
1. Validate block list is non-empty
2. Write 8-byte BCP header
3. For each block: serialize body via `BlockContent::encode_body()`, prepend summary if set, wrap in `BlockFrame`, write to output
4. Validate body size <= 16 MiB per block
5. Write END sentinel
6. Return complete `Vec<u8>`

---

## BlockWriter

Internal TLV field serializer used by `BlockContent::encode_body()`.

```rust
pub struct BlockWriter { buf: Vec<u8> }

impl BlockWriter {
    pub fn new() -> Self;
    pub fn with_capacity(capacity: usize) -> Self;

    // field_id | wire_type=0 | varint value
    pub fn write_varint_field(&mut self, field_id: u64, value: u64);

    // field_id | wire_type=1 | length | bytes
    pub fn write_bytes_field(&mut self, field_id: u64, value: &[u8]);

    // field_id | wire_type=2 | length | nested TLV bytes
    pub fn write_nested_field(&mut self, field_id: u64, nested: &[u8]);

    pub fn finish(self) -> Vec<u8>;
}
```

---

## Error Types

```rust
pub enum EncodeError {
    EmptyPayload,                              // No blocks added
    BlockTooLarge { size: usize, limit: usize }, // Body > 16 MiB
    InvalidSummaryTarget,                      // Defined but unused (panics used instead)
    Wire(WireError),                           // From bcp-wire
    Io(std::io::Error),                        // I/O failure
}
```

---

## Phase 3 Stubs

`compression.rs` and `content_store.rs` are comment-only stubs. Wire format flags for compression (`BlockFlags::COMPRESSED`, `HeaderFlags::COMPRESSED`) and content addressing (`BlockFlags::IS_REFERENCE`) are already defined in `bcp-wire` and will be activated in SPEC_06 and SPEC_07.

---

## Verification

```bash
cargo build -p bcp-encoder
cargo test -p bcp-encoder
cargo clippy -p bcp-encoder -- -W clippy::pedantic
```
