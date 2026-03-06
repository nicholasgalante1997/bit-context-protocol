use std::path::PathBuf;

use anyhow::Result;
use bcp_bench_real::fixture::encode_fixture;
use bcp_bench_real::markdown::build_realistic_markdown;
use bcp_bench_real::token_counter::TokenCounter;
use bcp_decoder::BcpDecoder;
use bcp_driver::{BcpDriver, DefaultDriver, DriverConfig, OutputMode};
use bcp_types::block::Block;
use bcp_types::BlockType;
use clap::Parser;

#[derive(Parser)]
#[command(name = "bench_tokens", about = "BCP token savings benchmark (cl100k_base)")]
struct Args {
    /// Path to a session fixture JSON file.
    #[arg(default_value = "fixtures/real_session_medium.json")]
    fixture: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let counter = TokenCounter::new()?;

    let payload = encode_fixture(&args.fixture)?;
    let decoded = BcpDecoder::decode(&payload)?;

    let mut results: Vec<(&str, usize)> = Vec::new();

    for (label, mode) in [
        ("BCP XML", OutputMode::Xml),
        ("BCP Markdown", OutputMode::Markdown),
        ("BCP Minimal", OutputMode::Minimal),
    ] {
        let config = DriverConfig {
            mode,
            ..Default::default()
        };
        let rendered = DefaultDriver.render(&decoded.blocks, &config)?;
        let tokens = counter.count(&rendered);
        results.push((label, tokens));
    }

    let naive_md = build_naive_markdown(&decoded.blocks);
    let naive_tokens = counter.count(&naive_md);
    results.push(("Raw MD (naive)", naive_tokens));

    let realistic_md = build_realistic_markdown(&decoded.blocks);
    let realistic_tokens = counter.count(&realistic_md);
    results.push(("Raw MD (agent)", realistic_tokens));

    println!();
    println!(
        "\u{2554}{:\u{2550}<22}\u{2566}{:\u{2550}<9}\u{2566}{:\u{2550}<19}\u{2566}{:\u{2550}<19}\u{2557}",
        "", "", "", ""
    );
    println!(
        "\u{2551} {:<20} \u{2551} {:>7} \u{2551} {:>17} \u{2551} {:>17} \u{2551}",
        "Format", "Tokens", "vs Naive MD", "vs Agent MD"
    );
    println!(
        "\u{2560}{:\u{2550}<22}\u{256C}{:\u{2550}<9}\u{256C}{:\u{2550}<19}\u{256C}{:\u{2550}<19}\u{2563}",
        "", "", "", ""
    );

    for (label, tokens) in &results {
        let is_baseline =
            *label == "Raw MD (naive)" || *label == "Raw MD (agent)";
        let vs_naive = if is_baseline {
            "\u{2014}".into()
        } else {
            let pct = savings_pct(*tokens, naive_tokens);
            format!("{pct:+.1}%")
        };
        let vs_realistic = if is_baseline {
            "\u{2014}".into()
        } else {
            let pct = savings_pct(*tokens, realistic_tokens);
            format!("{pct:+.1}%")
        };
        println!(
            "\u{2551} {label:<20} \u{2551} {tokens:>7} \u{2551} {vs_naive:>17} \u{2551} {vs_realistic:>17} \u{2551}"
        );
    }

    println!(
        "\u{255A}{:\u{2550}<22}\u{2569}{:\u{2550}<9}\u{2569}{:\u{2550}<19}\u{2569}{:\u{2550}<19}\u{255D}",
        "", "", "", ""
    );

    println!();
    print_per_block_breakdown(&decoded.blocks, &counter)?;

    println!();
    println!("BCP wire size:     {} bytes", payload.len());
    println!("Naive MD size:     {} bytes", naive_md.len());
    println!("Agent MD size:     {} bytes", realistic_md.len());
    println!(
        "Wire compression:  {:.1}% vs naive MD bytes",
        savings_pct(payload.len(), naive_md.len())
    );

    println!();
    print_rfc_table(&decoded.blocks, &counter)?;

    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn savings_pct(actual: usize, baseline: usize) -> f64 {
    if baseline == 0 {
        return 0.0;
    }
    (1.0 - actual as f64 / baseline as f64) * 100.0
}

fn print_per_block_breakdown(blocks: &[Block], counter: &TokenCounter) -> Result<()> {
    println!("Per-Block-Type Overhead Breakdown:");
    println!(
        "\u{250C}{:\u{2500}<20}\u{252C}{:\u{2500}<12}\u{252C}{:\u{2500}<12}\u{252C}{:\u{2500}<10}\u{2510}",
        "", "", "", ""
    );
    println!(
        "\u{2502} {:<18} \u{2502} {:>10} \u{2502} {:>10} \u{2502} {:>8} \u{2502}",
        "Block Type", "BCP Min.", "Naive MD", "Savings"
    );
    println!(
        "\u{251C}{:\u{2500}<20}\u{253C}{:\u{2500}<12}\u{253C}{:\u{2500}<12}\u{253C}{:\u{2500}<10}\u{2524}",
        "", "", "", ""
    );

    let type_groups: &[(BlockType, &str)] = &[
        (BlockType::Code, "Code"),
        (BlockType::Conversation, "Conversation"),
        (BlockType::ToolResult, "ToolResult"),
        (BlockType::FileTree, "FileTree"),
        (BlockType::Document, "Document"),
        (BlockType::StructuredData, "StructuredData"),
        (BlockType::Diff, "Diff"),
    ];

    for (bt, label) in type_groups {
        let group: Vec<&Block> = blocks
            .iter()
            .filter(|b| b.block_type == *bt)
            .collect();
        if group.is_empty() {
            continue;
        }

        let owned_blocks: Vec<Block> = group.into_iter().cloned().collect();

        let config = DriverConfig {
            mode: OutputMode::Minimal,
            ..Default::default()
        };
        let bcp_out = DefaultDriver.render(&owned_blocks, &config)?;
        let bcp_tokens = counter.count(&bcp_out);

        let naive_out = build_naive_markdown(&owned_blocks);
        let naive_tokens = counter.count(&naive_out);

        let pct = savings_pct(bcp_tokens, naive_tokens);
        println!(
            "\u{2502} {label:<18} \u{2502} {bcp_tokens:>10} \u{2502} {naive_tokens:>10} \u{2502} {pct:>7.1}% \u{2502}"
        );
    }

    println!(
        "\u{2514}{:\u{2500}<20}\u{2534}{:\u{2500}<12}\u{2534}{:\u{2500}<12}\u{2534}{:\u{2500}<10}\u{2518}",
        "", "", "", ""
    );
    Ok(())
}

fn build_naive_markdown(blocks: &[Block]) -> String {
    bcp_bench_real::markdown::build_naive_markdown(blocks)
}

fn print_rfc_table(blocks: &[Block], counter: &TokenCounter) -> Result<()> {
    println!("RFC \u{00A7}6 Replacement Data (cl100k_base tokenizer):");
    println!();
    println!("   +----------------------------+-----------+-----------+---------+");
    println!("   | Context Pattern            | Markdown  | BCP Min.  | Savings |");
    println!("   |                            | Tokens    | Tokens    |         |");
    println!("   +----------------------------+-----------+-----------+---------+");

    let type_groups: &[(BlockType, &str)] = &[
        (BlockType::Code, "Code blocks"),
        (BlockType::Conversation, "Conversation turns"),
        (BlockType::ToolResult, "Tool results"),
        (BlockType::FileTree, "File trees"),
        (BlockType::Document, "Documents"),
        (BlockType::StructuredData, "Structured data"),
        (BlockType::Diff, "Diffs"),
    ];

    let mut total_md: usize = 0;
    let mut total_bcp: usize = 0;

    for (bt, label) in type_groups {
        let group: Vec<Block> = blocks
            .iter()
            .filter(|b| b.block_type == *bt)
            .cloned()
            .collect();
        if group.is_empty() {
            continue;
        }

        let config = DriverConfig {
            mode: OutputMode::Minimal,
            ..Default::default()
        };
        let bcp_out = DefaultDriver.render(&group, &config)?;
        let bcp_tokens = counter.count(&bcp_out);

        let naive_out = bcp_bench_real::markdown::build_naive_markdown(&group);
        let naive_tokens = counter.count(&naive_out);

        total_md += naive_tokens;
        total_bcp += bcp_tokens;

        let pct = savings_pct(bcp_tokens, naive_tokens);
        println!(
            "   | {label:<26} | {naive_tokens:>9} | {bcp_tokens:>9} | {pct:>6.0}% |"
        );
    }

    println!("   +----------------------------+-----------+-----------+---------+");
    let total_pct = savings_pct(total_bcp, total_md);
    println!(
        "   | {:<26} | {:>9} | {:>9} | {:>6.1}% |",
        "TOTAL", total_md, total_bcp, total_pct
    );
    println!("   +----------------------------+-----------+-----------+---------+");
    Ok(())
}
