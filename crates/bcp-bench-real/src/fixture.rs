use std::path::Path;

use anyhow::{Context, Result};
use bcp_encoder::BcpEncoder;
use bcp_types::enums::{DataFormat, FormatHint, Lang, Role, Status};
use bcp_types::file_tree::{FileEntry, FileEntryKind};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SessionFixture {
    #[allow(dead_code)]
    pub description: String,
    pub blocks: Vec<FixtureBlock>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FixtureBlock {
    Code {
        language: String,
        path: String,
        content: String,
        #[serde(default)]
        summary: Option<String>,
    },
    Conversation {
        role: String,
        content: String,
    },
    ToolResult {
        tool_name: String,
        status: String,
        content: String,
        #[serde(default)]
        summary: Option<String>,
    },
    FileTree {
        root_path: String,
        entries: Vec<FixtureEntry>,
    },
    Document {
        title: String,
        content: String,
        #[serde(default)]
        format_hint: Option<String>,
    },
    StructuredData {
        format: String,
        content: String,
    },
    Diff {
        path: String,
        hunks: Vec<FixtureHunk>,
    },
}

#[derive(Deserialize)]
pub struct FixtureEntry {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub children: Vec<FixtureEntry>,
}

#[derive(Deserialize)]
pub struct FixtureHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: String,
}

pub fn encode_fixture(path: &Path) -> Result<Vec<u8>> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("reading fixture {}", path.display()))?;
    let fixture: SessionFixture =
        serde_json::from_str(&json).context("parsing fixture JSON")?;
    encode_session(&fixture)
}

pub fn encode_session(fixture: &SessionFixture) -> Result<Vec<u8>> {
    let mut encoder = BcpEncoder::new();

    for block in &fixture.blocks {
        match block {
            FixtureBlock::Code {
                language,
                path,
                content,
                summary,
            } => {
                let lang = parse_lang(language);
                encoder.add_code(lang, path, content.as_bytes());
                if let Some(s) = summary {
                    encoder.with_summary(s)?;
                }
            }
            FixtureBlock::Conversation { role, content } => {
                let r = parse_role(role);
                encoder.add_conversation(r, content.as_bytes());
            }
            FixtureBlock::ToolResult {
                tool_name,
                status,
                content,
                summary,
            } => {
                let s = parse_status(status);
                encoder.add_tool_result(tool_name, s, content.as_bytes());
                if let Some(sm) = summary {
                    encoder.with_summary(sm)?;
                }
            }
            FixtureBlock::FileTree { root_path, entries } => {
                let converted = entries.iter().map(convert_entry).collect();
                encoder.add_file_tree(root_path, converted);
            }
            FixtureBlock::Document {
                title,
                content,
                format_hint,
            } => {
                let hint = format_hint
                    .as_deref()
                    .map_or(FormatHint::Plain, parse_format_hint);
                encoder.add_document(title, content.as_bytes(), hint);
            }
            FixtureBlock::StructuredData { format, content } => {
                let fmt = parse_data_format(format);
                encoder.add_structured_data(fmt, content.as_bytes());
            }
            FixtureBlock::Diff { path, hunks } => {
                let converted = hunks
                    .iter()
                    .map(|h| bcp_types::diff::DiffHunk {
                        old_start: h.old_start,
                        new_start: h.new_start,
                        lines: h.lines.as_bytes().to_vec(),
                    })
                    .collect();
                encoder.add_diff(path, converted);
            }
        }
    }

    Ok(encoder.encode()?)
}

fn convert_entry(e: &FixtureEntry) -> FileEntry {
    let kind = if e.kind == "dir" || e.kind == "directory" {
        FileEntryKind::Directory
    } else {
        FileEntryKind::File
    };
    FileEntry {
        name: e.name.clone(),
        kind,
        size: e.size,
        children: e.children.iter().map(convert_entry).collect(),
    }
}

fn parse_lang(s: &str) -> Lang {
    match s.to_lowercase().as_str() {
        "rust" | "rs" => Lang::Rust,
        "typescript" | "ts" => Lang::TypeScript,
        "javascript" | "js" => Lang::JavaScript,
        "python" | "py" => Lang::Python,
        "go" => Lang::Go,
        "java" => Lang::Java,
        "c" => Lang::C,
        "cpp" | "c++" => Lang::Cpp,
        "ruby" | "rb" => Lang::Ruby,
        "shell" | "bash" | "sh" => Lang::Shell,
        "sql" => Lang::Sql,
        "html" => Lang::Html,
        "css" => Lang::Css,
        "json" => Lang::Json,
        "yaml" | "yml" => Lang::Yaml,
        "toml" => Lang::Toml,
        "markdown" | "md" => Lang::Markdown,
        _ => Lang::Unknown,
    }
}

fn parse_role(s: &str) -> Role {
    match s.to_lowercase().as_str() {
        "system" => Role::System,
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        _ => Role::User,
    }
}

fn parse_status(s: &str) -> Status {
    match s.to_lowercase().as_str() {
        "ok" | "success" => Status::Ok,
        "error" | "fail" | "failed" => Status::Error,
        "timeout" => Status::Timeout,
        _ => Status::Ok,
    }
}

fn parse_format_hint(s: &str) -> FormatHint {
    match s.to_lowercase().as_str() {
        "markdown" | "md" => FormatHint::Markdown,
        "html" => FormatHint::Html,
        _ => FormatHint::Plain,
    }
}

fn parse_data_format(s: &str) -> DataFormat {
    match s.to_lowercase().as_str() {
        "json" => DataFormat::Json,
        "yaml" | "yml" => DataFormat::Yaml,
        "toml" => DataFormat::Toml,
        "csv" => DataFormat::Csv,
        _ => DataFormat::Json,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lang_known_variants() {
        assert_eq!(parse_lang("rust"), Lang::Rust);
        assert_eq!(parse_lang("TypeScript"), Lang::TypeScript);
        assert_eq!(parse_lang("py"), Lang::Python);
        assert_eq!(parse_lang("unknown_lang"), Lang::Unknown);
    }

    #[test]
    fn parse_role_known_variants() {
        assert_eq!(parse_role("user"), Role::User);
        assert_eq!(parse_role("Assistant"), Role::Assistant);
    }

    #[test]
    fn encode_minimal_fixture() {
        let fixture = SessionFixture {
            description: "test".into(),
            blocks: vec![FixtureBlock::Conversation {
                role: "user".into(),
                content: "hello".into(),
            }],
        };
        let payload = encode_session(&fixture).unwrap();
        assert!(payload.len() > 8);
    }
}
