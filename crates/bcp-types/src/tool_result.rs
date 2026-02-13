use crate::enums::Status;
use crate::error::TypeError;
use crate::fields::{
  decode_bytes_value, decode_field_header, decode_varint_value, encode_bytes_field,
  encode_varint_field, skip_field,
};

/// TOOL_RESULT block — represents output from a tool or MCP server.
///
/// Captures the name of the tool that was invoked, its execution status,
/// the raw output, and an optional schema hint so downstream consumers
/// know how to parse the content.
///
/// Field layout within body:
///
/// ```text
/// ┌──────────┬───────────┬─────────────┬──────────────────────────┐
/// │ Field ID │ Wire Type │ Name        │ Description              │
/// ├──────────┼───────────┼─────────────┼──────────────────────────┤
/// │ 1        │ Bytes     │ tool_name   │ Tool identifier          │
/// │ 2        │ Varint    │ status      │ Status enum byte         │
/// │ 3        │ Bytes     │ content     │ Tool output bytes        │
/// │ 4        │ Bytes     │ schema_hint │ Schema hint (optional)   │
/// └──────────┴───────────┴─────────────┴──────────────────────────┘
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResultBlock {
  pub tool_name: String,
  pub status: Status,
  pub content: Vec<u8>,
  /// Optional schema hint (e.g. "json-schema://...") to help
  /// consumers parse the content field.
  pub schema_hint: Option<String>,
}

impl ToolResultBlock {
  /// Serialize this block's fields into a TLV-encoded body.
  pub fn encode_body(&self) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_bytes_field(&mut buf, 1, self.tool_name.as_bytes());
    encode_varint_field(&mut buf, 2, u64::from(self.status.to_wire_byte()));
    encode_bytes_field(&mut buf, 3, &self.content);
    if let Some(ref hint) = self.schema_hint {
      encode_bytes_field(&mut buf, 4, hint.as_bytes());
    }
    buf
  }

  /// Deserialize a TOOL_RESULT block from a TLV-encoded body.
  pub fn decode_body(mut buf: &[u8]) -> Result<Self, TypeError> {
    let mut tool_name: Option<String> = None;
    let mut status: Option<Status> = None;
    let mut content: Option<Vec<u8>> = None;
    let mut schema_hint: Option<String> = None;

    while !buf.is_empty() {
      let (header, n) = decode_field_header(buf)?;
      buf = &buf[n..];

      match header.field_id {
        1 => {
          let (data, n) = decode_bytes_value(buf)?;
          buf = &buf[n..];
          tool_name = Some(String::from_utf8_lossy(data).into_owned());
        }
        2 => {
          let (v, n) = decode_varint_value(buf)?;
          buf = &buf[n..];
          status = Some(Status::from_wire_byte(v as u8)?);
        }
        3 => {
          let (data, n) = decode_bytes_value(buf)?;
          buf = &buf[n..];
          content = Some(data.to_vec());
        }
        4 => {
          let (data, n) = decode_bytes_value(buf)?;
          buf = &buf[n..];
          schema_hint = Some(String::from_utf8_lossy(data).into_owned());
        }
        _ => {
          let n = skip_field(buf, header.wire_type)?;
          buf = &buf[n..];
        }
      }
    }

    Ok(Self {
      tool_name: tool_name.ok_or(TypeError::MissingRequiredField { field: "tool_name" })?,
      status: status.ok_or(TypeError::MissingRequiredField { field: "status" })?,
      content: content.ok_or(TypeError::MissingRequiredField { field: "content" })?,
      schema_hint,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn roundtrip_ok_result() {
    let block = ToolResultBlock {
      tool_name: "read_file".to_string(),
      status: Status::Ok,
      content: b"file contents here".to_vec(),
      schema_hint: None,
    };
    let body = block.encode_body();
    let decoded = ToolResultBlock::decode_body(&body).unwrap();
    assert_eq!(decoded, block);
  }

  #[test]
  fn roundtrip_error_with_schema() {
    let block = ToolResultBlock {
      tool_name: "api_call".to_string(),
      status: Status::Error,
      content: b"404 Not Found".to_vec(),
      schema_hint: Some("application/json".to_string()),
    };
    let body = block.encode_body();
    let decoded = ToolResultBlock::decode_body(&body).unwrap();
    assert_eq!(decoded, block);
  }

  #[test]
  fn roundtrip_timeout() {
    let block = ToolResultBlock {
      tool_name: "slow_tool".to_string(),
      status: Status::Timeout,
      content: b"".to_vec(),
      schema_hint: None,
    };
    let body = block.encode_body();
    let decoded = ToolResultBlock::decode_body(&body).unwrap();
    assert_eq!(decoded, block);
  }
}
