use bcp_types::block::{Block, BlockContent};
use bcp_types::enums::{DataFormat, FormatHint, MediaType, Role, Status};
use bcp_types::file_tree::{FileEntry, FileEntryKind};

use crate::budget::RenderDecision;
use crate::config::OutputMode;
use crate::error::DriverError;
use crate::placeholder::render_placeholder;

/// XML-tagged renderer — emits `<context>`-wrapped XML elements.
///
/// This renderer produces semantic XML output optimized for Claude-family
/// models, which have strong XML comprehension. Each block type maps to
/// a specific XML element with descriptive attributes.
///
/// The complete output is wrapped in a `<context>` root element, and each
/// block is separated by a blank line for readability.
///
/// Per RFC §5.4 and §12.3, the XML mode wraps blocks in semantic elements:
///
/// ```text
/// <context>
/// <code lang="rust" path="src/main.rs">
/// fn main() {
///     let config = Config::load()?;
/// }
/// </code>
///
/// <tool name="ripgrep" status="ok">
/// 3 matches for 'ConnectionPool' across 2 files.
/// </tool>
///
/// <turn role="user">Fix the connection timeout bug.</turn>
/// <turn role="assistant">I'll examine the pool config...</turn>
/// </context>
/// ```
///
/// Element mapping:
///
/// ```text
/// ┌───────────────────┬──────────────────────────────────────────┐
/// │ Block Type        │ XML Element                              │
/// ├───────────────────┼──────────────────────────────────────────┤
/// │ Code              │ <code lang="X" path="Y">...</code>      │
/// │ Conversation      │ <turn role="X">...</turn>                │
/// │ FileTree          │ <tree root="X">...</tree>                │
/// │ ToolResult        │ <tool name="X" status="Y">...</tool>     │
/// │ Document          │ <doc title="X" format="Y">...</doc>      │
/// │ StructuredData    │ <data format="X">...</data>              │
/// │ Diff              │ <diff path="X">...</diff>                │
/// │ Annotation        │ (not rendered — metadata only)           │
/// │ EmbeddingRef      │ <embed-ref model="X" />                  │
/// │ Image             │ <image type="X" alt="Y">...</image>      │
/// │ Extension         │ <ext ns="X" type="Y">...</ext>           │
/// └───────────────────┴──────────────────────────────────────────┘
/// ```
pub struct XmlRenderer;

impl XmlRenderer {
    /// Render a filtered slice of blocks into a complete XML document.
    ///
    /// Wraps all rendered blocks in `<context>...</context>` with blank-line
    /// separators between blocks.
    ///
    /// # Errors
    ///
    /// Returns `DriverError::InvalidContent` if any block contains
    /// non-UTF-8 content bytes.
    pub fn render_all(blocks: &[&Block]) -> Result<String, DriverError> {
        let mut parts = Vec::with_capacity(blocks.len());
        for (i, block) in blocks.iter().enumerate() {
            parts.push(Self::render_block(block, i)?);
        }
        let inner = parts.join("\n\n");
        Ok(format!("<context>\n{inner}\n</context>"))
    }

    /// Render a filtered slice of blocks with per-block render decisions.
    ///
    /// This is the budget-aware entry point. Each block is paired with a
    /// [`RenderDecision`] that determines how it should be rendered:
    /// - `Full`: render complete content (ignore any attached summary)
    /// - `Summary`: render summary text only
    /// - `Placeholder`: emit a compact omission notice
    /// - `Omit`: skip the block entirely
    ///
    /// Wraps all rendered blocks in `<context>...</context>`.
    ///
    /// # Errors
    ///
    /// Returns `DriverError::EmptyInput` if all blocks are omitted.
    /// Returns `DriverError::InvalidContent` if any block contains
    /// non-UTF-8 content bytes.
    pub fn render_all_with_decisions(
        items: &[(&Block, &RenderDecision)],
    ) -> Result<String, DriverError> {
        let mut parts = Vec::with_capacity(items.len());
        for (i, (block, decision)) in items.iter().enumerate() {
            match decision {
                RenderDecision::Full => {
                    parts.push(Self::render_block_inner(block, i, false)?);
                }
                RenderDecision::Summary => {
                    parts.push(Self::render_block_inner(block, i, true)?);
                }
                RenderDecision::Placeholder {
                    block_type,
                    description,
                    omitted_tokens,
                } => {
                    parts.push(render_placeholder(
                        OutputMode::Xml,
                        block_type,
                        description,
                        *omitted_tokens,
                    ));
                }
                RenderDecision::Omit => {}
            }
        }
        if parts.is_empty() {
            return Err(DriverError::EmptyInput);
        }
        let inner = parts.join("\n\n");
        Ok(format!("<context>\n{inner}\n</context>"))
    }

    /// Render a single block to its XML element string.
    ///
    /// Returns the XML element without the outer `<context>` wrapper.
    /// The caller is responsible for joining multiple blocks and adding
    /// the root element.
    fn render_block(block: &Block, index: usize) -> Result<String, DriverError> {
        let use_summary = block.summary.is_some();
        Self::render_block_inner(block, index, use_summary)
    }

    /// Inner rendering logic shared by `render_block` and the
    /// decision-aware path.
    ///
    /// When `use_summary` is true and the block has a summary, the
    /// summary text replaces the block content. When false, the full
    /// content is always rendered regardless of summary presence.
    fn render_block_inner(
        block: &Block,
        index: usize,
        use_summary: bool,
    ) -> Result<String, DriverError> {
        let use_summary = use_summary && block.summary.is_some();

        match &block.content {
            BlockContent::Code(code) => {
                let lang = lang_display_name(code.lang);
                let content = if use_summary {
                    block.summary.as_ref().unwrap().text.clone()
                } else {
                    content_to_string(&code.content, index)?
                };
                if use_summary {
                    Ok(format!(
                        "<code lang=\"{lang}\" path=\"{}\" summary=\"true\">\n{content}\n</code>",
                        xml_escape(&code.path)
                    ))
                } else {
                    Ok(format!(
                        "<code lang=\"{lang}\" path=\"{}\">\n{content}\n</code>",
                        xml_escape(&code.path)
                    ))
                }
            }

            BlockContent::Conversation(conv) => {
                let role = role_display_name(conv.role);
                let content = content_to_string(&conv.content, index)?;
                Ok(format!("<turn role=\"{role}\">{content}</turn>"))
            }

            BlockContent::FileTree(tree) => {
                let rendered_tree = render_file_tree_entries(&tree.entries, 0);
                Ok(format!(
                    "<tree root=\"{}\">\n{rendered_tree}</tree>",
                    xml_escape(&tree.root_path)
                ))
            }

            BlockContent::ToolResult(tool) => {
                let status = status_display_name(tool.status);
                let content = content_to_string(&tool.content, index)?;
                Ok(format!(
                    "<tool name=\"{}\" status=\"{status}\">\n{content}\n</tool>",
                    xml_escape(&tool.tool_name)
                ))
            }

            BlockContent::Document(doc) => {
                let format = format_hint_display_name(doc.format_hint);
                let content = content_to_string(&doc.content, index)?;
                Ok(format!(
                    "<doc title=\"{}\" format=\"{format}\">\n{content}\n</doc>",
                    xml_escape(&doc.title)
                ))
            }

            BlockContent::StructuredData(data) => {
                let format = data_format_display_name(data.format);
                let content = content_to_string(&data.content, index)?;
                Ok(format!("<data format=\"{format}\">\n{content}\n</data>"))
            }

            BlockContent::Diff(diff) => {
                let mut lines = String::new();
                for hunk in &diff.hunks {
                    let hunk_content = String::from_utf8_lossy(&hunk.lines);
                    lines.push_str(&hunk_content);
                }
                Ok(format!(
                    "<diff path=\"{}\">\n{lines}</diff>",
                    xml_escape(&diff.path)
                ))
            }

            BlockContent::EmbeddingRef(emb) => Ok(format!(
                "<embed-ref model=\"{}\" />",
                xml_escape(&emb.model)
            )),

            BlockContent::Image(img) => {
                let media = media_type_display_name(img.media_type);
                let content = content_to_string(&img.data, index)?;
                Ok(format!(
                    "<image type=\"{media}\" alt=\"{}\">\n{content}\n</image>",
                    xml_escape(&img.alt_text)
                ))
            }

            BlockContent::Extension(ext) => {
                let content = content_to_string(&ext.content, index)?;
                Ok(format!(
                    "<ext ns=\"{}\" type=\"{}\">\n{content}\n</ext>",
                    xml_escape(&ext.namespace),
                    xml_escape(&ext.type_name)
                ))
            }

            // Annotation and End are filtered out by DefaultDriver before
            // reaching the renderer. Unknown blocks are rendered as comments.
            BlockContent::Annotation(_) | BlockContent::End => Ok(String::new()),

            BlockContent::Unknown { type_id, body } => {
                let content = String::from_utf8_lossy(body);
                Ok(format!(
                    "<!-- unknown block type 0x{type_id:02X} -->\n{content}"
                ))
            }
        }
    }
}

// ── Display name helpers ─────────────────────────────────────────────
//
// These convert enum variants to lowercase string representations for
// use in XML attributes, markdown headers, and minimal-mode delimiters.
// They are shared across all three renderers via pub(crate).

pub(crate) fn lang_display_name(lang: bcp_types::enums::Lang) -> &'static str {
    use bcp_types::enums::Lang;
    match lang {
        Lang::Rust => "rust",
        Lang::TypeScript => "typescript",
        Lang::JavaScript => "javascript",
        Lang::Python => "python",
        Lang::Go => "go",
        Lang::Java => "java",
        Lang::C => "c",
        Lang::Cpp => "cpp",
        Lang::Ruby => "ruby",
        Lang::Shell => "shell",
        Lang::Sql => "sql",
        Lang::Html => "html",
        Lang::Css => "css",
        Lang::Json => "json",
        Lang::Yaml => "yaml",
        Lang::Toml => "toml",
        Lang::Markdown => "markdown",
        Lang::Unknown | Lang::Other(_) => "text",
    }
}

pub(crate) fn role_display_name(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

pub(crate) fn status_display_name(status: Status) -> &'static str {
    match status {
        Status::Ok => "ok",
        Status::Error => "error",
        Status::Timeout => "timeout",
    }
}

pub(crate) fn format_hint_display_name(hint: FormatHint) -> &'static str {
    match hint {
        FormatHint::Markdown => "markdown",
        FormatHint::Plain => "plain",
        FormatHint::Html => "html",
    }
}

pub(crate) fn data_format_display_name(format: DataFormat) -> &'static str {
    match format {
        DataFormat::Json => "json",
        DataFormat::Yaml => "yaml",
        DataFormat::Toml => "toml",
        DataFormat::Csv => "csv",
    }
}

pub(crate) fn media_type_display_name(media: MediaType) -> &'static str {
    match media {
        MediaType::Png => "png",
        MediaType::Jpeg => "jpeg",
        MediaType::Gif => "gif",
        MediaType::Svg => "svg",
        MediaType::Webp => "webp",
    }
}

/// Convert raw content bytes to a UTF-8 string, returning a
/// `DriverError::InvalidContent` if the bytes are not valid UTF-8.
pub(crate) fn content_to_string(content: &[u8], block_index: usize) -> Result<String, DriverError> {
    String::from_utf8(content.to_vec()).map_err(|_| DriverError::InvalidContent { block_index })
}

/// Render file tree entries with indentation.
///
/// Produces output like:
/// ```text
/// src/
///   main.rs (1024 bytes)
///   lib.rs (512 bytes)
///   utils/
///     helpers.rs (256 bytes)
/// ```
pub(crate) fn render_file_tree_entries(entries: &[FileEntry], depth: usize) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let indent = "  ".repeat(depth);
    for entry in entries {
        match entry.kind {
            FileEntryKind::Directory => {
                let _ = writeln!(out, "{indent}{}/", entry.name);
                out.push_str(&render_file_tree_entries(&entry.children, depth + 1));
            }
            FileEntryKind::File => {
                let _ = writeln!(out, "{indent}{} ({} bytes)", entry.name, entry.size);
            }
        }
    }
    out
}

/// Escape XML special characters in attribute values.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcp_types::BlockType;
    use bcp_types::block::Block;
    use bcp_types::code::CodeBlock;
    use bcp_types::conversation::ConversationBlock;
    use bcp_types::enums::Lang;
    use bcp_wire::block_frame::BlockFlags;

    #[test]
    fn xml_code_block() {
        let block = Block {
            block_type: BlockType::Code,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::Code(CodeBlock {
                lang: Lang::Rust,
                path: "src/main.rs".to_string(),
                content: b"fn main() {}".to_vec(),
                line_range: None,
            }),
        };
        let result = XmlRenderer::render_all(&[&block]).unwrap();
        assert!(result.starts_with("<context>"));
        assert!(result.ends_with("</context>"));
        assert!(result.contains("<code lang=\"rust\" path=\"src/main.rs\">"));
        assert!(result.contains("fn main() {}"));
        assert!(result.contains("</code>"));
    }

    #[test]
    fn xml_conversation_block() {
        let block = Block {
            block_type: BlockType::Conversation,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::Conversation(ConversationBlock {
                role: bcp_types::enums::Role::User,
                content: b"Fix the bug.".to_vec(),
                tool_call_id: None,
            }),
        };
        let result = XmlRenderer::render_all(&[&block]).unwrap();
        assert!(result.contains("<turn role=\"user\">Fix the bug.</turn>"));
    }

    #[test]
    fn xml_escapes_attributes() {
        let block = Block {
            block_type: BlockType::Code,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::Code(CodeBlock {
                lang: Lang::Rust,
                path: "path/with\"quotes.rs".to_string(),
                content: b"code".to_vec(),
                line_range: None,
            }),
        };
        let result = XmlRenderer::render_all(&[&block]).unwrap();
        assert!(result.contains("path/with&quot;quotes.rs"));
    }

    #[test]
    fn xml_summary_rendering() {
        let block = Block {
            block_type: BlockType::Code,
            flags: BlockFlags::NONE,
            summary: Some(bcp_types::summary::Summary {
                text: "Entry point: CLI args, config loading.".to_string(),
            }),
            content: BlockContent::Code(CodeBlock {
                lang: Lang::Rust,
                path: "src/main.rs".to_string(),
                content: b"fn main() { /* long content */ }".to_vec(),
                line_range: None,
            }),
        };
        let result = XmlRenderer::render_all(&[&block]).unwrap();
        assert!(result.contains("summary=\"true\""));
        assert!(result.contains("Entry point: CLI args, config loading."));
        assert!(!result.contains("long content"));
    }
}
