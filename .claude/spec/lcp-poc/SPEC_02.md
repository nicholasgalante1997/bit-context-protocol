# SPEC_02 — Block Type Definitions

**Crate**: `lcp-types`
**Phase**: 1 (Foundation)
**Prerequisites**: SPEC_01 (uses `lcp-wire` varint and block frame)
**Dependencies**: `lcp-wire`

---

## Context

The LCP format defines 11 semantic block types plus an END sentinel. Each block
type carries typed fields encoded with varint field IDs and length-prefixed
values, following a protobuf-like tag-length-value (TLV) pattern. This crate
defines the Rust structs, enums, and field-level serialization for all block
types specified in RFC §4.4.

The `lcp-types` crate is a pure data definition layer — it knows how to
serialize its fields into a block body and deserialize from one, but it does
not own the block frame envelope (that's `lcp-wire`) or the encode/decode
orchestration (that's `lcp-encoder`/`lcp-decoder`).

---

## Requirements

### 1. Block Type Enum

```rust
/// Semantic block type identifiers.
///
/// Each variant maps to the wire byte value specified in RFC §4.4.
/// Unknown values are captured by `Unknown(u8)` for forward compatibility.
///
/// ┌──────┬──────────────────┬──────────────────────────────────┐
/// │ Wire │ Variant          │ Description                      │
/// ├──────┼──────────────────┼──────────────────────────────────┤
/// │ 0x01 │ Code             │ Source code with language/path   │
/// │ 0x02 │ Conversation     │ Chat turn with role              │
/// │ 0x03 │ FileTree         │ Directory structure              │
/// │ 0x04 │ ToolResult       │ Tool/MCP output                  │
/// │ 0x05 │ Document         │ Prose/markdown content           │
/// │ 0x06 │ StructuredData   │ JSON/YAML/TOML/CSV data          │
/// │ 0x07 │ Diff             │ Code changes with hunks          │
/// │ 0x08 │ Annotation       │ Metadata overlay                 │
/// │ 0x09 │ EmbeddingRef     │ Vector reference                 │
/// │ 0x0A │ Image            │ Image reference or embed         │
/// │ 0xFE │ Extension        │ User-defined block               │
/// │ 0xFF │ End              │ End-of-stream sentinel           │
/// └──────┴──────────────────┴──────────────────────────────────┘
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockType {
    Code,
    Conversation,
    FileTree,
    ToolResult,
    Document,
    StructuredData,
    Diff,
    Annotation,
    EmbeddingRef,
    Image,
    Extension,
    End,
    Unknown(u8),
}

impl BlockType {
    pub fn wire_id(&self) -> u8 { /* ... */ }
    pub fn from_wire_id(id: u8) -> Self { /* ... */ }
}
```

### 2. Shared Enumerations

These enums are used as fields within block types. Each uses a single-byte
wire encoding for compactness.

```rust
/// Programming language identifiers for CODE blocks.
///
/// Wire: single byte. Extensible — unknown values preserved as `Other(u8)`.
///
/// ┌──────┬────────────┐
/// │ Wire │ Language   │
/// ├──────┼────────────┤
/// │ 0x01 │ Rust       │
/// │ 0x02 │ TypeScript │
/// │ 0x03 │ JavaScript │
/// │ 0x04 │ Python     │
/// │ 0x05 │ Go         │
/// │ 0x06 │ Java       │
/// │ 0x07 │ C          │
/// │ 0x08 │ Cpp        │
/// │ 0x09 │ Ruby       │
/// │ 0x0A │ Shell      │
/// │ 0x0B │ SQL        │
/// │ 0x0C │ HTML       │
/// │ 0x0D │ CSS        │
/// │ 0x0E │ JSON       │
/// │ 0x0F │ YAML       │
/// │ 0x10 │ TOML       │
/// │ 0x11 │ Markdown   │
/// │ 0xFF │ Unknown    │
/// └──────┴────────────┘
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Java,
    C,
    Cpp,
    Ruby,
    Shell,
    Sql,
    Html,
    Css,
    Json,
    Yaml,
    Toml,
    Markdown,
    Unknown,
    Other(u8),
}

/// Conversation role for CONVERSATION blocks.
///
/// Wire: single byte.
/// ┌──────┬───────────┐
/// │ 0x01 │ System    │
/// │ 0x02 │ User      │
/// │ 0x03 │ Assistant │
/// │ 0x04 │ Tool      │
/// └──────┴───────────┘
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Tool execution status for TOOL_RESULT blocks.
///
/// Wire: single byte.
/// ┌──────┬─────────┐
/// │ 0x01 │ Ok      │
/// │ 0x02 │ Error   │
/// │ 0x03 │ Timeout │
/// └──────┴─────────┘
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    Error,
    Timeout,
}

/// Content priority for ANNOTATION blocks.
///
/// Wire: single byte.
/// ┌──────┬────────────┐
/// │ 0x01 │ Critical   │
/// │ 0x02 │ High       │
/// │ 0x03 │ Normal     │
/// │ 0x04 │ Low        │
/// │ 0x05 │ Background │
/// └──────┴────────────┘
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Critical,
    High,
    Normal,
    Low,
    Background,
}

/// Document format hint for DOCUMENT blocks.
///
/// Wire: single byte.
/// ┌──────┬──────────┐
/// │ 0x01 │ Markdown │
/// │ 0x02 │ Plain    │
/// │ 0x03 │ Html     │
/// └──────┴──────────┘
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatHint {
    Markdown,
    Plain,
    Html,
}

/// Data format for STRUCTURED_DATA blocks.
///
/// Wire: single byte.
/// ┌──────┬──────┐
/// │ 0x01 │ Json │
/// │ 0x02 │ Yaml │
/// │ 0x03 │ Toml │
/// │ 0x04 │ Csv  │
/// └──────┴──────┘
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataFormat {
    Json,
    Yaml,
    Toml,
    Csv,
}

/// Annotation kind for ANNOTATION blocks.
///
/// Wire: single byte.
/// ┌──────┬──────────┐
/// │ 0x01 │ Priority │
/// │ 0x02 │ Summary  │
/// │ 0x03 │ Tag      │
/// └──────┴──────────┘
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnotationKind {
    Priority,
    Summary,
    Tag,
}

/// Image media type for IMAGE blocks.
///
/// Wire: single byte.
/// ┌──────┬──────┐
/// │ 0x01 │ Png  │
/// │ 0x02 │ Jpeg │
/// │ 0x03 │ Gif  │
/// │ 0x04 │ Svg  │
/// │ 0x05 │ Webp │
/// └──────┴──────┘
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaType {
    Png,
    Jpeg,
    Gif,
    Svg,
    Webp,
}
```

### 3. Field Encoding Convention

Within a block's body, fields are encoded using a tag-length-value pattern:

```
  field_id (varint) | wire_type (varint) | payload
```

Wire types:
- `0` = varint (single varint value follows)
- `1` = bytes (varint length prefix, then raw bytes)
- `2` = nested (varint length prefix, then nested TLV fields)

```rust
/// Field wire types within a block body.
///
/// ┌──────┬──────────┬────────────────────────────────┐
/// │ Wire │ Type     │ Payload format                 │
/// ├──────┼──────────┼────────────────────────────────┤
/// │ 0    │ Varint   │ Single varint value             │
/// │ 1    │ Bytes    │ Varint length + raw bytes       │
/// │ 2    │ Nested   │ Varint length + nested TLV      │
/// └──────┴──────────┴────────────────────────────────┘
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldWireType {
    Varint = 0,
    Bytes = 1,
    Nested = 2,
}
```

### 4. Block Type Structs

Each block type has a corresponding Rust struct with typed fields and
`encode_body` / `decode_body` methods.

#### CODE Block (0x01)

```rust
/// CODE block — represents a source code file or fragment.
///
/// Field layout within body:
/// ┌──────────┬───────────┬─────────┬────────────────────────────┐
/// │ Field ID │ Wire Type │ Name    │ Description                │
/// ├──────────┼───────────┼─────────┼────────────────────────────┤
/// │ 1        │ Varint    │ lang    │ Language enum byte         │
/// │ 2        │ Bytes     │ path    │ UTF-8 file path            │
/// │ 3        │ Bytes     │ content │ Raw source code bytes      │
/// │ 4        │ Varint    │ line_start │ Start line (optional)   │
/// │ 5        │ Varint    │ line_end   │ End line (optional)     │
/// └──────────┴───────────┴─────────┴────────────────────────────┘
pub struct CodeBlock {
    pub lang: Lang,
    pub path: String,
    pub content: Vec<u8>,
    pub line_range: Option<(u32, u32)>,
}
```

#### CONVERSATION Block (0x02)

```rust
/// CONVERSATION block — represents a single chat turn.
///
/// Field layout within body:
/// ┌──────────┬───────────┬──────────────┬──────────────────────────┐
/// │ Field ID │ Wire Type │ Name         │ Description              │
/// ├──────────┼───────────┼──────────────┼──────────────────────────┤
/// │ 1        │ Varint    │ role         │ Role enum byte           │
/// │ 2        │ Bytes     │ content      │ Message body (UTF-8)     │
/// │ 3        │ Bytes     │ tool_call_id │ Tool call ID (optional)  │
/// └──────────┴───────────┴──────────────┴──────────────────────────┘
pub struct ConversationBlock {
    pub role: Role,
    pub content: Vec<u8>,
    pub tool_call_id: Option<String>,
}
```

#### FILE_TREE Block (0x03)

```rust
/// FILE_TREE block — represents a directory structure.
///
/// Field layout within body:
/// ┌──────────┬───────────┬───────────┬────────────────────────────┐
/// │ Field ID │ Wire Type │ Name      │ Description                │
/// ├──────────┼───────────┼───────────┼────────────────────────────┤
/// │ 1        │ Bytes     │ root_path │ Root directory path        │
/// │ 2        │ Nested    │ entries   │ Repeated FileEntry         │
/// └──────────┴───────────┴───────────┴────────────────────────────┘
///
/// FileEntry nested fields:
/// ┌──────────┬───────────┬──────────┬─────────────────────────────┐
/// │ Field ID │ Wire Type │ Name     │ Description                 │
/// ├──────────┼───────────┼──────────┼─────────────────────────────┤
/// │ 1        │ Bytes     │ name     │ Entry name (not full path)  │
/// │ 2        │ Varint    │ kind     │ 0=file, 1=directory         │
/// │ 3        │ Varint    │ size     │ File size in bytes          │
/// │ 4        │ Nested    │ children │ Repeated FileEntry (dirs)   │
/// └──────────┴───────────┴──────────┴─────────────────────────────┘
pub struct FileTreeBlock {
    pub root_path: String,
    pub entries: Vec<FileEntry>,
}

pub struct FileEntry {
    pub name: String,
    pub kind: FileEntryKind,
    pub size: u64,
    pub children: Vec<FileEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileEntryKind {
    File = 0,
    Directory = 1,
}
```

#### TOOL_RESULT Block (0x04)

```rust
/// TOOL_RESULT block — represents output from a tool or MCP server.
///
/// Field layout within body:
/// ┌──────────┬───────────┬─────────────┬──────────────────────────┐
/// │ Field ID │ Wire Type │ Name        │ Description              │
/// ├──────────┼───────────┼─────────────┼──────────────────────────┤
/// │ 1        │ Bytes     │ tool_name   │ Tool identifier          │
/// │ 2        │ Varint    │ status      │ Status enum byte         │
/// │ 3        │ Bytes     │ content     │ Tool output bytes        │
/// │ 4        │ Bytes     │ schema_hint │ Schema hint (optional)   │
/// └──────────┴───────────┴─────────────┴──────────────────────────┘
pub struct ToolResultBlock {
    pub tool_name: String,
    pub status: Status,
    pub content: Vec<u8>,
    pub schema_hint: Option<String>,
}
```

#### DOCUMENT Block (0x05)

```rust
/// DOCUMENT block — represents prose or documentation content.
///
/// Field layout within body:
/// ┌──────────┬───────────┬─────────────┬──────────────────────────┐
/// │ Field ID │ Wire Type │ Name        │ Description              │
/// ├──────────┼───────────┼─────────────┼──────────────────────────┤
/// │ 1        │ Bytes     │ title       │ Document title           │
/// │ 2        │ Bytes     │ content     │ Document body            │
/// │ 3        │ Varint    │ format_hint │ FormatHint enum byte     │
/// └──────────┴───────────┴─────────────┴──────────────────────────┘
pub struct DocumentBlock {
    pub title: String,
    pub content: Vec<u8>,
    pub format_hint: FormatHint,
}
```

#### STRUCTURED_DATA Block (0x06)

```rust
/// STRUCTURED_DATA block — represents tables, JSON, configs, etc.
///
/// Field layout within body:
/// ┌──────────┬───────────┬─────────┬──────────────────────────────┐
/// │ Field ID │ Wire Type │ Name    │ Description                  │
/// ├──────────┼───────────┼─────────┼──────────────────────────────┤
/// │ 1        │ Varint    │ format  │ DataFormat enum byte         │
/// │ 2        │ Bytes     │ schema  │ Optional schema descriptor   │
/// │ 3        │ Bytes     │ content │ Raw data bytes               │
/// └──────────┴───────────┴─────────┴──────────────────────────────┘
pub struct StructuredDataBlock {
    pub format: DataFormat,
    pub schema: Option<String>,
    pub content: Vec<u8>,
}
```

#### DIFF Block (0x07)

```rust
/// DIFF block — represents code changes.
///
/// Field layout within body:
/// ┌──────────┬───────────┬───────┬───────────────────────────────┐
/// │ Field ID │ Wire Type │ Name  │ Description                   │
/// ├──────────┼───────────┼───────┼───────────────────────────────┤
/// │ 1        │ Bytes     │ path  │ File path                     │
/// │ 2        │ Nested    │ hunks │ Repeated DiffHunk             │
/// └──────────┴───────────┴───────┴───────────────────────────────┘
///
/// DiffHunk nested fields:
/// ┌──────────┬───────────┬───────────┬────────────────────────────┐
/// │ Field ID │ Wire Type │ Name      │ Description                │
/// ├──────────┼───────────┼───────────┼────────────────────────────┤
/// │ 1        │ Varint    │ old_start │ Start line in old file     │
/// │ 2        │ Varint    │ new_start │ Start line in new file     │
/// │ 3        │ Bytes     │ lines     │ Hunk content (unified fmt) │
/// └──────────┴───────────┴───────────┴────────────────────────────┘
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

#### ANNOTATION Block (0x08)

```rust
/// ANNOTATION block — metadata overlay for other blocks.
///
/// Field layout within body:
/// ┌──────────┬───────────┬─────────────────┬──────────────────────┐
/// │ Field ID │ Wire Type │ Name            │ Description          │
/// ├──────────┼───────────┼─────────────────┼──────────────────────┤
/// │ 1        │ Varint    │ target_block_id │ Index of target blk  │
/// │ 2        │ Varint    │ kind            │ AnnotationKind byte  │
/// │ 3        │ Bytes     │ value           │ Annotation payload   │
/// └──────────┴───────────┴─────────────────┴──────────────────────┘
pub struct AnnotationBlock {
    pub target_block_id: u32,
    pub kind: AnnotationKind,
    pub value: Vec<u8>,
}
```

#### EMBEDDING_REF Block (0x09)

```rust
/// EMBEDDING_REF block — vector embedding reference.
///
/// Field layout within body:
/// ┌──────────┬───────────┬─────────────┬─────────────────────────┐
/// │ Field ID │ Wire Type │ Name        │ Description             │
/// ├──────────┼───────────┼─────────────┼─────────────────────────┤
/// │ 1        │ Bytes     │ vector_id   │ Vector store identifier │
/// │ 2        │ Bytes     │ source_hash │ BLAKE3 content hash     │
/// │ 3        │ Bytes     │ model       │ Embedding model name    │
/// └──────────┴───────────┴─────────────┴─────────────────────────┘
pub struct EmbeddingRefBlock {
    pub vector_id: Vec<u8>,
    pub source_hash: Vec<u8>,
    pub model: String,
}
```

#### IMAGE Block (0x0A)

```rust
/// IMAGE block — image content or reference.
///
/// Field layout within body:
/// ┌──────────┬───────────┬────────────┬──────────────────────────┐
/// │ Field ID │ Wire Type │ Name       │ Description              │
/// ├──────────┼───────────┼────────────┼──────────────────────────┤
/// │ 1        │ Varint    │ media_type │ MediaType enum byte      │
/// │ 2        │ Bytes     │ alt_text   │ Alt text description     │
/// │ 3        │ Bytes     │ data       │ Image bytes or URI       │
/// └──────────┴───────────┴────────────┴──────────────────────────┘
pub struct ImageBlock {
    pub media_type: MediaType,
    pub alt_text: String,
    pub data: Vec<u8>,
}
```

#### EXTENSION Block (0xFE)

```rust
/// EXTENSION block — user-defined block type.
///
/// Field layout within body:
/// ┌──────────┬───────────┬───────────┬───────────────────────────┐
/// │ Field ID │ Wire Type │ Name      │ Description               │
/// ├──────────┼───────────┼───────────┼───────────────────────────┤
/// │ 1        │ Bytes     │ namespace │ Namespace (e.g. "myorg")  │
/// │ 2        │ Bytes     │ type_name │ Type within namespace     │
/// │ 3        │ Bytes     │ content   │ Opaque content bytes      │
/// └──────────┴───────────┴───────────┴───────────────────────────┘
pub struct ExtensionBlock {
    pub namespace: String,
    pub type_name: String,
    pub content: Vec<u8>,
}
```

### 5. Summary Sub-Block

When a block's `HAS_SUMMARY` flag is set, the body begins with a summary
sub-block before the main TLV fields:

```rust
/// Summary sub-block — prefixed to the body when block_flags.has_summary() is true.
///
/// Wire layout (within the block body, before main fields):
///   summary_len (varint) | summary_bytes [summary_len]
///
/// The summary is a compact UTF-8 description of the block's content,
/// suitable for token-budget-aware rendering.
pub struct Summary {
    pub text: String,
}
```

### 6. Unified Block Enum

```rust
/// A parsed LCP block — the union of all block types with optional metadata.
pub struct Block {
    pub block_type: BlockType,
    pub flags: BlockFlags,
    pub summary: Option<Summary>,
    pub content: BlockContent,
}

/// The typed content within a block.
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
    Unknown { type_id: u8, body: Vec<u8> },
}
```

---

## File Structure

```
crates/lcp-types/
├── Cargo.toml
└── src/
    ├── lib.rs              # Crate root: pub mod + re-exports
    ├── block_type.rs       # BlockType enum
    ├── enums.rs            # Lang, Role, Status, Priority, etc.
    ├── fields.rs           # FieldWireType, field encoding helpers
    ├── code.rs             # CodeBlock
    ├── conversation.rs     # ConversationBlock
    ├── file_tree.rs        # FileTreeBlock, FileEntry
    ├── tool_result.rs      # ToolResultBlock
    ├── document.rs         # DocumentBlock
    ├── structured_data.rs  # StructuredDataBlock
    ├── diff.rs             # DiffBlock, DiffHunk
    ├── annotation.rs       # AnnotationBlock
    ├── embedding_ref.rs    # EmbeddingRefBlock
    ├── image.rs            # ImageBlock
    ├── extension.rs        # ExtensionBlock
    ├── end.rs              # End sentinel (no fields)
    ├── summary.rs          # Summary sub-block
    ├── block.rs            # Block, BlockContent unified types
    └── error.rs            # TypeError
```

---

## Acceptance Criteria

- [ ] All 11 block types + END have corresponding Rust structs
- [ ] All enum types (Lang, Role, Status, etc.) round-trip through `to_wire_byte` / `from_wire_byte`
- [ ] Unknown enum values are preserved (not lost during deserialization)
- [ ] Each block type's `encode_body` produces bytes that `decode_body` reconstructs exactly
- [ ] Optional fields (line_range, tool_call_id, schema_hint) serialize only when present
- [ ] Summary sub-block is correctly prefixed to body when flag is set
- [ ] `BlockContent::Unknown` captures unrecognized block types without error
- [ ] All structs derive `Debug`, `Clone`, `PartialEq`
- [ ] All public items have rustdoc with field layout tables

---

## Verification

```bash
cargo build -p lcp-types
cargo test -p lcp-types
cargo clippy -p lcp-types -- -W clippy::pedantic
cargo doc -p lcp-types --no-deps
```
