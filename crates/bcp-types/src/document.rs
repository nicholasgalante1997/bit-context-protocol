use crate::enums::FormatHint;
use crate::error::TypeError;
use crate::fields::{
    decode_bytes_value, decode_field_header, decode_varint_value, encode_bytes_field,
    encode_varint_field, skip_field,
};

/// DOCUMENT block — represents prose or documentation content.
///
/// Used for READMEs, design docs, comments, or any non-code textual
/// content that provides context to the LLM. The `format_hint` tells
/// the renderer how to interpret the body (markdown, plain text, HTML).
///
/// Field layout within body:
///
/// ```text
/// ┌──────────┬───────────┬─────────────┬──────────────────────────┐
/// │ Field ID │ Wire Type │ Name        │ Description              │
/// ├──────────┼───────────┼─────────────┼──────────────────────────┤
/// │ 1        │ Bytes     │ title       │ Document title           │
/// │ 2        │ Bytes     │ content     │ Document body            │
/// │ 3        │ Varint    │ format_hint │ FormatHint enum byte     │
/// └──────────┴───────────┴─────────────┴──────────────────────────┘
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentBlock {
    pub title: String,
    pub content: Vec<u8>,
    pub format_hint: FormatHint,
}

impl DocumentBlock {
    /// Serialize this block's fields into a TLV-encoded body.
    pub fn encode_body(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_bytes_field(&mut buf, 1, self.title.as_bytes());
        encode_bytes_field(&mut buf, 2, &self.content);
        encode_varint_field(&mut buf, 3, u64::from(self.format_hint.to_wire_byte()));
        buf
    }

    /// Deserialize a DOCUMENT block from a TLV-encoded body.
    pub fn decode_body(mut buf: &[u8]) -> Result<Self, TypeError> {
        let mut title: Option<String> = None;
        let mut content: Option<Vec<u8>> = None;
        let mut format_hint: Option<FormatHint> = None;

        while !buf.is_empty() {
            let (header, n) = decode_field_header(buf)?;
            buf = &buf[n..];

            match header.field_id {
                1 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    title = Some(String::from_utf8_lossy(data).into_owned());
                }
                2 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    content = Some(data.to_vec());
                }
                3 => {
                    let (v, n) = decode_varint_value(buf)?;
                    buf = &buf[n..];
                    format_hint = Some(FormatHint::from_wire_byte(v as u8)?);
                }
                _ => {
                    let n = skip_field(buf, header.wire_type)?;
                    buf = &buf[n..];
                }
            }
        }

        Ok(Self {
            title: title.ok_or(TypeError::MissingRequiredField { field: "title" })?,
            content: content.ok_or(TypeError::MissingRequiredField { field: "content" })?,
            format_hint: format_hint.ok_or(TypeError::MissingRequiredField {
                field: "format_hint",
            })?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_markdown_doc() {
        let block = DocumentBlock {
            title: "README".to_string(),
            content: b"# Hello\n\nThis is a test.".to_vec(),
            format_hint: FormatHint::Markdown,
        };
        let body = block.encode_body();
        let decoded = DocumentBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn roundtrip_plain_text() {
        let block = DocumentBlock {
            title: "notes.txt".to_string(),
            content: b"Just plain text.".to_vec(),
            format_hint: FormatHint::Plain,
        };
        let body = block.encode_body();
        let decoded = DocumentBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }
}
