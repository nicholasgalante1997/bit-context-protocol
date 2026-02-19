use bcp_types::BlockType;
use bcp_types::block::Block;

use crate::budget::{CodeAwareEstimator, RenderDecision, compute_budget_decisions};
use crate::config::{DriverConfig, OutputMode, Verbosity};
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
/// model comprehension within a token budget.
///
/// The trait takes an immutable block slice and a configuration reference,
/// returning either the rendered text or a `DriverError`. Implementations
/// are expected to be stateless — all configuration comes through
/// `DriverConfig`.
///
/// ```text
/// Vec<Block> ──▶ BcpDriver::render() ──▶ model-ready String
///                        │
///                  DriverConfig
///                  (mode, verbosity, token_budget, ...)
/// ```
pub trait BcpDriver {
    /// Render a complete set of decoded blocks into model-ready text.
    ///
    /// # Errors
    ///
    /// Returns `DriverError::EmptyInput` if `blocks` is empty (after
    /// filtering by `config.include_types`, if set).
    fn render(&self, blocks: &[Block], config: &DriverConfig) -> Result<String, DriverError>;
}

/// Default driver implementation — filtering, budget allocation, and
/// renderer dispatch.
///
/// This is the standard entry point for rendering. It handles:
///
/// 1. **Block filtering** — removes Annotation/End blocks and applies
///    `config.include_types` to skip non-matching blocks.
/// 2. **Budget decisions** — based on `config.verbosity` and
///    `config.token_budget`, computes a [`RenderDecision`] per block
///    (Full, Summary, Placeholder, or Omit).
/// 3. **Renderer dispatch** — selects `XmlRenderer`, `MarkdownRenderer`,
///    or `MinimalRenderer` based on `config.mode`, using the
///    decision-aware rendering path.
///
/// ```text
/// ┌─────────────┐     ┌───────────────┐     ┌──────────────────┐
/// │ &[Block]    │────▶│ filter +      │────▶│ XmlRenderer      │
/// │             │     │ budget engine │     │ MarkdownRenderer │
/// │             │     │ + dispatch    │     │ MinimalRenderer  │
/// └─────────────┘     └───────────────┘     └──────────────────┘
///                           │                       │
///                     DriverConfig            String output
///                     (mode, verbosity,
///                      token_budget, ...)
/// ```
pub struct DefaultDriver;

impl BcpDriver for DefaultDriver {
    /// Render decoded blocks into model-ready text.
    ///
    /// The rendering pipeline:
    ///
    /// 1. Filter: remove Annotation/End blocks, apply `include_types`.
    /// 2. Decide: compute per-block [`RenderDecision`] based on verbosity
    ///    and token budget.
    /// 3. Render: dispatch to the appropriate renderer with decisions.
    ///
    /// # Errors
    ///
    /// - `DriverError::EmptyInput` if no renderable blocks remain after filtering.
    /// - `DriverError::InvalidContent` if a block contains non-UTF-8 bytes.
    fn render(&self, blocks: &[Block], config: &DriverConfig) -> Result<String, DriverError> {
        // Step 1: Filter blocks, tracking original indices for annotation mapping
        let mut filtered: Vec<&Block> = Vec::new();
        let mut original_indices: Vec<usize> = Vec::new();

        for (i, b) in blocks.iter().enumerate() {
            if b.block_type == BlockType::Annotation || b.block_type == BlockType::End {
                continue;
            }
            if let Some(ref types) = config.include_types
                && !types.contains(&b.block_type)
            {
                continue;
            }
            filtered.push(b);
            original_indices.push(i);
        }

        if filtered.is_empty() {
            return Err(DriverError::EmptyInput);
        }

        // Step 2: Compute render decisions
        let decisions = match (config.token_budget, config.verbosity) {
            // Summary mode (with or without budget): summaries where available
            (_, Verbosity::Summary) => filtered
                .iter()
                .map(|b| {
                    if b.summary.is_some() {
                        RenderDecision::Summary
                    } else {
                        RenderDecision::Full
                    }
                })
                .collect(),
            // Budget + Adaptive: run the full budget engine
            (Some(budget), Verbosity::Adaptive) => compute_budget_decisions(
                blocks,
                &filtered,
                &original_indices,
                budget,
                &CodeAwareEstimator,
            ),
            // All other cases: render everything in full
            // (no budget, or Full verbosity regardless of budget)
            _ => vec![RenderDecision::Full; filtered.len()],
        };

        // Step 3: Build (block, decision) pairs and dispatch to renderer
        let items: Vec<(&Block, &RenderDecision)> =
            filtered.iter().copied().zip(decisions.iter()).collect();

        match config.mode {
            OutputMode::Xml => XmlRenderer::render_all_with_decisions(&items),
            OutputMode::Markdown => MarkdownRenderer::render_all_with_decisions(&items),
            OutputMode::Minimal => MinimalRenderer::render_all_with_decisions(&items),
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
            include_types: Some(vec![BlockType::Code]),
            ..DriverConfig::default()
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
            include_types: Some(vec![BlockType::Diff]),
            ..DriverConfig::default()
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
            ..DriverConfig::default()
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
            ..DriverConfig::default()
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
            ..DriverConfig::default()
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
            ..DriverConfig::default()
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
            ..DriverConfig::default()
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
    fn summary_replaces_content_with_summary_verbosity() {
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
                verbosity: Verbosity::Summary,
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
    fn adaptive_without_budget_renders_full_content() {
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

        // Adaptive without budget → Full rendering (summary ignored)
        let config = DriverConfig {
            mode: OutputMode::Xml,
            ..DriverConfig::default()
        };
        let result = driver.render(&blocks, &config).unwrap();
        assert!(
            result.contains("very long implementation"),
            "Adaptive without budget should render full content"
        );
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
