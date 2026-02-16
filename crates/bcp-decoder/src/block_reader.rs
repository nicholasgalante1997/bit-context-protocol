use bcp_types::fields::{
  decode_bytes_value, decode_field_header, decode_varint_value, FieldWireType,
};

use crate::error::DecodeError;

/// A raw TLV field before type-specific interpretation.
///
/// Produced by [`BlockReader::next_field`]. The caller matches on
/// `field_id` to decide which struct field to populate, and uses
/// `wire_type` to interpret `data`:
///
///   - `Varint`: `data` contains the raw varint bytes (use
///     [`decode_varint_value`] to extract the `u64`).
///   - `Bytes` / `Nested`: `data` is the length-prefixed payload
///     (the length prefix has already been consumed).
///
/// Unknown field IDs should be silently skipped for forward
/// compatibility.
pub struct RawField<'a> {
  pub field_id: u64,
  pub wire_type: FieldWireType,
  pub data: &'a [u8],
}

/// Cursor-based TLV field reader for block bodies.
///
/// `BlockReader` wraps a byte slice and provides an iterator-like
/// interface for consuming TLV fields one at a time. It delegates
/// to the decode functions in `bcp_types::fields` but adds a
/// stateful cursor so the caller doesn't have to manually track
/// offsets.
///
/// This is an internal implementation detail of the decoder — it is
/// not part of the public API.
///
/// # Usage pattern
///
/// ```text
///   let mut reader = BlockReader::new(body);
///   while let Some(field) = reader.next_field()? {
///       match field.field_id {
///           1 => { /* handle field 1 */ }
///           2 => { /* handle field 2 */ }
///           _ => { /* skip unknown */ }
///       }
///   }
/// ```
pub struct BlockReader<'a> {
  buf: &'a [u8],
  pos: usize,
}

impl<'a> BlockReader<'a> {
  /// Create a new reader over the given body bytes.
  ///
  /// The reader starts at position 0 and advances through the buffer
  /// as fields are consumed via [`next_field`](Self::next_field).
  #[must_use]
  pub fn new(buf: &'a [u8]) -> Self {
    Self { buf, pos: 0 }
  }

  /// Read the next TLV field from the body.
  ///
  /// Returns `Ok(Some(field))` if a field was successfully read, or
  /// `Ok(None)` when the buffer is exhausted. Returns `Err` if the
  /// field header or payload is malformed.
  ///
  /// For `Varint` fields, `data` points to the varint bytes in the
  /// original buffer (the caller should use [`decode_varint_value`]
  /// to extract the value). For `Bytes` and `Nested` fields, `data`
  /// is the raw payload after the length prefix.
  ///
  /// # Errors
  ///
  /// Returns [`DecodeError::Type`] or [`DecodeError::Wire`] if the
  /// field header is malformed or the payload is truncated.
  pub fn next_field(&mut self) -> Result<Option<RawField<'a>>, DecodeError> {
    let remaining = &self.buf[self.pos..];
    if remaining.is_empty() {
      return Ok(None);
    }

    let (header, header_len) = decode_field_header(remaining)?;
    let payload_buf = &remaining[header_len..];

    let (data, payload_consumed) = match header.wire_type {
      FieldWireType::Varint => {
        // For varints, we return the raw varint bytes so the caller
        // can decode the value. We need to know how many bytes it
        // consumed to advance the cursor.
        let (_, n) = decode_varint_value(payload_buf)?;
        (&payload_buf[..n], n)
      }
      FieldWireType::Bytes | FieldWireType::Nested => {
        // For bytes/nested, decode_bytes_value returns the inner
        // slice (after length prefix) and total bytes consumed.
        let (inner, n) = decode_bytes_value(payload_buf)?;
        (inner, n)
      }
    };

    self.pos += header_len + payload_consumed;

    Ok(Some(RawField {
      field_id: header.field_id,
      wire_type: header.wire_type,
      data,
    }))
  }

  /// Return the number of bytes consumed so far.
  #[must_use]
  pub fn position(&self) -> usize {
    self.pos
  }

  /// Return the remaining unread bytes.
  #[must_use]
  pub fn remaining(&self) -> &'a [u8] {
    &self.buf[self.pos..]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use bcp_types::fields::{encode_bytes_field, encode_nested_field, encode_varint_field};

  #[test]
  fn empty_buffer_returns_none() {
    let mut reader = BlockReader::new(&[]);
    assert!(reader.next_field().unwrap().is_none());
  }

  #[test]
  fn reads_varint_field() {
    let mut buf = Vec::new();
    encode_varint_field(&mut buf, 1, 42);

    let mut reader = BlockReader::new(&buf);
    let field = reader.next_field().unwrap().unwrap();
    assert_eq!(field.field_id, 1);
    assert_eq!(field.wire_type, FieldWireType::Varint);

    // Decode the varint value from the raw bytes
    let (value, _) = decode_varint_value(field.data).unwrap();
    assert_eq!(value, 42);

    assert!(reader.next_field().unwrap().is_none());
  }

  #[test]
  fn reads_bytes_field() {
    let mut buf = Vec::new();
    encode_bytes_field(&mut buf, 2, b"hello");

    let mut reader = BlockReader::new(&buf);
    let field = reader.next_field().unwrap().unwrap();
    assert_eq!(field.field_id, 2);
    assert_eq!(field.wire_type, FieldWireType::Bytes);
    assert_eq!(field.data, b"hello");

    assert!(reader.next_field().unwrap().is_none());
  }

  #[test]
  fn reads_nested_field() {
    let mut inner = Vec::new();
    encode_varint_field(&mut inner, 1, 99);

    let mut buf = Vec::new();
    encode_nested_field(&mut buf, 3, &inner);

    let mut reader = BlockReader::new(&buf);
    let field = reader.next_field().unwrap().unwrap();
    assert_eq!(field.field_id, 3);
    assert_eq!(field.wire_type, FieldWireType::Nested);
    assert_eq!(field.data, &inner);
  }

  #[test]
  fn reads_multiple_fields_sequentially() {
    let mut buf = Vec::new();
    encode_varint_field(&mut buf, 1, 7);
    encode_bytes_field(&mut buf, 2, b"world");
    encode_varint_field(&mut buf, 3, 256);

    let mut reader = BlockReader::new(&buf);

    let f1 = reader.next_field().unwrap().unwrap();
    assert_eq!(f1.field_id, 1);

    let f2 = reader.next_field().unwrap().unwrap();
    assert_eq!(f2.field_id, 2);
    assert_eq!(f2.data, b"world");

    let f3 = reader.next_field().unwrap().unwrap();
    assert_eq!(f3.field_id, 3);

    assert!(reader.next_field().unwrap().is_none());
    assert_eq!(reader.position(), buf.len());
  }

  #[test]
  fn position_tracks_correctly() {
    let mut buf = Vec::new();
    encode_varint_field(&mut buf, 1, 42);
    encode_bytes_field(&mut buf, 2, b"hi");

    let mut reader = BlockReader::new(&buf);
    assert_eq!(reader.position(), 0);

    reader.next_field().unwrap();
    let mid = reader.position();
    assert!(mid > 0 && mid < buf.len());

    reader.next_field().unwrap();
    assert_eq!(reader.position(), buf.len());
    assert!(reader.remaining().is_empty());
  }
}
