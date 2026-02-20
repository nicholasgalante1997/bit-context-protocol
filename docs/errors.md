# Error Catalog

> All error types across the workspace, organized by crate.

## bcp-wire: `WireError`

Low-level binary format errors. Used directly by `bcp-wire` and wrapped by downstream crates.

| Variant | Trigger | Context |
|---------|---------|---------|
| `VarintTooLong` | Varint exceeds 10 bytes without termination | Malformed or corrupted varint data |
| `UnexpectedEof { offset }` | Input ends before a complete read | Truncated payload or buffer too short |
| `InvalidMagic { found }` | First 4 bytes are not `BCP\0` | Not a BCP file, or wrong byte order |
| `UnsupportedVersion { major, minor }` | Major version is not 1 | Future version or corrupted header |
| `ReservedNonZero { offset, value }` | Reserved byte at offset is not 0x00 | Corrupted header or incompatible producer |
| `Io(io::Error)` | Underlying I/O failure | File read/write errors |

---

## bcp-types: `TypeError`

Block body deserialization errors. Raised when TLV field parsing encounters structural problems.

| Variant | Trigger | Context |
|---------|---------|---------|
| `MissingRequiredField` | Mandatory field absent in block body | Required TLV field not found after scanning all fields |
| `UnknownFieldWireType` | Wire type value outside 0-2 | Corrupted body or incompatible producer |
| `InvalidEnumValue` | Enum byte out of defined range | Unknown role, status, format hint, etc. |
| `Wire(WireError)` | Varint/framing error during field parsing | Transparent delegation to bcp-wire |

---

## bcp-encoder: `CompressionError`

Zstd compression/decompression errors. Raised when the codec encounters an issue or safety limits are exceeded.

| Variant | Trigger | Context |
|---------|---------|---------|
| `CompressFailed(String)` | Zstd encoder returned an error | Rare — typically indicates invalid compression level |
| `DecompressFailed(String)` | Zstd decoder cannot parse input | Invalid frame, truncated data, or non-zstd input |
| `DecompressionBomb { actual, limit }` | Decompressed size exceeds safety limit | Crafted input designed to decompress to excessive size |

## bcp-encoder: `EncodeError`

Payload construction errors. Raised during the `encode()` call.

| Variant | Trigger | Context |
|---------|---------|---------|
| `EmptyPayload` | `encode()` called with no blocks added | Builder has zero pending blocks |
| `BlockTooLarge { size, limit }` | Single block body exceeds 16 MiB | Extremely large content field |
| `NoBlockTarget { method }` | Modifier called with no preceding block | `with_summary()`, `with_priority()`, `with_compression()`, or `with_content_addressing()` called before any `.add_*()` |
| `MissingContentStore` | Content addressing enabled without a store | `with_content_addressing()` or `auto_dedup()` called, but `set_content_store()` was not |
| `Compression(CompressionError)` | Zstd compression/decompression failure | Transparent delegation |
| `Wire(WireError)` | Wire-level serialization failure | Header or frame write error |
| `Io(io::Error)` | I/O failure during write | Writer-backed serialization |

---

## bcp-decoder: `DecodeError`

Payload parsing errors. Raised by both sync and streaming decoders.

| Variant | Trigger | Context |
|---------|---------|---------|
| `InvalidHeader(WireError)` | Bad magic, version, or reserved byte | First 8 bytes don't form a valid header |
| `BlockTooLarge { size, offset }` | Block body exceeds limit at given offset | Corrupted content_len or oversized block |
| `MissingField { block_type, field_name, field_id }` | Required TLV field absent in block body | Known block type missing a mandatory field |
| `InvalidUtf8 { block_type, field_name }` | String field contains invalid UTF-8 | Corrupted or binary data in string field |
| `MissingEndSentinel` | Payload does not end with END block | Truncated payload or missing 0xFF terminator |
| `TrailingData { extra_bytes }` | Extra bytes after END sentinel | Possible corruption or buggy encoder |
| `DecompressFailed(String)` | Zstd decompression failed | Invalid zstd frame, truncated compressed data |
| `DecompressionBomb { actual, limit }` | Decompressed output exceeds safety limit | 16 MiB per block, 256 MiB per payload |
| `UnresolvedReference { hash }` | BLAKE3 hash not found in content store | Content was encoded with a different store |
| `MissingContentStore` | `IS_REFERENCE` block but no store provided | Use `decode_with_store()` instead of `decode()` |
| `Type(TypeError)` | Body deserialization failure | Delegated to bcp-types |
| `Wire(WireError)` | Frame-level read failure | Delegated to bcp-wire |
| `Io(io::Error)` | Async I/O failure | Streaming decoder read errors |

---

## bcp-driver: `DriverError`

Rendering errors. Raised when the driver cannot produce valid output from decoded blocks.

| Variant | Trigger | Context |
|---------|---------|---------|
| `EmptyInput` | No renderable blocks remain after filtering | All blocks were Annotation/End, or `include_types` excluded everything |
| `UnsupportedBlockType { block_type }` | Block type cannot be rendered (reserved) | Future block types not yet supported by the renderer |
| `InvalidContent { block_index }` | Block body contains invalid UTF-8 | Binary content passed to a text renderer |

---

## Error Propagation

```
WireError ──┬──▶ TypeError ──▶ DecodeError
            │                      ▲
            └──▶ EncodeError       │
                      ▲            │
CompressionError ─────┘            │
                                   │
            io::Error ─────────────┘

DecodeError ──▶ Vec<Block> ──▶ DriverError
```

All errors use `#[from]` for automatic `?` conversion. Wire errors propagate transparently through the stack. `CompressionError` propagates into `EncodeError`. Driver errors are independent — they don't wrap upstream errors because the driver operates on already-decoded blocks.
