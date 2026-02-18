# bcp-cli

<span class="badge badge-green">Complete</span> <span class="badge badge-blue">Phase 4</span>

> The developer interface for the LCP stack. A five-command binary that lets you inspect, validate, encode, decode, and profile `.lcp` files without writing a single line of Rust.

## Crate Info

| Field | Value |
|-------|-------|
| Path | `crates/bcp-cli/` |
| Binary | `bcp` |
| Spec | [SPEC_09](driver.md) |
| Dependencies | `bcp-encoder`, `bcp-decoder`, `bcp-driver`, `bcp-types`, `clap`, `anyhow`, `serde`, `serde_json` |

---

## Purpose and Role in the Protocol

`bcp-cli` is the outermost layer of the protocol stack. Where the library crates expose Rust APIs, the CLI exposes the same functionality as shell-friendly subcommands, making the full encode→decode→render pipeline accessible without a build step.

```
JSON manifest ──▶  bcp encode  ──▶  .lcp file  ──▶  bcp validate / inspect / decode / stats
                    (LcpEncoder)                      (LcpDecoder + DefaultDriver)
```

Every subcommand maps directly to a library API:

```text
┌────────────┬─────────────────────────────────────────────────────┐
│ Command    │ Library API used                                    │
├────────────┼─────────────────────────────────────────────────────┤
│ inspect    │ LcpDecoder::decode → print BlockContent variants    │
│ validate   │ LcpDecoder::decode → Ok / Err diagnostic           │
│ encode     │ LcpEncoder builder → fs::write                      │
│ decode     │ LcpDecoder::decode + DefaultDriver::render          │
│ stats      │ LcpDecoder::decode + HeuristicEstimator             │
└────────────┴─────────────────────────────────────────────────────┘
```

---

## Command Reference

### `bcp inspect`

Print a human-readable block summary of an LCP file.

```bash
bcp inspect <FILE> [--show-body] [--show-hex] [--block N]
```

**Flags:**

```text
┌─────────────┬────────────────────────────────────────────────────────┐
│ Flag        │ Effect                                                 │
├─────────────┼────────────────────────────────────────────────────────┤
│ --show-body │ Include first 80 chars of block content (UTF-8 lossy) │
│ --show-hex  │ Include 16-byte-per-line hex dump of block body       │
│ --block N   │ Show only the block at zero-based index N             │
└─────────────┴────────────────────────────────────────────────────────┘
```

**Example output:**

```text
Header: LCP v1.0, flags=0x00, 6 blocks
Block 0: CODE [rust] path="src/main.rs" (32 bytes)
         Summary: "Entry point"
         Body:    fn main() { println!("hello"); }
Block 1: CONVERSATION [user] (20 bytes)
Block 2: ANNOTATION target=1 kind=priority value="high" (1 bytes)
Block 3: TOOL_RESULT [ripgrep] status=ok (32 bytes)
Block 4: DOCUMENT title="API Reference" (38 bytes)
         Summary: "API docs"
Block 5: STRUCTURED_DATA format=json (29 bytes)
---
END sentinel at offset 278
```

**Decision flow:**

```
Read file bytes
      │
      ▼
LcpDecoder::decode
      │
      ├── Err ──▶ return Err (anyhow propagation → exit 1)
      │
      ▼
Print header line
      │
      ▼
For each block:
      ├── if --block N: skip non-matching indices
      ├── Print: Block N: TYPE [detail] (M bytes)
      ├── if block.summary: Print: Summary: "..."
      ├── if --show-body: Print: Body: <first 80 chars>
      └── if --show-hex: Print 16-byte hex dump rows
      │
      ▼
Print: --- END sentinel at offset M
```

---

### `bcp validate`

Check an LCP file for structural correctness. Exits 0 on success, 1 on failure.

```bash
bcp validate <FILE>
```

**Success output:**

```text
✓ Header: valid (LCP v1.0)
✓ Blocks: 6 blocks parsed successfully
✓ Sentinel: END block present
✓ Integrity: all block bodies parse without error
```

**Failure output:**

```text
✗ Error: invalid header — invalid magic number: expected 0x4C435000, got 0xDEADBEEF
```

The validation runs a single `LcpDecoder::decode` call, which covers all four structural layers defined in RFC §4:

```text
1. Header      — magic number, version, reserved byte
2. Decompression — whole-payload zstd (if compressed flag set)
3. Block frames — block_type varint, flags byte, content_len varint, body
4. Block bodies — TLV field deserialization for each typed block
```

**Error mapping:**

```text
┌──────────────────────────┬──────────────────────────────────────────┐
│ DecodeError variant      │ Diagnostic message prefix                │
├──────────────────────────┼──────────────────────────────────────────┤
│ InvalidHeader            │ "invalid header — <inner error>"         │
│ MissingEndSentinel       │ "missing END sentinel"                   │
│ TrailingData             │ "trailing data after END ({n} bytes)"    │
│ MissingContentStore      │ "content-addressed block with no store"  │
│ UnresolvedReference      │ "unresolved BLAKE3 reference <hex8>…"    │
│ Wire / Type / Decompress │ "<error Display>"                        │
└──────────────────────────┴──────────────────────────────────────────┘
```

---

### `bcp encode`

Create an LCP file from a JSON manifest.

```bash
bcp encode <MANIFEST> -o <OUTPUT> [--compress-blocks] [--compress-payload] [--dedup]
```

**Flags:**

```text
┌──────────────────────┬─────────────────────────────────────────────┐
│ Flag                 │ Effect                                      │
├──────────────────────┼─────────────────────────────────────────────┤
│ --compress-blocks    │ zstd-compress each block body individually  │
│ --compress-payload   │ zstd-compress all blocks as one stream      │
│ --dedup              │ BLAKE3 dedup via in-memory content store    │
└──────────────────────┴─────────────────────────────────────────────┘
```

**Manifest format:**

```json
{
  "blocks": [
    {
      "type": "code",
      "lang": "rust",
      "path": "src/main.rs",
      "content": "fn main() { println!(\"hello\"); }",
      "summary": "Entry point",
      "priority": "high"
    },
    {
      "type": "conversation",
      "role": "user",
      "content": "Fix the timeout bug."
    },
    {
      "type": "tool_result",
      "name": "ripgrep",
      "status": "ok",
      "content": "src/main.rs:5: let timeout = 30;"
    },
    {
      "type": "document",
      "title": "API Reference",
      "format": "markdown",
      "content": "# API\n\n## Overview\n\nThe API is simple.",
      "summary": "API docs"
    },
    {
      "type": "structured_data",
      "format": "json",
      "content": "{\"key\": \"value\", \"count\": 42}"
    }
  ]
}
```

**Supported block types:**

```text
┌──────────────────┬──────────────────────────────────────────────────────┐
│ Type             │ Required fields                                      │
├──────────────────┼──────────────────────────────────────────────────────┤
│ code             │ lang, path, content (or content_file)                │
│ conversation     │ role, content (or content_file)                      │
│ tool_result      │ name, content (or content_file)                      │
│ document         │ title, content (or content_file)                     │
│ structured_data  │ format, content (or content_file)                    │
└──────────────────┴──────────────────────────────────────────────────────┘
```

Optional on any block: `summary` (string), `priority` (`critical` | `high` | `normal` | `low` | `background`).

The `content_file` key substitutes `content` — the encoder reads the file at the given path relative to the manifest's parent directory, so you can ship a manifest alongside source files:

```json
{ "type": "code", "lang": "rust", "path": "src/lib.rs", "content_file": "src/lib.rs" }
```

**Enum values:**

```text
lang:     rust | typescript | javascript | python | go | java | c | cpp |
          ruby | shell | sql | html | css | json | yaml | toml | markdown
          (unrecognised → Lang::Unknown)

role:     system | user | assistant | tool

status:   ok | error | timeout  (default: ok)

format:   markdown | plain | html  (document)
          json | yaml | toml | csv  (structured_data)

priority: critical | high | normal | low | background
```

**Encoding pipeline:**

```
Parse JSON manifest
      │
      ▼
Create LcpEncoder
      │
      ├── --dedup: set MemoryContentStore + auto_dedup()
      ├── --compress-blocks: compress_blocks()
      └── --compress-payload: compress_payload()
      │
      ▼
For each block in manifest:
      ├── resolve_content (inline or content_file)
      ├── parse enum fields (lang, role, status, format, priority)
      ├── encoder.add_*(...)
      └── if summary/priority: encoder.with_summary/with_priority
      │
      ▼
encoder.encode() → Vec<u8>
      │
      ▼
fs::write(output, bytes)
      │
      ▼
Wrote N bytes to <path>
```

---

### `bcp decode`

Render an LCP file as model-ready text.

```bash
bcp decode <FILE> [--mode xml|markdown|minimal] [--verbosity full|summary|adaptive]
                  [--budget N] [--include types] [-o <FILE>]
```

**Flags:**

```text
┌─────────────┬──────────────────────────────────────────────────────┐
│ Flag        │ Values / default                                     │
├─────────────┼──────────────────────────────────────────────────────┤
│ --mode      │ xml (default) | markdown | minimal                   │
│ --verbosity │ full | summary | adaptive (default)                  │
│ --budget    │ approximate token count (none = no limit)            │
│ --include   │ comma-separated block types to render                │
│ -o / --output │ write to file instead of stdout                   │
└─────────────┴──────────────────────────────────────────────────────┘
```

**Output modes:**

```text
┌──────────┬──────────────────────────────────────────────────────────────┐
│ Mode     │ Format                                                       │
├──────────┼──────────────────────────────────────────────────────────────┤
│ xml      │ <code lang="rust" path="...">...</code>   (default)         │
│ markdown │ ```rust\n// src/main.rs\n...\n```                            │
│ minimal  │ --- src/main.rs [rust] ---\n...                             │
└──────────┴──────────────────────────────────────────────────────────────┘
```

**Type filtering example:**

```bash
# Only render code and conversation blocks
bcp decode context.lcp --include code,conversation --mode minimal
```

**Budget-aware decoding:**

When `--budget N` is set with `--verbosity adaptive`, the driver's budget engine assigns `RenderDecision` per block based on block priorities and budget consumption. High-priority blocks get full content first; when the budget is exhausted, lower-priority blocks fall back to summaries then placeholders. See [bcp-driver](crate-bcp-driver.md) for budget engine details.

---

### `bcp stats`

Print size and token-efficiency statistics for an LCP file.

```bash
bcp stats <FILE>
```

**Example output:**

```text
File:    context.lcp  (282 bytes)
Header:  LCP v1.0, flags=0x00 (uncompressed)
Blocks:  6 total

Type                 Count   Bytes
────────────────────────────────────
CODE                     1      32
CONVERSATION             1      20
ANNOTATION               1       1
TOOL_RESULT              1      32
DOCUMENT                 1      38
STRUCTURED_DATA          1      29
────────────────────────────────────
Total                    6     152

Tokens (heuristic estimate):
  xml mode      ~93 tokens
  markdown mode ~68 tokens
  minimal mode  ~64 tokens
```

Token estimates use `HeuristicEstimator` (4 chars ≈ 1 token) on the rendered output of each mode with `Verbosity::Full`. The `~` prefix signals they are approximations; actual tokenisation varies by model and content type.

**Compression note:** When the header flag `0x01` is set, the stats output shows `(payload zstd-compressed)`. The file size reflects the compressed payload, while the bytes column shows decoded (uncompressed) content sizes.

---

## Module Map

```text
crates/bcp-cli/
├── Cargo.toml          — bin [[bin]] name="bcp", workspace deps
└── src/
    ├── main.rs         — Cli/Commands/Args structs, dispatch, exit code handling
    ├── cmd_inspect.rs  — bcp inspect
    ├── cmd_validate.rs — bcp validate
    ├── cmd_encode.rs   — bcp encode (manifest parsing, LcpEncoder builder)
    ├── cmd_decode.rs   — bcp decode (DefaultDriver dispatch)
    └── cmd_stats.rs    — bcp stats (block distribution, HeuristicEstimator)
```

### `main.rs` — CLI Root

Uses `clap` derive macros to define the `Cli` struct (global flags), `Commands` enum (subcommands), and `*Args` structs. The `main()` function dispatches to the appropriate `cmd_*::run(&args)` function and converts `Err` to `process::exit(1)` so stderr errors always produce exit code 1.

```rust
#[derive(Parser)]
#[command(name = "bcp", version, about = "Bit Context Protocol CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, global = true)]
    verbose: bool,

    #[arg(long, global = true)]
    no_color: bool,
}
```

Global flags (`--verbose`, `--no-color`) are defined with `global = true` so they're accepted before or after the subcommand name.

### `cmd_encode.rs` — Manifest Parsing

Uses a `#[serde(tag = "type", rename_all = "snake_case")]` enum to dispatch on the `"type"` field of each block object. `content_file` resolves relative to the manifest's parent directory. The `resolve_content` helper takes `Option<&str>` (not `&Option<String>`) per clippy's `ref_option` pedantic lint.

### `cmd_stats.rs` — Block Distribution Table

Maintains insertion order for the block-type table via a `Vec<String>` in parallel with the `HashMap`, so rows appear in the same order blocks appear in the file rather than hash-ordered.

---

## Build & Test

```bash
# Build the CLI
cargo build -p bcp-cli

# Run help
cargo run -p bcp-cli -- --help
cargo run -p bcp-cli -- inspect --help

# End-to-end pipeline with test manifest
cat > /tmp/test.json << 'EOF'
{
  "blocks": [
    {"type": "code", "lang": "rust", "path": "src/main.rs",
     "content": "fn main() { println!(\"hello\"); }", "summary": "Entry point"},
    {"type": "conversation", "role": "user", "content": "Fix the timeout bug.", "priority": "high"}
  ]
}
EOF

cargo run -p bcp-cli -- encode /tmp/test.json -o /tmp/test.lcp
cargo run -p bcp-cli -- validate /tmp/test.lcp
cargo run -p bcp-cli -- inspect /tmp/test.lcp --show-body
cargo run -p bcp-cli -- decode /tmp/test.lcp --mode xml
cargo run -p bcp-cli -- decode /tmp/test.lcp --mode minimal --include code
cargo run -p bcp-cli -- stats /tmp/test.lcp

# Pedantic clippy
cargo clippy -p bcp-cli -- -W clippy::pedantic
```

---

## Error Handling

All commands return `anyhow::Result<()>`. The `main()` dispatcher prints errors using `anyhow`'s `{:#}` format (chain of context) and calls `process::exit(1)`.

```text
$ bcp encode /tmp/missing.json -o /tmp/out.lcp
error: cannot read /tmp/missing.json: No such file or directory (os error 2)

$ bcp decode /tmp/corrupt.lcp
error: failed to decode /tmp/corrupt.lcp: invalid header — invalid magic number: expected 0x4C435000, got 0x00000000
```

Exit codes:

```text
┌──────┬────────────────────────────────────────────────────┐
│ Code │ Meaning                                            │
├──────┼────────────────────────────────────────────────────┤
│  0   │ Success                                            │
│  1   │ Error (I/O failure, invalid file, bad flag, etc.) │
└──────┴────────────────────────────────────────────────────┘
```

`bcp validate` is the only command that intentionally returns exit 1 for valid-but-invalid-LCP files (as opposed to I/O errors) — the distinction is intentional so you can use it as a pre-commit check:

```bash
bcp validate context.lcp || exit 1
```
