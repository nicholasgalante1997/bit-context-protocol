use bcp_types::block::{Block, BlockContent};

use crate::budget::RenderDecision;
use crate::config::OutputMode;
use crate::error::DriverError;
use crate::placeholder::render_placeholder;
use crate::render_xml::{
    content_to_string, data_format_display_name, format_hint_display_name, lang_display_name,
    media_type_display_name, render_file_tree_entries, role_display_name, status_display_name,
};

/// Markdown renderer — emits conventional fenced code blocks and headers.
///
/// This renderer produces markdown output compatible with all model families.
/// It uses `##` headers for block identification, fenced code blocks with
/// language hints, and bold labels for conversation roles.
///
/// The output uses more tokens than XML or Minimal modes due to markdown's
/// structural overhead (triple backticks, header markers, blank lines), but
/// every model handles markdown well.
///
/// Example output (showing how code blocks, tool results, and
/// conversation turns are formatted):
///
/// ```text
///   ## src/main.rs
///
///   ~~~rust
///   fn main() {
///       let config = Config::load()?;
///   }
///   ~~~
///
///   ### Tool: ripgrep (ok)
///
///   3 matches for `ConnectionPool` across 2 files.
///
///   **User**: Fix the connection timeout bug.
///
///   **Assistant**: I'll examine the pool config...
/// ```
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    /// Render a filtered slice of blocks into a complete markdown document.
    ///
    /// Blocks are separated by blank lines. No outer wrapper element is
    /// added (unlike XML mode's `<context>`).
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
        Ok(parts.join("\n\n"))
    }

    /// Render a filtered slice of blocks with per-block render decisions.
    ///
    /// This is the budget-aware entry point. Each block is paired with a
    /// [`RenderDecision`] that controls rendering. See
    /// [`XmlRenderer::render_all_with_decisions`] for decision semantics.
    ///
    /// No outer wrapper is added (unlike XML mode).
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
                        OutputMode::Markdown,
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
        Ok(parts.join("\n\n"))
    }

    /// Render a single block to its markdown representation.
    fn render_block(block: &Block, index: usize) -> Result<String, DriverError> {
        let use_summary = block.summary.is_some();
        Self::render_block_inner(block, index, use_summary)
    }

    /// Inner rendering logic shared by `render_block` and the
    /// decision-aware path.
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
                    Ok(format!("## {} (summary)\n\n{content}", code.path))
                } else {
                    Ok(format!("## {}\n\n```{lang}\n{content}\n```", code.path))
                }
            }

            BlockContent::Conversation(conv) => {
                let role = role_display_name(conv.role);
                let label = capitalize_first(role);
                let content = content_to_string(&conv.content, index)?;
                Ok(format!("**{label}**: {content}"))
            }

            BlockContent::FileTree(tree) => {
                let rendered_tree = render_file_tree_entries(&tree.entries, 0);
                Ok(format!(
                    "### File Tree: {}\n\n```\n{rendered_tree}```",
                    tree.root_path
                ))
            }

            BlockContent::ToolResult(tool) => {
                let status = status_display_name(tool.status);
                let content = content_to_string(&tool.content, index)?;
                Ok(format!(
                    "### Tool: {} ({status})\n\n{content}",
                    tool.tool_name
                ))
            }

            BlockContent::Document(doc) => {
                let format = format_hint_display_name(doc.format_hint);
                let content = content_to_string(&doc.content, index)?;
                Ok(format!(
                    "### Document: {} [{format}]\n\n{content}",
                    doc.title
                ))
            }

            BlockContent::StructuredData(data) => {
                let format = data_format_display_name(data.format);
                let content = content_to_string(&data.content, index)?;
                Ok(format!("```{format}\n{content}\n```"))
            }

            BlockContent::Diff(diff) => {
                let mut lines = String::new();
                for hunk in &diff.hunks {
                    let hunk_content = String::from_utf8_lossy(&hunk.lines);
                    lines.push_str(&hunk_content);
                }
                Ok(format!("### Diff: {}\n\n```diff\n{lines}```", diff.path))
            }

            BlockContent::EmbeddingRef(emb) => {
                Ok(format!("*[Embedding ref: model={}]*", emb.model))
            }

            BlockContent::Image(img) => {
                let media = media_type_display_name(img.media_type);
                let content = content_to_string(&img.data, index)?;
                Ok(format!(
                    "### Image ({media}): {}\n\n{content}",
                    img.alt_text
                ))
            }

            BlockContent::Extension(ext) => {
                let content = content_to_string(&ext.content, index)?;
                Ok(format!(
                    "### Extension: {}/{}\n\n{content}",
                    ext.namespace, ext.type_name
                ))
            }

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

/// Capitalize the first letter of a string.
///
/// Used to convert role names ("user" → "User") for markdown labels.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcp_types::BlockType;
    use bcp_types::block::Block;
    use bcp_types::code::CodeBlock;
    use bcp_types::conversation::ConversationBlock;
    use bcp_types::enums::{Lang, Role};
    use bcp_wire::block_frame::BlockFlags;

    #[test]
    fn markdown_code_block() {
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
        let result = MarkdownRenderer::render_all(&[&block]).unwrap();
        assert!(result.contains("## src/main.rs"));
        assert!(result.contains("```rust"));
        assert!(result.contains("fn main() {}"));
        assert!(result.contains("```"));
    }

    #[test]
    fn markdown_conversation_block() {
        let block = Block {
            block_type: BlockType::Conversation,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::Conversation(ConversationBlock {
                role: Role::Assistant,
                content: b"I'll look into it.".to_vec(),
                tool_call_id: None,
            }),
        };
        let result = MarkdownRenderer::render_all(&[&block]).unwrap();
        assert!(result.contains("**Assistant**: I'll look into it."));
    }

    #[test]
    fn markdown_summary_rendering() {
        let block = Block {
            block_type: BlockType::Code,
            flags: BlockFlags::NONE,
            summary: Some(bcp_types::summary::Summary {
                text: "Entry point.".to_string(),
            }),
            content: BlockContent::Code(CodeBlock {
                lang: Lang::Rust,
                path: "src/main.rs".to_string(),
                content: b"fn main() { /* long */ }".to_vec(),
                line_range: None,
            }),
        };
        let result = MarkdownRenderer::render_all(&[&block]).unwrap();
        assert!(result.contains("(summary)"));
        assert!(result.contains("Entry point."));
        assert!(!result.contains("long"));
    }
}
