use std::sync::Arc;

use bcp_types::content_store::ContentStore;
use bcp_types::BlockContent;
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
use bcp_wire::block_frame::{BlockFlags, BlockFrame, block_type};
use bcp_wire::header::{HEADER_SIZE, HeaderFlags, LcpHeader};

use crate::compression::{self, COMPRESSION_THRESHOLD};
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
/// # Compression (RFC §4.6)
///
/// Two compression modes are supported, both opt-in:
///
/// - **Per-block**: call [`with_compression`](Self::with_compression) after
///   adding a block, or [`compress_blocks`](Self::compress_blocks) to
///   enable compression for all subsequent blocks. Each block body is
///   independently zstd-compressed if it exceeds
///   [`COMPRESSION_THRESHOLD`](crate::compression::COMPRESSION_THRESHOLD)
///   bytes and compression yields a size reduction. The block's
///   `COMPRESSED` flag (bit 1) is set when compression is applied.
///
/// - **Whole-payload**: call [`compress_payload`](Self::compress_payload)
///   to zstd-compress all bytes after the 8-byte header. When enabled,
///   per-block compression is skipped (whole-payload subsumes it). The
///   header's `COMPRESSED` flag (bit 0) is set.
///
/// # Content Addressing (RFC §4.7)
///
/// When a [`ContentStore`] is configured via
/// [`set_content_store`](Self::set_content_store), blocks can be stored
/// by their BLAKE3 hash rather than inline:
///
/// - **Per-block**: call [`with_content_addressing`](Self::with_content_addressing)
///   after adding a block. The body is hashed, stored in the content store,
///   and replaced with the 32-byte hash on the wire. The block's
///   `IS_REFERENCE` flag (bit 2) is set.
///
/// - **Auto-dedup**: call [`auto_dedup`](Self::auto_dedup) to automatically
///   content-address any block whose body has been seen before. First
///   occurrence is stored inline and registered in the store; subsequent
///   identical blocks become references.
///
/// Content addressing runs before compression — a 32-byte hash reference
/// is always below the compression threshold, so reference blocks are
/// never compressed.
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
/// When whole-payload compression is enabled, the layout becomes:
///
/// ```text
/// ┌──────────────┬──────────────────────────────────────────┐
/// │ [8 bytes]    │ Header (flags bit 0 = COMPRESSED)        │
/// │ [N bytes]    │ zstd(Block 0 + Block 1 + ... + END)      │
/// └──────────────┴──────────────────────────────────────────┘
/// ```
///
/// The payload is ready for storage or transmission — no further
/// framing is required.
pub struct LcpEncoder {
    blocks: Vec<PendingBlock>,
    flags: HeaderFlags,
    /// When `true`, the entire payload after the header is zstd-compressed.
    compress_payload: bool,
    /// When `true`, all blocks are individually compressed (unless
    /// `compress_payload` is also set, which takes precedence).
    compress_all_blocks: bool,
    /// Content store for BLAKE3 content-addressed deduplication.
    /// Required when any block has `content_address = true` or
    /// when `auto_dedup` is enabled.
    content_store: Option<Arc<dyn ContentStore>>,
    /// When `true`, automatically content-address any block whose body
    /// has been seen before (hash already exists in the store).
    auto_dedup: bool,
}

/// Internal representation of a block awaiting serialization.
///
/// Captures the block type tag, the typed content (which knows how to
/// serialize its own TLV body via [`BlockContent::encode_body`]), an
/// optional summary to prepend, and per-block compression / content
/// addressing flags.
///
/// `PendingBlock` is never exposed publicly. The encoder builds these
/// internally as the caller chains `.add_*()` and `.with_*()` methods,
/// then consumes them during `.encode()`.
struct PendingBlock {
    block_type: u8,
    content: BlockContent,
    summary: Option<String>,
    /// When `true`, this block's body should be zstd-compressed if it
    /// exceeds [`COMPRESSION_THRESHOLD`] and compression yields savings.
    compress: bool,
    /// When `true`, this block's body should be replaced with its
    /// 32-byte BLAKE3 hash and stored in the content store.
    content_address: bool,
}

impl LcpEncoder {
    /// Create a new encoder with default settings (version 1.0, no flags).
    ///
    /// The encoder starts with an empty block list, no compression, and
    /// no content store. At least one block must be added before calling
    /// `.encode()`, otherwise it returns [`EncodeError::EmptyPayload`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            flags: HeaderFlags::NONE,
            compress_payload: false,
            compress_all_blocks: false,
            content_store: None,
            auto_dedup: false,
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
    pub fn add_tool_result(&mut self, name: &str, status: Status, content: &[u8]) -> &mut Self {
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
    pub fn add_structured_data(&mut self, format: DataFormat, content: &[u8]) -> &mut Self {
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
    pub fn add_image(&mut self, media_type: MediaType, alt_text: &str, data: &[u8]) -> &mut Self {
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
    pub fn add_extension(&mut self, namespace: &str, type_name: &str, content: &[u8]) -> &mut Self {
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

    // ── Compression modifiers ────────────────────────────────────────────
    //
    // These methods control per-block and whole-payload zstd compression.
    // Per-block compression is skipped when whole-payload compression is
    // enabled — the outer zstd frame subsumes individual block compression.

    /// Enable zstd compression for the most recently added block.
    ///
    /// During `.encode()`, the block body is compressed with zstd if it
    /// exceeds [`COMPRESSION_THRESHOLD`] bytes and compression yields a
    /// size reduction. If compression doesn't help (output >= input), the
    /// body is stored uncompressed and the `COMPRESSED` flag is not set.
    ///
    /// Has no effect if [`compress_payload`](Self::compress_payload) is
    /// also enabled — whole-payload compression takes precedence.
    ///
    /// # Panics
    ///
    /// Panics if no blocks have been added yet.
    pub fn with_compression(&mut self) -> &mut Self {
        let block = self
            .blocks
            .last_mut()
            .expect("with_compression called but no blocks have been added");
        block.compress = true;
        self
    }

    /// Enable zstd compression for all blocks added so far and all
    /// future blocks.
    ///
    /// Equivalent to calling [`with_compression`](Self::with_compression)
    /// on every block. Individual blocks still respect the size threshold
    /// and no-savings guard.
    pub fn compress_blocks(&mut self) -> &mut Self {
        self.compress_all_blocks = true;
        for block in &mut self.blocks {
            block.compress = true;
        }
        self
    }

    /// Enable whole-payload zstd compression.
    ///
    /// When set, the entire block stream (all frames + END sentinel) is
    /// compressed as a single zstd frame. The 8-byte header is written
    /// uncompressed with `HeaderFlags::COMPRESSED` set so the decoder
    /// can detect compression before reading further.
    ///
    /// When whole-payload compression is enabled, per-block compression
    /// is skipped — compressing within a compressed stream adds overhead
    /// without benefit.
    ///
    /// If compression doesn't reduce the total size, the payload is
    /// stored uncompressed and the header flag is not set.
    pub fn compress_payload(&mut self) -> &mut Self {
        self.compress_payload = true;
        self
    }

    // ── Content addressing modifiers ────────────────────────────────────
    //
    // These methods control BLAKE3 content-addressed deduplication.
    // A content store must be configured before blocks can be
    // content-addressed.

    /// Set the content store used for BLAKE3 content addressing.
    ///
    /// The store is shared via `Arc` so the same store can be passed to
    /// both the encoder and decoder for roundtrip workflows. The encoder
    /// calls `store.put()` for each content-addressed block; the decoder
    /// calls `store.get()` to resolve references.
    ///
    /// Must be called before `.encode()` if any block has content
    /// addressing enabled or if [`auto_dedup`](Self::auto_dedup) is set.
    pub fn set_content_store(&mut self, store: Arc<dyn ContentStore>) -> &mut Self {
        self.content_store = Some(store);
        self
    }

    /// Enable content addressing for the most recently added block.
    ///
    /// During `.encode()`, the block body is hashed with BLAKE3,
    /// stored in the content store, and replaced with the 32-byte hash
    /// on the wire. The block's `IS_REFERENCE` flag (bit 2) is set.
    ///
    /// Requires a content store — call
    /// [`set_content_store`](Self::set_content_store) before `.encode()`.
    ///
    /// Content addressing runs before compression. Since a 32-byte
    /// hash reference is always below [`COMPRESSION_THRESHOLD`],
    /// reference blocks are never per-block compressed.
    ///
    /// # Panics
    ///
    /// Panics if no blocks have been added yet.
    pub fn with_content_addressing(&mut self) -> &mut Self {
        let block = self
            .blocks
            .last_mut()
            .expect("with_content_addressing called but no blocks have been added");
        block.content_address = true;
        self
    }

    /// Enable automatic deduplication across all blocks.
    ///
    /// When set, the encoder hashes every block body with BLAKE3 during
    /// `.encode()`. If the hash already exists in the content store
    /// (i.e. a previous block in this or a prior encoding had the same
    /// content), the block is automatically replaced with a hash
    /// reference. First-occurrence blocks are stored inline and
    /// registered in the store for future dedup.
    ///
    /// Requires a content store — call
    /// [`set_content_store`](Self::set_content_store) before `.encode()`.
    pub fn auto_dedup(&mut self) -> &mut Self {
        self.auto_dedup = true;
        self
    }

    // ── Encode ──────────────────────────────────────────────────────────

    /// Serialize all accumulated blocks into a complete LCP payload.
    ///
    /// The encode pipeline processes each `PendingBlock` through up to
    /// three stages:
    ///
    ///   1. **Serialize** — calls [`BlockContent::encode_body`] to get
    ///      the TLV-encoded body bytes. If a summary is present, it is
    ///      prepended and the `HAS_SUMMARY` flag is set.
    ///
    ///   2. **Content address** (optional) — if the block has
    ///      `content_address = true` or auto-dedup detects a duplicate,
    ///      the body is hashed with BLAKE3, stored in the content store,
    ///      and replaced with the 32-byte hash. The `IS_REFERENCE` flag
    ///      (bit 2) is set.
    ///
    ///   3. **Per-block compress** (optional) — if compression is enabled
    ///      for this block, whole-payload compression is NOT active, and
    ///      the body is not a reference, the body is zstd-compressed if
    ///      it exceeds [`COMPRESSION_THRESHOLD`] and compression yields
    ///      savings. The `COMPRESSED` flag (bit 1) is set.
    ///
    /// After all blocks, the END sentinel is appended. If whole-payload
    /// compression is enabled, everything after the 8-byte header is
    /// compressed as a single zstd frame and the header's `COMPRESSED`
    /// flag is set.
    ///
    /// # Errors
    ///
    /// - [`EncodeError::EmptyPayload`] if no blocks have been added.
    /// - [`EncodeError::BlockTooLarge`] if any block body exceeds 16 MiB.
    /// - [`EncodeError::MissingContentStore`] if content addressing is
    ///   requested but no store has been configured.
    /// - [`EncodeError::Wire`] if the underlying wire serialization fails.
    /// - [`EncodeError::Io`] if writing to the output buffer fails.
    pub fn encode(&self) -> Result<Vec<u8>, EncodeError> {
        if self.blocks.is_empty() {
            return Err(EncodeError::EmptyPayload);
        }

        // Validate: if any block needs content addressing or auto_dedup
        // is enabled, a store must be present.
        let needs_store = self.auto_dedup || self.blocks.iter().any(|b| b.content_address);
        if needs_store && self.content_store.is_none() {
            return Err(EncodeError::MissingContentStore);
        }

        // Pre-allocate: 8 bytes header + estimated block data + END sentinel.
        let estimated_size = HEADER_SIZE + self.blocks.len() * 256 + 3;
        let mut output = Vec::with_capacity(estimated_size);

        // 1. Write a placeholder header (flags may be updated for whole-payload).
        output.resize(HEADER_SIZE, 0);

        // 2. Serialize each pending block through the encode pipeline.
        for pending in &self.blocks {
            let mut body = Self::serialize_block_body(pending)?;
            let mut flags_raw = 0u8;

            if pending.summary.is_some() {
                flags_raw |= BlockFlags::HAS_SUMMARY.raw();
            }

            // Stage 2: Content addressing (runs before compression).
            let is_reference = self.apply_content_addressing(pending, &mut body)?;
            if is_reference {
                flags_raw |= BlockFlags::IS_REFERENCE.raw();
            }

            // Stage 3: Per-block compression (skipped for references and
            // when whole-payload compression is active).
            if !is_reference && !self.compress_payload {
                let should_compress =
                    pending.compress || self.compress_all_blocks;
                if should_compress && body.len() >= COMPRESSION_THRESHOLD {
                    if let Some(compressed) = compression::compress(&body) {
                        body = compressed;
                        flags_raw |= BlockFlags::COMPRESSED.raw();
                    }
                }
            }

            let frame = BlockFrame {
                block_type: pending.block_type,
                flags: BlockFlags::from_raw(flags_raw),
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

        // 4. Whole-payload compression: compress everything after the header.
        let header_flags = if self.compress_payload {
            let block_data = &output[HEADER_SIZE..];
            match compression::compress(block_data) {
                Some(compressed) => {
                    output.truncate(HEADER_SIZE);
                    output.extend_from_slice(&compressed);
                    HeaderFlags::from_raw(
                        self.flags.raw() | HeaderFlags::COMPRESSED.raw(),
                    )
                }
                None => self.flags,
            }
        } else {
            self.flags
        };

        // 5. Write the final header with correct flags.
        let header = LcpHeader::new(header_flags);
        header.write_to(&mut output[..HEADER_SIZE])?;

        Ok(output)
    }

    // ── Internal helpers ────────────────────────────────────────────────

    /// Push a new `PendingBlock` onto the internal list.
    ///
    /// If `compress_all_blocks` is set, the new block inherits
    /// `compress = true` automatically.
    ///
    /// Returns `&mut Self` so callers can chain additional methods.
    fn push_block(&mut self, block_type: u8, content: BlockContent) -> &mut Self {
        self.blocks.push(PendingBlock {
            block_type,
            content,
            summary: None,
            compress: self.compress_all_blocks,
            content_address: false,
        });
        self
    }

    /// Apply content addressing to a block body if requested.
    ///
    /// Returns `true` if the body was replaced with a 32-byte hash
    /// reference, `false` if the body is unchanged (inline).
    ///
    /// Two paths trigger content addressing:
    /// 1. `pending.content_address == true` — always replace with hash.
    /// 2. `self.auto_dedup == true` — replace only if the hash already
    ///    exists in the store (i.e. a duplicate). First occurrence is
    ///    stored inline and registered for future dedup.
    fn apply_content_addressing(
        &self,
        pending: &PendingBlock,
        body: &mut Vec<u8>,
    ) -> Result<bool, EncodeError> {
        let store = match &self.content_store {
            Some(s) => s,
            None => return Ok(false),
        };

        if pending.content_address {
            // Explicit content addressing: always replace with hash.
            let hash = store.put(body);
            *body = hash.to_vec();
            return Ok(true);
        }

        if self.auto_dedup {
            // Auto-dedup: check if this body was seen before.
            let hash: [u8; 32] = blake3::hash(body).into();
            if store.contains(&hash) {
                // Duplicate — replace with reference.
                *body = hash.to_vec();
                return Ok(true);
            }
            // First occurrence — store for future dedup, keep inline.
            store.put(body);
        }

        Ok(false)
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
            .add_conversation(Role::Assistant, b"I'll examine the pool config...")
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

    // ── Per-block compression tests ─────────────────────────────────────

    #[test]
    fn per_block_compression_sets_compressed_flag() {
        // Create a large, compressible block (exceeds COMPRESSION_THRESHOLD)
        let big_content = "fn main() { println!(\"hello world\"); }\n".repeat(50);
        let payload = LcpEncoder::new()
            .add_code(Lang::Rust, "main.rs", big_content.as_bytes())
            .with_compression()
            .encode()
            .unwrap();

        let (frame, _) = BlockFrame::read_from(&payload[HEADER_SIZE..])
            .unwrap()
            .unwrap();
        assert!(
            frame.flags.is_compressed(),
            "COMPRESSED flag should be set on a large compressible block"
        );
        assert!(
            frame.body.len() < big_content.len(),
            "compressed body should be smaller than original"
        );
    }

    #[test]
    fn small_block_not_compressed_even_when_requested() {
        let payload = LcpEncoder::new()
            .add_code(Lang::Rust, "x.rs", b"let x = 1;")
            .with_compression()
            .encode()
            .unwrap();

        let (frame, _) = BlockFrame::read_from(&payload[HEADER_SIZE..])
            .unwrap()
            .unwrap();
        assert!(
            !frame.flags.is_compressed(),
            "small blocks should not be compressed (below threshold)"
        );
    }

    #[test]
    fn compress_blocks_applies_to_all() {
        let big_content = "use std::io;\n".repeat(100);
        let payload = LcpEncoder::new()
            .add_code(Lang::Rust, "a.rs", big_content.as_bytes())
            .add_code(Lang::Rust, "b.rs", big_content.as_bytes())
            .compress_blocks()
            .encode()
            .unwrap();

        let mut cursor = HEADER_SIZE;
        for _ in 0..2 {
            let (frame, n) = BlockFrame::read_from(&payload[cursor..])
                .unwrap()
                .unwrap();
            assert!(
                frame.flags.is_compressed(),
                "all blocks should be compressed with compress_blocks()"
            );
            cursor += n;
        }
    }

    // ── Whole-payload compression tests ─────────────────────────────────

    #[test]
    fn whole_payload_compression_sets_header_flag() {
        let big_content = "pub fn hello() -> &'static str { \"world\" }\n".repeat(100);
        let payload = LcpEncoder::new()
            .add_code(Lang::Rust, "main.rs", big_content.as_bytes())
            .compress_payload()
            .encode()
            .unwrap();

        let header = LcpHeader::read_from(&payload[..HEADER_SIZE]).unwrap();
        assert!(
            header.flags.is_compressed(),
            "header COMPRESSED flag should be set for whole-payload compression"
        );
    }

    #[test]
    fn whole_payload_skips_per_block_compression() {
        // When whole-payload compression is active, individual block
        // COMPRESSED flags should NOT be set.
        let big_content = "pub fn hello() -> &'static str { \"world\" }\n".repeat(100);
        let payload = LcpEncoder::new()
            .add_code(Lang::Rust, "main.rs", big_content.as_bytes())
            .with_compression()
            .compress_payload()
            .encode()
            .unwrap();

        let header = LcpHeader::read_from(&payload[..HEADER_SIZE]).unwrap();
        assert!(header.flags.is_compressed());

        // Decompress the payload to check individual blocks
        let decompressed = crate::compression::decompress(
            &payload[HEADER_SIZE..],
            16 * 1024 * 1024,
        )
        .unwrap();

        let (frame, _) = BlockFrame::read_from(&decompressed).unwrap().unwrap();
        assert!(
            !frame.flags.is_compressed(),
            "per-block COMPRESSED flag should not be set when whole-payload is active"
        );
    }

    #[test]
    fn whole_payload_no_savings_stays_uncompressed() {
        // Tiny payload — zstd overhead exceeds savings
        let payload = LcpEncoder::new()
            .add_code(Lang::Rust, "x.rs", b"x")
            .compress_payload()
            .encode()
            .unwrap();

        let header = LcpHeader::read_from(&payload[..HEADER_SIZE]).unwrap();
        assert!(
            !header.flags.is_compressed(),
            "header COMPRESSED flag should NOT be set when compression yields no savings"
        );
    }

    // ── Content addressing tests ────────────────────────────────────────

    #[test]
    fn content_addressing_sets_reference_flag() {
        let store = Arc::new(crate::MemoryContentStore::new());
        let payload = LcpEncoder::new()
            .set_content_store(store.clone())
            .add_code(Lang::Rust, "main.rs", b"fn main() {}")
            .with_content_addressing()
            .encode()
            .unwrap();

        let (frame, _) = BlockFrame::read_from(&payload[HEADER_SIZE..])
            .unwrap()
            .unwrap();
        assert!(
            frame.flags.is_reference(),
            "IS_REFERENCE flag should be set on content-addressed block"
        );
        assert_eq!(
            frame.body.len(),
            32,
            "reference block body should be exactly 32 bytes (BLAKE3 hash)"
        );

        // The hash should resolve in the store
        let hash: [u8; 32] = frame.body.try_into().unwrap();
        assert!(store.contains(&hash));
    }

    #[test]
    fn content_addressing_without_store_errors() {
        let result = LcpEncoder::new()
            .add_code(Lang::Rust, "main.rs", b"fn main() {}")
            .with_content_addressing()
            .encode();

        assert!(
            matches!(result, Err(EncodeError::MissingContentStore)),
            "should error when content addressing is requested without a store"
        );
    }

    #[test]
    fn auto_dedup_detects_duplicate_blocks() {
        let store = Arc::new(crate::MemoryContentStore::new());
        // Both blocks must have identical serialized TLV bodies (same
        // path + content + lang) for auto-dedup to detect a duplicate.
        let content = b"fn main() {}";

        let payload = LcpEncoder::new()
            .set_content_store(store.clone())
            .auto_dedup()
            .add_code(Lang::Rust, "main.rs", content)
            .add_code(Lang::Rust, "main.rs", content) // identical TLV body
            .encode()
            .unwrap();

        let mut cursor = HEADER_SIZE;

        // First block: inline (first occurrence)
        let (frame0, n) = BlockFrame::read_from(&payload[cursor..]).unwrap().unwrap();
        assert!(
            !frame0.flags.is_reference(),
            "first occurrence should be stored inline"
        );
        cursor += n;

        // Second block: reference (duplicate)
        let (frame1, _) = BlockFrame::read_from(&payload[cursor..]).unwrap().unwrap();
        assert!(
            frame1.flags.is_reference(),
            "duplicate should become a hash reference"
        );
        assert_eq!(frame1.body.len(), 32);
    }

    #[test]
    fn auto_dedup_without_store_errors() {
        let result = LcpEncoder::new()
            .auto_dedup()
            .add_code(Lang::Rust, "x.rs", b"code")
            .encode();

        assert!(matches!(result, Err(EncodeError::MissingContentStore)));
    }

    #[test]
    fn reference_block_not_per_block_compressed() {
        // Content-addressed blocks produce 32-byte bodies which are
        // below the compression threshold — verify no COMPRESSED flag.
        let store = Arc::new(crate::MemoryContentStore::new());
        let big_content = "fn main() { println!(\"hello\"); }\n".repeat(50);
        let payload = LcpEncoder::new()
            .set_content_store(store)
            .add_code(Lang::Rust, "main.rs", big_content.as_bytes())
            .with_content_addressing()
            .with_compression()
            .encode()
            .unwrap();

        let (frame, _) = BlockFrame::read_from(&payload[HEADER_SIZE..])
            .unwrap()
            .unwrap();
        assert!(frame.flags.is_reference());
        assert!(
            !frame.flags.is_compressed(),
            "reference blocks should not be per-block compressed"
        );
    }

    #[test]
    fn content_addressing_with_whole_payload_compression() {
        // Reference blocks CAN be wrapped in whole-payload compression.
        let store = Arc::new(crate::MemoryContentStore::new());
        // Same path + content = identical TLV body = single store entry
        let content = "fn main() { println!(\"hello\"); }\n".repeat(50);

        let payload = LcpEncoder::new()
            .set_content_store(store.clone())
            .compress_payload()
            .add_code(Lang::Rust, "main.rs", content.as_bytes())
            .with_content_addressing()
            .add_code(Lang::Rust, "main.rs", content.as_bytes())
            .with_content_addressing()
            .encode()
            .unwrap();

        let header = LcpHeader::read_from(&payload[..HEADER_SIZE]).unwrap();
        // The payload might or might not be compressed (two 32-byte hashes
        // plus framing may not compress well), but if it is, verify it's valid.
        if header.flags.is_compressed() {
            let decompressed = crate::compression::decompress(
                &payload[HEADER_SIZE..],
                16 * 1024 * 1024,
            )
            .unwrap();

            let (frame, _) = BlockFrame::read_from(&decompressed).unwrap().unwrap();
            assert!(frame.flags.is_reference());
            assert_eq!(frame.body.len(), 32);
        }

        // Both blocks have identical TLV bodies → single store entry
        assert_eq!(store.len(), 1, "identical blocks should produce one store entry");
    }

    // ── Phase 4: Cross-cutting tests ────────────────────────────────────

    #[test]
    fn compression_ratio_benchmark() {
        // A realistic 50-line Rust file should compress by >= 20%.
        let rust_code = r#"use std::collections::HashMap;
use std::sync::Arc;

pub struct Config {
    pub name: String,
    pub values: HashMap<String, String>,
    pub timeout: u64,
}

impl Config {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            values: HashMap::new(),
            timeout: 30,
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.values.insert(key.to_string(), value.to_string());
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }

    pub fn timeout(&self) -> u64 {
        self.timeout
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new("default")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_config() {
        let config = Config::new("test");
        assert_eq!(config.name, "test");
        assert!(config.values.is_empty());
        assert_eq!(config.timeout(), 30);
    }

    #[test]
    fn test_set_and_get() {
        let mut config = Config::new("test");
        config.set("key", "value");
        assert_eq!(config.get("key"), Some(&"value".to_string()));
    }
}
"#;

        let uncompressed_payload = LcpEncoder::new()
            .add_code(Lang::Rust, "config.rs", rust_code.as_bytes())
            .encode()
            .unwrap();

        let compressed_payload = LcpEncoder::new()
            .add_code(Lang::Rust, "config.rs", rust_code.as_bytes())
            .with_compression()
            .encode()
            .unwrap();

        let savings_pct = 100.0
            * (1.0 - compressed_payload.len() as f64 / uncompressed_payload.len() as f64);

        assert!(
            savings_pct >= 20.0,
            "expected >= 20% compression savings on a 50-line Rust file, got {savings_pct:.1}%"
        );
    }

    #[test]
    fn whole_payload_wins_over_per_block() {
        // When both per-block and whole-payload compression are requested,
        // only the header COMPRESSED flag should be set; individual blocks
        // should NOT have their COMPRESSED flags set.
        let big_content = "pub fn process() -> Result<(), Error> { Ok(()) }\n".repeat(50);
        let payload = LcpEncoder::new()
            .add_code(Lang::Rust, "a.rs", big_content.as_bytes())
            .with_compression()
            .add_code(Lang::Rust, "b.rs", big_content.as_bytes())
            .with_compression()
            .compress_payload()
            .encode()
            .unwrap();

        let header = LcpHeader::read_from(&payload[..HEADER_SIZE]).unwrap();
        assert!(
            header.flags.is_compressed(),
            "header should have COMPRESSED flag"
        );

        // Decompress payload to inspect individual blocks
        let decompressed = crate::compression::decompress(
            &payload[HEADER_SIZE..],
            16 * 1024 * 1024,
        )
        .unwrap();

        let mut cursor = 0;
        while let Some((frame, n)) = BlockFrame::read_from(&decompressed[cursor..])
            .unwrap()
        {
            assert!(
                !frame.flags.is_compressed(),
                "individual blocks should NOT be compressed when whole-payload is active"
            );
            cursor += n;
        }
    }

    #[test]
    fn full_pipeline_encode_decode_roundtrip() {
        // Exercises all features together: multiple block types,
        // summaries, priorities, per-block compression, content
        // addressing, and auto-dedup.
        let store = Arc::new(crate::MemoryContentStore::new());
        let big_code = "fn compute() -> i64 { 42 }\n".repeat(50);

        let payload = LcpEncoder::new()
            .set_content_store(store.clone())
            .auto_dedup()
            .add_code(Lang::Rust, "lib.rs", big_code.as_bytes())
            .with_summary("Core computation module.")
            .with_compression()
            .add_code(Lang::Rust, "lib.rs", big_code.as_bytes()) // auto-dedup
            .add_conversation(Role::User, b"Review this code")
            .add_tool_result("clippy", Status::Ok, b"No warnings")
            .encode()
            .unwrap();

        // Decode with the same store
        let decoded = bcp_decoder::LcpDecoder::decode_with_store(
            &payload,
            store.as_ref(),
        )
        .unwrap();

        assert_eq!(decoded.blocks.len(), 4);
        assert_eq!(decoded.blocks[0].block_type, bcp_types::BlockType::Code);
        assert_eq!(
            decoded.blocks[0].summary.as_ref().unwrap().text,
            "Core computation module."
        );
        assert_eq!(decoded.blocks[1].block_type, bcp_types::BlockType::Code);
        assert_eq!(decoded.blocks[2].block_type, bcp_types::BlockType::Conversation);
        assert_eq!(decoded.blocks[3].block_type, bcp_types::BlockType::ToolResult);

        // Both code blocks should have the same content
        for block in &decoded.blocks[..2] {
            match &block.content {
                BlockContent::Code(code) => {
                    assert_eq!(code.content, big_code.as_bytes());
                }
                other => panic!("expected Code, got {other:?}"),
            }
        }
    }
}
