/// Implementation of `bcp encode`.
///
/// Parses a JSON manifest describing a sequence of BCP blocks and serialises
/// them into a binary `.bcp` payload using `BcpEncoder`. The manifest path is
/// the sole positional argument; the output file is required via `-o`.
///
/// # Manifest format
///
/// ```json
/// {
///   "blocks": [
///     {
///       "type": "code",
///       "lang": "rust",
///       "path": "src/main.rs",
///       "content": "fn main() {}",
///       "summary": "Entry point",
///       "priority": "high"
///     },
///     {
///       "type": "conversation",
///       "role": "user",
///       "content": "Fix the timeout bug."
///     },
///     {
///       "type": "tool_result",
///       "name": "ripgrep",
///       "status": "ok",
///       "content": "src/main.rs:42: let timeout = 30;"
///     },
///     {
///       "type": "document",
///       "title": "API Reference",
///       "format": "markdown",
///       "content": "# API\n..."
///     },
///     {
///       "type": "structured_data",
///       "format": "json",
///       "content": "{\"key\": \"value\"}"
///     }
///   ]
/// }
/// ```
///
/// The `content_file` key may substitute `content` for any block that accepts
/// text — the encoder reads the file at the given path relative to the
/// manifest file's parent directory.
///
/// # Supported block types
///
/// ```text
/// ┌──────────────────┬──────────────────────────────────────────────────────┐
/// │ Type             │ Required fields                                      │
/// ├──────────────────┼──────────────────────────────────────────────────────┤
/// │ code             │ lang, path, content (or content_file)                │
/// │ conversation     │ role, content (or content_file)                      │
/// │ tool_result      │ name, content (or content_file)                      │
/// │ document         │ title, content (or content_file)                     │
/// │ structured_data  │ format, content (or content_file)                    │
/// └──────────────────┴──────────────────────────────────────────────────────┘
/// ```
///
/// Optional fields for all blocks: `summary` (string), `priority`
/// (`critical` | `high` | `normal` | `low` | `background`).
///
/// # Flags
///
/// ```text
/// ┌──────────────────────┬─────────────────────────────────────────────┐
/// │ Flag                 │ Effect                                      │
/// ├──────────────────────┼─────────────────────────────────────────────┤
/// │ --compress-blocks    │ zstd-compress each block body individually  │
/// │ --compress-payload   │ zstd-compress all blocks as one stream      │
/// │ --dedup              │ BLAKE3 dedup via in-memory content store    │
/// └──────────────────────┴─────────────────────────────────────────────┘
/// ```
use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use bcp_encoder::{BcpEncoder, MemoryContentStore};
use bcp_types::enums::{DataFormat, FormatHint, Lang, Priority, Role, Status};

use crate::EncodeArgs;

// ── Manifest serde types ──────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct Manifest {
    blocks: Vec<ManifestBlock>,
}

/// A single block entry in the JSON manifest.
///
/// The `type` field selects the variant. Optional fields (`summary`,
/// `priority`, `content_file`) are shared across variants but only
/// meaningful where documented.
#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ManifestBlock {
    /// Source code block. `lang` must be a recognised language name.
    Code {
        lang: String,
        path: String,
        /// Inline text content. Mutually exclusive with `content_file`.
        content: Option<String>,
        /// Read content from this path (relative to manifest dir).
        content_file: Option<String>,
        summary: Option<String>,
        priority: Option<String>,
    },
    /// Conversation turn (system / user / assistant / tool).
    Conversation {
        role: String,
        content: Option<String>,
        content_file: Option<String>,
        summary: Option<String>,
        priority: Option<String>,
    },
    /// Tool invocation result.
    ToolResult {
        name: String,
        /// `ok` | `error` | `timeout`. Defaults to `ok`.
        status: Option<String>,
        content: Option<String>,
        content_file: Option<String>,
    },
    /// Free-form document (prose, references, specs).
    Document {
        title: String,
        /// `markdown` | `plain` | `html`. Defaults to `markdown`.
        format: Option<String>,
        content: Option<String>,
        content_file: Option<String>,
        summary: Option<String>,
        priority: Option<String>,
    },
    /// Structured / tabular data.
    StructuredData {
        /// `json` | `yaml` | `toml` | `csv`.
        format: String,
        content: Option<String>,
        content_file: Option<String>,
    },
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Run the `bcp encode` command.
///
/// Reads and parses the JSON manifest, builds an [`BcpEncoder`], applies
/// compression / dedup flags, and writes the encoded payload to the output
/// file. Prints a one-line summary (`Wrote N bytes to <path>`) on success.
///
/// # Errors
///
/// Returns an error if the manifest cannot be read or parsed, if a block
/// references a `content_file` that does not exist, if an enum value
/// (lang, role, priority, …) is unrecognised, or if `BcpEncoder::encode`
/// fails (e.g. zstd compression error).
pub fn run(args: &EncodeArgs) -> Result<()> {
    let manifest_src = fs::read_to_string(&args.input)
        .with_context(|| format!("cannot read {}", args.input.display()))?;

    let manifest: Manifest = serde_json::from_str(&manifest_src)
        .with_context(|| format!("failed to parse manifest {}", args.input.display()))?;

    let manifest_dir = args.input.parent().unwrap_or_else(|| Path::new("."));

    let mut encoder = BcpEncoder::new();

    // When --dedup is requested a MemoryContentStore must be provided before
    // any encoding; auto_dedup() alone without a store would error at encode().
    if args.dedup {
        let store = Arc::new(MemoryContentStore::new());
        encoder.set_content_store(store);
        encoder.auto_dedup();
    }

    if args.compress_blocks {
        encoder.compress_blocks();
    }
    if args.compress_payload {
        encoder.compress_payload();
    }

    for (idx, block) in manifest.blocks.iter().enumerate() {
        apply_block(&mut encoder, block, manifest_dir)
            .with_context(|| format!("block {idx}: failed to apply"))?;
    }

    let bytes = encoder
        .encode()
        .with_context(|| "BcpEncoder::encode failed")?;

    fs::write(&args.output, &bytes)
        .with_context(|| format!("cannot write {}", args.output.display()))?;

    println!("Wrote {} bytes to {}", bytes.len(), args.output.display());
    Ok(())
}

// ── Block application helpers ─────────────────────────────────────────────────

/// Adds a single manifest block to `encoder`, resolving content from inline
/// text or a `content_file` path relative to `manifest_dir`.
fn apply_block(encoder: &mut BcpEncoder, block: &ManifestBlock, manifest_dir: &Path) -> Result<()> {
    match block {
        ManifestBlock::Code {
            lang,
            path,
            content,
            content_file,
            summary,
            priority,
        } => {
            let bytes = resolve_content(
                content.as_deref(),
                content_file.as_deref(),
                manifest_dir,
                "code",
            )?;
            let lang_val = parse_lang(lang);
            encoder.add_code(lang_val, path, &bytes);
            apply_meta(encoder, summary.as_deref(), priority.as_deref())?;
        }

        ManifestBlock::Conversation {
            role,
            content,
            content_file,
            summary,
            priority,
        } => {
            let bytes = resolve_content(
                content.as_deref(),
                content_file.as_deref(),
                manifest_dir,
                "conversation",
            )?;
            let role_val = parse_role(role)?;
            encoder.add_conversation(role_val, &bytes);
            apply_meta(encoder, summary.as_deref(), priority.as_deref())?;
        }

        ManifestBlock::ToolResult {
            name,
            status,
            content,
            content_file,
        } => {
            let bytes = resolve_content(
                content.as_deref(),
                content_file.as_deref(),
                manifest_dir,
                "tool_result",
            )?;
            let status_val = status.as_deref().map_or(Ok(Status::Ok), parse_status)?;
            encoder.add_tool_result(name, status_val, &bytes);
        }

        ManifestBlock::Document {
            title,
            format,
            content,
            content_file,
            summary,
            priority,
        } => {
            let bytes = resolve_content(
                content.as_deref(),
                content_file.as_deref(),
                manifest_dir,
                "document",
            )?;
            let fmt = format
                .as_deref()
                .map_or(Ok(FormatHint::Markdown), parse_format_hint)?;
            encoder.add_document(title, &bytes, fmt);
            apply_meta(encoder, summary.as_deref(), priority.as_deref())?;
        }

        ManifestBlock::StructuredData {
            format,
            content,
            content_file,
        } => {
            let bytes = resolve_content(
                content.as_deref(),
                content_file.as_deref(),
                manifest_dir,
                "structured_data",
            )?;
            let fmt = parse_data_format(format)?;
            encoder.add_structured_data(fmt, &bytes);
        }
    }

    Ok(())
}

/// Applies `summary` and `priority` metadata to the most recently added block.
fn apply_meta(
    encoder: &mut BcpEncoder,
    summary: Option<&str>,
    priority: Option<&str>,
) -> Result<()> {
    if let Some(s) = summary {
        encoder.with_summary(s)?;
    }
    if let Some(p) = priority {
        encoder.with_priority(parse_priority(p)?)?;
    }
    Ok(())
}

// ── Content resolution ────────────────────────────────────────────────────────

/// Returns the UTF-8 bytes for a block's content field.
///
/// Prefers `content` (inline string) over `content_file` (file path).
/// Returns an error if neither is provided or if the file cannot be read.
fn resolve_content(
    content: Option<&str>,
    content_file: Option<&str>,
    manifest_dir: &Path,
    block_type: &str,
) -> Result<Vec<u8>> {
    if let Some(text) = content {
        return Ok(text.as_bytes().to_vec());
    }
    if let Some(path_str) = content_file {
        let path = manifest_dir.join(path_str);
        return fs::read(&path)
            .with_context(|| format!("cannot read content_file {}", path.display()));
    }
    Err(anyhow!(
        "{block_type} block requires either \"content\" or \"content_file\""
    ))
}

// ── Enum parsers ──────────────────────────────────────────────────────────────

/// Maps a language name string to a [`Lang`] variant.
///
/// Unrecognised names map to [`Lang::Unknown`] rather than erroring, so
/// arbitrary language tags in manifests produce a valid (if opaque) block.
fn parse_lang(s: &str) -> Lang {
    match s.to_lowercase().as_str() {
        "rust" => Lang::Rust,
        "typescript" | "ts" => Lang::TypeScript,
        "javascript" | "js" => Lang::JavaScript,
        "python" | "py" => Lang::Python,
        "go" => Lang::Go,
        "java" => Lang::Java,
        "c" => Lang::C,
        "cpp" | "c++" => Lang::Cpp,
        "ruby" | "rb" => Lang::Ruby,
        "shell" | "sh" | "bash" => Lang::Shell,
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

/// Parses a conversation role name.
///
/// # Errors
///
/// Returns an error for unrecognised role names, since an invalid role
/// would produce a structurally incorrect block.
fn parse_role(s: &str) -> Result<Role> {
    match s.to_lowercase().as_str() {
        "system" => Ok(Role::System),
        "user" => Ok(Role::User),
        "assistant" => Ok(Role::Assistant),
        "tool" => Ok(Role::Tool),
        _ => Err(anyhow!(
            "unknown role {s:?} — expected system|user|assistant|tool"
        )),
    }
}

/// Parses a tool result status name. Defaults to `ok`.
///
/// # Errors
///
/// Returns an error for unrecognised status names.
fn parse_status(s: &str) -> Result<Status> {
    match s.to_lowercase().as_str() {
        "ok" => Ok(Status::Ok),
        "error" | "err" => Ok(Status::Error),
        "timeout" => Ok(Status::Timeout),
        _ => Err(anyhow!("unknown status {s:?} — expected ok|error|timeout")),
    }
}

/// Parses a block priority name.
///
/// # Errors
///
/// Returns an error for unrecognised priority names.
fn parse_priority(s: &str) -> Result<Priority> {
    match s.to_lowercase().as_str() {
        "critical" => Ok(Priority::Critical),
        "high" => Ok(Priority::High),
        "normal" => Ok(Priority::Normal),
        "low" => Ok(Priority::Low),
        "background" => Ok(Priority::Background),
        _ => Err(anyhow!(
            "unknown priority {s:?} — expected critical|high|normal|low|background"
        )),
    }
}

/// Parses a document format hint name. Defaults to `markdown`.
///
/// # Errors
///
/// Returns an error for unrecognised format names.
fn parse_format_hint(s: &str) -> Result<FormatHint> {
    match s.to_lowercase().as_str() {
        "markdown" | "md" => Ok(FormatHint::Markdown),
        "plain" | "text" | "txt" => Ok(FormatHint::Plain),
        "html" => Ok(FormatHint::Html),
        _ => Err(anyhow!(
            "unknown document format {s:?} — expected markdown|plain|html"
        )),
    }
}

/// Parses a structured data format name.
///
/// # Errors
///
/// Returns an error for unrecognised format names.
fn parse_data_format(s: &str) -> Result<DataFormat> {
    match s.to_lowercase().as_str() {
        "json" => Ok(DataFormat::Json),
        "yaml" | "yml" => Ok(DataFormat::Yaml),
        "toml" => Ok(DataFormat::Toml),
        "csv" => Ok(DataFormat::Csv),
        _ => Err(anyhow!(
            "unknown data format {s:?} — expected json|yaml|toml|csv"
        )),
    }
}
