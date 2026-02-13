use crate::error::TypeError;
use crate::fields::{
  decode_bytes_value, decode_field_header, decode_varint_value, encode_bytes_field,
  encode_nested_field, encode_varint_field, skip_field,
};

/// FILE_TREE block — represents a directory structure.
///
/// Used to give the LLM spatial context about the project layout.
/// Entries are nested recursively: directories contain child entries,
/// which may themselves be directories.
///
/// Field layout within body:
///
/// ```text
/// ┌──────────┬───────────┬───────────┬────────────────────────────┐
/// │ Field ID │ Wire Type │ Name      │ Description                │
/// ├──────────┼───────────┼───────────┼────────────────────────────┤
/// │ 1        │ Bytes     │ root_path │ Root directory path        │
/// │ 2        │ Nested    │ entries   │ Repeated FileEntry         │
/// └──────────┴───────────┴───────────┴────────────────────────────┘
/// ```
///
/// Each `entries` field (ID=2) contains one `FileEntry` encoded as
/// nested TLV. Multiple entries produce multiple field-2 occurrences,
/// similar to protobuf repeated fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTreeBlock {
  pub root_path: String,
  pub entries: Vec<FileEntry>,
}

/// A single entry in a file tree — either a file or a directory.
///
/// Nested fields within a `FileEntry`:
///
/// ```text
/// ┌──────────┬───────────┬──────────┬─────────────────────────────┐
/// │ Field ID │ Wire Type │ Name     │ Description                 │
/// ├──────────┼───────────┼──────────┼─────────────────────────────┤
/// │ 1        │ Bytes     │ name     │ Entry name (not full path)  │
/// │ 2        │ Varint    │ kind     │ 0=file, 1=directory         │
/// │ 3        │ Varint    │ size     │ File size in bytes          │
/// │ 4        │ Nested    │ children │ Repeated FileEntry (dirs)   │
/// └──────────┴───────────┴──────────┴─────────────────────────────┘
/// ```
///
/// The `children` field is recursive: a directory entry contains nested
/// `FileEntry` values, each encoded as a nested TLV sub-message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
  pub name: String,
  pub kind: FileEntryKind,
  pub size: u64,
  pub children: Vec<FileEntry>,
}

/// Whether a file tree entry is a regular file or a directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileEntryKind {
  File = 0,
  Directory = 1,
}

impl FileEntry {
  /// Encode this entry into TLV bytes (used as nested field payload).
  fn encode(&self) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_bytes_field(&mut buf, 1, self.name.as_bytes());
    encode_varint_field(&mut buf, 2, self.kind as u64);
    encode_varint_field(&mut buf, 3, self.size);
    for child in &self.children {
      encode_nested_field(&mut buf, 4, &child.encode());
    }
    buf
  }

  /// Decode a `FileEntry` from nested TLV bytes.
  fn decode(mut buf: &[u8]) -> Result<Self, TypeError> {
    let mut name: Option<String> = None;
    let mut kind: Option<FileEntryKind> = None;
    let mut size: u64 = 0;
    let mut children = Vec::new();

    while !buf.is_empty() {
      let (header, n) = decode_field_header(buf)?;
      buf = &buf[n..];

      match header.field_id {
        1 => {
          let (data, n) = decode_bytes_value(buf)?;
          buf = &buf[n..];
          name = Some(String::from_utf8_lossy(data).into_owned());
        }
        2 => {
          let (v, n) = decode_varint_value(buf)?;
          buf = &buf[n..];
          kind = Some(match v {
            0 => FileEntryKind::File,
            1 => FileEntryKind::Directory,
            other => {
              return Err(TypeError::InvalidEnumValue {
                enum_name: "FileEntryKind",
                value: other as u8,
              });
            }
          });
        }
        3 => {
          let (v, n) = decode_varint_value(buf)?;
          buf = &buf[n..];
          size = v;
        }
        4 => {
          let (data, n) = decode_bytes_value(buf)?;
          buf = &buf[n..];
          children.push(FileEntry::decode(data)?);
        }
        _ => {
          let n = skip_field(buf, header.wire_type)?;
          buf = &buf[n..];
        }
      }
    }

    Ok(Self {
      name: name.ok_or(TypeError::MissingRequiredField { field: "name" })?,
      kind: kind.ok_or(TypeError::MissingRequiredField { field: "kind" })?,
      size,
      children,
    })
  }
}

impl FileTreeBlock {
  /// Serialize this block's fields into a TLV-encoded body.
  pub fn encode_body(&self) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_bytes_field(&mut buf, 1, self.root_path.as_bytes());
    for entry in &self.entries {
      encode_nested_field(&mut buf, 2, &entry.encode());
    }
    buf
  }

  /// Deserialize a FILE_TREE block from a TLV-encoded body.
  pub fn decode_body(mut buf: &[u8]) -> Result<Self, TypeError> {
    let mut root_path: Option<String> = None;
    let mut entries = Vec::new();

    while !buf.is_empty() {
      let (header, n) = decode_field_header(buf)?;
      buf = &buf[n..];

      match header.field_id {
        1 => {
          let (data, n) = decode_bytes_value(buf)?;
          buf = &buf[n..];
          root_path = Some(String::from_utf8_lossy(data).into_owned());
        }
        2 => {
          let (data, n) = decode_bytes_value(buf)?;
          buf = &buf[n..];
          entries.push(FileEntry::decode(data)?);
        }
        _ => {
          let n = skip_field(buf, header.wire_type)?;
          buf = &buf[n..];
        }
      }
    }

    Ok(Self {
      root_path: root_path
        .ok_or(TypeError::MissingRequiredField { field: "root_path" })?,
      entries,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn roundtrip_flat_tree() {
    let block = FileTreeBlock {
      root_path: "/project".to_string(),
      entries: vec![
        FileEntry {
          name: "Cargo.toml".to_string(),
          kind: FileEntryKind::File,
          size: 256,
          children: vec![],
        },
        FileEntry {
          name: "README.md".to_string(),
          kind: FileEntryKind::File,
          size: 1024,
          children: vec![],
        },
      ],
    };
    let body = block.encode_body();
    let decoded = FileTreeBlock::decode_body(&body).unwrap();
    assert_eq!(decoded, block);
  }

  #[test]
  fn roundtrip_nested_directories() {
    let block = FileTreeBlock {
      root_path: "/app".to_string(),
      entries: vec![FileEntry {
        name: "src".to_string(),
        kind: FileEntryKind::Directory,
        size: 0,
        children: vec![
          FileEntry {
            name: "main.rs".to_string(),
            kind: FileEntryKind::File,
            size: 512,
            children: vec![],
          },
          FileEntry {
            name: "lib".to_string(),
            kind: FileEntryKind::Directory,
            size: 0,
            children: vec![FileEntry {
              name: "utils.rs".to_string(),
              kind: FileEntryKind::File,
              size: 128,
              children: vec![],
            }],
          },
        ],
      }],
    };
    let body = block.encode_body();
    let decoded = FileTreeBlock::decode_body(&body).unwrap();
    assert_eq!(decoded, block);
  }

  #[test]
  fn empty_tree() {
    let block = FileTreeBlock {
      root_path: "/empty".to_string(),
      entries: vec![],
    };
    let body = block.encode_body();
    let decoded = FileTreeBlock::decode_body(&body).unwrap();
    assert_eq!(decoded, block);
  }
}
