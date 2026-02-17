use crate::error::TypeError;
use crate::fields::{
    decode_bytes_value, decode_field_header, decode_varint_value, encode_bytes_field,
    encode_nested_field, encode_varint_field, skip_field,
};

/// DIFF block — represents code changes for a single file.
///
/// Used to compactly represent modifications (e.g. from a git diff or
/// an edit operation). Each hunk captures a contiguous range of changes
/// in unified diff format.
///
/// Field layout within body:
///
/// ```text
/// ┌──────────┬───────────┬───────┬───────────────────────────────┐
/// │ Field ID │ Wire Type │ Name  │ Description                   │
/// ├──────────┼───────────┼───────┼───────────────────────────────┤
/// │ 1        │ Bytes     │ path  │ File path                     │
/// │ 2        │ Nested    │ hunks │ Repeated DiffHunk             │
/// └──────────┴───────────┴───────┴───────────────────────────────┘
/// ```
///
/// Multiple hunks produce multiple field-2 occurrences (repeated field
/// pattern, same as `FileEntry` in FILE_TREE).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffBlock {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
}

/// A single contiguous range of changes within a diff.
///
/// Nested fields within a `DiffHunk`:
///
/// ```text
/// ┌──────────┬───────────┬───────────┬────────────────────────────┐
/// │ Field ID │ Wire Type │ Name      │ Description                │
/// ├──────────┼───────────┼───────────┼────────────────────────────┤
/// │ 1        │ Varint    │ old_start │ Start line in old file     │
/// │ 2        │ Varint    │ new_start │ Start line in new file     │
/// │ 3        │ Bytes     │ lines     │ Hunk content (unified fmt) │
/// └──────────┴───────────┴───────────┴────────────────────────────┘
/// ```
///
/// The `lines` field contains the hunk body in unified diff format:
/// lines prefixed with `+` (added), `-` (removed), or ` ` (context).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<u8>,
}

impl DiffHunk {
    /// Encode this hunk into TLV bytes (used as nested field payload).
    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_varint_field(&mut buf, 1, u64::from(self.old_start));
        encode_varint_field(&mut buf, 2, u64::from(self.new_start));
        encode_bytes_field(&mut buf, 3, &self.lines);
        buf
    }

    /// Decode a `DiffHunk` from nested TLV bytes.
    fn decode(mut buf: &[u8]) -> Result<Self, TypeError> {
        let mut old_start: Option<u32> = None;
        let mut new_start: Option<u32> = None;
        let mut lines: Option<Vec<u8>> = None;

        while !buf.is_empty() {
            let (header, n) = decode_field_header(buf)?;
            buf = &buf[n..];

            match header.field_id {
                1 => {
                    let (v, n) = decode_varint_value(buf)?;
                    buf = &buf[n..];
                    old_start = Some(v as u32);
                }
                2 => {
                    let (v, n) = decode_varint_value(buf)?;
                    buf = &buf[n..];
                    new_start = Some(v as u32);
                }
                3 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    lines = Some(data.to_vec());
                }
                _ => {
                    let n = skip_field(buf, header.wire_type)?;
                    buf = &buf[n..];
                }
            }
        }

        Ok(Self {
            old_start: old_start.ok_or(TypeError::MissingRequiredField { field: "old_start" })?,
            new_start: new_start.ok_or(TypeError::MissingRequiredField { field: "new_start" })?,
            lines: lines.ok_or(TypeError::MissingRequiredField { field: "lines" })?,
        })
    }
}

impl DiffBlock {
    /// Serialize this block's fields into a TLV-encoded body.
    pub fn encode_body(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_bytes_field(&mut buf, 1, self.path.as_bytes());
        for hunk in &self.hunks {
            encode_nested_field(&mut buf, 2, &hunk.encode());
        }
        buf
    }

    /// Deserialize a DIFF block from a TLV-encoded body.
    pub fn decode_body(mut buf: &[u8]) -> Result<Self, TypeError> {
        let mut path: Option<String> = None;
        let mut hunks = Vec::new();

        while !buf.is_empty() {
            let (header, n) = decode_field_header(buf)?;
            buf = &buf[n..];

            match header.field_id {
                1 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    path = Some(String::from_utf8_lossy(data).into_owned());
                }
                2 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    hunks.push(DiffHunk::decode(data)?);
                }
                _ => {
                    let n = skip_field(buf, header.wire_type)?;
                    buf = &buf[n..];
                }
            }
        }

        Ok(Self {
            path: path.ok_or(TypeError::MissingRequiredField { field: "path" })?,
            hunks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_single_hunk() {
        let block = DiffBlock {
            path: "src/main.rs".to_string(),
            hunks: vec![DiffHunk {
                old_start: 10,
                new_start: 10,
                lines: b" fn main() {\n-    println!(\"old\");\n+    println!(\"new\");\n }\n"
                    .to_vec(),
            }],
        };
        let body = block.encode_body();
        let decoded = DiffBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn roundtrip_multiple_hunks() {
        let block = DiffBlock {
            path: "lib.rs".to_string(),
            hunks: vec![
                DiffHunk {
                    old_start: 1,
                    new_start: 1,
                    lines: b"+use std::io;\n".to_vec(),
                },
                DiffHunk {
                    old_start: 50,
                    new_start: 51,
                    lines: b"-    old_call();\n+    new_call();\n".to_vec(),
                },
            ],
        };
        let body = block.encode_body();
        let decoded = DiffBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn empty_hunks() {
        let block = DiffBlock {
            path: "empty.rs".to_string(),
            hunks: vec![],
        };
        let body = block.encode_body();
        let decoded = DiffBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }
}
