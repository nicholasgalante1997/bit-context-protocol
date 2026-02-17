use crate::enums::Role;
use crate::error::TypeError;
use crate::fields::{
    decode_bytes_value, decode_field_header, decode_varint_value, encode_bytes_field,
    encode_varint_field, skip_field,
};

/// CONVERSATION block — represents a single chat turn.
///
/// Each turn in a conversation (system prompt, user message, assistant
/// response, tool output) becomes one CONVERSATION block. The `role`
/// field determines the speaker, and `content` holds the message body.
///
/// Field layout within body:
///
/// ```text
/// ┌──────────┬───────────┬──────────────┬──────────────────────────┐
/// │ Field ID │ Wire Type │ Name         │ Description              │
/// ├──────────┼───────────┼──────────────┼──────────────────────────┤
/// │ 1        │ Varint    │ role         │ Role enum byte           │
/// │ 2        │ Bytes     │ content      │ Message body (UTF-8)     │
/// │ 3        │ Bytes     │ tool_call_id │ Tool call ID (optional)  │
/// └──────────┴───────────┴──────────────┴──────────────────────────┘
/// ```
///
/// Field 3 is only present when `role` is `Tool`, linking the response
/// back to the tool invocation that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationBlock {
    pub role: Role,
    pub content: Vec<u8>,
    /// Optional tool call ID, present only for `Role::Tool` turns.
    pub tool_call_id: Option<String>,
}

impl ConversationBlock {
    /// Serialize this block's fields into a TLV-encoded body.
    pub fn encode_body(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_varint_field(&mut buf, 1, u64::from(self.role.to_wire_byte()));
        encode_bytes_field(&mut buf, 2, &self.content);
        if let Some(ref id) = self.tool_call_id {
            encode_bytes_field(&mut buf, 3, id.as_bytes());
        }
        buf
    }

    /// Deserialize a CONVERSATION block from a TLV-encoded body.
    pub fn decode_body(mut buf: &[u8]) -> Result<Self, TypeError> {
        let mut role: Option<Role> = None;
        let mut content: Option<Vec<u8>> = None;
        let mut tool_call_id: Option<String> = None;

        while !buf.is_empty() {
            let (header, n) = decode_field_header(buf)?;
            buf = &buf[n..];

            match header.field_id {
                1 => {
                    let (v, n) = decode_varint_value(buf)?;
                    buf = &buf[n..];
                    role = Some(Role::from_wire_byte(v as u8)?);
                }
                2 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    content = Some(data.to_vec());
                }
                3 => {
                    let (data, n) = decode_bytes_value(buf)?;
                    buf = &buf[n..];
                    tool_call_id = Some(String::from_utf8_lossy(data).into_owned());
                }
                _ => {
                    let n = skip_field(buf, header.wire_type)?;
                    buf = &buf[n..];
                }
            }
        }

        Ok(Self {
            role: role.ok_or(TypeError::MissingRequiredField { field: "role" })?,
            content: content.ok_or(TypeError::MissingRequiredField { field: "content" })?,
            tool_call_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_user_message() {
        let block = ConversationBlock {
            role: Role::User,
            content: b"What is Rust?".to_vec(),
            tool_call_id: None,
        };
        let body = block.encode_body();
        let decoded = ConversationBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn roundtrip_tool_with_call_id() {
        let block = ConversationBlock {
            role: Role::Tool,
            content: b"{ \"result\": 42 }".to_vec(),
            tool_call_id: Some("call_abc123".to_string()),
        };
        let body = block.encode_body();
        let decoded = ConversationBlock::decode_body(&body).unwrap();
        assert_eq!(decoded, block);
    }

    #[test]
    fn tool_call_id_absent_when_not_tool() {
        let block = ConversationBlock {
            role: Role::Assistant,
            content: b"Here's the answer.".to_vec(),
            tool_call_id: None,
        };
        let body = block.encode_body();
        let decoded = ConversationBlock::decode_body(&body).unwrap();
        assert_eq!(decoded.tool_call_id, None);
    }
}
