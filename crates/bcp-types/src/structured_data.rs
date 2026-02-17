use crate::enums::DataFormat;
use crate::error::TypeError;
use crate::fields::{
    decode_bytes_value, decode_field_header, decode_varint_value, encode_bytes_field,
    encode_varint_field, skip_field,
};

/// STRUCTURED_DATA block — represents tables, JSON, configs, etc.
///
/// Used for any structured content that isn't source code: API responses,
/// configuration files, CSV datasets, YAML manifests. The `format` field
/// tells the renderer which parser/highlighter to apply.
///
/// Field layout within body:
///
/// ```text
/// ┌──────────┬───────────┬─────────┬──────────────────────────────┐
/// │ Field ID │ Wire Type │ Name    │ Description                  │
/// ├──────────┼───────────┼─────────┼──────────────────────────────┤
/// │ 1        │ Varint    │ format  │ DataFormat enum byte         │
/// │ 2        │ Bytes     │ schema  │ Optional schema descriptor   │
/// │ 3        │ Bytes     │ content │ Raw data bytes               │
/// └──────────┴───────────┴─────────┴──────────────────────────────┘
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuredDataBlock {
    pub format: DataFormat,
    /// Optional schema descriptor (e.g. a JSON Schema URI or inline schema).
    pub schema: Option<String>,
    pub content: Vec<u8>,
}

impl StructuredDataBlock {
    /// Serialize this block's fields into a TLV-encoded body.
    pub fn encode_body(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_varint_field(&mut buf, 1, u64::from(self.format.to_wire_byte()));
        if let Some(ref schema) = self.schema {
            encode_bytes_field(&mut buf, 2, schema.as_bytes());
        }
        encode_bytes_field(&mut buf, 3, &self.content);
        buf
    }

    /// Deserialize a STRUCTURED_DATA block from a TLV-encoded body.
    pub fn decode_body(mut buf: &[u8]) -> Result<Self, TypeError> {
        let mut format: Option<DataFormat> = None;
        let mut schema: Option<String> = None;
        let mut content: Option<Vec<u8>> = None;

        while !buf.is_empty() {
            let (header, n) = decode_field_header(buf)?;
            buf = &buf[n..];

            match header.field_id {
                1 => {
                    let (v, n) = decode_varint_value(buf)?;
                    buf = &buf[n..];
                    format = Some(DataFormat::from_wire_byte(v as u8)?);
                }
                2 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    schema = Some(String::from_utf8_lossy(data).into_owned());
                }
                3 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    content = Some(data.to_vec());
                }
                _ => {
                    let n = skip_field(buf, header.wire_type)?;
                    buf = &buf[n..];
                }
            }
        }

        Ok(Self {
            format: format.ok_or(TypeError::MissingRequiredField { field: "format" })?,
            schema,
            content: content.ok_or(TypeError::MissingRequiredField { field: "content" })?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_json_no_schema() {
        let block = StructuredDataBlock {
            format: DataFormat::Json,
            schema: None,
            content: b"{\"key\": \"value\"}".to_vec(),
        };
        let body = block.encode_body();
        let decoded = StructuredDataBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn roundtrip_csv_with_schema() {
        let block = StructuredDataBlock {
            format: DataFormat::Csv,
            schema: Some("name,age,city".to_string()),
            content: b"Alice,30,NYC\nBob,25,LA".to_vec(),
        };
        let body = block.encode_body();
        let decoded = StructuredDataBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }
}
