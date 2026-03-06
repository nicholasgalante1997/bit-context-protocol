#![warn(clippy::pedantic)]
//! BCP WASM bindings — exposes decode, encode, inspect, and validate
//! to JavaScript via wasm-bindgen.

use bcp_decoder::BcpDecoder;
use bcp_driver::{BcpDriver, DefaultDriver, DriverConfig, OutputMode, Verbosity};
use bcp_encoder::BcpEncoder;
use bcp_types::block::Block;
use bcp_types::enums::{Lang, Priority, Role, Status, FormatHint, DataFormat};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// ── Decode ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct DecodeConfig {
    mode: Option<String>,
    verbosity: Option<String>,
    #[serde(rename = "tokenBudget")]
    token_budget: Option<u32>,
}

#[derive(Serialize)]
struct DecodeResult {
    text: String,
}

/// Decode a .bcp payload into model-ready text.
///
/// `payload` is the raw .bcp bytes. `config_json` is an optional JSON
/// string with `{ mode?, verbosity?, tokenBudget? }`.
#[wasm_bindgen]
pub fn decode(payload: &[u8], config_json: Option<String>) -> Result<String, JsError> {
    let decoded = BcpDecoder::decode(payload).map_err(|e| JsError::new(&e.to_string()))?;

    let config = match config_json {
        Some(json) => {
            let c: DecodeConfig =
                serde_json::from_str(&json).map_err(|e| JsError::new(&e.to_string()))?;
            DriverConfig {
                mode: parse_output_mode(c.mode.as_deref()),
                verbosity: parse_verbosity(c.verbosity.as_deref()),
                token_budget: c.token_budget,
                ..DriverConfig::default()
            }
        }
        None => DriverConfig::default(),
    };

    let driver = DefaultDriver;
    let text = driver
        .render(&decoded.blocks, &config)
        .map_err(|e| JsError::new(&e.to_string()))?;

    let result = DecodeResult { text };
    serde_json::to_string(&result).map_err(|e| JsError::new(&e.to_string()))
}

// ── Inspect ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct InspectBlock {
    index: usize,
    #[serde(rename = "type")]
    block_type: String,
    size: usize,
}

#[derive(Serialize)]
struct InspectResult {
    #[serde(rename = "blockCount")]
    block_count: usize,
    #[serde(rename = "totalSize")]
    total_size: usize,
    blocks: Vec<InspectBlock>,
}

/// Inspect a .bcp payload — returns a JSON summary of blocks.
#[wasm_bindgen]
pub fn inspect(payload: &[u8]) -> Result<String, JsError> {
    let decoded = BcpDecoder::decode(payload).map_err(|e| JsError::new(&e.to_string()))?;

    let mut blocks = Vec::new();
    let mut total_size = 0;

    for (i, block) in decoded.blocks.iter().enumerate() {
        let size = block_body_size(block);
        total_size += size;
        blocks.push(InspectBlock {
            index: i,
            block_type: format!("{:?}", block.block_type),
            size,
        });
    }

    let result = InspectResult {
        block_count: decoded.blocks.len(),
        total_size,
        blocks,
    };

    serde_json::to_string(&result).map_err(|e| JsError::new(&e.to_string()))
}

// ── Validate ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ValidateResult {
    valid: bool,
    errors: Vec<String>,
}

/// Validate a .bcp payload for structural correctness.
#[wasm_bindgen]
pub fn validate(payload: &[u8]) -> String {
    match BcpDecoder::decode(payload) {
        Ok(_) => {
            let result = ValidateResult {
                valid: true,
                errors: vec![],
            };
            serde_json::to_string(&result).unwrap_or_else(|_| r#"{"valid":true,"errors":[]}"#.to_string())
        }
        Err(e) => {
            let result = ValidateResult {
                valid: false,
                errors: vec![e.to_string()],
            };
            serde_json::to_string(&result)
                .unwrap_or_else(|_| format!(r#"{{"valid":false,"errors":["{e}"]}}"#))
        }
    }
}

// ── Encode ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Manifest {
    blocks: Vec<ManifestBlock>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ManifestBlock {
    Code {
        lang: String,
        path: String,
        content: Option<String>,
        summary: Option<String>,
        priority: Option<String>,
    },
    Conversation {
        role: String,
        content: Option<String>,
        summary: Option<String>,
        priority: Option<String>,
    },
    ToolResult {
        name: String,
        status: Option<String>,
        content: Option<String>,
    },
    Document {
        title: String,
        format: Option<String>,
        content: Option<String>,
        summary: Option<String>,
        priority: Option<String>,
    },
    StructuredData {
        format: String,
        content: Option<String>,
    },
}

/// Encode a JSON manifest into a .bcp payload. Returns the raw bytes.
#[wasm_bindgen]
pub fn encode(manifest_json: &str) -> Result<Vec<u8>, JsError> {
    let manifest: Manifest =
        serde_json::from_str(manifest_json).map_err(|e| JsError::new(&e.to_string()))?;

    let mut encoder = BcpEncoder::new();

    for block in &manifest.blocks {
        apply_block(&mut encoder, block).map_err(|e| JsError::new(&e.to_string()))?;
    }

    encoder
        .encode()
        .map_err(|e| JsError::new(&e.to_string()))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn apply_block(encoder: &mut BcpEncoder, block: &ManifestBlock) -> Result<(), String> {
    match block {
        ManifestBlock::Code {
            lang,
            path,
            content,
            summary,
            priority,
        } => {
            let bytes = content.as_deref().unwrap_or("").as_bytes();
            encoder.add_code(parse_lang(lang), path, bytes);
            apply_meta(encoder, summary.as_deref(), priority.as_deref())?;
        }
        ManifestBlock::Conversation {
            role,
            content,
            summary,
            priority,
        } => {
            let bytes = content.as_deref().unwrap_or("").as_bytes();
            let role_val = parse_role(role)?;
            encoder.add_conversation(role_val, bytes);
            apply_meta(encoder, summary.as_deref(), priority.as_deref())?;
        }
        ManifestBlock::ToolResult {
            name,
            status,
            content,
        } => {
            let bytes = content.as_deref().unwrap_or("").as_bytes();
            let status_val = status.as_deref().map_or(Ok(Status::Ok), parse_status)?;
            encoder.add_tool_result(name, status_val, bytes);
        }
        ManifestBlock::Document {
            title,
            format,
            content,
            summary,
            priority,
        } => {
            let bytes = content.as_deref().unwrap_or("").as_bytes();
            let fmt = format
                .as_deref()
                .map_or(Ok(FormatHint::Markdown), parse_format_hint)?;
            encoder.add_document(title, bytes, fmt);
            apply_meta(encoder, summary.as_deref(), priority.as_deref())?;
        }
        ManifestBlock::StructuredData { format, content } => {
            let bytes = content.as_deref().unwrap_or("").as_bytes();
            let fmt = parse_data_format(format)?;
            encoder.add_structured_data(fmt, bytes);
        }
    }
    Ok(())
}

fn apply_meta(
    encoder: &mut BcpEncoder,
    summary: Option<&str>,
    priority: Option<&str>,
) -> Result<(), String> {
    if let Some(s) = summary {
        encoder
            .with_summary(s)
            .map_err(|e| format!("summary: {e}"))?;
    }
    if let Some(p) = priority {
        let prio = parse_priority(p)?;
        encoder
            .with_priority(prio)
            .map_err(|e| format!("priority: {e}"))?;
    }
    Ok(())
}

fn parse_output_mode(s: Option<&str>) -> OutputMode {
    match s {
        Some("markdown") => OutputMode::Markdown,
        Some("minimal") => OutputMode::Minimal,
        _ => OutputMode::Xml,
    }
}

fn parse_verbosity(s: Option<&str>) -> Verbosity {
    match s {
        Some("full") => Verbosity::Full,
        Some("summary") => Verbosity::Summary,
        _ => Verbosity::Adaptive,
    }
}

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

fn parse_role(s: &str) -> Result<Role, String> {
    match s.to_lowercase().as_str() {
        "system" => Ok(Role::System),
        "user" => Ok(Role::User),
        "assistant" => Ok(Role::Assistant),
        "tool" => Ok(Role::Tool),
        _ => Err(format!("unknown role: {s}")),
    }
}

fn parse_status(s: &str) -> Result<Status, String> {
    match s.to_lowercase().as_str() {
        "ok" => Ok(Status::Ok),
        "error" => Ok(Status::Error),
        "timeout" => Ok(Status::Timeout),
        _ => Err(format!("unknown status: {s}")),
    }
}

fn parse_priority(s: &str) -> Result<Priority, String> {
    match s.to_lowercase().as_str() {
        "critical" => Ok(Priority::Critical),
        "high" => Ok(Priority::High),
        "normal" => Ok(Priority::Normal),
        "low" => Ok(Priority::Low),
        "background" => Ok(Priority::Background),
        _ => Err(format!("unknown priority: {s}")),
    }
}

fn parse_format_hint(s: &str) -> Result<FormatHint, String> {
    match s.to_lowercase().as_str() {
        "markdown" | "md" => Ok(FormatHint::Markdown),
        "plain" | "text" => Ok(FormatHint::Plain),
        "html" => Ok(FormatHint::Html),
        _ => Err(format!("unknown format hint: {s}")),
    }
}

fn parse_data_format(s: &str) -> Result<DataFormat, String> {
    match s.to_lowercase().as_str() {
        "json" => Ok(DataFormat::Json),
        "yaml" | "yml" => Ok(DataFormat::Yaml),
        "toml" => Ok(DataFormat::Toml),
        "csv" => Ok(DataFormat::Csv),
        _ => Err(format!("unknown data format: {s}")),
    }
}

fn block_body_size(block: &Block) -> usize {
    use bcp_types::block::BlockContent;
    match &block.content {
        BlockContent::Code(b) => b.content.len(),
        BlockContent::Conversation(b) => b.content.len(),
        BlockContent::FileTree(_) => 0, // size is structural
        BlockContent::ToolResult(b) => b.content.len(),
        BlockContent::Document(b) => b.content.len(),
        BlockContent::StructuredData(b) => b.content.len(),
        BlockContent::Diff(b) => b.hunks.iter().map(|h| h.lines.len()).sum(),
        BlockContent::Annotation(_) => 0,
        BlockContent::EmbeddingRef(_) => 32,
        BlockContent::Image(b) => b.data.len(),
        BlockContent::Extension(b) => b.content.len(),
        BlockContent::End => 0,
        BlockContent::Unknown { body, .. } => body.len(),
    }
}
