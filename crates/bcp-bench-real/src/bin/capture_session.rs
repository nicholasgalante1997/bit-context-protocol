#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(name = "capture_session", about = "Generate BCP benchmark fixture JSON")]
struct Args {
    /// Capture mode.
    #[arg(long, value_enum, default_value = "dir-scan")]
    mode: CaptureMode,

    /// Source path (directory for dir-scan, JSON file for transcript).
    #[arg(long)]
    path: PathBuf,

    /// Maximum number of files to include (dir-scan mode only).
    #[arg(long, default_value = "15")]
    max_files: usize,

    /// Output fixture JSON path.
    #[arg(long, short)]
    output: PathBuf,
}

#[derive(Clone, ValueEnum)]
enum CaptureMode {
    DirScan,
    Transcript,
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.mode {
        CaptureMode::DirScan => capture_dir_scan(&args.path, args.max_files, &args.output),
        CaptureMode::Transcript => capture_transcript(&args.path, &args.output),
    }
}

fn capture_dir_scan(dir: &Path, max_files: usize, output: &Path) -> Result<()> {
    let mut blocks = Vec::new();
    let mut file_count = 0;

    let walker = walkdir(dir, max_files)?;

    for (path, content) in &walker {
        let lang = detect_lang(path);
        let rel_path = path
            .strip_prefix(dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        blocks.push(serde_json::json!({
            "type": "code",
            "language": lang,
            "path": rel_path,
            "content": content,
        }));
        file_count += 1;
    }

    blocks.push(serde_json::json!({
        "type": "conversation",
        "role": "user",
        "content": format!("I've loaded {file_count} source files from {}. Can you analyze the architecture?", dir.display()),
    }));
    blocks.push(serde_json::json!({
        "type": "conversation",
        "role": "assistant",
        "content": format!("I'll review the {file_count} files. Let me start by understanding the module structure and key types."),
    }));
    blocks.push(serde_json::json!({
        "type": "tool_result",
        "tool_name": "ripgrep",
        "status": "ok",
        "content": format!("Found {file_count} source files in {}", dir.display()),
    }));

    let fixture = serde_json::json!({
        "description": format!("Dir scan of {} ({file_count} files)", dir.display()),
        "blocks": blocks,
    });

    let json = serde_json::to_string_pretty(&fixture)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, json)?;

    println!(
        "Wrote fixture: {} ({file_count} code blocks + 3 synthetic blocks)",
        output.display()
    );
    Ok(())
}

fn capture_transcript(input: &Path, output: &Path) -> Result<()> {
    let json = std::fs::read_to_string(input)
        .with_context(|| format!("reading {}", input.display()))?;

    let _: serde_json::Value = serde_json::from_str(&json).context("invalid JSON")?;

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, &json)?;
    println!("Wrote fixture: {}", output.display());
    Ok(())
}

fn walkdir(dir: &Path, max_files: usize) -> Result<Vec<(PathBuf, String)>> {
    let mut results = Vec::new();
    collect_files(dir, &mut results, max_files)?;
    Ok(results)
}

fn collect_files(
    dir: &Path,
    results: &mut Vec<(PathBuf, String)>,
    max_files: usize,
) -> Result<()> {
    if results.len() >= max_files {
        return Ok(());
    }

    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        if results.len() >= max_files {
            break;
        }
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            collect_files(&path, results, max_files)?;
        } else if is_source_file(&path) && let Ok(content) = std::fs::read_to_string(&path) {
            results.push((path, content));
        }
    }
    Ok(())
}

fn is_source_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    matches!(
        ext.as_str(),
        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "c" | "cpp" | "h"
            | "rb" | "sh" | "sql" | "html" | "css" | "json" | "yaml" | "yml" | "toml" | "md"
    )
}

fn detect_lang(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    match ext.as_str() {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" => "cpp",
        "rb" => "ruby",
        "sh" => "shell",
        "sql" => "sql",
        "html" => "html",
        "css" => "css",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "md" => "markdown",
        _ => "text",
    }
}
