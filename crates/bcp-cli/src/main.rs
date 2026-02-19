/// BCP command-line tool — inspect, validate, encode, decode, and analyse
/// `.bcp` files produced by the Bit Context Protocol stack.
///
/// # Command overview
///
/// ```text
/// bcp <COMMAND> [OPTIONS]
///
/// Commands:
///   inspect    Print a human-readable block summary of a BCP file
///   validate   Check a BCP file for structural correctness
///   encode     Create a BCP file from a JSON manifest
///   decode     Render a BCP file as model-ready text
///   stats      Print size and token-efficiency statistics
///   help       Print help information
///
/// Global options:
///   -v, --verbose    Enable verbose output
///   --no-color       Disable coloured output (Unicode glyphs still used)
///   -h, --help       Print help
///   -V, --version    Print version
/// ```
///
/// # Exit codes
///
/// | Code | Meaning                                 |
/// |------|-----------------------------------------|
/// | 0    | Success                                 |
/// | 1    | Error (I/O failure, invalid file, etc.) |
///
/// All error details are written to stderr so stdout can be piped cleanly.
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

mod cmd_decode;
mod cmd_encode;
mod cmd_inspect;
mod cmd_stats;
mod cmd_validate;

// ── CLI root ──────────────────────────────────────────────────────────────────

/// The BCP (Bit Context Protocol) command-line tool.
///
/// Inspect, validate, encode, decode, and analyse `.bcp` binary payloads.
#[derive(Parser)]
#[command(name = "bcp", version, about = "Bit Context Protocol CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output (show extra decode/render details).
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Disable coloured output (ANSI escape codes are suppressed).
    #[arg(long, global = true)]
    no_color: bool,
}

// ── Sub-commands ──────────────────────────────────────────────────────────────

#[derive(Subcommand)]
enum Commands {
    /// Print a human-readable summary of each block in a BCP file.
    Inspect(InspectArgs),
    /// Check a BCP file for structural correctness.
    Validate(ValidateArgs),
    /// Create a BCP file from a JSON manifest.
    Encode(EncodeArgs),
    /// Render a BCP file as model-ready text.
    Decode(DecodeArgs),
    /// Print size and token-efficiency statistics.
    Stats(StatsArgs),
}

// ── Argument structs ──────────────────────────────────────────────────────────

/// Arguments for `bcp inspect`.
///
/// Reads and decodes a BCP file, then prints a human-readable summary of
/// every block (or a single block when `--block` is set). Useful for
/// quickly understanding what context a payload contains without writing
/// custom tooling.
///
/// ```text
/// ┌─────────────┬────────────────────────────────────────────────────────┐
/// │ Flag        │ Effect                                                 │
/// ├─────────────┼────────────────────────────────────────────────────────┤
/// │ --show-body │ Include first 80 chars of block content (UTF-8 lossy) │
/// │ --show-hex  │ Include 16-byte-per-line hex dump of block body       │
/// │ --block N   │ Show only the block at index N                        │
/// └─────────────┴────────────────────────────────────────────────────────┘
/// ```
#[derive(clap::Args)]
pub struct InspectArgs {
    /// Path to the `.bcp` file to inspect.
    pub file: PathBuf,

    /// Show block body content (first 80 characters, UTF-8 lossy).
    #[arg(long)]
    pub show_body: bool,

    /// Show raw hex dump of block bodies (16 bytes per line).
    #[arg(long)]
    pub show_hex: bool,

    /// Inspect only the block at this zero-based index.
    #[arg(long)]
    pub block: Option<usize>,
}

/// Arguments for `bcp validate`.
///
/// Attempts a full decode of the BCP file and reports either a set of
/// success checkmarks or a diagnostic error. The process exits with code 0
/// on success and code 1 on any structural problem.
#[derive(clap::Args)]
pub struct ValidateArgs {
    /// Path to the `.bcp` file to validate.
    pub file: PathBuf,
}

/// Arguments for `bcp encode`.
///
/// Reads a JSON manifest describing the blocks to encode, then serialises
/// them into a BCP binary payload. The manifest format is:
///
/// ```json
/// {
///   "blocks": [
///     { "type": "code",         "lang": "rust", "path": "src/main.rs",
///       "content": "fn main() {}", "summary": "Entry point" },
///     { "type": "conversation", "role": "user",
///       "content": "Fix the timeout bug." }
///   ]
/// }
/// ```
///
/// The `content_file` key may substitute `content` for code blocks — the
/// encoder reads the file at the given path relative to the manifest.
///
/// ```text
/// ┌──────────────────┬────────────────────────────────────────────────┐
/// │ Flag             │ Effect                                         │
/// ├──────────────────┼────────────────────────────────────────────────┤
/// │ --compress-blocks  │ zstd-compress each block body individually   │
/// │ --compress-payload │ zstd-compress everything after the header    │
/// │ --dedup            │ BLAKE3 content-addressing + deduplication    │
/// └──────────────────┴────────────────────────────────────────────────┘
/// ```
#[derive(clap::Args)]
pub struct EncodeArgs {
    /// Path to the JSON manifest file describing the blocks to encode.
    pub input: PathBuf,

    /// Output `.bcp` file path.
    #[arg(short, long)]
    pub output: PathBuf,

    /// Enable per-block zstd compression (each block body independently).
    #[arg(long)]
    pub compress_blocks: bool,

    /// Enable whole-payload zstd compression (all blocks as one stream).
    #[arg(long)]
    pub compress_payload: bool,

    /// Enable BLAKE3 content-addressed deduplication.
    #[arg(long)]
    pub dedup: bool,
}

/// Arguments for `bcp decode`.
///
/// Decodes a BCP file and renders the blocks as model-ready text on stdout
/// (or to a file). The output format, verbosity, token budget, and block
/// type filter are all configurable.
///
/// ```text
/// ┌─────────────┬──────────────────────────────────────────────────────┐
/// │ Flag        │ Values / default                                     │
/// ├─────────────┼──────────────────────────────────────────────────────┤
/// │ --mode      │ xml (default) | markdown | minimal                   │
/// │ --verbosity │ full | summary | adaptive (default)                  │
/// │ --budget    │ approximate token count (none = no limit)            │
/// │ --include   │ comma-separated block types to render                │
/// │ -o / --output │ write to file instead of stdout                   │
/// └─────────────┴──────────────────────────────────────────────────────┘
/// ```
#[derive(clap::Args)]
pub struct DecodeArgs {
    /// Path to the `.bcp` file to decode.
    pub file: PathBuf,

    /// Output format: `xml`, `markdown`, or `minimal`.
    #[arg(long, default_value = "xml")]
    pub mode: String,

    /// Token budget for adaptive rendering.
    ///
    /// When set, the driver uses the budget engine (RFC §5.5) to prefer full
    /// content for high-priority blocks and summaries / placeholders for
    /// lower-priority blocks until the budget is exhausted.
    #[arg(long)]
    pub budget: Option<u32>,

    /// Verbosity: `full`, `summary`, or `adaptive` (default).
    #[arg(long, default_value = "adaptive")]
    pub verbosity: String,

    /// Comma-separated list of block types to include (e.g. `code,conversation`).
    ///
    /// When set, only blocks of matching types appear in the output.
    /// Recognised names: `code`, `conversation`, `file_tree`, `tool_result`,
    /// `document`, `structured_data`, `diff`, `annotation`, `image`, `extension`.
    #[arg(long)]
    pub include: Option<String>,

    /// Write rendered output to this file instead of stdout.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

/// Arguments for `bcp stats`.
///
/// Decodes a BCP file and prints size, block-type distribution, compression
/// ratio (when applicable), and heuristic token estimates for multiple render
/// modes. Useful for evaluating the token-efficiency impact of a payload.
#[derive(clap::Args)]
pub struct StatsArgs {
    /// Path to the `.bcp` file to analyse.
    pub file: PathBuf,
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Inspect(args) => cmd_inspect::run(&args),
        Commands::Validate(args) => cmd_validate::run(&args),
        Commands::Encode(args) => cmd_encode::run(&args),
        Commands::Decode(args) => cmd_decode::run(&args),
        Commands::Stats(args) => cmd_stats::run(&args),
    };

    if let Err(e) = result {
        eprintln!("error: {e:#}");
        process::exit(1);
    }
}
