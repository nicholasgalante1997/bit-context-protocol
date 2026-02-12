# SPEC_09 — CLI Tool

**Crate**: `lcp-cli`
**Phase**: 4 (Tooling)
**Prerequisites**: SPEC_01 through SPEC_08
**Dependencies**: `lcp-wire`, `lcp-types`, `lcp-encoder`, `lcp-decoder`, `lcp-driver`, `clap`, `anyhow`

---

## Context

The `lcp` CLI tool is the primary development and debugging interface for
working with LCP payloads. It provides commands to inspect, validate, encode,
decode, and compute statistics on `.lcp` files. The CLI uses `clap` for
argument parsing and `anyhow` for error handling (application-level crate).

---

## Requirements

### 1. Command Structure

```
lcp <COMMAND> [OPTIONS]

Commands:
  inspect    Print a human-readable summary of blocks in an LCP file
  validate   Check an LCP file for structural correctness
  encode     Create an LCP file from a JSON/TOML manifest
  decode     Render an LCP file as model-ready text
  stats      Print size and compression statistics
  help       Print help information

Global Options:
  -v, --verbose    Enable verbose output
  --no-color       Disable colored output
  -h, --help       Print help
  -V, --version    Print version
```

### 2. `lcp inspect`

```
lcp inspect <FILE> [OPTIONS]

Print a human-readable summary of each block in the file.

Options:
  --show-body      Show block body content (truncated to 80 chars)
  --show-hex       Show raw hex dump of block bodies
  --block <N>      Inspect only block at index N

Output format:
  Header: LCP v1.0, flags=0x00, N blocks
  Block 0: CODE [rust] path="src/main.rs" (1024 bytes)
           Summary: "Entry point with CLI setup"
  Block 1: CONVERSATION [user] (42 bytes)
  Block 2: CONVERSATION [assistant] (156 bytes)
  Block 3: TOOL_RESULT [ripgrep] status=ok (89 bytes)
  Block 4: ANNOTATION target=0 kind=priority value="high"
  ---
  END sentinel at offset 1342
```

```rust
#[derive(clap::Args)]
pub struct InspectArgs {
    /// Path to the .lcp file to inspect.
    pub file: PathBuf,

    /// Show block body content (truncated).
    #[arg(long)]
    pub show_body: bool,

    /// Show raw hex dump of block bodies.
    #[arg(long)]
    pub show_hex: bool,

    /// Inspect only block at this index.
    #[arg(long)]
    pub block: Option<usize>,
}
```

### 3. `lcp validate`

```
lcp validate <FILE>

Validate structural correctness of an LCP file.

Exit codes:
  0 = valid
  1 = invalid (with diagnostic messages to stderr)

Output on success:
  ✓ Header: valid (LCP v1.0)
  ✓ Blocks: 5 blocks parsed successfully
  ✓ Sentinel: END block present
  ✓ Integrity: all block bodies parse without error

Output on failure:
  ✗ Error at offset 0x00: invalid magic number (expected 0x4C435000, got 0xDEADBEEF)
```

```rust
#[derive(clap::Args)]
pub struct ValidateArgs {
    /// Path to the .lcp file to validate.
    pub file: PathBuf,
}
```

### 4. `lcp encode`

```
lcp encode <INPUT> -o <OUTPUT> [OPTIONS]

Create an LCP file from a JSON or TOML manifest.

Options:
  -o, --output <FILE>      Output .lcp file path (required)
  --compress-blocks        Enable per-block zstd compression
  --compress-payload       Enable whole-payload zstd compression
  --dedup                  Enable content-addressed deduplication

Manifest format (JSON):
  {
    "blocks": [
      {
        "type": "code",
        "lang": "rust",
        "path": "src/main.rs",
        "content_file": "src/main.rs",
        "summary": "Entry point with CLI setup",
        "priority": "high"
      },
      {
        "type": "conversation",
        "role": "user",
        "content": "Fix the timeout bug."
      }
    ]
  }
```

```rust
#[derive(clap::Args)]
pub struct EncodeArgs {
    /// Path to the JSON/TOML manifest.
    pub input: PathBuf,

    /// Output .lcp file path.
    #[arg(short, long)]
    pub output: PathBuf,

    /// Enable per-block zstd compression.
    #[arg(long)]
    pub compress_blocks: bool,

    /// Enable whole-payload zstd compression.
    #[arg(long)]
    pub compress_payload: bool,

    /// Enable content-addressed deduplication.
    #[arg(long)]
    pub dedup: bool,
}
```

### 5. `lcp decode`

```
lcp decode <FILE> [OPTIONS]

Render an LCP file as model-ready text to stdout.

Options:
  --mode <MODE>            Output mode: xml, markdown, minimal [default: xml]
  --budget <TOKENS>        Token budget for adaptive rendering
  --verbosity <V>          full, summary, adaptive [default: full]
  --include <TYPES>        Comma-separated block types to include
  -o, --output <FILE>      Write to file instead of stdout
```

```rust
#[derive(clap::Args)]
pub struct DecodeArgs {
    /// Path to the .lcp file to decode.
    pub file: PathBuf,

    /// Output mode.
    #[arg(long, default_value = "xml")]
    pub mode: String,

    /// Token budget for adaptive rendering.
    #[arg(long)]
    pub budget: Option<u32>,

    /// Verbosity mode.
    #[arg(long, default_value = "full")]
    pub verbosity: String,

    /// Comma-separated block types to include.
    #[arg(long)]
    pub include: Option<String>,

    /// Write output to file instead of stdout.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}
```

### 6. `lcp stats`

```
lcp stats <FILE>

Print size and compression statistics.

Output:
  File: example.lcp (2,847 bytes)
  Header: LCP v1.0, flags=compressed

  Block Statistics:
    Total blocks: 5
    CODE:          2 blocks, 1,823 bytes (64.0%)
    CONVERSATION:  2 blocks,   198 bytes ( 7.0%)
    TOOL_RESULT:   1 block,     89 bytes ( 3.1%)
    ANNOTATION:    1 block,     12 bytes ( 0.4%)

  Compression:
    Uncompressed: 4,210 bytes
    Compressed:   2,847 bytes
    Ratio:        32.4% reduction

  Token Estimates (heuristic):
    Full render (XML):      1,024 tokens
    Full render (Minimal):    892 tokens
    Equivalent markdown:    1,340 tokens
    Savings (Minimal):        33.4%
```

```rust
#[derive(clap::Args)]
pub struct StatsArgs {
    /// Path to the .lcp file.
    pub file: PathBuf,
}
```

### 7. Main Entry Point

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lcp", version, about = "LLM Context Pack CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output.
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Disable colored output.
    #[arg(long, global = true)]
    no_color: bool,
}

#[derive(Subcommand)]
enum Commands {
    Inspect(InspectArgs),
    Validate(ValidateArgs),
    Encode(EncodeArgs),
    Decode(DecodeArgs),
    Stats(StatsArgs),
}
```

---

## File Structure

```
crates/lcp-cli/
├── Cargo.toml
└── src/
    ├── main.rs           # Entry point, Cli struct, dispatch
    ├── cmd_inspect.rs    # `lcp inspect` implementation
    ├── cmd_validate.rs   # `lcp validate` implementation
    ├── cmd_encode.rs     # `lcp encode` implementation
    ├── cmd_decode.rs     # `lcp decode` implementation
    └── cmd_stats.rs      # `lcp stats` implementation
```

---

## Acceptance Criteria

- [ ] `lcp inspect example.lcp` prints block summary for each block
- [ ] `lcp inspect --show-body` includes truncated body content
- [ ] `lcp inspect --block 0` shows only the first block
- [ ] `lcp validate valid.lcp` exits with code 0 and prints success
- [ ] `lcp validate invalid.lcp` exits with code 1 and prints diagnostic
- [ ] `lcp encode manifest.json -o output.lcp` creates a valid .lcp file
- [ ] `lcp encode --compress-blocks` produces smaller output than uncompressed
- [ ] `lcp decode example.lcp` prints XML-tagged output to stdout
- [ ] `lcp decode --mode minimal` prints minimal-mode output
- [ ] `lcp decode --budget 500 --verbosity adaptive` uses summaries for low-priority blocks
- [ ] `lcp stats example.lcp` prints correct block counts and byte sizes
- [ ] `lcp --version` prints the version
- [ ] `lcp --help` prints help text with all commands listed
- [ ] All commands handle missing files gracefully (anyhow error with path)

---

## Verification

```bash
# Build the CLI
cargo build -p lcp-cli

# Run with --help
cargo run -p lcp-cli -- --help

# End-to-end test: encode → validate → inspect → decode → stats
cargo run -p lcp-cli -- encode test.json -o test.lcp
cargo run -p lcp-cli -- validate test.lcp
cargo run -p lcp-cli -- inspect test.lcp
cargo run -p lcp-cli -- decode test.lcp --mode xml
cargo run -p lcp-cli -- decode test.lcp --mode minimal
cargo run -p lcp-cli -- stats test.lcp

# Clippy
cargo clippy -p lcp-cli -- -W clippy::pedantic
```
