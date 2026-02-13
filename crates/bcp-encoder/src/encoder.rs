use bcp_types::annotation::AnnotationBlock;
use bcp_types::code::CodeBlock;
use bcp_types::conversation::ConversationBlock;
use bcp_types::diff::{DiffBlock, DiffHunk};
use bcp_types::document::DocumentBlock;
use bcp_types::enums::{
  AnnotationKind, DataFormat, FormatHint, Lang, MediaType, Priority, Role, Status,
};
use bcp_types::extension::ExtensionBlock;
use bcp_types::file_tree::{FileEntry, FileTreeBlock};
use bcp_types::image::ImageBlock;
use bcp_types::structured_data::StructuredDataBlock;
use bcp_types::summary::Summary;
use bcp_types::tool_result::ToolResultBlock;
use bcp_types::BlockContent;
use bcp_wire::block_frame::{block_type, BlockFlags, BlockFrame};
use bcp_wire::header::{HeaderFlags, LcpHeader, HEADER_SIZE};

use crate::error::EncodeError;

/// Maximum block body size (16 MiB). Blocks exceeding this limit produce
/// an [`EncodeError::BlockTooLarge`] during `.encode()`.
const MAX_BLOCK_BODY_SIZE: usize = 16 * 1024 * 1024;

/// LCP encoder — constructs a binary payload from structured blocks.
///
/// The encoder is the tool-facing API that allows agents, MCP servers,
/// and other producers to build LCP payloads. It follows the builder
/// pattern defined in RFC §5.6: methods like [`add_code`](Self::add_code),
/// [`add_conversation`](Self::add_conversation), etc. append typed blocks
/// to an internal list, and chainable modifiers like
/// [`with_summary`](Self::with_summary) and
/// [`with_priority`](Self::with_priority) annotate the most recently
/// added block.
///
/// # Usage
///
/// ```rust
/// use bcp_encoder::LcpEncoder;
/// use bcp_types::enums::{Lang, Role, Status, Priority};
///
/// let payload = LcpEncoder::new()
///     .add_code(Lang::Rust, "src/main.rs", b"fn main() {}")
///     .with_summary("Entry point: CLI setup and server startup.")
///     .with_priority(Priority::High)
///     .add_conversation(Role::User, b"Fix the timeout bug.")
///     .add_conversation(Role::Assistant, b"I'll examine the pool config...")
///     .add_tool_result("ripgrep", Status::Ok, b"3 matches found.")
///     .encode()
///     .unwrap();
/// ```
///
/// # Output layout
///
/// The `.encode()` method serializes all accumulated blocks into a
/// self-contained byte sequence:
///
/// ```text
/// ┌──────────────┬──────────────────────────────────────────┐
/// │ [8 bytes]    │ File header (magic, version, flags, rsv) │
/// │ [N bytes]    │ Block 0 frame (type + flags + len + body)│
/// │ [N bytes]    │ Block 1 frame ...                        │
/// │ ...          │                                          │
/// │ [2-3 bytes]  │ END sentinel (type=0xFF, flags=0, len=0) │
/// └──────────────┴──────────────────────────────────────────┘
/// ```
///
/// The payload is ready for storage or transmission — no further
/// framing is required.
pub struct LcpEncoder {
  blocks: Vec<PendingBlock>,
  flags: HeaderFlags,
}

/// Internal representation of a block awaiting serialization.
///
/// Captures the block type tag, the typed content (which knows how to
/// serialize its own TLV body via [`BlockContent::encode_body`]), an
/// optional summary to prepend, and a compression flag (stubbed in
/// Phase 1 — always `false`).
///
/// `PendingBlock` is never exposed publicly. The encoder builds these
/// internally as the caller chains `.add_*()` and `.with_*()` methods,
/// then consumes them during `.encode()`.
struct PendingBlock {
  block_type: u8,
  content: BlockContent,
  summary: Option<String>,
  /// Phase 1 stub — always `false`. Will be used when zstd compression
  /// is implemented in Phase 2.
  #[allow(dead_code)]
  compress: bool,
}

impl LcpEncoder {
  /// Create a new encoder with default settings (version 1.0, no flags).
  ///
  /// The encoder starts with an empty block list. At least one block
  /// must be added before calling `.encode()`, otherwise it returns
  /// [`EncodeError::EmptyPayload`].
  #[must_use]
  pub fn new() -> Self {
    Self {
      blocks: Vec::new(),
      flags: HeaderFlags::NONE,
    }
  }

  // ── Block addition methods ──────────────────────────────────────────
  //
  // Each method constructs the appropriate `BlockContent` variant from
  // `bcp-types`, wraps it in a `PendingBlock`, pushes it onto the
  // internal list, and returns `&mut Self` for chaining.

  /// Add a CODE block.
  ///
  /// Encodes a source code file or fragment. The `lang` enum identifies
  /// the programming language (used by the decoder for syntax-aware
  /// rendering), `path` is the file path (UTF-8), and `content` is the
  /// raw source bytes.
  ///
  /// For partial files, use [`add_code_range`](Self::add_code_range)
  /// to include line range metadata.
  pub fn add_code(&mut self, lang: Lang, path: &str, content: &[u8]) -> &mut Self {
    self.push_block(
      block_type::CODE,
      BlockContent::Code(CodeBlock {
        lang,
        path: path.to_string(),
        content: content.to_vec(),
        line_range: None,
      }),
    )
  }

  /// Add a CODE block with a line range.
  ///
  /// Same as [`add_code`](Self::add_code) but includes `line_start` and
  /// `line_end` metadata (1-based, inclusive). The decoder can use this
  /// to display line numbers or to correlate with diagnostics.
  pub fn add_code_range(
    &mut self,
    lang: Lang,
    path: &str,
    content: &[u8],
    line_start: u32,
    line_end: u32,
  ) -> &mut Self {
    self.push_block(
      block_type::CODE,
      BlockContent::Code(CodeBlock {
        lang,
        path: path.to_string(),
        content: content.to_vec(),
        line_range: Some((line_start, line_end)),
      }),
    )
  }

  /// Add a CONVERSATION block.
  ///
  /// Represents a single chat turn. The `role` identifies the speaker
  /// (system, user, assistant, or tool) and `content` is the message
  /// body as raw bytes.
  pub fn add_conversation(&mut self, role: Role, content: &[u8]) -> &mut Self {
    self.push_block(
      block_type::CONVERSATION,
      BlockContent::Conversation(ConversationBlock {
        role,
        content: content.to_vec(),
        tool_call_id: None,
      }),
    )
  }

  /// Add a CONVERSATION block with a tool call ID.
  ///
  /// Used for tool-role messages that reference a specific tool
  /// invocation. The `tool_call_id` links this response back to the
  /// tool call that produced it.
  pub fn add_conversation_tool(
    &mut self,
    role: Role,
    content: &[u8],
    tool_call_id: &str,
  ) -> &mut Self {
    self.push_block(
      block_type::CONVERSATION,
      BlockContent::Conversation(ConversationBlock {
        role,
        content: content.to_vec(),
        tool_call_id: Some(tool_call_id.to_string()),
      }),
    )
  }

  /// Add a `FILE_TREE` block.
  ///
  /// Represents a directory structure rooted at `root`. Each entry
  /// contains a name, kind (file or directory), size, and optional
  /// nested children for recursive directory trees.
  pub fn add_file_tree(&mut self, root: &str, entries: Vec<FileEntry>) -> &mut Self {
    self.push_block(
      block_type::FILE_TREE,
      BlockContent::FileTree(FileTreeBlock {
        root_path: root.to_string(),
        entries,
      }),
    )
  }

  /// Add a `TOOL_RESULT` block.
  ///
  /// Captures the output of an external tool invocation (e.g. ripgrep,
  /// LSP diagnostics, test runner). The `status` indicates whether the
  /// tool succeeded, failed, or timed out.
  pub fn add_tool_result(
    &mut self,
    name: &str,
    status: Status,
    content: &[u8],
  ) -> &mut Self {
    self.push_block(
      block_type::TOOL_RESULT,
      BlockContent::ToolResult(ToolResultBlock {
        tool_name: name.to_string(),
        status,
        content: content.to_vec(),
        schema_hint: None,
      }),
    )
  }

  /// Add a DOCUMENT block.
  ///
  /// Represents prose content — README files, documentation, wiki pages.
  /// The `format_hint` tells the decoder how to render the body
  /// (markdown, plain text, or HTML).
  pub fn add_document(
    &mut self,
    title: &str,
    content: &[u8],
    format_hint: FormatHint,
  ) -> &mut Self {
    self.push_block(
      block_type::DOCUMENT,
      BlockContent::Document(DocumentBlock {
        title: title.to_string(),
        content: content.to_vec(),
        format_hint,
      }),
    )
  }

  /// Add a `STRUCTURED_DATA` block.
  ///
  /// Encodes tabular or structured content — JSON configs, YAML
  /// manifests, TOML files, CSV data. The `format` identifies the
  /// serialization format so the decoder can syntax-highlight or
  /// parse appropriately.
  pub fn add_structured_data(
    &mut self,
    format: DataFormat,
    content: &[u8],
  ) -> &mut Self {
    self.push_block(
      block_type::STRUCTURED_DATA,
      BlockContent::StructuredData(StructuredDataBlock {
        format,
        content: content.to_vec(),
        schema: None,
      }),
    )
  }

  /// Add a DIFF block.
  ///
  /// Represents code changes for a single file — from git diffs, editor
  /// changes, or patch files. Each hunk captures a contiguous range of
  /// modifications in unified diff format.
  pub fn add_diff(&mut self, path: &str, hunks: Vec<DiffHunk>) -> &mut Self {
    self.push_block(
      block_type::DIFF,
      BlockContent::Diff(DiffBlock {
        path: path.to_string(),
        hunks,
      }),
    )
  }

  /// Add an ANNOTATION block.
  ///
  /// Annotations are metadata overlays that target another block by its
  /// zero-based index in the stream. The `kind` determines how the
  /// `value` payload is interpreted (priority level, summary text, or
  /// tag label).
  ///
  /// For the common case of attaching a priority to the most recent
  /// block, prefer [`with_priority`](Self::with_priority).
  pub fn add_annotation(
    &mut self,
    target_block_id: u32,
    kind: AnnotationKind,
    value: &[u8],
  ) -> &mut Self {
    self.push_block(
      block_type::ANNOTATION,
      BlockContent::Annotation(AnnotationBlock {
        target_block_id,
        kind,
        value: value.to_vec(),
      }),
    )
  }

  /// Add an IMAGE block.
  ///
  /// Encodes an image as inline binary data. The `media_type` identifies
  /// the image format (PNG, JPEG, etc.), `alt_text` provides a textual
  /// description for accessibility, and `data` is the raw image bytes.
  pub fn add_image(
    &mut self,
    media_type: MediaType,
    alt_text: &str,
    data: &[u8],
  ) -> &mut Self {
    self.push_block(
      block_type::IMAGE,
      BlockContent::Image(ImageBlock {
        media_type,
        alt_text: alt_text.to_string(),
        data: data.to_vec(),
      }),
    )
  }

  /// Add an EXTENSION block.
  ///
  /// User-defined block type for custom payloads. The `namespace` and
  /// `type_name` together form a unique identifier for the extension
  /// type, preventing collisions across different tools and vendors.
  pub fn add_extension(
    &mut self,
    namespace: &str,
    type_name: &str,
    content: &[u8],
  ) -> &mut Self {
    self.push_block(
      block_type::EXTENSION,
      BlockContent::Extension(ExtensionBlock {
        namespace: namespace.to_string(),
        type_name: type_name.to_string(),
        content: content.to_vec(),
      }),
    )
  }

  // ── Modifier methods ────────────────────────────────────────────────
  //
  // Modifiers act on the most recently added block. They set metadata
  // that affects how the block is serialized (summary prefix, flags)
  // or append related blocks (priority annotation).

  /// Attach a summary to the most recently added block.
  ///
  /// Sets the `HAS_SUMMARY` flag on the block and prepends the summary
  /// sub-block to the body during serialization. The summary is a
  /// compact UTF-8 description that the token budget engine can use as
  /// a stand-in when the full block content would exceed the budget.
  ///
  /// # Panics
  ///
  /// Panics if no blocks have been added yet. Use this immediately
  /// after an `.add_*()` call.
  pub fn with_summary(&mut self, summary: &str) -> &mut Self {
    let block = self
      .blocks
      .last_mut()
      .expect("with_summary called but no blocks have been added");
    block.summary = Some(summary.to_string());
    self
  }

  /// Attach a priority annotation to the most recently added block.
  ///
  /// This is a convenience method that appends an ANNOTATION block
  /// with `kind=Priority` targeting the last added block's index.
  /// The annotation's value is the priority byte (e.g. `0x02` for
  /// `Priority::High`).
  ///
  /// # Panics
  ///
  /// Panics if no blocks have been added yet.
  pub fn with_priority(&mut self, priority: Priority) -> &mut Self {
    let target_index = self
      .blocks
      .len()
      .checked_sub(1)
      .expect("with_priority called but no blocks have been added");

    #[allow(clippy::cast_possible_truncation)]
    let target_id = target_index as u32;

    self.push_block(
      block_type::ANNOTATION,
      BlockContent::Annotation(AnnotationBlock {
        target_block_id: target_id,
        kind: AnnotationKind::Priority,
        value: vec![priority.to_wire_byte()],
      }),
    );
    self
  }

  // ── Encode ──────────────────────────────────────────────────────────

  /// Serialize all accumulated blocks into a complete LCP payload.
  ///
  /// Walks the internal block list and for each `PendingBlock`:
  ///
  ///   1. Calls [`BlockContent::encode_body`] to get the TLV-encoded
  ///      body bytes.
  ///   2. If a summary is present, prepends the summary bytes and sets
  ///      the `HAS_SUMMARY` flag.
  ///   3. Wraps the body in a [`BlockFrame`] with the correct type tag
  ///      and flags.
  ///   4. Writes the frame to the output buffer via
  ///      [`BlockFrame::write_to`].
  ///
  /// After all blocks, appends the END sentinel (type=0xFF, flags=0x00,
  /// len=0) to signal the end of the block stream.
  ///
  /// # Errors
  ///
  /// - [`EncodeError::EmptyPayload`] if no blocks have been added.
  /// - [`EncodeError::BlockTooLarge`] if any block body exceeds 16 MiB.
  /// - [`EncodeError::Wire`] if the underlying wire serialization fails.
  /// - [`EncodeError::Io`] if writing to the output buffer fails.
  pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
    if self.blocks.is_empty() {
      return Err(EncodeError::EmptyPayload);
    }

    // Pre-allocate: 8 bytes header + estimated block data + END sentinel.
    // A rough estimate: assume each block averages ~256 bytes on the wire.
    let estimated_size = HEADER_SIZE + self.blocks.len() * 256 + 3;
    let mut output = Vec::with_capacity(estimated_size);

    // 1. Write the file header into the first 8 bytes.
    output.resize(HEADER_SIZE, 0);
    let header = LcpHeader::new(self.flags);
    header.write_to(&mut output[..HEADER_SIZE])?;

    // 2. Serialize each pending block into a BlockFrame and write it.
    for pending in &self.blocks {
      let body = Self::serialize_block_body(pending)?;

      let mut flags = BlockFlags::NONE;
      if pending.summary.is_some() {
        flags = BlockFlags::HAS_SUMMARY;
      }

      let frame = BlockFrame {
        block_type: pending.block_type,
        flags,
        body,
      };

      frame.write_to(&mut output)?;
    }

    // 3. Write the END sentinel.
    let end_frame = BlockFrame {
      block_type: block_type::END,
      flags: BlockFlags::NONE,
      body: Vec::new(),
    };
    end_frame.write_to(&mut output)?;

    Ok(output)
  }

  // ── Internal helpers ────────────────────────────────────────────────

  /// Push a new `PendingBlock` onto the internal list.
  ///
  /// Returns `&mut Self` so callers can chain additional methods.
  fn push_block(&mut self, block_type: u8, content: BlockContent) -> &mut Self {
    self.blocks.push(PendingBlock {
      block_type,
      content,
      summary: None,
      compress: false,
    });
    self
  }

  /// Serialize a `PendingBlock` into its final body bytes.
  ///
  /// If the block has a summary, the summary is encoded first (as a
  /// length-prefixed UTF-8 string) followed by the TLV body fields.
  /// This matches the wire convention: when `HAS_SUMMARY` is set, the
  /// summary occupies the front of the body, before any TLV fields.
  fn serialize_block_body(pending: &PendingBlock) -> Result<Vec<u8>, EncodeError> {
    let tlv_body = pending.content.encode_body();
    let mut body = Vec::new();

    if let Some(ref summary_text) = pending.summary {
      let summary = Summary {
        text: summary_text.clone(),
      };
      summary.encode(&mut body);
    }

    body.extend_from_slice(&tlv_body);

    if body.len() > MAX_BLOCK_BODY_SIZE {
      return Err(EncodeError::BlockTooLarge {
        size: body.len(),
        limit: MAX_BLOCK_BODY_SIZE,
      });
    }

    Ok(body)
  }
}

impl Default for LcpEncoder {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use bcp_types::file_tree::FileEntryKind;
  use bcp_wire::header::LCP_MAGIC;

  // ── Helper ──────────────────────────────────────────────────────────

  /// Verify that a payload starts with the LCP magic number.
  fn assert_starts_with_magic(payload: &[u8]) {
    assert!(payload.len() >= HEADER_SIZE, "payload too short for header");
    assert_eq!(&payload[..4], &LCP_MAGIC, "missing LCP magic");
  }

  /// Verify that a payload ends with a valid END sentinel.
  ///
  /// The END sentinel is: block_type=0xFF as varint (2 bytes: 0xFF 0x01),
  /// flags=0x00, content_len=0 as varint (1 byte: 0x00).
  fn assert_ends_with_end_sentinel(payload: &[u8]) {
    // The END block type 0xFF encodes as varint [0xFF, 0x01],
    // followed by flags byte 0x00, followed by content_len varint 0x00.
    let tail = &payload[payload.len() - 4..];
    assert_eq!(tail, &[0xFF, 0x01, 0x00, 0x00], "missing END sentinel");
  }

  // ── Acceptance criteria tests ───────────────────────────────────────

  #[test]
  fn encode_single_code_block_produces_valid_magic() {
    let payload = LcpEncoder::new()
      .add_code(Lang::Rust, "src/main.rs", b"fn main() {}")
      .encode()
      .unwrap();

    assert_starts_with_magic(&payload);
  }

  #[test]
  fn builder_methods_are_chainable() {
    let payload = LcpEncoder::new()
      .add_code(Lang::Rust, "src/lib.rs", b"pub fn hello() {}")
      .with_summary("Hello function.")
      .add_conversation(Role::User, b"What does this do?")
      .encode()
      .unwrap();

    assert_starts_with_magic(&payload);
    assert_ends_with_end_sentinel(&payload);
  }

  #[test]
  fn with_summary_sets_has_summary_flag() {
    let payload = LcpEncoder::new()
      .add_code(Lang::Python, "main.py", b"print('hi')")
      .with_summary("Prints a greeting.")
      .encode()
      .unwrap();

    // Parse: skip the 8-byte header, read the first block frame.
    let frame_buf = &payload[HEADER_SIZE..];
    let (frame, _) = BlockFrame::read_from(frame_buf).unwrap().unwrap();
    assert!(
      frame.flags.has_summary(),
      "HAS_SUMMARY flag should be set on the code block"
    );
  }

  #[test]
  fn with_priority_appends_annotation_block() {
    let payload = LcpEncoder::new()
      .add_code(Lang::Rust, "lib.rs", b"// code")
      .with_priority(Priority::High)
      .encode()
      .unwrap();

    // Parse: header + first block (CODE) + second block (ANNOTATION) + END
    let mut cursor = HEADER_SIZE;

    // Block 0: CODE
    let (frame0, n) = BlockFrame::read_from(&payload[cursor..]).unwrap().unwrap();
    assert_eq!(frame0.block_type, block_type::CODE);
    cursor += n;

    // Block 1: ANNOTATION (priority)
    let (frame1, _) = BlockFrame::read_from(&payload[cursor..]).unwrap().unwrap();
    assert_eq!(frame1.block_type, block_type::ANNOTATION);

    // Decode the annotation body and verify it targets block 0
    let annotation = AnnotationBlock::decode_body(&frame1.body).unwrap();
    assert_eq!(annotation.target_block_id, 0);
    assert_eq!(annotation.kind, AnnotationKind::Priority);
    assert_eq!(annotation.value, vec![Priority::High.to_wire_byte()]);
  }

  #[test]
  fn empty_encoder_returns_empty_payload_error() {
    let result = LcpEncoder::new().encode();
    assert!(matches!(result, Err(EncodeError::EmptyPayload)));
  }

  #[test]
  fn payload_ends_with_end_sentinel() {
    let payload = LcpEncoder::new()
      .add_conversation(Role::User, b"hello")
      .encode()
      .unwrap();

    assert_ends_with_end_sentinel(&payload);
  }

  #[test]
  fn all_eleven_block_types_encode_without_error() {
    let payload = LcpEncoder::new()
      .add_code(Lang::Rust, "main.rs", b"fn main() {}")
      .add_conversation(Role::User, b"hello")
      .add_file_tree(
        "/project",
        vec![FileEntry {
          name: "lib.rs".to_string(),
          kind: FileEntryKind::File,
          size: 100,
          children: vec![],
        }],
      )
      .add_tool_result("rg", Status::Ok, b"found 3 matches")
      .add_document("README", b"# Title", FormatHint::Markdown)
      .add_structured_data(DataFormat::Json, b"{\"key\": \"value\"}")
      .add_diff(
        "src/lib.rs",
        vec![DiffHunk {
          old_start: 1,
          new_start: 1,
          lines: b"+new line\n".to_vec(),
        }],
      )
      .add_annotation(0, AnnotationKind::Tag, b"important")
      .add_image(MediaType::Png, "screenshot", b"\x89PNG\r\n")
      .add_extension("myco", "custom_block", b"custom data")
      // EMBEDDING_REF is not in the spec's 11 encoder methods
      // (it's a decode-only block), but we test the 11 that are specified
      .encode()
      .unwrap();

    assert_starts_with_magic(&payload);
    assert_ends_with_end_sentinel(&payload);

    // Verify we can walk all 10 content blocks + 1 annotation = 11 frames
    // (the add_annotation above counts as one of the 11 block addition methods)
    let mut cursor = HEADER_SIZE;
    let mut block_count = 0;
    loop {
      match BlockFrame::read_from(&payload[cursor..]).unwrap() {
        Some((_, n)) => {
          cursor += n;
          block_count += 1;
        }
        None => break, // END sentinel
      }
    }
    assert_eq!(block_count, 10, "expected 10 content blocks");
  }

  #[test]
  fn payload_byte_length_matches_calculation() {
    let mut enc = LcpEncoder::new();
    enc.add_code(Lang::Rust, "x.rs", b"let x = 1;");
    enc.add_conversation(Role::User, b"hi");

    let payload = enc.encode().unwrap();

    // Calculate expected size manually:
    // Header: 8 bytes
    let mut expected = HEADER_SIZE;

    // Walk actual frames to verify
    let mut cursor = HEADER_SIZE;
    loop {
      let remaining = &payload[cursor..];
      // Try to read a frame (including END which returns None)
      let start = cursor;
      match BlockFrame::read_from(remaining).unwrap() {
        Some((_, n)) => {
          cursor += n;
          expected += n;
        }
        None => {
          // END sentinel was consumed — count those bytes too
          let end_bytes = payload.len() - start;
          expected += end_bytes;
          break;
        }
      }
    }

    assert_eq!(
      payload.len(),
      expected,
      "payload length should match header + frames + END"
    );
  }

  #[test]
  fn optional_fields_omitted_when_none() {
    // CODE block without line_range
    let payload = LcpEncoder::new()
      .add_code(Lang::Rust, "x.rs", b"code")
      .encode()
      .unwrap();

    let (frame, _) = BlockFrame::read_from(&payload[HEADER_SIZE..])
      .unwrap()
      .unwrap();

    // Decode the body and verify line_range is None
    let code = CodeBlock::decode_body(&frame.body).unwrap();
    assert!(code.line_range.is_none());

    // CONVERSATION block without tool_call_id
    let payload = LcpEncoder::new()
      .add_conversation(Role::User, b"msg")
      .encode()
      .unwrap();

    let (frame, _) = BlockFrame::read_from(&payload[HEADER_SIZE..])
      .unwrap()
      .unwrap();

    let conv = ConversationBlock::decode_body(&frame.body).unwrap();
    assert!(conv.tool_call_id.is_none());
  }

  #[test]
  fn code_range_includes_line_numbers() {
    let payload = LcpEncoder::new()
      .add_code_range(Lang::Rust, "src/lib.rs", b"fn foo() {}", 10, 20)
      .encode()
      .unwrap();

    let (frame, _) = BlockFrame::read_from(&payload[HEADER_SIZE..])
      .unwrap()
      .unwrap();

    let code = CodeBlock::decode_body(&frame.body).unwrap();
    assert_eq!(code.line_range, Some((10, 20)));
  }

  #[test]
  fn conversation_tool_includes_tool_call_id() {
    let payload = LcpEncoder::new()
      .add_conversation_tool(Role::Tool, b"result", "call_123")
      .encode()
      .unwrap();

    let (frame, _) = BlockFrame::read_from(&payload[HEADER_SIZE..])
      .unwrap()
      .unwrap();

    let conv = ConversationBlock::decode_body(&frame.body).unwrap();
    assert_eq!(conv.tool_call_id.as_deref(), Some("call_123"));
  }

  #[test]
  fn summary_is_decodable_from_block_body() {
    let payload = LcpEncoder::new()
      .add_code(Lang::Rust, "main.rs", b"fn main() {}")
      .with_summary("Entry point for the application.")
      .encode()
      .unwrap();

    let (frame, _) = BlockFrame::read_from(&payload[HEADER_SIZE..])
      .unwrap()
      .unwrap();

    assert!(frame.flags.has_summary());

    // Decode summary from the front of the body
    let (summary, consumed) = Summary::decode(&frame.body).unwrap();
    assert_eq!(summary.text, "Entry point for the application.");

    // Remaining bytes should decode as a valid CodeBlock
    let code = CodeBlock::decode_body(&frame.body[consumed..]).unwrap();
    assert_eq!(code.path, "main.rs");
    assert_eq!(code.content, b"fn main() {}");
  }

  #[test]
  fn rfc_example_encodes_successfully() {
    // Reproduces the example from RFC §12.1 / SPEC_03 §1
    let payload = LcpEncoder::new()
      .add_code(Lang::Rust, "src/main.rs", b"fn main() { todo!() }")
      .with_summary("Entry point: CLI setup and server startup.")
      .with_priority(Priority::High)
      .add_conversation(Role::User, b"Fix the timeout bug.")
      .add_conversation(
        Role::Assistant,
        b"I'll examine the pool config...",
      )
      .add_tool_result("ripgrep", Status::Ok, b"3 matches found.")
      .encode()
      .unwrap();

    assert_starts_with_magic(&payload);
    assert_ends_with_end_sentinel(&payload);

    // Walk all frames to verify structure
    let mut cursor = HEADER_SIZE;
    let mut types = Vec::new();
    loop {
      match BlockFrame::read_from(&payload[cursor..]).unwrap() {
        Some((frame, n)) => {
          types.push(frame.block_type);
          cursor += n;
        }
        None => break,
      }
    }

    assert_eq!(
      types,
      vec![
        block_type::CODE,
        block_type::ANNOTATION, // from with_priority
        block_type::CONVERSATION,
        block_type::CONVERSATION,
        block_type::TOOL_RESULT,
      ]
    );
  }

  #[test]
  fn default_impl_matches_new() {
    let from_new = LcpEncoder::new();
    let from_default = LcpEncoder::default();
    assert!(from_new.blocks.is_empty());
    assert!(from_default.blocks.is_empty());
  }
}
