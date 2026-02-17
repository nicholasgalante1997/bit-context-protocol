use crate::error::TypeError;
use crate::fields::{decode_bytes_value, decode_field_header, encode_bytes_field, skip_field};

/// EXTENSION block — user-defined block type.
///
/// Provides an escape hatch for custom content that doesn't fit the
/// 10 built-in block types. Extensions are namespaced to avoid collisions
/// between different organizations or tools.
///
/// Field layout within body:
///
/// ```text
/// ┌──────────┬───────────┬───────────┬───────────────────────────┐
/// │ Field ID │ Wire Type │ Name      │ Description               │
/// ├──────────┼───────────┼───────────┼───────────────────────────┤
/// │ 1        │ Bytes     │ namespace │ Namespace (e.g. "myorg")  │
/// │ 2        │ Bytes     │ type_name │ Type within namespace     │
/// │ 3        │ Bytes     │ content   │ Opaque content bytes      │
/// └──────────┴───────────┴───────────┴───────────────────────────┘
/// ```
///
/// The `content` field is opaque — the LCP decoder does not attempt to
/// parse it. Only consumers that understand the `namespace/type_name`
/// pair will interpret the content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionBlock {
    pub namespace: String,
    pub type_name: String,
    pub content: Vec<u8>,
}

impl ExtensionBlock {
    /// Serialize this block's fields into a TLV-encoded body.
    pub fn encode_body(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_bytes_field(&mut buf, 1, self.namespace.as_bytes());
        encode_bytes_field(&mut buf, 2, self.type_name.as_bytes());
        encode_bytes_field(&mut buf, 3, &self.content);
        buf
    }

    /// Deserialize an EXTENSION block from a TLV-encoded body.
    pub fn decode_body(mut buf: &[u8]) -> Result<Self, TypeError> {
        let mut namespace: Option<String> = None;
        let mut type_name: Option<String> = None;
        let mut content: Option<Vec<u8>> = None;

        while !buf.is_empty() {
            let (header, n) = decode_field_header(buf)?;
            buf = &buf[n..];

            match header.field_id {
                1 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    namespace = Some(String::from_utf8_lossy(data).into_owned());
                }
                2 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    type_name = Some(String::from_utf8_lossy(data).into_owned());
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
            namespace: namespace.ok_or(TypeError::MissingRequiredField { field: "namespace" })?,
            type_name: type_name.ok_or(TypeError::MissingRequiredField { field: "type_name" })?,
            content: content.ok_or(TypeError::MissingRequiredField { field: "content" })?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_extension() {
        let block = ExtensionBlock {
            namespace: "myorg".to_string(),
            type_name: "custom_metric".to_string(),
            content: b"{\"latency_ms\": 42}".to_vec(),
        };
        let body = block.encode_body();
        let decoded = ExtensionBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn roundtrip_empty_content() {
        let block = ExtensionBlock {
            namespace: "test".to_string(),
            type_name: "marker".to_string(),
            content: vec![],
        };
        let body = block.encode_body();
        let decoded = ExtensionBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }
}
