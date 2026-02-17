use crate::enums::AnnotationKind;
use crate::error::TypeError;
use crate::fields::{
    decode_bytes_value, decode_field_header, decode_varint_value, encode_bytes_field,
    encode_varint_field, skip_field,
};

/// ANNOTATION block — metadata overlay for other blocks.
///
/// Annotations are secondary blocks that attach metadata to a primary
/// block identified by `target_block_id` (the zero-based index of the
/// target block in the stream). The `kind` field determines how the
/// `value` payload should be interpreted:
///
/// - `Priority`: value is a [`Priority`](crate::enums::Priority) byte
/// - `Summary`: value is UTF-8 text summarizing the target block
/// - `Tag`: value is a UTF-8 label/tag string
///
/// Field layout within body:
///
/// ```text
/// ┌──────────┬───────────┬─────────────────┬──────────────────────┐
/// │ Field ID │ Wire Type │ Name            │ Description          │
/// ├──────────┼───────────┼─────────────────┼──────────────────────┤
/// │ 1        │ Varint    │ target_block_id │ Index of target blk  │
/// │ 2        │ Varint    │ kind            │ AnnotationKind byte  │
/// │ 3        │ Bytes     │ value           │ Annotation payload   │
/// └──────────┴───────────┴─────────────────┴──────────────────────┘
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnnotationBlock {
    pub target_block_id: u32,
    pub kind: AnnotationKind,
    pub value: Vec<u8>,
}

impl AnnotationBlock {
    /// Serialize this block's fields into a TLV-encoded body.
    pub fn encode_body(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_varint_field(&mut buf, 1, u64::from(self.target_block_id));
        encode_varint_field(&mut buf, 2, u64::from(self.kind.to_wire_byte()));
        encode_bytes_field(&mut buf, 3, &self.value);
        buf
    }

    /// Deserialize an ANNOTATION block from a TLV-encoded body.
    pub fn decode_body(mut buf: &[u8]) -> Result<Self, TypeError> {
        let mut target_block_id: Option<u32> = None;
        let mut kind: Option<AnnotationKind> = None;
        let mut value: Option<Vec<u8>> = None;

        while !buf.is_empty() {
            let (header, n) = decode_field_header(buf)?;
            buf = &buf[n..];

            match header.field_id {
                1 => {
                    let (v, n) = decode_varint_value(buf)?;
                    buf = &buf[n..];
                    target_block_id = Some(v as u32);
                }
                2 => {
                    let (v, n) = decode_varint_value(buf)?;
                    buf = &buf[n..];
                    kind = Some(AnnotationKind::from_wire_byte(v as u8)?);
                }
                3 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    value = Some(data.to_vec());
                }
                _ => {
                    let n = skip_field(buf, header.wire_type)?;
                    buf = &buf[n..];
                }
            }
        }

        Ok(Self {
            target_block_id: target_block_id.ok_or(TypeError::MissingRequiredField {
                field: "target_block_id",
            })?,
            kind: kind.ok_or(TypeError::MissingRequiredField { field: "kind" })?,
            value: value.ok_or(TypeError::MissingRequiredField { field: "value" })?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_priority_annotation() {
        let block = AnnotationBlock {
            target_block_id: 0,
            kind: AnnotationKind::Priority,
            value: vec![0x01], // Critical
        };
        let body = block.encode_body();
        let decoded = AnnotationBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn roundtrip_tag_annotation() {
        let block = AnnotationBlock {
            target_block_id: 5,
            kind: AnnotationKind::Tag,
            value: b"security-critical".to_vec(),
        };
        let body = block.encode_body();
        let decoded = AnnotationBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn roundtrip_summary_annotation() {
        let block = AnnotationBlock {
            target_block_id: 2,
            kind: AnnotationKind::Summary,
            value: b"Authentication middleware for JWT tokens".to_vec(),
        };
        let body = block.encode_body();
        let decoded = AnnotationBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }
}
