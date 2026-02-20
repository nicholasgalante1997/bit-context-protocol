/// Implementation of `bcp stats`.
///
/// Decodes a BCP file and prints a structured statistics report covering
/// file size, block-type distribution, compression status, and heuristic
/// token estimates for all three render modes.
///
/// # Example output
///
/// ```text
/// File:    /tmp/context.bcp  (282 bytes)
/// Header:  BCP v1.0, flags=0x00  (uncompressed)
/// Blocks:  6 total
///
/// Type              Count   Bytes
/// ──────────────────────────────────
/// CODE                  1      32
/// CONVERSATION          1      20
/// ANNOTATION            1       1
/// TOOL_RESULT           1      32
/// DOCUMENT              1      38
/// STRUCTURED_DATA       1      29
/// ──────────────────────────────────
/// Total                 6     152
///
/// Tokens (heuristic estimate):
///   xml mode      ~84 tokens
///   markdown mode ~71 tokens
///   minimal mode  ~58 tokens
/// ```
///
/// The token estimates use [`HeuristicEstimator`] (4 chars ≈ 1 token) on the
/// rendered output of each mode. These are rough lower-bound estimates; actual
/// tokenisation varies by model and content.
use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result};
use bcp_decoder::BcpDecoder;
use bcp_driver::{
    DefaultDriver, DriverConfig, HeuristicEstimator, BcpDriver, OutputMode, TokenEstimator,
    Verbosity,
};
use bcp_types::block::BlockContent;
use bcp_types::block_type::BlockType;

use crate::StatsArgs;

/// Run the `bcp stats` command.
///
/// Decodes the file, tabulates block-type distribution and content byte sums,
/// checks header compression flags, renders the payload in each output mode,
/// and prints a formatted summary to stdout.
///
/// # Errors
///
/// Returns an error if the file cannot be read or the BCP payload fails
/// structural validation.
pub fn run(args: &StatsArgs) -> Result<()> {
    let bytes =
        fs::read(&args.file).with_context(|| format!("cannot read {}", args.file.display()))?;

    let file_size = bytes.len();

    let decoded = BcpDecoder::decode(&bytes)
        .with_context(|| format!("failed to decode {}", args.file.display()))?;

    let header = &decoded.header;
    let compressed = header.flags.is_compressed();

    // ── Block distribution ────────────────────────────────────────────────────

    // Ordered list of (block_type, content_bytes) for each block (excluding END).
    let block_stats: Vec<(BlockType, usize)> = decoded
        .blocks
        .iter()
        .filter(|b| b.block_type != BlockType::End)
        .map(|b| (b.block_type.clone(), content_size(&b.content)))
        .collect();

    // Aggregate by block type: (count, total_bytes).
    let mut by_type: HashMap<String, (usize, usize)> = HashMap::new();
    let mut insertion_order: Vec<String> = Vec::new();

    for (bt, sz) in &block_stats {
        let label = block_type_label(bt);
        by_type
            .entry(label.to_string())
            .and_modify(|(cnt, total)| {
                *cnt += 1;
                *total += sz;
            })
            .or_insert_with(|| {
                insertion_order.push(label.to_string());
                (1, *sz)
            });
    }

    let total_blocks: usize = block_stats.len();
    let total_bytes: usize = block_stats.iter().map(|(_, sz)| sz).sum();

    // ── Token estimates ───────────────────────────────────────────────────────

    let estimator = HeuristicEstimator;
    let est = |mode: OutputMode| -> u32 {
        let config = DriverConfig {
            mode,
            verbosity: Verbosity::Full,
            token_budget: None,
            include_types: None,
            target_model: None,
        };
        DefaultDriver
            .render(&decoded.blocks, &config)
            .ok()
            .as_deref()
            .map_or(0, |text| estimator.estimate(text))
    };

    let xml_tokens = est(OutputMode::Xml);
    let md_tokens = est(OutputMode::Markdown);
    let min_tokens = est(OutputMode::Minimal);

    // ── Print report ──────────────────────────────────────────────────────────

    let compression_note = if compressed {
        " (payload zstd-compressed)"
    } else {
        " (uncompressed)"
    };

    println!("File:    {}  ({file_size} bytes)", args.file.display());
    println!(
        "Header:  BCP v{}.{}, flags=0x{:02X}{compression_note}",
        header.version_major,
        header.version_minor,
        header.flags.raw()
    );
    println!("Blocks:  {total_blocks} total");
    println!();

    let sep = "─".repeat(36);
    println!("{:<20}{:>6}{:>8}", "Type", "Count", "Bytes");
    println!("{sep}");

    for label in &insertion_order {
        let (cnt, sz) = by_type[label];
        println!("{label:<20}{cnt:>6}{sz:>8}");
    }

    println!("{sep}");
    println!("{:<20}{:>6}{:>8}", "Total", total_blocks, total_bytes);

    println!();
    println!("Tokens (heuristic estimate):");
    println!("  xml mode      ~{xml_tokens} tokens");
    println!("  markdown mode ~{md_tokens} tokens");
    println!("  minimal mode  ~{min_tokens} tokens");

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns the primary content byte count for a block, used for the stats table.
fn content_size(content: &BlockContent) -> usize {
    match content {
        BlockContent::Code(c) => c.content.len(),
        BlockContent::Conversation(c) => c.content.len(),
        BlockContent::FileTree(t) => t.entries.len() * 8,
        BlockContent::ToolResult(t) => t.content.len(),
        BlockContent::Document(d) => d.content.len(),
        BlockContent::StructuredData(s) => s.content.len(),
        BlockContent::Diff(d) => d.hunks.iter().map(|h| h.lines.len()).sum(),
        BlockContent::Annotation(a) => a.value.len(),
        BlockContent::EmbeddingRef(_) => 32,
        BlockContent::Image(i) => i.data.len(),
        BlockContent::Extension(e) => e.content.len(),
        BlockContent::End => 0,
        BlockContent::Unknown { body, .. } => body.len(),
    }
}

/// Returns the uppercase display label for a block type, matching the format
/// used by `bcp inspect`.
fn block_type_label(bt: &BlockType) -> &'static str {
    match bt {
        BlockType::Code => "CODE",
        BlockType::Conversation => "CONVERSATION",
        BlockType::FileTree => "FILE_TREE",
        BlockType::ToolResult => "TOOL_RESULT",
        BlockType::Document => "DOCUMENT",
        BlockType::StructuredData => "STRUCTURED_DATA",
        BlockType::Diff => "DIFF",
        BlockType::Annotation => "ANNOTATION",
        BlockType::EmbeddingRef => "EMBEDDING_REF",
        BlockType::Image => "IMAGE",
        BlockType::Extension => "EXTENSION",
        BlockType::End => "END",
        BlockType::Unknown(_) => "UNKNOWN",
    }
}
