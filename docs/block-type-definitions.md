# Block Type Definitions

<span class="badge badge-green">SPEC_02</span> <span class="badge badge-mauve">bcp-types</span>

> Pure data definition layer: Rust structs, enums, and TLV field encoding for all 11 block types. Knows how to serialize fields into a block body and deserialize from one, but does not own the frame envelope or encode/decode orchestration.

## Overview

```
crates/bcp-types/
├── Cargo.toml
└── src/
    ├── lib.rs              # Crate root, re-exports
    ├── block_type.rs       # BlockType enum (0x01..0xFF)
    ├── enums.rs            # Lang, Role, Status, Priority, etc.
    ├── fields.rs           # FieldWireType, TLV field helpers
    ├── summary.rs          # Summary sub-block
    ├── block.rs            # Block, BlockContent unified types
    ├── code.rs             # CodeBlock (0x01)
    ├── conversation.rs     # ConversationBlock (0x02)
    ├── file_tree.rs        # FileTreeBlock (0x03)
    ├── tool_result.rs      # ToolResultBlock (0x04)
    ├── document.rs         # DocumentBlock (0x05)
    ├── structured_data.rs  # StructuredDataBlock (0x06)
    ├── diff.rs             # DiffBlock (0x07)
    ├── annotation.rs       # AnnotationBlock (0x08)
    ├── embedding_ref.rs    # EmbeddingRefBlock (0x09)
    ├── image.rs            # ImageBlock (0x0A)
    ├── extension.rs        # ExtensionBlock (0xFE)
    ├── end.rs              # End sentinel (0xFF)
    └── error.rs            # TypeError
```

**Dependencies**: `bcp-wire`, `thiserror`

---

## BlockType Enum

Maps semantic types to wire byte IDs. The `Unknown(u8)` variant preserves unrecognized types for forward compatibility.

```rust
pub enum BlockType {
    Code,           // 0x01
    Conversation,   // 0x02
    FileTree,       // 0x03
    ToolResult,     // 0x04
    Document,       // 0x05
    StructuredData, // 0x06
    Diff,           // 0x07
    Annotation,     // 0x08
    EmbeddingRef,   // 0x09
    Image,          // 0x0A
    Extension,      // 0xFE
    End,            // 0xFF
    Unknown(u8),    // Forward compatibility
}

impl BlockType {
    pub fn wire_id(&self) -> u8;
    pub fn from_wire_id(id: u8) -> Self;
}
```

---

## Shared Enumerations

All enums use single-byte wire encoding. Most are generated via a `wire_enum!` macro. Each implements `to_wire_byte()` / `from_wire_byte()`.

### Lang

| Wire | Language | Wire | Language |
|------|----------|------|----------|
| `0x01` | Rust | `0x0A` | Shell |
| `0x02` | TypeScript | `0x0B` | SQL |
| `0x03` | JavaScript | `0x0C` | HTML |
| `0x04` | Python | `0x0D` | CSS |
| `0x05` | Go | `0x0E` | JSON |
| `0x06` | Java | `0x0F` | YAML |
| `0x07` | C | `0x10` | TOML |
| `0x08` | Cpp | `0x11` | Markdown |
| `0x09` | Ruby | `0xFF` | Unknown |

`Lang` has a special `Other(u8)` variant for forward compatibility (manual impl, not macro-generated).

### Role

| Wire | Role |
|------|------|
| `0x01` | System |
| `0x02` | User |
| `0x03` | Assistant |
| `0x04` | Tool |

### Status

| Wire | Status |
|------|--------|
| `0x01` | Ok |
| `0x02` | Error |
| `0x03` | Timeout |

### Priority

| Wire | Priority |
|------|----------|
| `0x01` | Critical |
| `0x02` | High |
| `0x03` | Normal |
| `0x04` | Low |
| `0x05` | Background |

Implements `PartialOrd` + `Ord` for comparison.

### FormatHint

| Wire | Format |
|------|--------|
| `0x01` | Markdown |
| `0x02` | Plain |
| `0x03` | Html |

### DataFormat

| Wire | Format |
|------|--------|
| `0x01` | Json |
| `0x02` | Yaml |
| `0x03` | Toml |
| `0x04` | Csv |

### AnnotationKind

| Wire | Kind |
|------|------|
| `0x01` | Priority |
| `0x02` | Summary |
| `0x03` | Tag |

### MediaType

| Wire | Type |
|------|------|
| `0x01` | Png |
| `0x02` | Jpeg |
| `0x03` | Gif |
| `0x04` | Svg |
| `0x05` | Webp |

---

## TLV Field Encoding

Within a block body, fields use a tag-length-value pattern:

```
field_id (varint) | wire_type (varint) | payload
```

| Wire Type | ID | Payload |
|-----------|----|---------|
| Varint | 0 | Single varint value |
| Bytes | 1 | Varint length prefix + raw bytes |
| Nested | 2 | Varint length prefix + nested TLV fields |

### Encoding Helpers

```rust
pub fn encode_varint_field(buf: &mut Vec<u8>, field_id: u64, value: u64);
pub fn encode_bytes_field(buf: &mut Vec<u8>, field_id: u64, value: &[u8]);
pub fn encode_nested_field(buf: &mut Vec<u8>, field_id: u64, nested: &[u8]);
```

### Decoding Helpers

```rust
pub fn decode_field_header(buf: &[u8]) -> Result<(u64, FieldWireType, usize), TypeError>;
pub fn decode_varint_value(buf: &[u8]) -> Result<(u64, usize), TypeError>;
pub fn decode_bytes_value(buf: &[u8]) -> Result<(&[u8], usize), TypeError>;
pub fn skip_field(buf: &[u8], wire_type: FieldWireType) -> Result<usize, TypeError>;
```

Unknown field IDs are skipped via `skip_field`, enabling forward compatibility.

---

## Block Type Field Layouts

Each block type implements `encode_body() -> Vec<u8>` and `decode_body(&[u8]) -> Result<Self, TypeError>`.

### CODE (0x01)

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Varint | lang | `Lang` enum |
| 2 | Bytes | path | UTF-8 string |
| 3 | Bytes | content | Raw source bytes |
| 4 | Varint | line_start | `u32` (optional) |
| 5 | Varint | line_end | `u32` (optional) |

```rust
pub struct CodeBlock {
    pub lang: Lang,
    pub path: String,
    pub content: Vec<u8>,
    pub line_range: Option<(u32, u32)>,
}
```

### CONVERSATION (0x02)

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Varint | role | `Role` enum |
| 2 | Bytes | content | UTF-8 message body |
| 3 | Bytes | tool_call_id | UTF-8 string (optional) |

```rust
pub struct ConversationBlock {
    pub role: Role,
    pub content: Vec<u8>,
    pub tool_call_id: Option<String>,
}
```

### FILE_TREE (0x03)

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Bytes | root_path | UTF-8 string |
| 2 | Nested | entries | Repeated `FileEntry` |

**FileEntry** nested fields:

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Bytes | name | UTF-8 string |
| 2 | Varint | kind | 0=File, 1=Directory |
| 3 | Varint | size | `u64` bytes |
| 4 | Nested | children | Recursive `FileEntry` |

```rust
pub struct FileTreeBlock {
    pub root_path: String,
    pub entries: Vec<FileEntry>,
}

pub struct FileEntry {
    pub name: String,
    pub kind: FileEntryKind,  // File = 0, Directory = 1
    pub size: u64,
    pub children: Vec<FileEntry>,
}
```

### TOOL_RESULT (0x04)

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Bytes | tool_name | UTF-8 string |
| 2 | Varint | status | `Status` enum |
| 3 | Bytes | content | Tool output bytes |
| 4 | Bytes | schema_hint | UTF-8 string (optional) |

```rust
pub struct ToolResultBlock {
    pub tool_name: String,
    pub status: Status,
    pub content: Vec<u8>,
    pub schema_hint: Option<String>,
}
```

### DOCUMENT (0x05)

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Bytes | title | UTF-8 string |
| 2 | Bytes | content | Document body |
| 3 | Varint | format_hint | `FormatHint` enum |

```rust
pub struct DocumentBlock {
    pub title: String,
    pub content: Vec<u8>,
    pub format_hint: FormatHint,
}
```

### STRUCTURED_DATA (0x06)

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Varint | format | `DataFormat` enum |
| 2 | Bytes | schema | UTF-8 string (optional) |
| 3 | Bytes | content | Raw data bytes |

```rust
pub struct StructuredDataBlock {
    pub format: DataFormat,
    pub schema: Option<String>,
    pub content: Vec<u8>,
}
```

### DIFF (0x07)

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Bytes | path | UTF-8 file path |
| 2 | Nested | hunks | Repeated `DiffHunk` |

**DiffHunk** nested fields:

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Varint | old_start | `u32` |
| 2 | Varint | new_start | `u32` |
| 3 | Bytes | lines | Unified diff content |

```rust
pub struct DiffBlock {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
}

pub struct DiffHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<u8>,
}
```

### ANNOTATION (0x08)

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Varint | target_block_id | `u32` index |
| 2 | Varint | kind | `AnnotationKind` enum |
| 3 | Bytes | value | Payload bytes |

```rust
pub struct AnnotationBlock {
    pub target_block_id: u32,
    pub kind: AnnotationKind,
    pub value: Vec<u8>,
}
```

### EMBEDDING_REF (0x09)

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Bytes | vector_id | Opaque bytes |
| 2 | Bytes | source_hash | BLAKE3 hash |
| 3 | Bytes | model | UTF-8 model name |

```rust
pub struct EmbeddingRefBlock {
    pub vector_id: Vec<u8>,
    pub source_hash: Vec<u8>,
    pub model: String,
}
```

### IMAGE (0x0A)

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Varint | media_type | `MediaType` enum |
| 2 | Bytes | alt_text | UTF-8 string |
| 3 | Bytes | data | Image bytes or URI |

```rust
pub struct ImageBlock {
    pub media_type: MediaType,
    pub alt_text: String,
    pub data: Vec<u8>,
}
```

### EXTENSION (0xFE)

| Field ID | Wire Type | Name | Type |
|----------|-----------|------|------|
| 1 | Bytes | namespace | UTF-8 string |
| 2 | Bytes | type_name | UTF-8 string |
| 3 | Bytes | content | Opaque bytes |

```rust
pub struct ExtensionBlock {
    pub namespace: String,
    pub type_name: String,
    pub content: Vec<u8>,
}
```

### END (0xFF)

No fields. `encode_body()` returns empty `Vec`. `decode_body()` always succeeds.

---

## Summary Sub-Block

When `BlockFlags::HAS_SUMMARY` is set, the body begins with a length-prefixed summary before TLV fields:

```
[varint] summary_len
[bytes]  summary_text (UTF-8, summary_len bytes)
[bytes]  remaining body (main TLV fields)
```

```rust
pub struct Summary {
    pub text: String,
}

impl Summary {
    pub fn encode(&self) -> Vec<u8>;
    pub fn decode(buf: &[u8]) -> Result<(Self, usize), TypeError>;
}
```

---

## Unified Block Types

```rust
pub struct Block {
    pub block_type: BlockType,
    pub flags: BlockFlags,       // From bcp-wire
    pub summary: Option<Summary>,
    pub content: BlockContent,
}

pub enum BlockContent {
    Code(CodeBlock),
    Conversation(ConversationBlock),
    FileTree(FileTreeBlock),
    ToolResult(ToolResultBlock),
    Document(DocumentBlock),
    StructuredData(StructuredDataBlock),
    Diff(DiffBlock),
    Annotation(AnnotationBlock),
    EmbeddingRef(EmbeddingRefBlock),
    Image(ImageBlock),
    Extension(ExtensionBlock),
    End,
    Unknown { type_id: u8, body: Vec<u8> },  // Forward compat
}

impl BlockContent {
    pub fn encode_body(&self) -> Vec<u8>;
    pub fn decode_body(block_type: &BlockType, body: &[u8]) -> Result<Self, TypeError>;
}
```

---

## Verification

```bash
cargo build -p bcp-types
cargo test -p bcp-types
cargo clippy -p bcp-types -- -W clippy::pedantic
```
