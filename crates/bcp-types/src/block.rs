use bcp_wire::block_frame::BlockFlags;

use crate::annotation::AnnotationBlock;
use crate::block_type::BlockType;
use crate::code::CodeBlock;
use crate::conversation::ConversationBlock;
use crate::diff::DiffBlock;
use crate::document::DocumentBlock;
use crate::embedding_ref::EmbeddingRefBlock;
use crate::error::TypeError;
use crate::extension::ExtensionBlock;
use crate::file_tree::FileTreeBlock;
use crate::image::ImageBlock;
use crate::structured_data::StructuredDataBlock;
use crate::summary::Summary;
use crate::tool_result::ToolResultBlock;

/// A fully parsed LCP block — the union of all block types with
/// optional metadata.
///
/// This is the primary type that higher-level crates (`lcp-encoder`,
/// `lcp-decoder`, `lcp-driver`) work with. It combines the block's
/// type tag, per-block flags, optional summary, and typed content
/// into a single value.
///
/// The `Block` struct sits between the wire layer (`BlockFrame` from
/// `bcp-wire`) and the application layer. The encoder converts a
/// `Block` into a `BlockFrame` by calling `content.encode_body()`
/// and prepending the summary if present. The decoder does the reverse:
/// it reads a `BlockFrame`, strips the summary if `flags.has_summary()`,
/// then dispatches to the appropriate `decode_body` method based on
/// `block_type`.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
  pub block_type: BlockType,
  pub flags: BlockFlags,
  pub summary: Option<Summary>,
  pub content: BlockContent,
}

/// The typed content within a block.
///
/// Each variant wraps the corresponding block struct from this crate.
/// The `Unknown` variant preserves unrecognized block types as raw bytes,
/// enabling forward compatibility: a decoder built against an older spec
/// can still read (and re-encode) blocks from a newer encoder.
///
/// ```text
/// ┌─────────────────┬────────────────────────┐
/// │ Variant         │ Block Type Wire ID     │
/// ├─────────────────┼────────────────────────┤
/// │ Code            │ 0x01                   │
/// │ Conversation    │ 0x02                   │
/// │ FileTree        │ 0x03                   │
/// │ ToolResult      │ 0x04                   │
/// │ Document        │ 0x05                   │
/// │ StructuredData  │ 0x06                   │
/// │ Diff            │ 0x07                   │
/// │ Annotation      │ 0x08                   │
/// │ EmbeddingRef    │ 0x09                   │
/// │ Image           │ 0x0A                   │
/// │ Extension       │ 0xFE                   │
/// │ End             │ 0xFF                   │
/// │ Unknown         │ any other byte         │
/// └─────────────────┴────────────────────────┘
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum BlockContent {
  Code(CodeBlock),
  Conversation(ConversationBlock),
  FileTree(FileTreeBlock),
  ToolResult(ToolResultBlock),
  Document(DocumentBlock),
  StructuredData(StructuredDataBlock),
  Diff(DiffBlock),
  Annotation(AnnotationBlock),
  EmbeddingRef(EmbeddingRefBlock),
  Image(ImageBlock),
  Extension(ExtensionBlock),
  End,
  /// Raw body bytes for an unrecognized block type.
  Unknown { type_id: u8, body: Vec<u8> },
}

impl BlockContent {
  /// Encode the typed content into a raw body byte vector.
  ///
  /// For `End`, returns an empty vec (END blocks have no body).
  /// For `Unknown`, returns the preserved raw bytes as-is.
  pub fn encode_body(&self) -> Vec<u8> {
    match self {
      Self::Code(b) => b.encode_body(),
      Self::Conversation(b) => b.encode_body(),
      Self::FileTree(b) => b.encode_body(),
      Self::ToolResult(b) => b.encode_body(),
      Self::Document(b) => b.encode_body(),
      Self::StructuredData(b) => b.encode_body(),
      Self::Diff(b) => b.encode_body(),
      Self::Annotation(b) => b.encode_body(),
      Self::EmbeddingRef(b) => b.encode_body(),
      Self::Image(b) => b.encode_body(),
      Self::Extension(b) => b.encode_body(),
      Self::End => Vec::new(),
      Self::Unknown { body, .. } => body.clone(),
    }
  }

  /// Decode typed content from a raw body, dispatching on block type.
  ///
  /// The caller is responsible for stripping the summary prefix from
  /// the body before calling this method (if `flags.has_summary()`).
  pub fn decode_body(block_type: &BlockType, body: &[u8]) -> Result<Self, TypeError> {
    match block_type {
      BlockType::Code => Ok(Self::Code(CodeBlock::decode_body(body)?)),
      BlockType::Conversation => {
        Ok(Self::Conversation(ConversationBlock::decode_body(body)?))
      }
      BlockType::FileTree => Ok(Self::FileTree(FileTreeBlock::decode_body(body)?)),
      BlockType::ToolResult => {
        Ok(Self::ToolResult(ToolResultBlock::decode_body(body)?))
      }
      BlockType::Document => Ok(Self::Document(DocumentBlock::decode_body(body)?)),
      BlockType::StructuredData => Ok(Self::StructuredData(
        StructuredDataBlock::decode_body(body)?,
      )),
      BlockType::Diff => Ok(Self::Diff(DiffBlock::decode_body(body)?)),
      BlockType::Annotation => {
        Ok(Self::Annotation(AnnotationBlock::decode_body(body)?))
      }
      BlockType::EmbeddingRef => {
        Ok(Self::EmbeddingRef(EmbeddingRefBlock::decode_body(body)?))
      }
      BlockType::Image => Ok(Self::Image(ImageBlock::decode_body(body)?)),
      BlockType::Extension => Ok(Self::Extension(ExtensionBlock::decode_body(body)?)),
      BlockType::End => Ok(Self::End),
      BlockType::Unknown(id) => Ok(Self::Unknown {
        type_id: *id,
        body: body.to_vec(),
      }),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::enums::{FormatHint, Lang, Role, Status};

  #[test]
  fn block_encode_decode_roundtrip() {
    let block = Block {
      block_type: BlockType::Code,
      flags: BlockFlags::NONE,
      summary: None,
      content: BlockContent::Code(CodeBlock {
        lang: Lang::Rust,
        path: "lib.rs".to_string(),
        content: b"pub fn hello() {}".to_vec(),
        line_range: None,
      }),
    };

    let body = block.content.encode_body();
    let decoded = BlockContent::decode_body(&block.block_type, &body).unwrap();
    assert_eq!(decoded, block.content);
  }

  #[test]
  fn block_with_summary() {
    let summary = Summary {
      text: "Main entry point".to_string(),
    };
    let content = BlockContent::Code(CodeBlock {
      lang: Lang::Rust,
      path: "main.rs".to_string(),
      content: b"fn main() {}".to_vec(),
      line_range: None,
    });

    // Encode: summary prefix + body
    let mut body = Vec::new();
    summary.encode(&mut body);
    body.extend_from_slice(&content.encode_body());

    // Decode: strip summary first, then decode content
    let (decoded_summary, consumed) = Summary::decode(&body).unwrap();
    let decoded_content =
      BlockContent::decode_body(&BlockType::Code, &body[consumed..]).unwrap();

    assert_eq!(decoded_summary, summary);
    assert_eq!(decoded_content, content);
  }

  #[test]
  fn unknown_block_type_preserved() {
    let raw_body = b"arbitrary bytes".to_vec();
    let block_type = BlockType::Unknown(0x42);

    let content = BlockContent::decode_body(&block_type, &raw_body).unwrap();
    assert_eq!(
      content,
      BlockContent::Unknown {
        type_id: 0x42,
        body: raw_body.clone()
      }
    );

    // Round-trip: encoding gives back the same bytes
    assert_eq!(content.encode_body(), raw_body);
  }

  #[test]
  fn end_block_empty_body() {
    let content = BlockContent::decode_body(&BlockType::End, &[]).unwrap();
    assert_eq!(content, BlockContent::End);
    assert!(content.encode_body().is_empty());
  }

  #[test]
  fn all_block_types_dispatch() {
    // Verify that decode_body dispatches to the right variant for each type.
    // We use minimal valid bodies for each.

    let code = CodeBlock {
      lang: Lang::Python,
      path: "x.py".to_string(),
      content: b"pass".to_vec(),
      line_range: None,
    };
    let body = code.encode_body();
    let result = BlockContent::decode_body(&BlockType::Code, &body).unwrap();
    assert!(matches!(result, BlockContent::Code(_)));

    let conv = ConversationBlock {
      role: Role::User,
      content: b"hi".to_vec(),
      tool_call_id: None,
    };
    let body = conv.encode_body();
    let result = BlockContent::decode_body(&BlockType::Conversation, &body).unwrap();
    assert!(matches!(result, BlockContent::Conversation(_)));

    let doc = DocumentBlock {
      title: "t".to_string(),
      content: b"c".to_vec(),
      format_hint: FormatHint::Plain,
    };
    let body = doc.encode_body();
    let result = BlockContent::decode_body(&BlockType::Document, &body).unwrap();
    assert!(matches!(result, BlockContent::Document(_)));

    let tool = ToolResultBlock {
      tool_name: "t".to_string(),
      status: Status::Ok,
      content: b"ok".to_vec(),
      schema_hint: None,
    };
    let body = tool.encode_body();
    let result = BlockContent::decode_body(&BlockType::ToolResult, &body).unwrap();
    assert!(matches!(result, BlockContent::ToolResult(_)));
  }
}
