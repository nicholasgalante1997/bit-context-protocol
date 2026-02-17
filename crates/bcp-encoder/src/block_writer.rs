/// TLV field serializer for block bodies.
///
/// `BlockWriter` accumulates tag-length-value encoded fields into an
/// internal byte buffer. It mirrors the field encoding convention from
/// `bcp_types::fields` but wraps it in a stateful builder that tracks
/// the buffer and provides a clean `finish()` hand-off.
///
/// This is an internal implementation detail of the encoder — it is
/// not part of the public API. Each block type's serialization calls
/// into `BlockWriter` to produce the body bytes that get framed by
/// [`BlockFrame`](bcp_wire::block_frame::BlockFrame).
///
/// Wire format per field:
///
/// ```text
/// ┌─────────────────┬──────────────────┬────────────────────────┐
/// │ field_id (varint)│ wire_type (varint)│ payload (varies)      │
/// ├─────────────────┼──────────────────┼────────────────────────┤
/// │                 │ 0 (Varint)       │ value (varint)         │
/// │                 │ 1 (Bytes)        │ length (varint) + data │
/// │                 │ 2 (Nested)       │ length (varint) + data │
/// └─────────────────┴──────────────────┴────────────────────────┘
/// ```
pub struct BlockWriter {
    buf: Vec<u8>,
}

impl BlockWriter {
    /// Create a new writer with an empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Create a new writer with a pre-allocated buffer capacity.
    ///
    /// Use this when you can estimate the final body size to avoid
    /// intermediate reallocations.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
        }
    }

    /// Write a varint field (wire type 0).
    ///
    /// Encodes: `field_id (varint) | 0 (varint) | value (varint)`
    pub fn write_varint_field(&mut self, field_id: u64, value: u64) {
        bcp_types::fields::encode_varint_field(&mut self.buf, field_id, value);
    }

    /// Write a bytes field (wire type 1).
    ///
    /// Encodes: `field_id (varint) | 1 (varint) | length (varint) | data [length]`
    ///
    /// Strings are encoded as bytes fields with UTF-8 content — there is
    /// no distinct string wire type.
    pub fn write_bytes_field(&mut self, field_id: u64, value: &[u8]) {
        bcp_types::fields::encode_bytes_field(&mut self.buf, field_id, value);
    }

    /// Write a nested field (wire type 2).
    ///
    /// Encodes: `field_id (varint) | 2 (varint) | length (varint) | nested [length]`
    ///
    /// The `nested` bytes are themselves a sequence of TLV-encoded fields,
    /// pre-serialized by the caller. This enables recursive structures like
    /// `FileEntry` children and `DiffHunk` sequences.
    pub fn write_nested_field(&mut self, field_id: u64, nested: &[u8]) {
        bcp_types::fields::encode_nested_field(&mut self.buf, field_id, nested);
    }

    /// Consume the writer and return the accumulated bytes.
    ///
    /// After calling `finish()`, the writer is consumed. The returned
    /// `Vec<u8>` is the complete TLV-encoded body ready to be wrapped
    /// in a `BlockFrame`.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

impl Default for BlockWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_writer_produces_empty_bytes() {
        let writer = BlockWriter::new();
        assert!(writer.finish().is_empty());
    }

    #[test]
    fn single_varint_field() {
        let mut writer = BlockWriter::new();
        writer.write_varint_field(1, 42);
        let bytes = writer.finish();
        assert!(!bytes.is_empty());

        // Verify via bcp-types decode path
        let (header, n) = bcp_types::fields::decode_field_header(&bytes).unwrap();
        assert_eq!(header.field_id, 1);
        assert_eq!(header.wire_type, bcp_types::fields::FieldWireType::Varint);
        let (val, m) = bcp_types::fields::decode_varint_value(&bytes[n..]).unwrap();
        assert_eq!(val, 42);
        assert_eq!(n + m, bytes.len());
    }

    #[test]
    fn single_bytes_field() {
        let mut writer = BlockWriter::new();
        writer.write_bytes_field(2, b"hello");
        let bytes = writer.finish();

        let (header, n) = bcp_types::fields::decode_field_header(&bytes).unwrap();
        assert_eq!(header.field_id, 2);
        assert_eq!(header.wire_type, bcp_types::fields::FieldWireType::Bytes);
        let (data, m) = bcp_types::fields::decode_bytes_value(&bytes[n..]).unwrap();
        assert_eq!(data, b"hello");
        assert_eq!(n + m, bytes.len());
    }

    #[test]
    fn nested_field_roundtrip() {
        let mut inner = BlockWriter::new();
        inner.write_varint_field(1, 99);
        let inner_bytes = inner.finish();

        let mut outer = BlockWriter::new();
        outer.write_nested_field(3, &inner_bytes);
        let bytes = outer.finish();

        let (header, n) = bcp_types::fields::decode_field_header(&bytes).unwrap();
        assert_eq!(header.field_id, 3);
        assert_eq!(header.wire_type, bcp_types::fields::FieldWireType::Nested);
        let (nested, m) = bcp_types::fields::decode_bytes_value(&bytes[n..]).unwrap();
        assert_eq!(n + m, bytes.len());

        // Decode the nested content
        let (inner_header, k) = bcp_types::fields::decode_field_header(nested).unwrap();
        assert_eq!(inner_header.field_id, 1);
        let (val, _) = bcp_types::fields::decode_varint_value(&nested[k..]).unwrap();
        assert_eq!(val, 99);
    }

    #[test]
    fn multiple_fields_sequential() {
        let mut writer = BlockWriter::new();
        writer.write_varint_field(1, 7);
        writer.write_bytes_field(2, b"world");
        writer.write_varint_field(3, 256);
        let bytes = writer.finish();

        // Should be decodable as 3 sequential fields
        let mut cursor = 0;

        let (h, n) = bcp_types::fields::decode_field_header(&bytes[cursor..]).unwrap();
        cursor += n;
        assert_eq!(h.field_id, 1);
        let (v, n) = bcp_types::fields::decode_varint_value(&bytes[cursor..]).unwrap();
        cursor += n;
        assert_eq!(v, 7);

        let (h, n) = bcp_types::fields::decode_field_header(&bytes[cursor..]).unwrap();
        cursor += n;
        assert_eq!(h.field_id, 2);
        let (data, n) = bcp_types::fields::decode_bytes_value(&bytes[cursor..]).unwrap();
        cursor += n;
        assert_eq!(data, b"world");

        let (h, n) = bcp_types::fields::decode_field_header(&bytes[cursor..]).unwrap();
        cursor += n;
        assert_eq!(h.field_id, 3);
        let (v, n) = bcp_types::fields::decode_varint_value(&bytes[cursor..]).unwrap();
        cursor += n;
        assert_eq!(v, 256);

        assert_eq!(cursor, bytes.len());
    }
}
