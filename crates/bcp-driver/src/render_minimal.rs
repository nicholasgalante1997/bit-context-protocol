use bcp_types::block::{Block, BlockContent};

use crate::error::DriverError;
use crate::render_xml::{
    content_to_string, data_format_display_name, lang_display_name, media_type_display_name,
    render_file_tree_entries, role_display_name, status_display_name,
};

/// Minimal renderer — single-line delimiters for maximum token efficiency.
///
/// This renderer uses the fewest structural tokens of any mode. Block
/// boundaries are marked with single-line `--- name [type] ---` delimiters,
/// and conversation turns use compact `[role]` prefixes.
///
/// This mode is ideal when token budget is tight and the consuming model
/// handles unstructured delimiters well. The tradeoff is less semantic
/// structure compared to XML mode.
///
/// Example output:
///
/// ```text
/// --- src/main.rs [rust] ---
/// fn main() {
///     let config = Config::load()?;
/// }
///
/// --- ripgrep [ok] ---
/// 3 matches for 'ConnectionPool' across 2 files.
///
/// [user] Fix the connection timeout bug.
/// [assistant] I'll examine the pool config...
/// ```
pub struct MinimalRenderer;

impl MinimalRenderer {
    /// Render a filtered slice of blocks into minimal-mode output.
    ///
    /// Blocks are separated by blank lines. No outer wrapper is added.
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

    /// Render a single block using minimal delimiters.
    fn render_block(block: &Block, index: usize) -> Result<String, DriverError> {
        let use_summary = block.summary.is_some();

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
                        "--- {} [{lang}] (summary) ---\n{content}",
                        code.path
                    ))
                } else {
                    Ok(format!("--- {} [{lang}] ---\n{content}", code.path))
                }
            }

            BlockContent::Conversation(conv) => {
                let role = role_display_name(conv.role);
                let content = content_to_string(&conv.content, index)?;
                Ok(format!("[{role}] {content}"))
            }

            BlockContent::FileTree(tree) => {
                let rendered_tree = render_file_tree_entries(&tree.entries, 0);
                Ok(format!("--- tree: {} ---\n{rendered_tree}", tree.root_path))
            }

            BlockContent::ToolResult(tool) => {
                let status = status_display_name(tool.status);
                let content = content_to_string(&tool.content, index)?;
                Ok(format!("--- {} [{status}] ---\n{content}", tool.tool_name))
            }

            BlockContent::Document(doc) => {
                let content = content_to_string(&doc.content, index)?;
                Ok(format!("--- {} ---\n{content}", doc.title))
            }

            BlockContent::StructuredData(data) => {
                let format = data_format_display_name(data.format);
                let content = content_to_string(&data.content, index)?;
                Ok(format!("--- data [{format}] ---\n{content}"))
            }

            BlockContent::Diff(diff) => {
                let mut lines = String::new();
                for hunk in &diff.hunks {
                    let hunk_content = String::from_utf8_lossy(&hunk.lines);
                    lines.push_str(&hunk_content);
                }
                Ok(format!("--- diff: {} ---\n{lines}", diff.path))
            }

            BlockContent::EmbeddingRef(emb) => {
                Ok(format!("[embed-ref: {}]", emb.model))
            }

            BlockContent::Image(img) => {
                let media = media_type_display_name(img.media_type);
                let content = content_to_string(&img.data, index)?;
                Ok(format!(
                    "--- image [{media}]: {} ---\n{content}",
                    img.alt_text
                ))
            }

            BlockContent::Extension(ext) => {
                let content = content_to_string(&ext.content, index)?;
                Ok(format!(
                    "--- ext: {}/{} ---\n{content}",
                    ext.namespace, ext.type_name
                ))
            }

            BlockContent::Annotation(_) | BlockContent::End => Ok(String::new()),

            BlockContent::Unknown { type_id, body } => {
                let content = String::from_utf8_lossy(body);
                Ok(format!("--- unknown 0x{type_id:02X} ---\n{content}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcp_types::block::Block;
    use bcp_types::code::CodeBlock;
    use bcp_types::conversation::ConversationBlock;
    use bcp_types::enums::{Lang, Role};
    use bcp_types::BlockType;
    use bcp_wire::block_frame::BlockFlags;

    #[test]
    fn minimal_code_block() {
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
        let result = MinimalRenderer::render_all(&[&block]).unwrap();
        assert!(result.contains("--- src/main.rs [rust] ---"));
        assert!(result.contains("fn main() {}"));
    }

    #[test]
    fn minimal_conversation_block() {
        let block = Block {
            block_type: BlockType::Conversation,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::Conversation(ConversationBlock {
                role: Role::User,
                content: b"Fix the bug.".to_vec(),
                tool_call_id: None,
            }),
        };
        let result = MinimalRenderer::render_all(&[&block]).unwrap();
        assert!(result.contains("[user] Fix the bug."));
    }

    #[test]
    fn minimal_summary_rendering() {
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
        let result = MinimalRenderer::render_all(&[&block]).unwrap();
        assert!(result.contains("(summary)"));
        assert!(result.contains("Entry point."));
        assert!(!result.contains("long"));
    }
}
