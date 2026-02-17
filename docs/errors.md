# Error Catalog

> All error types across the workspace, organized by crate.

## bcp-wire: `WireError`

Low-level binary format errors. Used directly by `bcp-wire` and wrapped by downstream crates.

| Variant | Trigger | Context |
|---------|---------|---------|
| `VarintTooLong` | Varint exceeds 10 bytes without termination | Malformed or corrupted varint data |
| `UnexpectedEof { offset }` | Input ends before a complete read | Truncated payload or buffer too short |
| `InvalidMagic { found }` | First 4 bytes are not `LCP\0` | Not an LCP file, or wrong byte order |
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

## bcp-encoder: `EncodeError`

Payload construction errors. Raised during the `encode()` call.

| Variant | Trigger | Context |
|---------|---------|---------|
| `EmptyPayload` | `encode()` called with no blocks added | Builder has zero pending blocks |
| `BlockTooLarge { size, limit }` | Single block body exceeds 16 MiB | Extremely large content field |
| `InvalidSummaryTarget` | Defined but unused in current impl | Panics are used instead |
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
| `Type(TypeError)` | Body deserialization failure | Delegated to bcp-types |
| `Wire(WireError)` | Frame-level read failure | Delegated to bcp-wire |
| `Io(io::Error)` | Async I/O failure | Streaming decoder read errors |

---

## Error Propagation

```
WireError ──┬──▶ TypeError ──▶ DecodeError
            │                      ▲
            └──▶ EncodeError       │
                                   │
            io::Error ─────────────┘
```

All errors use `#[from]` for automatic `?` conversion. Wire errors propagate transparently through the stack.
