use bcp_types::BlockType;

use crate::budget::block_type_label;
use crate::config::OutputMode;

/// Render a placeholder string for an omitted block.
///
/// When a block is over budget and has no summary (or its summary is
/// also too expensive), the driver emits a placeholder — a compact
/// one-line notice that tells the model what was omitted and how much
/// content was skipped.
///
/// The format varies by output mode to match the surrounding structure:
///
/// ```text
/// ┌──────────┬────────────────────────────────────────────────────────┐
/// │ Mode     │ Output                                                 │
/// ├──────────┼────────────────────────────────────────────────────────┤
/// │ Xml      │ <omitted type="code" desc="src/main.rs" tokens="823"/>│
/// │ Markdown │ _[Omitted: code src/main.rs, ~823 tokens]_            │
/// │ Minimal  │ [omitted: code src/main.rs ~823tok]                   │
/// └──────────┴────────────────────────────────────────────────────────┘
/// ```
///
/// Placeholders are intentionally cheap: they cost roughly 10-15 tokens
/// regardless of the omitted block's size. This lets the model know
/// that context exists without paying the full token cost.
pub(crate) fn render_placeholder(
    mode: OutputMode,
    block_type: &BlockType,
    description: &str,
    omitted_tokens: u32,
) -> String {
    let type_label = block_type_label(block_type);
    match mode {
        OutputMode::Xml => format!(
            "<omitted type=\"{type_label}\" desc=\"{description}\" tokens=\"{omitted_tokens}\" />"
        ),
        OutputMode::Markdown => {
            format!("_[Omitted: {type_label} {description}, ~{omitted_tokens} tokens]_")
        }
        OutputMode::Minimal => {
            format!("[omitted: {type_label} {description} ~{omitted_tokens}tok]")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_xml_format() {
        let result = render_placeholder(OutputMode::Xml, &BlockType::Code, "src/main.rs", 823);
        assert_eq!(
            result,
            "<omitted type=\"code\" desc=\"src/main.rs\" tokens=\"823\" />"
        );
    }

    #[test]
    fn placeholder_markdown_format() {
        let result = render_placeholder(OutputMode::Markdown, &BlockType::Code, "src/main.rs", 823);
        assert_eq!(result, "_[Omitted: code src/main.rs, ~823 tokens]_");
    }

    #[test]
    fn placeholder_minimal_format() {
        let result = render_placeholder(OutputMode::Minimal, &BlockType::Code, "src/main.rs", 823);
        assert_eq!(result, "[omitted: code src/main.rs ~823tok]");
    }

    #[test]
    fn placeholder_tool_result() {
        let result = render_placeholder(OutputMode::Xml, &BlockType::ToolResult, "ripgrep", 150);
        assert_eq!(
            result,
            "<omitted type=\"tool-result\" desc=\"ripgrep\" tokens=\"150\" />"
        );
    }
}
