# Block Types

> Quick reference for all LCP block type IDs. See [Block Type Definitions](block-type-definitions.md) for full field layouts and Rust structs.

## Summary

| ID | Type | Rust Struct | Description |
|----|------|-------------|-------------|
| `0x01` | **CODE** | `CodeBlock` | Source code with language and path |
| `0x02` | **CONVERSATION** | `ConversationBlock` | Chat turn with role (system/user/assistant/tool) |
| `0x03` | **FILE_TREE** | `FileTreeBlock` | Directory structure with nested entries |
| `0x04` | **TOOL_RESULT** | `ToolResultBlock` | Tool/MCP output with status |
| `0x05` | **DOCUMENT** | `DocumentBlock` | Prose content (markdown/plain/html) |
| `0x06` | **STRUCTURED_DATA** | `StructuredDataBlock` | Tables, JSON, YAML, TOML, CSV |
| `0x07` | **DIFF** | `DiffBlock` | Code changes with hunks |
| `0x08` | **ANNOTATION** | `AnnotationBlock` | Metadata overlay (priority/summary/tag) |
| `0x09` | **EMBEDDING_REF** | `EmbeddingRefBlock` | Vector reference |
| `0x0A` | **IMAGE** | `ImageBlock` | Image data or URI |
| `0xFE` | **EXTENSION** | `ExtensionBlock` | User-defined block (namespace + type_name) |
| `0xFF` | **END** | — | Stream sentinel (empty body) |

## Wire Convention

Block bodies use a TLV (Tag-Length-Value) field encoding:

```
field_id (varint) | wire_type (varint) | payload
```

| Wire Type | ID | Payload Format |
|-----------|----|----------------|
| Varint | 0 | Single varint value |
| Bytes | 1 | Varint length prefix + raw bytes |
| Nested | 2 | Varint length prefix + nested TLV fields |

Unknown field IDs are skipped by decoders for forward compatibility.

## Shared Enums

| Enum | Used By | Values |
|------|---------|--------|
| `Lang` | CODE | Rust, TypeScript, JavaScript, Python, Go, Java, C, Cpp, Ruby, Shell, SQL, HTML, CSS, JSON, YAML, TOML, Markdown, Unknown, Other(u8) |
| `Role` | CONVERSATION | System, User, Assistant, Tool |
| `Status` | TOOL_RESULT | Ok, Error, Timeout |
| `Priority` | ANNOTATION | Critical, High, Normal, Low, Background |
| `FormatHint` | DOCUMENT | Markdown, Plain, Html |
| `DataFormat` | STRUCTURED_DATA | Json, Yaml, Toml, Csv |
| `AnnotationKind` | ANNOTATION | Priority, Summary, Tag |
| `MediaType` | IMAGE | Png, Jpeg, Gif, Svg, Webp |
