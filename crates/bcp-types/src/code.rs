use crate::enums::Lang;
use crate::error::TypeError;
use crate::fields::{
    decode_bytes_value, decode_field_header, decode_varint_value, encode_bytes_field,
    encode_varint_field, skip_field,
};

/// CODE block — represents a source code file or fragment.
///
/// This is the most common block type in practice: every source file,
/// snippet, or code region in a context pack becomes a CODE block.
///
/// Field layout within body:
///
/// ```text
/// ┌──────────┬───────────┬────────────┬────────────────────────────┐
/// │ Field ID │ Wire Type │ Name       │ Description                │
/// ├──────────┼───────────┼────────────┼────────────────────────────┤
/// │ 1        │ Varint    │ lang       │ Language enum byte         │
/// │ 2        │ Bytes     │ path       │ UTF-8 file path            │
/// │ 3        │ Bytes     │ content    │ Raw source code bytes      │
/// │ 4        │ Varint    │ line_start │ Start line (optional)      │
/// │ 5        │ Varint    │ line_end   │ End line (optional)        │
/// └──────────┴───────────┴────────────┴────────────────────────────┘
/// ```
///
/// Fields 4 and 5 are optional — they are only encoded when `line_range`
/// is `Some`. This lets you represent either a full file or a specific
/// line range within it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeBlock {
    pub lang: Lang,
    pub path: String,
    pub content: Vec<u8>,
    /// Optional line range `(start, end)` for code fragments.
    /// Both values are 1-indexed and inclusive.
    pub line_range: Option<(u32, u32)>,
}

impl CodeBlock {
    /// Serialize this block's fields into a TLV-encoded body.
    pub fn encode_body(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_varint_field(&mut buf, 1, u64::from(self.lang.to_wire_byte()));
        encode_bytes_field(&mut buf, 2, self.path.as_bytes());
        encode_bytes_field(&mut buf, 3, &self.content);
        if let Some((start, end)) = self.line_range {
            encode_varint_field(&mut buf, 4, u64::from(start));
            encode_varint_field(&mut buf, 5, u64::from(end));
        }
        buf
    }

    /// Deserialize a CODE block from a TLV-encoded body.
    ///
    /// Unknown field IDs are silently skipped for forward compatibility.
    pub fn decode_body(mut buf: &[u8]) -> Result<Self, TypeError> {
        let mut lang: Option<Lang> = None;
        let mut path: Option<String> = None;
        let mut content: Option<Vec<u8>> = None;
        let mut line_start: Option<u32> = None;
        let mut line_end: Option<u32> = None;

        while !buf.is_empty() {
            let (header, n) = decode_field_header(buf)?;
            buf = &buf[n..];

            match header.field_id {
                1 => {
                    let (v, n) = decode_varint_value(buf)?;
                    buf = &buf[n..];
                    lang = Some(Lang::from_wire_byte(v as u8));
                }
                2 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    path = Some(String::from_utf8_lossy(data).into_owned());
                }
                3 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    content = Some(data.to_vec());
                }
                4 => {
                    let (v, n) = decode_varint_value(buf)?;
                    buf = &buf[n..];
                    line_start = Some(v as u32);
                }
                5 => {
                    let (v, n) = decode_varint_value(buf)?;
                    buf = &buf[n..];
                    line_end = Some(v as u32);
                }
                _ => {
                    let n = skip_field(buf, header.wire_type)?;
                    buf = &buf[n..];
                }
            }
        }

        Ok(Self {
            lang: lang.ok_or(TypeError::MissingRequiredField { field: "lang" })?,
            path: path.ok_or(TypeError::MissingRequiredField { field: "path" })?,
            content: content.ok_or(TypeError::MissingRequiredField { field: "content" })?,
            line_range: match (line_start, line_end) {
                (Some(s), Some(e)) => Some((s, e)),
                _ => None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_full_file() {
        let block = CodeBlock {
            lang: Lang::Rust,
            path: "src/main.rs".to_string(),
            content: b"fn main() {}".to_vec(),
            line_range: None,
        };
        let body = block.encode_body();
        let decoded = CodeBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn roundtrip_with_line_range() {
        let block = CodeBlock {
            lang: Lang::TypeScript,
            path: "src/index.ts".to_string(),
            content: b"console.log('hello');".to_vec(),
            line_range: Some((10, 25)),
        };
        let body = block.encode_body();
        let decoded = CodeBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn roundtrip_unknown_language() {
        let block = CodeBlock {
            lang: Lang::Other(0x42),
            path: "script.xyz".to_string(),
            content: b"custom code".to_vec(),
            line_range: None,
        };
        let body = block.encode_body();
        let decoded = CodeBlock::decode_body(&body).unwrap();
        assert_eq!(decoded.lang, Lang::Other(0x42));
    }

    #[test]
    fn missing_content_field() {
        // Encode only lang and path, no content
        let mut buf = Vec::new();
        encode_varint_field(&mut buf, 1, 0x01);
        encode_bytes_field(&mut buf, 2, b"test.rs");

        let result = CodeBlock::decode_body(&buf);
        assert!(matches!(
            result,
            Err(TypeError::MissingRequiredField { field: "content" })
        ));
    }
}
