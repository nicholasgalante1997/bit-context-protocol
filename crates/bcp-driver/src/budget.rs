use std::collections::HashMap;

use bcp_types::BlockType;
use bcp_types::block::{Block, BlockContent};
use bcp_types::enums::{AnnotationKind, Priority};

use crate::render_xml::{content_to_string, role_display_name};

/// How to render a single block under budget constraints.
///
/// The budget engine produces one `RenderDecision` per renderable block.
/// Renderers use this to override the default summary-presence logic —
/// a block with a summary may still be rendered in full if the budget
/// allows, or a block without a summary may be reduced to a placeholder
/// if the budget is exhausted.
///
/// ```text
/// ┌─────────────┬──────────────────────────────────────────────────┐
/// │ Variant     │ Behavior                                         │
/// ├─────────────┼──────────────────────────────────────────────────┤
/// │ Full        │ Render complete block content (ignore summary)   │
/// │ Summary     │ Render summary text only                         │
/// │ Placeholder │ Emit a compact omission notice with metadata     │
/// │ Omit        │ Skip the block entirely (no output)              │
/// └─────────────┴──────────────────────────────────────────────────┘
/// ```
///
/// The budget engine assigns decisions based on priority (from
/// ANNOTATION blocks) and available token budget:
///
/// ```text
/// ┌────────────┬──────────────────────────────────────────────────┐
/// │ Priority   │ Degradation path                                 │
/// ├────────────┼──────────────────────────────────────────────────┤
/// │ Critical   │ Full (always, even over budget)                  │
/// │ High       │ Full → Summary → Full (forced, over budget)      │
/// │ Normal     │ Full → Summary → Placeholder                     │
/// │ Low        │ Summary → Placeholder                            │
/// │ Background │ Placeholder → Omit                               │
/// └────────────┴──────────────────────────────────────────────────┘
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderDecision {
    /// Render the full block content, ignoring any attached summary.
    Full,

    /// Render the summary text only. If the block has no summary,
    /// the renderer falls back to full content.
    Summary,

    /// Render a compact placeholder showing the block's type,
    /// description, and estimated omitted token count.
    Placeholder {
        /// The type of the omitted block (e.g., `Code`, `ToolResult`).
        block_type: BlockType,
        /// Human-readable description (e.g., file path, tool name).
        description: String,
        /// Estimated tokens that were omitted.
        omitted_tokens: u32,
    },

    /// Omit the block entirely — produce no output for it.
    Omit,
}

/// Token count estimator trait.
///
/// Implementations estimate how many tokens a text string will consume
/// in an LLM's context window. The trait is pluggable: the `PoC` uses
/// character-count heuristics, but a real tokenizer (tiktoken, etc.)
/// can be swapped in by implementing this trait.
///
/// Implementations must be `Send + Sync` so the driver can be shared
/// across threads.
pub trait TokenEstimator: Send + Sync {
    /// Estimate the token count for the given text.
    ///
    /// Returns 0 for empty strings and at least 1 for non-empty strings.
    fn estimate(&self, text: &str) -> u32;
}

/// Heuristic token estimator: `chars / 4`, minimum 1.
///
/// This matches the common rule-of-thumb that English prose averages
/// roughly 4 characters per token. It systematically underestimates
/// code (which tends toward shorter tokens due to operators and short
/// identifiers), but is a reasonable default when no tokenizer is
/// available.
///
/// ```text
/// ┌──────────────────────┬────────┬──────────┐
/// │ Input                │ Chars  │ Estimate │
/// ├──────────────────────┼────────┼──────────┤
/// │ ""                   │ 0      │ 0        │
/// │ "a"                  │ 1      │ 1 (min)  │
/// │ "hello world"        │ 11     │ 2        │
/// │ "fn main() {}"       │ 13     │ 3        │
/// └──────────────────────┴────────┴──────────┘
/// ```
pub struct HeuristicEstimator;

impl TokenEstimator for HeuristicEstimator {
    #[allow(clippy::cast_possible_truncation)]
    fn estimate(&self, text: &str) -> u32 {
        if text.is_empty() {
            return 0;
        }
        let chars = text.len() as u32;
        (chars / 4).max(1)
    }
}

/// Code-aware token estimator.
///
/// Uses different character-to-token ratios depending on whether the
/// text looks like code or prose:
///
/// - **Code** (>30% of non-empty lines are indented): `chars / 3`
/// - **Prose** (everything else): `chars / 4`
///
/// Code tends to produce more tokens per character because of short
/// identifiers, operators, and punctuation that each consume a token.
///
/// ```text
/// ┌──────────────────────┬───────┬──────────┐
/// │ Input type           │ Ratio │ Example  │
/// ├──────────────────────┼───────┼──────────┤
/// │ English prose        │ ÷ 4   │ 400ch→100│
/// │ Source code          │ ÷ 3   │ 300ch→100│
/// │ Empty string         │ —     │ 0        │
/// └──────────────────────┴───────┴──────────┘
/// ```
pub struct CodeAwareEstimator;

impl CodeAwareEstimator {
    /// Determine whether a text block looks like code.
    ///
    /// Heuristic: if more than 30% of non-empty lines start with
    /// whitespace (space or tab), treat the text as code. This catches
    /// most indented source code while ignoring prose paragraphs.
    fn is_code_like(text: &str) -> bool {
        let non_empty: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        if non_empty.is_empty() {
            return false;
        }
        let indented = non_empty
            .iter()
            .filter(|l| l.starts_with(' ') || l.starts_with('\t'))
            .count();
        (indented * 100 / non_empty.len()) > 30
    }
}

impl TokenEstimator for CodeAwareEstimator {
    #[allow(clippy::cast_possible_truncation)]
    fn estimate(&self, text: &str) -> u32 {
        if text.is_empty() {
            return 0;
        }
        let chars = text.len() as u32;
        let divisor = if Self::is_code_like(text) { 3 } else { 4 };
        (chars / divisor).max(1)
    }
}

/// Per-block budget metadata computed during the scan pass.
///
/// One `BlockBudgetInfo` is created for every renderable (non-Annotation,
/// non-End) block during the first pass of the budget engine. It captures
/// estimated token costs and resolved priority so the allocation pass can
/// make render decisions.
///
/// ```text
/// ┌────────────────┬──────────────────────────────────────────────────┐
/// │ Field          │ Purpose                                          │
/// ├────────────────┼──────────────────────────────────────────────────┤
/// │ priority       │ Resolved from ANNOTATION blocks or Normal        │
/// │ full_tokens    │ Estimated tokens for full content rendering      │
/// │ summary_tokens │ Estimated tokens for summary (None if no summary)│
/// │ has_summary    │ Whether the block has an attached summary        │
/// └────────────────┴──────────────────────────────────────────────────┘
/// ```
pub(crate) struct BlockBudgetInfo {
    pub priority: Priority,
    pub full_tokens: u32,
    pub summary_tokens: Option<u32>,
    pub has_summary: bool,
}

// ── Priority Resolution ──────────────────────────────────────────────

/// Resolve block priorities from ANNOTATION blocks.
///
/// Walks the full block list (including annotations and sentinels) and
/// extracts `AnnotationKind::Priority` annotations into a map keyed by
/// `target_block_id`. The target ID is the zero-based index of the
/// target block in the original (unfiltered) block stream.
///
/// If multiple annotations target the same block, the last one wins.
/// Non-priority annotations (`Summary`, `Tag`) are ignored — they are
/// handled elsewhere in the pipeline.
///
/// Blocks without a priority annotation default to `Priority::Normal`
/// (applied by the caller, not this function).
pub(crate) fn resolve_priorities(blocks: &[Block]) -> HashMap<u32, Priority> {
    let mut map = HashMap::new();
    for block in blocks {
        if let BlockContent::Annotation(ann) = &block.content
            && ann.kind == AnnotationKind::Priority
            && let Some(&byte) = ann.value.first()
            && let Ok(pri) = Priority::from_wire_byte(byte)
        {
            map.insert(ann.target_block_id, pri);
        }
    }
    map
}

// ── Text Extraction for Estimation ──────────────────────────────────

/// Extract the text content from a block for token estimation.
///
/// This mirrors the content extraction logic in the renderers, pulling
/// out the UTF-8 text from each block variant. For blocks with non-UTF-8
/// content (or variants where text extraction doesn't apply), returns a
/// synthetic string whose length approximates the byte count.
///
/// The returned string is used only for token estimation — it is never
/// rendered directly.
pub(crate) fn estimate_block_text(block: &Block) -> String {
    match &block.content {
        BlockContent::Code(c) => {
            content_to_string(&c.content, 0).unwrap_or_else(|_| "x".repeat(c.content.len()))
        }
        BlockContent::Conversation(c) => {
            content_to_string(&c.content, 0).unwrap_or_else(|_| "x".repeat(c.content.len()))
        }
        BlockContent::ToolResult(t) => {
            content_to_string(&t.content, 0).unwrap_or_else(|_| "x".repeat(t.content.len()))
        }
        BlockContent::Document(d) => {
            content_to_string(&d.content, 0).unwrap_or_else(|_| "x".repeat(d.content.len()))
        }
        BlockContent::StructuredData(d) => {
            content_to_string(&d.content, 0).unwrap_or_else(|_| "x".repeat(d.content.len()))
        }
        BlockContent::Diff(d) => {
            let mut text = String::new();
            for hunk in &d.hunks {
                text.push_str(&String::from_utf8_lossy(&hunk.lines));
            }
            text
        }
        BlockContent::Image(i) => {
            content_to_string(&i.data, 0).unwrap_or_else(|_| "x".repeat(i.data.len()))
        }
        BlockContent::Extension(e) => {
            content_to_string(&e.content, 0).unwrap_or_else(|_| "x".repeat(e.content.len()))
        }
        BlockContent::FileTree(t) => crate::render_xml::render_file_tree_entries(&t.entries, 0),
        BlockContent::EmbeddingRef(e) => format!("embedding: {}", e.model),
        BlockContent::Unknown { body, .. } => "x".repeat(body.len()),
        BlockContent::Annotation(_) | BlockContent::End => String::new(),
    }
}

// ── Scan Pass ────────────────────────────────────────────────────────

/// Scan pass: compute token estimates and resolve priorities.
///
/// Produces one [`BlockBudgetInfo`] per filtered block. The caller
/// provides the priority map from [`resolve_priorities`] and the
/// original indices mapping (filtered index → original block index)
/// so that annotation targets resolve correctly.
///
/// ```text
/// filtered[0] → original_indices[0] = 2 → priorities.get(2) → High
/// filtered[1] → original_indices[1] = 4 → priorities.get(4) → None → Normal
/// ```
pub(crate) fn scan_blocks(
    filtered: &[&Block],
    priorities: &HashMap<u32, Priority>,
    estimator: &dyn TokenEstimator,
    original_indices: &[usize],
) -> Vec<BlockBudgetInfo> {
    filtered
        .iter()
        .enumerate()
        .map(|(i, block)| {
            let orig_idx = original_indices[i];
            #[allow(clippy::cast_possible_truncation)]
            let priority = priorities
                .get(&(orig_idx as u32))
                .copied()
                .unwrap_or(Priority::Normal);

            let full_text = estimate_block_text(block);
            let full_tokens = estimator.estimate(&full_text);

            let (summary_tokens, has_summary) = if let Some(ref summary) = block.summary {
                (Some(estimator.estimate(&summary.text)), true)
            } else {
                (None, false)
            };

            BlockBudgetInfo {
                priority,
                full_tokens,
                summary_tokens,
                has_summary,
            }
        })
        .collect()
}

// ── Block Description ────────────────────────────────────────────────

/// Extract a human-readable description for placeholder rendering.
///
/// Returns a string identifying the block — typically a file path for
/// code blocks, a tool name for tool results, a title for documents,
/// etc. Used in placeholder output like `[omitted: code src/main.rs]`.
pub(crate) fn block_description(block: &Block) -> String {
    match &block.content {
        BlockContent::Code(c) => c.path.clone(),
        BlockContent::Conversation(c) => format!("{} turn", role_display_name(c.role)),
        BlockContent::FileTree(t) => format!("tree: {}", t.root_path),
        BlockContent::ToolResult(t) => t.tool_name.clone(),
        BlockContent::Document(d) => d.title.clone(),
        BlockContent::StructuredData(d) => {
            format!(
                "{} data",
                crate::render_xml::data_format_display_name(d.format)
            )
        }
        BlockContent::Diff(d) => d.path.clone(),
        BlockContent::EmbeddingRef(e) => format!("embedding: {}", e.model),
        BlockContent::Image(i) => i.alt_text.clone(),
        BlockContent::Extension(e) => format!("{}/{}", e.namespace, e.type_name),
        BlockContent::Unknown { type_id, .. } => format!("unknown 0x{type_id:02X}"),
        BlockContent::Annotation(_) | BlockContent::End => String::new(),
    }
}

/// Map a [`BlockType`] to a short label for placeholder rendering.
///
/// ```text
/// ┌───────────────┬──────────────┐
/// │ BlockType     │ Label        │
/// ├───────────────┼──────────────┤
/// │ Code          │ "code"       │
/// │ Conversation  │ "conversation"│
/// │ FileTree      │ "file-tree"  │
/// │ ToolResult    │ "tool-result"│
/// │ Document      │ "document"   │
/// │ StructuredData│ "data"       │
/// │ Diff          │ "diff"       │
/// │ Image         │ "image"      │
/// │ Extension     │ "extension"  │
/// │ (other)       │ "block"      │
/// └───────────────┴──────────────┘
/// ```
pub(crate) fn block_type_label(bt: &BlockType) -> &'static str {
    match bt {
        BlockType::Code => "code",
        BlockType::Conversation => "conversation",
        BlockType::FileTree => "file-tree",
        BlockType::ToolResult => "tool-result",
        BlockType::Document => "document",
        BlockType::StructuredData => "data",
        BlockType::Diff => "diff",
        BlockType::Image => "image",
        BlockType::Extension => "extension",
        _ => "block",
    }
}

// ── Allocation Pass ──────────────────────────────────────────────────

/// Estimated token cost of a placeholder line.
///
/// Placeholders are short strings like `[omitted: code src/main.rs ~823tok]`
/// which typically consume around 10-15 tokens. We use a conservative
/// fixed estimate rather than measuring each one.
const PLACEHOLDER_TOKEN_COST: u32 = 10;

/// Budget allocation pass — assign a [`RenderDecision`] to each block.
///
/// Takes the scanned block metadata, the total token budget, and the
/// filtered block slice. Returns a `Vec<RenderDecision>` parallel to
/// the filtered slice (same length, same order).
///
/// The algorithm:
/// 1. Create an index list sorted by priority (ascending: Critical first).
///    Within the same priority, original order is preserved (stable sort).
/// 2. Walk sorted indices, greedily subtracting from remaining budget:
///    - **Critical**: always `Full` (never degraded, even over budget).
///    - **High**: `Full` if budget allows, else `Summary` if available,
///      else `Full` anyway (high-priority content is too important to omit).
///    - **Normal**: `Full` if budget allows, else `Summary` if available,
///      else `Placeholder`.
///    - **Low**: `Summary` if budget allows, else `Placeholder`.
///    - **Background**: `Placeholder` if budget allows, else `Omit`.
/// 3. Return decisions reordered to match the original block sequence.
pub(crate) fn allocate_budget(
    infos: &[BlockBudgetInfo],
    budget: u32,
    filtered: &[&Block],
) -> Vec<RenderDecision> {
    let mut decisions = vec![RenderDecision::Omit; infos.len()];
    let mut remaining = budget;

    // Sort indices by priority (stable: preserves original order within
    // the same priority level). Priority::Critical < Priority::High < ...
    let mut sorted: Vec<usize> = (0..infos.len()).collect();
    sorted.sort_by(|&a, &b| infos[a].priority.cmp(&infos[b].priority));

    for idx in sorted {
        let info = &infos[idx];
        let block = filtered[idx];

        match info.priority {
            Priority::Critical => {
                decisions[idx] = RenderDecision::Full;
                remaining = remaining.saturating_sub(info.full_tokens);
            }
            Priority::High => {
                if info.full_tokens <= remaining {
                    decisions[idx] = RenderDecision::Full;
                    remaining -= info.full_tokens;
                } else if info.has_summary {
                    let stok = info.summary_tokens.unwrap_or(0);
                    if stok <= remaining {
                        decisions[idx] = RenderDecision::Summary;
                        remaining -= stok;
                    } else {
                        // High-priority: render full anyway, over budget
                        decisions[idx] = RenderDecision::Full;
                        remaining = 0;
                    }
                } else {
                    // No summary available, render full over budget
                    decisions[idx] = RenderDecision::Full;
                    remaining = 0;
                }
            }
            Priority::Normal => {
                if info.full_tokens <= remaining {
                    decisions[idx] = RenderDecision::Full;
                    remaining -= info.full_tokens;
                } else if info.has_summary {
                    let stok = info.summary_tokens.unwrap_or(0);
                    if stok <= remaining {
                        decisions[idx] = RenderDecision::Summary;
                        remaining -= stok;
                    } else {
                        decisions[idx] = make_placeholder(block, info.full_tokens);
                    }
                } else {
                    decisions[idx] = make_placeholder(block, info.full_tokens);
                }
            }
            Priority::Low => {
                if info.has_summary {
                    let stok = info.summary_tokens.unwrap_or(0);
                    if stok <= remaining {
                        decisions[idx] = RenderDecision::Summary;
                        remaining -= stok;
                    } else {
                        decisions[idx] = make_placeholder(block, info.full_tokens);
                    }
                } else {
                    decisions[idx] = make_placeholder(block, info.full_tokens);
                }
            }
            Priority::Background => {
                if PLACEHOLDER_TOKEN_COST <= remaining {
                    decisions[idx] = make_placeholder(block, info.full_tokens);
                    remaining = remaining.saturating_sub(PLACEHOLDER_TOKEN_COST);
                } else {
                    decisions[idx] = RenderDecision::Omit;
                }
            }
        }
    }

    decisions
}

/// Build a `RenderDecision::Placeholder` for a block.
fn make_placeholder(block: &Block, omitted_tokens: u32) -> RenderDecision {
    RenderDecision::Placeholder {
        block_type: block.block_type.clone(),
        description: block_description(block),
        omitted_tokens,
    }
}

// ── Public Entry Point ───────────────────────────────────────────────

/// Run the complete budget engine: resolve priorities, scan, allocate.
///
/// This is the main entry point called by `DefaultDriver::render()`.
/// It ties together the three pipeline stages:
///
/// ```text
/// all_blocks ──▶ resolve_priorities() ──▶ HashMap<u32, Priority>
///                                               │
/// filtered ──────────────────────────────▶ scan_blocks()
///                                               │
///                                         Vec<BlockBudgetInfo>
///                                               │
///                                        allocate_budget()
///                                               │
///                                        Vec<RenderDecision>
/// ```
///
/// Returns a `Vec<RenderDecision>` parallel to `filtered` — each entry
/// tells the renderer how to handle the corresponding block.
pub(crate) fn compute_budget_decisions(
    all_blocks: &[Block],
    filtered: &[&Block],
    original_indices: &[usize],
    budget: u32,
    estimator: &dyn TokenEstimator,
) -> Vec<RenderDecision> {
    let priorities = resolve_priorities(all_blocks);
    let infos = scan_blocks(filtered, &priorities, estimator, original_indices);
    allocate_budget(&infos, budget, filtered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcp_types::annotation::AnnotationBlock;
    use bcp_types::code::CodeBlock;
    use bcp_types::conversation::ConversationBlock;
    use bcp_types::enums::{Lang, Role};
    use bcp_types::summary::Summary;
    use bcp_types::tool_result::ToolResultBlock;
    use bcp_wire::block_frame::BlockFlags;

    // ── Test Helpers ────────────────────────────────────────────────

    fn code_block(path: &str, content: &[u8]) -> Block {
        Block {
            block_type: BlockType::Code,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::Code(CodeBlock {
                lang: Lang::Rust,
                path: path.to_string(),
                content: content.to_vec(),
                line_range: None,
            }),
        }
    }

    fn code_block_with_summary(path: &str, content: &[u8], summary: &str) -> Block {
        Block {
            block_type: BlockType::Code,
            flags: BlockFlags::HAS_SUMMARY,
            summary: Some(Summary {
                text: summary.to_string(),
            }),
            content: BlockContent::Code(CodeBlock {
                lang: Lang::Rust,
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

    fn priority_annotation(target: u32, priority: Priority) -> Block {
        Block {
            block_type: BlockType::Annotation,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::Annotation(AnnotationBlock {
                target_block_id: target,
                kind: AnnotationKind::Priority,
                value: vec![priority.to_wire_byte()],
            }),
        }
    }

    fn tag_annotation(target: u32, tag: &str) -> Block {
        Block {
            block_type: BlockType::Annotation,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::Annotation(AnnotationBlock {
                target_block_id: target,
                kind: AnnotationKind::Tag,
                value: tag.as_bytes().to_vec(),
            }),
        }
    }

    // ── HeuristicEstimator tests ────────────────────────────────────

    #[test]
    fn heuristic_estimator_empty() {
        assert_eq!(HeuristicEstimator.estimate(""), 0);
    }

    #[test]
    fn heuristic_estimator_min_one() {
        assert_eq!(HeuristicEstimator.estimate("a"), 1);
        assert_eq!(HeuristicEstimator.estimate("ab"), 1);
        assert_eq!(HeuristicEstimator.estimate("abc"), 1);
    }

    #[test]
    fn heuristic_estimator_basic() {
        assert_eq!(HeuristicEstimator.estimate("hello world"), 2);
        let text = "a".repeat(100);
        assert_eq!(HeuristicEstimator.estimate(&text), 25);
    }

    // ── CodeAwareEstimator tests ────────────────────────────────────

    #[test]
    fn code_aware_estimator_empty() {
        assert_eq!(CodeAwareEstimator.estimate(""), 0);
    }

    #[test]
    fn code_aware_estimator_prose() {
        let prose = "This is a paragraph of English text.\n\
                     It has no indentation at all.\n\
                     Every line starts at column zero.";
        let expected = prose.len() as u32 / 4;
        assert_eq!(CodeAwareEstimator.estimate(prose), expected);
    }

    #[test]
    fn code_aware_estimator_code() {
        let code = "fn main() {\n    let x = 42;\n    println!(\"{x}\");\n}";
        let expected = code.len() as u32 / 3;
        assert_eq!(CodeAwareEstimator.estimate(code), expected);
    }

    #[test]
    fn code_aware_estimator_min_one() {
        assert_eq!(CodeAwareEstimator.estimate("x"), 1);
    }

    #[test]
    fn code_aware_estimator_ignores_empty_lines() {
        let text = "line one\n\n\n    indented\n\n    also indented\nflat";
        let expected = text.len() as u32 / 3;
        assert_eq!(CodeAwareEstimator.estimate(text), expected);
    }

    // ── Priority Resolution tests ───────────────────────────────────

    #[test]
    fn resolve_priorities_empty() {
        let blocks: Vec<Block> = vec![];
        let map = resolve_priorities(&blocks);
        assert!(map.is_empty());
    }

    #[test]
    fn resolve_priorities_single() {
        let blocks = vec![
            code_block("main.rs", b"fn main() {}"),
            priority_annotation(0, Priority::Critical),
        ];
        let map = resolve_priorities(&blocks);
        assert_eq!(map.get(&0), Some(&Priority::Critical));
    }

    #[test]
    fn resolve_priorities_multiple_targets() {
        let blocks = vec![
            code_block("a.rs", b"a"),
            code_block("b.rs", b"b"),
            priority_annotation(0, Priority::High),
            priority_annotation(1, Priority::Low),
        ];
        let map = resolve_priorities(&blocks);
        assert_eq!(map.get(&0), Some(&Priority::High));
        assert_eq!(map.get(&1), Some(&Priority::Low));
    }

    #[test]
    fn resolve_priorities_last_wins() {
        let blocks = vec![
            code_block("main.rs", b"fn main() {}"),
            priority_annotation(0, Priority::Low),
            priority_annotation(0, Priority::Critical),
        ];
        let map = resolve_priorities(&blocks);
        assert_eq!(map.get(&0), Some(&Priority::Critical));
    }

    #[test]
    fn resolve_priorities_ignores_non_priority() {
        let blocks = vec![
            code_block("main.rs", b"fn main() {}"),
            tag_annotation(0, "security"),
        ];
        let map = resolve_priorities(&blocks);
        assert!(map.is_empty());
    }

    // ── Budget Allocation tests ─────────────────────────────────────

    #[test]
    fn allocate_budget_unlimited() {
        // Large budget: all blocks should get Full
        let blocks = vec![
            code_block("a.rs", &"a".repeat(100).into_bytes()),
            code_block("b.rs", &"b".repeat(100).into_bytes()),
        ];
        let filtered: Vec<&Block> = blocks.iter().collect();
        let original_indices = vec![0, 1];

        let decisions = compute_budget_decisions(
            &blocks,
            &filtered,
            &original_indices,
            100_000,
            &HeuristicEstimator,
        );
        assert_eq!(decisions.len(), 2);
        assert_eq!(decisions[0], RenderDecision::Full);
        assert_eq!(decisions[1], RenderDecision::Full);
    }

    #[test]
    fn allocate_budget_critical_always_full() {
        // Budget of 0, but Critical block still gets Full
        let blocks = vec![
            code_block("main.rs", &"x".repeat(400).into_bytes()),
            priority_annotation(0, Priority::Critical),
        ];
        let filtered: Vec<&Block> = blocks
            .iter()
            .filter(|b| b.block_type != BlockType::Annotation)
            .collect();
        let original_indices = vec![0];

        let decisions = compute_budget_decisions(
            &blocks,
            &filtered,
            &original_indices,
            0,
            &HeuristicEstimator,
        );
        assert_eq!(decisions[0], RenderDecision::Full);
    }

    #[test]
    fn allocate_budget_normal_degrades_to_summary() {
        // Normal block with summary, tight budget → Summary
        let content = "x".repeat(400); // ~100 tokens
        let blocks = vec![code_block_with_summary(
            "main.rs",
            content.as_bytes(),
            "Entry point.", // ~3 tokens
        )];
        let filtered: Vec<&Block> = blocks.iter().collect();
        let original_indices = vec![0];

        // Budget of 10: not enough for full (100 tokens) but enough for summary (3)
        let decisions = compute_budget_decisions(
            &blocks,
            &filtered,
            &original_indices,
            10,
            &HeuristicEstimator,
        );
        assert_eq!(decisions[0], RenderDecision::Summary);
    }

    #[test]
    fn allocate_budget_low_gets_placeholder() {
        // Low priority block without summary, tight budget → Placeholder
        let content = "x".repeat(400);
        let blocks = vec![
            code_block("main.rs", content.as_bytes()),
            priority_annotation(0, Priority::Low),
        ];
        let filtered: Vec<&Block> = blocks
            .iter()
            .filter(|b| b.block_type != BlockType::Annotation)
            .collect();
        let original_indices = vec![0];

        let decisions = compute_budget_decisions(
            &blocks,
            &filtered,
            &original_indices,
            5,
            &HeuristicEstimator,
        );
        assert!(
            matches!(decisions[0], RenderDecision::Placeholder { .. }),
            "Low priority without summary should be Placeholder, got {:?}",
            decisions[0]
        );
    }

    #[test]
    fn allocate_budget_background_omit() {
        // Background block with zero budget → Omit
        let blocks = vec![
            code_block("bg.rs", b"background stuff"),
            priority_annotation(0, Priority::Background),
        ];
        let filtered: Vec<&Block> = blocks
            .iter()
            .filter(|b| b.block_type != BlockType::Annotation)
            .collect();
        let original_indices = vec![0];

        let decisions = compute_budget_decisions(
            &blocks,
            &filtered,
            &original_indices,
            0,
            &HeuristicEstimator,
        );
        assert_eq!(decisions[0], RenderDecision::Omit);
    }

    #[test]
    fn allocate_budget_respects_priority_ordering() {
        // Critical block consumes budget before Normal block
        let big_content = "x".repeat(400); // ~100 tokens each
        let blocks = vec![
            // Block 0: Normal priority (default)
            code_block_with_summary("normal.rs", big_content.as_bytes(), "Normal summary."),
            // Block 1: Critical priority
            code_block("critical.rs", big_content.as_bytes()),
            priority_annotation(1, Priority::Critical),
        ];
        let filtered: Vec<&Block> = blocks
            .iter()
            .filter(|b| b.block_type != BlockType::Annotation)
            .collect();
        let original_indices = vec![0, 1];

        // Budget = 120: enough for one full (100) + one summary (4), not two fulls
        let decisions = compute_budget_decisions(
            &blocks,
            &filtered,
            &original_indices,
            120,
            &HeuristicEstimator,
        );
        // Critical block should be Full (processed first due to priority)
        assert_eq!(
            decisions[1],
            RenderDecision::Full,
            "Critical should be Full"
        );
        // Normal block should be Summary (budget was consumed by Critical)
        assert_eq!(
            decisions[0],
            RenderDecision::Summary,
            "Normal should degrade to Summary"
        );
    }

    #[test]
    fn no_annotations_all_normal() {
        // Without annotations, all blocks default to Normal
        let content = "x".repeat(400);
        let blocks = vec![
            code_block_with_summary("a.rs", content.as_bytes(), "Summary A."),
            code_block_with_summary("b.rs", content.as_bytes(), "Summary B."),
        ];
        let filtered: Vec<&Block> = blocks.iter().collect();
        let original_indices = vec![0, 1];

        // Budget enough for one full + one summary
        let decisions = compute_budget_decisions(
            &blocks,
            &filtered,
            &original_indices,
            120,
            &HeuristicEstimator,
        );
        // First Normal block should get Full (has budget), second should degrade
        assert_eq!(decisions[0], RenderDecision::Full);
        assert_eq!(decisions[1], RenderDecision::Summary);
    }

    #[test]
    fn block_description_code() {
        let block = code_block("src/main.rs", b"fn main() {}");
        assert_eq!(block_description(&block), "src/main.rs");
    }

    #[test]
    fn block_description_conversation() {
        let block = conversation_block(Role::User, b"Hello");
        assert_eq!(block_description(&block), "user turn");
    }

    #[test]
    fn zero_budget_all_critical() {
        // Zero budget, all Critical → all Full (Critical is never degraded)
        let blocks = vec![
            code_block("a.rs", &"a".repeat(400).into_bytes()),
            priority_annotation(0, Priority::Critical),
            code_block("b.rs", &"b".repeat(400).into_bytes()),
            priority_annotation(2, Priority::Critical),
        ];
        let filtered: Vec<&Block> = blocks
            .iter()
            .filter(|b| b.block_type != BlockType::Annotation)
            .collect();
        let original_indices = vec![0, 2];

        let decisions = compute_budget_decisions(
            &blocks,
            &filtered,
            &original_indices,
            0,
            &HeuristicEstimator,
        );
        assert_eq!(decisions[0], RenderDecision::Full);
        assert_eq!(decisions[1], RenderDecision::Full);
    }

    #[test]
    fn mixed_priorities_five_blocks() {
        // 5 blocks with mixed priorities and tight budget
        let content = "x".repeat(400); // ~100 tokens each
        let blocks = vec![
            // 0: Background
            code_block("bg.rs", content.as_bytes()),
            priority_annotation(0, Priority::Background),
            // 2: Critical
            code_block("crit.rs", content.as_bytes()),
            priority_annotation(2, Priority::Critical),
            // 4: Normal with summary
            code_block_with_summary("normal.rs", content.as_bytes(), "Normal summary."),
            // 5: Low
            code_block("low.rs", content.as_bytes()),
            priority_annotation(5, Priority::Low),
            // 7: High with summary
            code_block_with_summary("high.rs", content.as_bytes(), "High summary."),
            priority_annotation(7, Priority::High),
        ];
        let filtered: Vec<&Block> = blocks
            .iter()
            .filter(|b| b.block_type != BlockType::Annotation)
            .collect();
        let original_indices = vec![0, 2, 4, 5, 7];

        // Budget = 150: enough for Critical (100) + some leftovers
        let decisions = compute_budget_decisions(
            &blocks,
            &filtered,
            &original_indices,
            150,
            &HeuristicEstimator,
        );

        // Critical → Full (always)
        assert_eq!(
            decisions[1],
            RenderDecision::Full,
            "Critical should be Full"
        );
        // High → Summary or Full depending on remaining budget
        assert!(
            matches!(decisions[4], RenderDecision::Full | RenderDecision::Summary),
            "High should be Full or Summary, got {:?}",
            decisions[4]
        );
        // Background → Placeholder or Omit
        assert!(
            matches!(
                decisions[0],
                RenderDecision::Placeholder { .. } | RenderDecision::Omit
            ),
            "Background should be Placeholder or Omit, got {:?}",
            decisions[0]
        );
    }

    #[test]
    fn block_without_summary_at_normal_gets_placeholder() {
        // Normal block without summary, tight budget → Placeholder (not Summary)
        let content = "x".repeat(400); // ~100 tokens
        let blocks = vec![code_block("nosummary.rs", content.as_bytes())];
        let filtered: Vec<&Block> = blocks.iter().collect();
        let original_indices = vec![0];

        let decisions = compute_budget_decisions(
            &blocks,
            &filtered,
            &original_indices,
            5, // way too small
            &HeuristicEstimator,
        );
        assert!(
            matches!(decisions[0], RenderDecision::Placeholder { .. }),
            "Normal without summary should be Placeholder, got {:?}",
            decisions[0]
        );
    }

    #[test]
    fn block_description_tool_result() {
        let block = Block {
            block_type: BlockType::ToolResult,
            flags: BlockFlags::NONE,
            summary: None,
            content: BlockContent::ToolResult(ToolResultBlock {
                tool_name: "ripgrep".to_string(),
                status: bcp_types::enums::Status::Ok,
                content: b"results".to_vec(),
                schema_hint: None,
            }),
        };
        assert_eq!(block_description(&block), "ripgrep");
    }
}
