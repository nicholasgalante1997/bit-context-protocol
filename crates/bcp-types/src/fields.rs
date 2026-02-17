use bcp_wire::WireError;
use bcp_wire::varint::{decode_varint, encode_varint};

use crate::error::TypeError;

/// Field wire types within a block body.
///
/// Every field in a block body is encoded as a tag-length-value (TLV) triple:
///
/// ```text
///   field_id (varint) │ wire_type (varint) │ payload
/// ```
///
/// The wire type determines how the payload is structured:
///
/// ```text
/// ┌──────┬──────────┬────────────────────────────────┐
/// │ Wire │ Type     │ Payload format                 │
/// ├──────┼──────────┼────────────────────────────────┤
/// │ 0    │ Varint   │ Single varint value             │
/// │ 1    │ Bytes    │ Varint length + raw bytes       │
/// │ 2    │ Nested   │ Varint length + nested TLV      │
/// └──────┴──────────┴────────────────────────────────┘
/// ```
///
/// This is intentionally protobuf-like: readers can skip unknown fields
/// by inspecting the wire type and consuming the correct number of bytes,
/// enabling forward compatibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldWireType {
    Varint = 0,
    Bytes = 1,
    Nested = 2,
}

impl FieldWireType {
    /// Convert a raw varint value to a [`FieldWireType`].
    ///
    /// Returns `Err(TypeError::UnknownFieldWireType)` for values outside 0..=2.
    pub fn from_raw(value: u64) -> Result<Self, TypeError> {
        match value {
            0 => Ok(Self::Varint),
            1 => Ok(Self::Bytes),
            2 => Ok(Self::Nested),
            other => Err(TypeError::UnknownFieldWireType { value: other }),
        }
    }
}

// ── Encoding helpers ──────────────────────────────────────────────────
//
// These functions write a single TLV field into a `Vec<u8>`. Each block
// type's `encode_body` method calls these to build up the body buffer.
// The pattern is always: push field_id varint, push wire_type varint,
// push the payload (which varies by wire type).

/// Maximum varint size in bytes, used for stack-allocated scratch buffers.
const MAX_VARINT_LEN: usize = 10;

/// Append a varint scratch-encode into `buf`.
///
/// Internal helper — encodes `value` as a varint and extends `buf`.
fn push_varint(buf: &mut Vec<u8>, value: u64) {
    let mut scratch = [0u8; MAX_VARINT_LEN];
    let n = encode_varint(value, &mut scratch);
    buf.extend_from_slice(&scratch[..n]);
}

/// Encode a varint field (wire type 0).
///
/// Wire layout:
/// ```text
///   field_id (varint) │ 0 (varint) │ value (varint)
/// ```
pub fn encode_varint_field(buf: &mut Vec<u8>, field_id: u64, value: u64) {
    push_varint(buf, field_id);
    push_varint(buf, 0); // wire type = Varint
    push_varint(buf, value);
}

/// Encode a bytes field (wire type 1).
///
/// Wire layout:
/// ```text
///   field_id (varint) │ 1 (varint) │ length (varint) │ data [length]
/// ```
pub fn encode_bytes_field(buf: &mut Vec<u8>, field_id: u64, data: &[u8]) {
    push_varint(buf, field_id);
    push_varint(buf, 1); // wire type = Bytes
    push_varint(buf, data.len() as u64);
    buf.extend_from_slice(data);
}

/// Encode a nested field (wire type 2).
///
/// Wire layout:
/// ```text
///   field_id (varint) │ 2 (varint) │ length (varint) │ nested_data [length]
/// ```
///
/// The `nested_data` is itself a sequence of TLV fields, pre-encoded
/// by the caller. This enables recursive structures like `FileEntry`
/// children inside a `FileTreeBlock`.
pub fn encode_nested_field(buf: &mut Vec<u8>, field_id: u64, nested_data: &[u8]) {
    push_varint(buf, field_id);
    push_varint(buf, 2); // wire type = Nested
    push_varint(buf, nested_data.len() as u64);
    buf.extend_from_slice(nested_data);
}

// ── Decoding helpers ──────────────────────────────────────────────────
//
// Decoding is a cursor-based walk through the body bytes. Each call to
// `decode_field_header` returns the field ID and wire type, then the
// caller uses `decode_varint_value` or `decode_bytes_value` to read
// the payload. Unknown field IDs are skipped by consuming the right
// number of bytes based on the wire type.

/// A decoded field header: the field ID and its wire type.
///
/// The caller matches on `field_id` to decide which struct field to
/// populate, and uses `wire_type` to know how to read the payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldHeader {
    pub field_id: u64,
    pub wire_type: FieldWireType,
}

/// Decode a field header (field_id + wire_type) from the body.
///
/// Returns `(header, bytes_consumed)`. The caller should advance their
/// cursor by `bytes_consumed` before reading the payload.
pub fn decode_field_header(buf: &[u8]) -> Result<(FieldHeader, usize), TypeError> {
    let mut cursor = 0;

    let (field_id, n) = decode_varint(buf).map_err(WireError::from)?;
    cursor += n;

    let (wire_type_raw, n) = decode_varint(
        buf.get(cursor..)
            .ok_or(WireError::UnexpectedEof { offset: cursor })?,
    )
    .map_err(WireError::from)?;
    cursor += n;

    let wire_type = FieldWireType::from_raw(wire_type_raw)?;

    Ok((
        FieldHeader {
            field_id,
            wire_type,
        },
        cursor,
    ))
}

/// Read a varint payload value from the body.
///
/// Call this after `decode_field_header` returns `FieldWireType::Varint`.
/// Returns `(value, bytes_consumed)`.
pub fn decode_varint_value(buf: &[u8]) -> Result<(u64, usize), TypeError> {
    let (value, n) = decode_varint(buf)?;
    Ok((value, n))
}

/// Read a length-prefixed byte payload from the body.
///
/// Call this after `decode_field_header` returns `FieldWireType::Bytes`
/// or `FieldWireType::Nested`. Returns `(byte_slice, bytes_consumed)`,
/// where `bytes_consumed` includes the length prefix varint.
pub fn decode_bytes_value(buf: &[u8]) -> Result<(&[u8], usize), TypeError> {
    let (len, n) = decode_varint(buf)?;
    let len = len as usize;
    let data = buf
        .get(n..n + len)
        .ok_or(WireError::UnexpectedEof { offset: n })?;
    Ok((data, n + len))
}

/// Skip a field's payload based on its wire type.
///
/// This enables forward compatibility: if the decoder encounters an
/// unknown field ID, it can skip the payload without understanding it.
/// Returns the number of bytes consumed.
pub fn skip_field(buf: &[u8], wire_type: FieldWireType) -> Result<usize, TypeError> {
    match wire_type {
        FieldWireType::Varint => {
            let (_value, n) = decode_varint(buf)?;
            Ok(n)
        }
        FieldWireType::Bytes | FieldWireType::Nested => {
            let (_data, n) = decode_bytes_value(buf)?;
            Ok(n)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_varint_field() {
        let mut buf = Vec::new();
        encode_varint_field(&mut buf, 1, 42);

        let (header, mut cursor) = decode_field_header(&buf).unwrap();
        assert_eq!(header.field_id, 1);
        assert_eq!(header.wire_type, FieldWireType::Varint);

        let (value, n) = decode_varint_value(&buf[cursor..]).unwrap();
        cursor += n;
        assert_eq!(value, 42);
        assert_eq!(cursor, buf.len());
    }

    #[test]
    fn roundtrip_bytes_field() {
        let mut buf = Vec::new();
        encode_bytes_field(&mut buf, 2, b"hello");

        let (header, mut cursor) = decode_field_header(&buf).unwrap();
        assert_eq!(header.field_id, 2);
        assert_eq!(header.wire_type, FieldWireType::Bytes);

        let (data, n) = decode_bytes_value(&buf[cursor..]).unwrap();
        cursor += n;
        assert_eq!(data, b"hello");
        assert_eq!(cursor, buf.len());
    }

    #[test]
    fn roundtrip_nested_field() {
        // Build an inner TLV: a varint field with id=1, value=99
        let mut inner = Vec::new();
        encode_varint_field(&mut inner, 1, 99);

        let mut buf = Vec::new();
        encode_nested_field(&mut buf, 3, &inner);

        let (header, mut cursor) = decode_field_header(&buf).unwrap();
        assert_eq!(header.field_id, 3);
        assert_eq!(header.wire_type, FieldWireType::Nested);

        let (nested_bytes, n) = decode_bytes_value(&buf[cursor..]).unwrap();
        cursor += n;
        assert_eq!(cursor, buf.len());

        // Decode the inner field
        let (inner_header, inner_cursor) = decode_field_header(nested_bytes).unwrap();
        assert_eq!(inner_header.field_id, 1);
        let (value, _) = decode_varint_value(&nested_bytes[inner_cursor..]).unwrap();
        assert_eq!(value, 99);
    }

    #[test]
    fn skip_varint_field() {
        let mut buf = Vec::new();
        encode_varint_field(&mut buf, 1, 12345);

        let (header, cursor) = decode_field_header(&buf).unwrap();
        let skipped = skip_field(&buf[cursor..], header.wire_type).unwrap();
        assert_eq!(cursor + skipped, buf.len());
    }

    #[test]
    fn skip_bytes_field() {
        let mut buf = Vec::new();
        encode_bytes_field(&mut buf, 2, b"skip me");

        let (header, cursor) = decode_field_header(&buf).unwrap();
        let skipped = skip_field(&buf[cursor..], header.wire_type).unwrap();
        assert_eq!(cursor + skipped, buf.len());
    }

    #[test]
    fn multiple_fields_sequential() {
        let mut buf = Vec::new();
        encode_varint_field(&mut buf, 1, 7);
        encode_bytes_field(&mut buf, 2, b"world");
        encode_varint_field(&mut buf, 3, 256);

        let mut cursor = 0;

        // Field 1: varint
        let (h, n) = decode_field_header(&buf[cursor..]).unwrap();
        cursor += n;
        assert_eq!(h.field_id, 1);
        let (v, n) = decode_varint_value(&buf[cursor..]).unwrap();
        cursor += n;
        assert_eq!(v, 7);

        // Field 2: bytes
        let (h, n) = decode_field_header(&buf[cursor..]).unwrap();
        cursor += n;
        assert_eq!(h.field_id, 2);
        let (data, n) = decode_bytes_value(&buf[cursor..]).unwrap();
        cursor += n;
        assert_eq!(data, b"world");

        // Field 3: varint
        let (h, n) = decode_field_header(&buf[cursor..]).unwrap();
        cursor += n;
        assert_eq!(h.field_id, 3);
        let (v, n) = decode_varint_value(&buf[cursor..]).unwrap();
        cursor += n;
        assert_eq!(v, 256);

        assert_eq!(cursor, buf.len());
    }

    #[test]
    fn unknown_wire_type_rejected() {
        let mut buf = Vec::new();
        push_varint(&mut buf, 1); // field_id
        push_varint(&mut buf, 5); // wire_type = invalid

        let result = decode_field_header(&buf);
        assert!(matches!(
            result,
            Err(TypeError::UnknownFieldWireType { value: 5 })
        ));
    }
}
