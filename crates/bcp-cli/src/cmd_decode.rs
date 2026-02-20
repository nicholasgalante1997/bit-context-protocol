/// Implementation of `bcp decode`.
///
/// Reads a BCP file, decodes all blocks with `BcpDecoder`, then passes
/// the block slice to `DefaultDriver::render` for model-ready text
/// output. The output is written to stdout or to `-o <file>`.
///
/// # Output modes
///
/// ```text
/// ┌──────────┬──────────────────────────────────────────────────────────────┐
/// │ Mode     │ Format                                                       │
/// ├──────────┼──────────────────────────────────────────────────────────────┤
/// │ xml      │ <code lang="rust" path="...">...</code>   (default)          │
/// │ markdown │ ```rust\n// src/main.rs\n...\n```                            │
/// │ minimal  │ --- src/main.rs [rust] ---\n...                              │
/// └──────────┴──────────────────────────────────────────────────────────────┘
/// ```
///
/// # Verbosity and budget
///
/// ```text
/// ┌──────────┬────────────────────────────────────────────────────────────┐
/// │ Verbosity│ Behaviour                                                  │
/// ├──────────┼────────────────────────────────────────────────────────────┤
/// │ full     │ Always render complete block content, ignore budget        │
/// │ summary  │ Render summaries where available, full content otherwise   │
/// │ adaptive │ Auto-select per block based on budget + priority (default) │
/// └──────────┴────────────────────────────────────────────────────────────┘
/// ```
///
/// When `--budget N` is set, the driver tracks tokens spent and falls back
/// to summaries or placeholders for lower-priority blocks once the budget
/// is exhausted (RFC §5.5). Without `--budget`, adaptive mode behaves like
/// `full`.
///
/// # Type filtering
///
/// `--include code,conversation` limits rendering to those block types.
/// All other blocks are silently excluded from the output.
use std::fs;
use std::io::{self, Write as _};

use anyhow::{Context, Result, anyhow};
use bcp_decoder::BcpDecoder;
use bcp_driver::{DefaultDriver, DriverConfig, BcpDriver, OutputMode, Verbosity};
use bcp_types::block_type::BlockType;

use crate::DecodeArgs;

/// Run the `bcp decode` command.
///
/// Decodes the BCP file, applies the mode / verbosity / budget / include
/// configuration, renders the blocks via [`DefaultDriver`], and writes the
/// result to stdout or an output file.
///
/// # Errors
///
/// Returns an error if the file cannot be read, the BCP payload is
/// structurally invalid, any CLI flag value is unrecognised, or the
/// driver fails to render.
pub fn run(args: &DecodeArgs) -> Result<()> {
    let bytes =
        fs::read(&args.file).with_context(|| format!("cannot read {}", args.file.display()))?;

    let decoded = BcpDecoder::decode(&bytes)
        .with_context(|| format!("failed to decode {}", args.file.display()))?;

    let mode = parse_output_mode(&args.mode)?;
    let verbosity = parse_verbosity(&args.verbosity)?;
    let include_types = args
        .include
        .as_deref()
        .map(parse_include_types)
        .transpose()?;

    let config = DriverConfig {
        mode,
        verbosity,
        token_budget: args.budget,
        include_types,
        target_model: None,
    };

    let driver = DefaultDriver;
    let rendered = driver
        .render(&decoded.blocks, &config)
        .with_context(|| "driver render failed")?;

    if let Some(path) = &args.output {
        fs::write(path, rendered.as_bytes())
            .with_context(|| format!("cannot write {}", path.display()))?;
    } else {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        handle
            .write_all(rendered.as_bytes())
            .context("cannot write to stdout")?;
        if !rendered.ends_with('\n') {
            handle.write_all(b"\n").context("cannot write to stdout")?;
        }
    }

    Ok(())
}

// ── Flag parsers ──────────────────────────────────────────────────────────────

/// Parses the `--mode` string to an [`OutputMode`].
///
/// # Errors
///
/// Returns an error for unrecognised mode names.
fn parse_output_mode(s: &str) -> Result<OutputMode> {
    match s.to_lowercase().as_str() {
        "xml" => Ok(OutputMode::Xml),
        "markdown" | "md" => Ok(OutputMode::Markdown),
        "minimal" => Ok(OutputMode::Minimal),
        _ => Err(anyhow!(
            "unknown mode {s:?} — expected xml|markdown|minimal"
        )),
    }
}

/// Parses the `--verbosity` string to a [`Verbosity`].
///
/// # Errors
///
/// Returns an error for unrecognised verbosity names.
fn parse_verbosity(s: &str) -> Result<Verbosity> {
    match s.to_lowercase().as_str() {
        "full" => Ok(Verbosity::Full),
        "summary" => Ok(Verbosity::Summary),
        "adaptive" => Ok(Verbosity::Adaptive),
        _ => Err(anyhow!(
            "unknown verbosity {s:?} — expected full|summary|adaptive"
        )),
    }
}

/// Parses a comma-separated `--include` string to a list of [`BlockType`]s.
///
/// # Errors
///
/// Returns an error if any token is not a recognised block type name.
fn parse_include_types(s: &str) -> Result<Vec<BlockType>> {
    s.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|token| parse_block_type(token).ok_or_else(|| anyhow!("unknown block type {token:?}")))
        .collect()
}

/// Maps a block type name to a [`BlockType`] variant, case-insensitive.
fn parse_block_type(s: &str) -> Option<BlockType> {
    match s.to_lowercase().as_str() {
        "code" => Some(BlockType::Code),
        "conversation" => Some(BlockType::Conversation),
        "file_tree" | "filetree" => Some(BlockType::FileTree),
        "tool_result" | "toolresult" => Some(BlockType::ToolResult),
        "document" => Some(BlockType::Document),
        "structured_data" | "structureddata" => Some(BlockType::StructuredData),
        "diff" => Some(BlockType::Diff),
        "annotation" => Some(BlockType::Annotation),
        "embedding_ref" | "embeddingref" => Some(BlockType::EmbeddingRef),
        "image" => Some(BlockType::Image),
        "extension" => Some(BlockType::Extension),
        _ => None,
    }
}
