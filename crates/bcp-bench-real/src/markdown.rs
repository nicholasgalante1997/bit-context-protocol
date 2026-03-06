use std::fmt::Write;

use bcp_types::block::{Block, BlockContent};

/// Build naive markdown equivalent — triple backticks, ### headers.
///
/// Represents how most tools dump context into a prompt:
/// fenced code blocks, role headers, bullet-list tool results.
/// This is the "strawman" baseline — easy to beat.
pub fn build_naive_markdown(blocks: &[Block]) -> String {
    let mut out = String::new();

    for block in blocks {
        match &block.content {
            BlockContent::Code(c) => {
                let lang = lang_name(c.lang);
                let _ = writeln!(out, "```{lang}\n// {}", c.path);
                out.push_str(&lossy(&c.content));
                out.push_str("\n```\n\n");
            }
            BlockContent::Conversation(c) => {
                let role = role_name(c.role);
                let _ = writeln!(out, "### {role}:\n");
                out.push_str(&lossy(&c.content));
                out.push_str("\n\n");
            }
            BlockContent::ToolResult(t) => {
                let status = status_name(t.status);
                let _ = writeln!(out, "### Tool Result ({}) [{status}]:\n\n```", t.tool_name);
                out.push_str(&lossy(&t.content));
                out.push_str("\n```\n\n");
            }
            BlockContent::FileTree(ft) => {
                let _ = writeln!(out, "### File Tree: {}\n\n```", ft.root_path);
                render_tree_entries_naive(&mut out, &ft.entries, 0);
                out.push_str("```\n\n");
            }
            BlockContent::Document(d) => {
                let _ = writeln!(out, "## {}\n", d.title);
                out.push_str(&lossy(&d.content));
                out.push_str("\n\n");
            }
            BlockContent::StructuredData(s) => {
                let fmt = data_format_name(s.format);
                let _ = writeln!(out, "```{fmt}");
                out.push_str(&lossy(&s.content));
                out.push_str("\n```\n\n");
            }
            BlockContent::Diff(d) => {
                let _ = writeln!(out, "### Diff: {}\n\n```diff", d.path);
                write_hunks(&mut out, &d.hunks);
                out.push_str("```\n\n");
            }
            BlockContent::EmbeddingRef(e) => {
                let _ = writeln!(
                    out,
                    "[embedding: model={}, vector_id={} bytes]\n",
                    e.model,
                    e.vector_id.len()
                );
            }
            BlockContent::Image(img) => {
                let _ = writeln!(
                    out,
                    "![{}](data:{};base64,...{} bytes)\n",
                    img.alt_text,
                    media_type_name(img.media_type),
                    img.data.len()
                );
            }
            BlockContent::Extension(ext) => {
                let _ = writeln!(out, "### Extension ({}/{})\n\n```", ext.namespace, ext.type_name);
                out.push_str(&lossy(&ext.content));
                out.push_str("\n```\n\n");
            }
            BlockContent::Annotation(_) | BlockContent::End | BlockContent::Unknown { .. } => {}
        }
    }

    out
}

/// Build realistic agent markdown — mimics Claude Code's actual context format.
///
/// This is the fairer comparison because it represents what models actually
/// receive today. Includes XML-style source tags, JSON tool-call envelopes,
/// and message wrappers with role attributes.
pub fn build_realistic_markdown(blocks: &[Block]) -> String {
    let mut out = String::new();
    out.push_str("<context>\n");

    for block in blocks {
        match &block.content {
            BlockContent::Code(c) => {
                let lang = lang_name(c.lang);
                let _ = writeln!(out, "<source path=\"{}\" language=\"{lang}\">\n```{lang}", c.path);
                out.push_str(&lossy(&c.content));
                out.push_str("\n```\n</source>\n\n");
            }
            BlockContent::Conversation(c) => {
                let role = role_name(c.role);
                let _ = writeln!(out, "<message role=\"{role}\">");
                out.push_str(&lossy(&c.content));
                out.push_str("\n</message>\n\n");
            }
            BlockContent::ToolResult(t) => {
                let status = status_name(t.status);
                let content_str = lossy(&t.content);
                let json_content =
                    serde_json::to_string(&content_str).unwrap_or_else(|_| content_str.clone());
                let _ = write!(
                    out,
                    "<tool_result>\n{{\n  \"tool\": \"{}\",\n  \"status\": \"{status}\",\n  \"output\": {json_content}\n}}\n</tool_result>\n\n",
                    t.tool_name
                );
            }
            BlockContent::FileTree(ft) => {
                let _ = writeln!(out, "<file_tree root=\"{}\">\n```", ft.root_path);
                render_tree_entries_naive(&mut out, &ft.entries, 0);
                out.push_str("```\n</file_tree>\n\n");
            }
            BlockContent::Document(d) => {
                let _ = writeln!(out, "<document title=\"{}\">", d.title);
                out.push_str(&lossy(&d.content));
                out.push_str("\n</document>\n\n");
            }
            BlockContent::StructuredData(s) => {
                let fmt = data_format_name(s.format);
                let _ = writeln!(out, "<data format=\"{fmt}\">");
                out.push_str(&lossy(&s.content));
                out.push_str("\n</data>\n\n");
            }
            BlockContent::Diff(d) => {
                let _ = writeln!(out, "<diff path=\"{}\">\n```diff", d.path);
                write_hunks(&mut out, &d.hunks);
                out.push_str("```\n</diff>\n\n");
            }
            BlockContent::Extension(ext) => {
                let _ = writeln!(
                    out,
                    "<extension namespace=\"{}\" type=\"{}\">",
                    ext.namespace, ext.type_name
                );
                out.push_str(&lossy(&ext.content));
                out.push_str("\n</extension>\n\n");
            }
            BlockContent::Annotation(_) | BlockContent::End | BlockContent::Unknown { .. } => {}
            _ => {}
        }
    }

    out.push_str("</context>\n");
    out
}

fn write_hunks(out: &mut String, hunks: &[bcp_types::diff::DiffHunk]) {
    for hunk in hunks {
        let _ = writeln!(
            out,
            "@@ -{},0 +{},0 @@",
            hunk.old_start, hunk.new_start
        );
        out.push_str(&lossy(&hunk.lines));
        if !hunk.lines.ends_with(b"\n") {
            out.push('\n');
        }
    }
}

fn render_tree_entries_naive(
    out: &mut String,
    entries: &[bcp_types::file_tree::FileEntry],
    depth: usize,
) {
    for entry in entries {
        let indent = "  ".repeat(depth);
        let suffix = if entry.kind == bcp_types::file_tree::FileEntryKind::Directory {
            "/"
        } else {
            ""
        };
        let _ = writeln!(out, "{indent}{}{suffix}", entry.name);
        if !entry.children.is_empty() {
            render_tree_entries_naive(out, &entry.children, depth + 1);
        }
    }
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn lang_name(lang: bcp_types::enums::Lang) -> &'static str {
    match lang {
        bcp_types::enums::Lang::Rust => "rust",
        bcp_types::enums::Lang::TypeScript => "typescript",
        bcp_types::enums::Lang::JavaScript => "javascript",
        bcp_types::enums::Lang::Python => "python",
        bcp_types::enums::Lang::Go => "go",
        bcp_types::enums::Lang::Java => "java",
        bcp_types::enums::Lang::C => "c",
        bcp_types::enums::Lang::Cpp => "cpp",
        bcp_types::enums::Lang::Ruby => "ruby",
        bcp_types::enums::Lang::Shell => "shell",
        bcp_types::enums::Lang::Sql => "sql",
        bcp_types::enums::Lang::Html => "html",
        bcp_types::enums::Lang::Css => "css",
        bcp_types::enums::Lang::Json => "json",
        bcp_types::enums::Lang::Yaml => "yaml",
        bcp_types::enums::Lang::Toml => "toml",
        bcp_types::enums::Lang::Markdown => "markdown",
        bcp_types::enums::Lang::Unknown | bcp_types::enums::Lang::Other(_) => "text",
    }
}

fn role_name(role: bcp_types::enums::Role) -> &'static str {
    match role {
        bcp_types::enums::Role::System => "system",
        bcp_types::enums::Role::User => "user",
        bcp_types::enums::Role::Assistant => "assistant",
        bcp_types::enums::Role::Tool => "tool",
    }
}

fn status_name(status: bcp_types::enums::Status) -> &'static str {
    match status {
        bcp_types::enums::Status::Ok => "ok",
        bcp_types::enums::Status::Error => "error",
        bcp_types::enums::Status::Timeout => "timeout",
    }
}

fn data_format_name(fmt: bcp_types::enums::DataFormat) -> &'static str {
    match fmt {
        bcp_types::enums::DataFormat::Json => "json",
        bcp_types::enums::DataFormat::Yaml => "yaml",
        bcp_types::enums::DataFormat::Toml => "toml",
        bcp_types::enums::DataFormat::Csv => "csv",
    }
}

fn media_type_name(mt: bcp_types::enums::MediaType) -> &'static str {
    match mt {
        bcp_types::enums::MediaType::Png => "image/png",
        bcp_types::enums::MediaType::Jpeg => "image/jpeg",
        bcp_types::enums::MediaType::Gif => "image/gif",
        bcp_types::enums::MediaType::Svg => "image/svg+xml",
        bcp_types::enums::MediaType::Webp => "image/webp",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bcp_types::block::BlockContent;
    use bcp_types::code::CodeBlock;
    use bcp_types::conversation::ConversationBlock;
    use bcp_types::enums::{Lang, Role};
    use bcp_wire::block_frame::BlockFlags;

    fn make_block(content: BlockContent) -> Block {
        Block {
            block_type: bcp_types::BlockType::Code,
            flags: BlockFlags::NONE,
            summary: None,
            content,
        }
    }

    #[test]
    fn naive_markdown_code_block() {
        let blocks = vec![make_block(BlockContent::Code(CodeBlock {
            lang: Lang::Rust,
            path: "src/main.rs".into(),
            content: b"fn main() {}".to_vec(),
            line_range: None,
        }))];
        let md = build_naive_markdown(&blocks);
        assert!(md.contains("```rust"));
        assert!(md.contains("// src/main.rs"));
        assert!(md.contains("fn main() {}"));
    }

    #[test]
    fn realistic_markdown_has_context_wrapper() {
        let blocks = vec![make_block(BlockContent::Conversation(
            ConversationBlock {
                role: Role::User,
                content: b"hello".to_vec(),
                tool_call_id: None,
            },
        ))];
        let md = build_realistic_markdown(&blocks);
        assert!(md.starts_with("<context>"));
        assert!(md.contains("<message role=\"user\">"));
        assert!(md.ends_with("</context>\n"));
    }
}
