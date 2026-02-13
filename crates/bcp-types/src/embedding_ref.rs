use crate::error::TypeError;
use crate::fields::{
  decode_bytes_value, decode_field_header, encode_bytes_field, skip_field,
};

/// EMBEDDING_REF block — vector embedding reference.
///
/// Points to a pre-computed vector embedding stored externally (e.g. in
/// a vector database). The `source_hash` provides a content-addressable
/// link back to the original content that was embedded, using BLAKE3.
///
/// Field layout within body:
///
/// ```text
/// ┌──────────┬───────────┬─────────────┬─────────────────────────┐
/// │ Field ID │ Wire Type │ Name        │ Description             │
/// ├──────────┼───────────┼─────────────┼─────────────────────────┤
/// │ 1        │ Bytes     │ vector_id   │ Vector store identifier │
/// │ 2        │ Bytes     │ source_hash │ BLAKE3 content hash     │
/// │ 3        │ Bytes     │ model       │ Embedding model name    │
/// └──────────┴───────────┴─────────────┴─────────────────────────┘
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddingRefBlock {
  /// Opaque identifier for the vector in the external store.
  pub vector_id: Vec<u8>,
  /// BLAKE3 hash of the source content that was embedded.
  pub source_hash: Vec<u8>,
  /// Name of the embedding model (e.g. "text-embedding-3-small").
  pub model: String,
}

impl EmbeddingRefBlock {
  /// Serialize this block's fields into a TLV-encoded body.
  pub fn encode_body(&self) -> Vec<u8> {
    let mut buf = Vec::new();
    encode_bytes_field(&mut buf, 1, &self.vector_id);
    encode_bytes_field(&mut buf, 2, &self.source_hash);
    encode_bytes_field(&mut buf, 3, self.model.as_bytes());
    buf
  }

  /// Deserialize an EMBEDDING_REF block from a TLV-encoded body.
  pub fn decode_body(mut buf: &[u8]) -> Result<Self, TypeError> {
    let mut vector_id: Option<Vec<u8>> = None;
    let mut source_hash: Option<Vec<u8>> = None;
    let mut model: Option<String> = None;

    while !buf.is_empty() {
      let (header, n) = decode_field_header(buf)?;
      buf = &buf[n..];

      match header.field_id {
        1 => {
          let (data, n) = decode_bytes_value(buf)?;
          buf = &buf[n..];
          vector_id = Some(data.to_vec());
        }
        2 => {
          let (data, n) = decode_bytes_value(buf)?;
          buf = &buf[n..];
          source_hash = Some(data.to_vec());
        }
        3 => {
          let (data, n) = decode_bytes_value(buf)?;
          buf = &buf[n..];
          model = Some(String::from_utf8_lossy(data).into_owned());
        }
        _ => {
          let n = skip_field(buf, header.wire_type)?;
          buf = &buf[n..];
        }
      }
    }

    Ok(Self {
      vector_id: vector_id
        .ok_or(TypeError::MissingRequiredField { field: "vector_id" })?,
      source_hash: source_hash
        .ok_or(TypeError::MissingRequiredField { field: "source_hash" })?,
      model: model.ok_or(TypeError::MissingRequiredField { field: "model" })?,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn roundtrip_embedding_ref() {
    let block = EmbeddingRefBlock {
      vector_id: b"vec-001-abc".to_vec(),
      source_hash: vec![0xAB; 32], // 32-byte BLAKE3 hash
      model: "text-embedding-3-small".to_string(),
    };
    let body = block.encode_body();
    let decoded = EmbeddingRefBlock::decode_body(&body).unwrap();
    assert_eq!(decoded, block);
  }
}
