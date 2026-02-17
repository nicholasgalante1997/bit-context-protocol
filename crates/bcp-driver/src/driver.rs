use bcp_types::block::Block;
use bcp_types::BlockType;

use crate::config::{DriverConfig, OutputMode};
use crate::error::DriverError;
use crate::render_markdown::MarkdownRenderer;
use crate::render_minimal::MinimalRenderer;
use crate::render_xml::XmlRenderer;

/// Core driver interface — renders decoded blocks into model-ready text.
///
/// This is the primary trait consumers use to convert a `Vec<Block>` (from
/// `bcp-decoder`) into a string that can be injected into an LLM's context
/// window. The driver is not a simple serializer — it is an opinionated
/// renderer that makes decisions about how to present context to maximize
/// model comprehension.
///
/// The trait takes an immutable block slice and a configuration reference,
/// returning either the rendered text or a `DriverError`. Implementations
/// are expected to be stateless — all configuration comes through
/// `DriverConfig`.
///
/// ```text
/// Vec<Block> ──▶ LcpDriver::render() ──▶ model-ready String
///                        │
///                  DriverConfig
///                  (mode, target_model, include_types)
/// ```
pub trait LcpDriver {
    /// Render a complete set of decoded blocks into model-ready text.
    ///
    /// # Errors
    ///
    /// Returns `DriverError::EmptyInput` if `blocks` is empty (after
    /// filtering by `config.include_types`, if set).
    fn render(&self, blocks: &[Block], config: &DriverConfig) -> Result<String, DriverError>;
}

/// Default driver implementation that dispatches to the appropriate
/// renderer based on the configured output mode.
///
/// This is the standard entry point for rendering. It handles:
///
/// 1. **Block filtering** — applies `config.include_types` to skip
///    non-matching blocks before any rendering occurs.
/// 2. **Annotation suppression** — `Annotation` blocks are metadata-only
///    and are never rendered as visible text, regardless of filter settings.
/// 3. **Renderer dispatch** — selects `XmlRenderer`, `MarkdownRenderer`,
///    or `MinimalRenderer` based on `config.mode`.
/// 4. **Output assembly** — joins rendered block strings with appropriate
///    separators (newlines for all modes; `<context>` wrapper for XML).
///
/// ```text
/// ┌─────────────┐     ┌──────────────┐     ┌──────────────────┐
/// │ &[Block]    │────▶│ filter +     │────▶│ XmlRenderer      │
/// │             │     │ dispatch     │     │ MarkdownRenderer │
/// │             │     │              │     │ MinimalRenderer  │
/// └─────────────┘     └──────────────┘     └──────────────────┘
///                           │                       │
///                     DriverConfig            String output
/// ```
pub struct DefaultDriver;

impl LcpDriver for DefaultDriver {
    /// Render decoded blocks into model-ready text.
    ///
    /// # Errors
    ///
    /// - `DriverError::EmptyInput` if no renderable blocks remain after filtering.
    /// - `DriverError::InvalidContent` if a block contains non-UTF-8 bytes.
    fn render(&self, blocks: &[Block], config: &DriverConfig) -> Result<String, DriverError> {
        let filtered: Vec<&Block> = blocks
            .iter()
            .filter(|b| {
                // Annotation blocks are metadata-only — never rendered
                if b.block_type == BlockType::Annotation {
                    return false;
                }
                // End blocks are sentinels — never rendered
                if b.block_type == BlockType::End {
                    return false;
                }
                // Apply include_types filter if set
                if let Some(ref types) = config.include_types {
                    return types.contains(&b.block_type);
                }
                true
            })
            .collect();

        if filtered.is_empty() {
            return Err(DriverError::EmptyInput);
        }

        match config.mode {
            OutputMode::Xml => XmlRenderer::render_all(&filtered),
            OutputMode::Markdown => MarkdownRenderer::render_all(&filtered),
            OutputMode::Minimal => MinimalRenderer::render_all(&filtered),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcp_types::annotation::AnnotationBlock;
    use bcp_types::block::BlockContent;
    use bcp_types::code::CodeBlock;
    use bcp_types::conversation::ConversationBlock;
    use bcp_types::enums::{AnnotationKind, Lang, Role, Status};
    use bcp_types::file_tree::{FileEntry, FileEntryKind, FileTreeBlock};
    use bcp_types::summary::Summary;
    use bcp_types::tool_result::ToolResultBlock;
    use bcp_wire::block_frame::BlockFlags;

    fn code_block(lang: Lang, path: &str, content: &[u8]) -> Block {
        Block {
            block_type: BlockType::Code,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::Code(CodeBlock {
                lang,
                path: path.to_string(),
                content: content.to_vec(),
                line_range: None,
            }),
        }
    }

    fn conversation_block(role: Role, content: &[u8]) -> Block {
        Block {
            block_type: BlockType::Conversation,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::Conversation(ConversationBlock {
                role,
                content: content.to_vec(),
                tool_call_id: None,
            }),
        }
    }

    #[test]
    fn empty_input_returns_error() {
        let driver = DefaultDriver;
        let config = DriverConfig::default();
        let result = driver.render(&[], &config);
        assert!(matches!(result, Err(DriverError::EmptyInput)));
    }

    #[test]
    fn annotation_blocks_filtered_out() {
        let driver = DefaultDriver;
        let config = DriverConfig::default();
        let blocks = vec![Block {
            block_type: BlockType::Annotation,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::Annotation(AnnotationBlock {
                target_block_id: 0,
                kind: AnnotationKind::Priority,
                value: vec![0x01],
            }),
        }];
        let result = driver.render(&blocks, &config);
        assert!(matches!(result, Err(DriverError::EmptyInput)));
    }

    #[test]
    fn include_types_filter() {
        let driver = DefaultDriver;
        let config = DriverConfig {
            mode: OutputMode::Minimal,
            target_model: None,
            include_types: Some(vec![BlockType::Code]),
        };
        let blocks = vec![
            code_block(Lang::Rust, "main.rs", b"fn main() {}"),
            conversation_block(Role::User, b"Hello"),
        ];
        let result = driver.render(&blocks, &config).unwrap();
        assert!(result.contains("main.rs"));
        assert!(!result.contains("Hello"));
    }

    #[test]
    fn include_types_filter_empty_result() {
        let driver = DefaultDriver;
        let config = DriverConfig {
            mode: OutputMode::Xml,
            target_model: None,
            include_types: Some(vec![BlockType::Diff]),
        };
        let blocks = vec![code_block(Lang::Rust, "main.rs", b"fn main() {}")];
        let result = driver.render(&blocks, &config);
        assert!(matches!(result, Err(DriverError::EmptyInput)));
    }

    #[test]
    fn xml_mode_wraps_in_context() {
        let driver = DefaultDriver;
        let config = DriverConfig {
            mode: OutputMode::Xml,
            target_model: None,
            include_types: None,
        };
        let blocks = vec![code_block(Lang::Rust, "main.rs", b"fn main() {}")];
        let result = driver.render(&blocks, &config).unwrap();
        assert!(result.starts_with("<context>"));
        assert!(result.ends_with("</context>"));
    }

    #[test]
    fn markdown_mode_no_context_wrapper() {
        let driver = DefaultDriver;
        let config = DriverConfig {
            mode: OutputMode::Markdown,
            target_model: None,
            include_types: None,
        };
        let blocks = vec![code_block(Lang::Rust, "main.rs", b"fn main() {}")];
        let result = driver.render(&blocks, &config).unwrap();
        assert!(!result.contains("<context>"));
        assert!(result.contains("## main.rs"));
    }

    #[test]
    fn minimal_mode_uses_dashes() {
        let driver = DefaultDriver;
        let config = DriverConfig {
            mode: OutputMode::Minimal,
            target_model: None,
            include_types: None,
        };
        let blocks = vec![code_block(Lang::Rust, "main.rs", b"fn main() {}")];
        let result = driver.render(&blocks, &config).unwrap();
        assert!(result.contains("--- main.rs [rust] ---"));
    }

    #[test]
    fn multiple_blocks_rendered() {
        let driver = DefaultDriver;
        let config = DriverConfig {
            mode: OutputMode::Xml,
            target_model: None,
            include_types: None,
        };
        let blocks = vec![
            code_block(Lang::Rust, "src/main.rs", b"fn main() {}"),
            conversation_block(Role::User, b"Fix the bug."),
            conversation_block(Role::Assistant, b"Looking into it."),
        ];
        let result = driver.render(&blocks, &config).unwrap();
        assert!(result.contains("<code lang=\"rust\""));
        assert!(result.contains("<turn role=\"user\">Fix the bug.</turn>"));
        assert!(result.contains("<turn role=\"assistant\">Looking into it.</turn>"));
    }

    #[test]
    fn file_tree_rendering_xml() {
        let driver = DefaultDriver;
        let config = DriverConfig {
            mode: OutputMode::Xml,
            target_model: None,
            include_types: None,
        };
        let blocks = vec![Block {
            block_type: BlockType::FileTree,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::FileTree(FileTreeBlock {
                root_path: "src/".to_string(),
                entries: vec![
                    FileEntry {
                        name: "main.rs".to_string(),
                        kind: FileEntryKind::File,
                        size: 1024,
                        children: vec![],
                    },
                    FileEntry {
                        name: "utils".to_string(),
                        kind: FileEntryKind::Directory,
                        size: 0,
                        children: vec![FileEntry {
                            name: "helpers.rs".to_string(),
                            kind: FileEntryKind::File,
                            size: 256,
                            children: vec![],
                        }],
                    },
                ],
            }),
        }];
        let result = driver.render(&blocks, &config).unwrap();
        assert!(result.contains("<tree root=\"src/\">"));
        assert!(result.contains("main.rs (1024 bytes)"));
        assert!(result.contains("utils/"));
        assert!(result.contains("  helpers.rs (256 bytes)"));
    }

    #[test]
    fn tool_result_rendering_all_modes() {
        let driver = DefaultDriver;
        let blocks = vec![Block {
            block_type: BlockType::ToolResult,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::ToolResult(ToolResultBlock {
                tool_name: "ripgrep".to_string(),
                status: Status::Ok,
                content: b"3 matches found.".to_vec(),
                schema_hint: None,
            }),
        }];

        let xml = driver
            .render(
                &blocks,
                &DriverConfig {
                    mode: OutputMode::Xml,
                    ..DriverConfig::default()
                },
            )
            .unwrap();
        assert!(xml.contains("<tool name=\"ripgrep\" status=\"ok\">"));

        let md = driver
            .render(
                &blocks,
                &DriverConfig {
                    mode: OutputMode::Markdown,
                    ..DriverConfig::default()
                },
            )
            .unwrap();
        assert!(md.contains("### Tool: ripgrep (ok)"));

        let min = driver
            .render(
                &blocks,
                &DriverConfig {
                    mode: OutputMode::Minimal,
                    ..DriverConfig::default()
                },
            )
            .unwrap();
        assert!(min.contains("--- ripgrep [ok] ---"));
    }

    #[test]
    fn summary_replaces_content_all_modes() {
        let driver = DefaultDriver;
        let blocks = vec![Block {
            block_type: BlockType::Code,
            flags: BlockFlags::NONE,
            summary: Some(Summary {
                text: "Entry point with CLI parsing.".to_string(),
            }),
            content: BlockContent::Code(CodeBlock {
                lang: Lang::Rust,
                path: "src/main.rs".to_string(),
                content: b"fn main() { /* very long implementation */ }".to_vec(),
                line_range: None,
            }),
        }];

        for mode in [OutputMode::Xml, OutputMode::Markdown, OutputMode::Minimal] {
            let config = DriverConfig {
                mode,
                ..DriverConfig::default()
            };
            let result = driver.render(&blocks, &config).unwrap();
            assert!(
                result.contains("Entry point with CLI parsing."),
                "mode {mode:?} should contain summary"
            );
            assert!(
                !result.contains("very long implementation"),
                "mode {mode:?} should not contain full content"
            );
        }
    }

    #[test]
    fn end_blocks_filtered_out() {
        let driver = DefaultDriver;
        let config = DriverConfig::default();
        let blocks = vec![
            code_block(Lang::Rust, "main.rs", b"fn main() {}"),
            Block {
                block_type: BlockType::End,
                flags: BlockFlags::NONE,
                summary: None,
                content: BlockContent::End,
            },
        ];
        let result = driver.render(&blocks, &config).unwrap();
        assert!(result.contains("fn main()"));
    }
}
