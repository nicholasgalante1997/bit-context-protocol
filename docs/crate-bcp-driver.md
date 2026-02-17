# bcp-driver

<span class="badge badge-green">Complete</span> <span class="badge badge-blue">Phase 2</span>

> The rendering layer. Takes decoded `Vec<Block>` from `bcp-decoder` and produces model-ready text in XML, Markdown, or Minimal output modes. This is the final stage of the LCP pipeline before text enters the LLM's context window.

## Crate Info

| Field | Value |
|-------|-------|
| Path | `crates/bcp-driver/` |
| Spec | [SPEC_05](driver.md) |
| Dependencies | `bcp-wire`, `bcp-types`, `thiserror` |
| Dev Dependencies | `bcp-encoder`, `bcp-decoder` (integration tests) |

---

## Purpose and Role in the Protocol

The driver is the consumption endpoint of the LCP pipeline. Where the decoder converts binary bytes into typed `Block` structs, the driver converts those structs into the text that an LLM actually reads:

```
.lcp binary ──▶ bcp-decoder ──▶ Vec<Block> ──▶ bcp-driver ──▶ model-ready text ──▶ LLM
                                                    │
                                              DriverConfig
                                              (mode, target_model, include_types)
```

The RFC (Section 5.1) describes the driver as "not a simple deserializer — it is an opinionated renderer that makes decisions about how to present context to maximize model comprehension within a token budget." The key insight is that the same set of blocks can be rendered in fundamentally different ways depending on the target model and available budget:

- **XML mode** wraps blocks in semantic elements like `<code lang="rust" path="src/main.rs">`, optimized for Claude-family models that have strong XML comprehension built into their training.
- **Markdown mode** produces conventional fenced code blocks and headers, compatible with every model but using more tokens for structural overhead.
- **Minimal mode** uses single-line delimiters like `--- src/main.rs [rust] ---`, achieving maximum token efficiency at the cost of less semantic structure.

The driver also handles **summary fallback**: when a block has an attached summary (from the encoder's `with_summary()` call), the summary text replaces the full content. This is the hook for the token budget engine (SPEC_08), which will automatically trigger summary mode for low-priority blocks when context space runs out.

---

## Core Trait: `LcpDriver`

The public interface is a single-method trait, keeping the contract minimal and easy to implement for custom drivers:

```rust
pub trait LcpDriver {
    fn render(
        &self,
        blocks: &[Block],
        config: &DriverConfig,
    ) -> Result<String, DriverError>;
}
```

Implementations are expected to be **stateless** — all configuration comes through `DriverConfig`, and the block slice is immutable. This makes drivers safe to share across threads and trivial to test.

---

## DefaultDriver

The standard implementation that ships with the crate. It performs three steps:

### Step 1: Filter

```
Input blocks ──▶ Remove Annotation blocks (metadata-only, never rendered)
             ──▶ Remove End blocks (wire sentinels, not content)
             ──▶ Apply include_types filter (if set in config)
             ──▶ If zero blocks remain → return DriverError::EmptyInput
```

Annotations are the LCP protocol's mechanism for attaching metadata to other blocks (priority hints, tags, summaries). They're never rendered as visible text — their data is consumed by the token budget engine during the scan pass. The driver suppresses them unconditionally.

### Step 2: Dispatch

Based on `config.mode`, the driver selects one of three internal renderers:

| `OutputMode` | Renderer | Wrapper |
|-------------|----------|---------|
| `Xml` | `XmlRenderer` | `<context>...</context>` |
| `Markdown` | `MarkdownRenderer` | None |
| `Minimal` | `MinimalRenderer` | None |

### Step 3: Render

The selected renderer iterates the filtered blocks, converts each to a string, and joins them with blank-line separators. XML mode additionally wraps the entire output in `<context>` tags.

---

## DriverConfig

```rust
pub struct DriverConfig {
    pub mode: OutputMode,
    pub target_model: Option<ModelFamily>,
    pub include_types: Option<Vec<BlockType>>,
}
```

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `mode` | `OutputMode` | `Xml` | Selects XML, Markdown, or Minimal rendering |
| `target_model` | `Option<ModelFamily>` | `None` | Model-specific formatting hints (reserved for future use) |
| `include_types` | `Option<Vec<BlockType>>` | `None` | When set, only render blocks of these types |

The default configuration uses XML mode with no model hint and no type filter. XML is the default because it produces the most semantically structured output.

### OutputMode

```rust
pub enum OutputMode {
    Xml,      // <code lang="rust" path="...">content</code>
    Markdown, // ## path\n\n```rust\ncontent\n```
    Minimal,  // --- path [lang] ---\ncontent
}
```

### ModelFamily

```rust
pub enum ModelFamily {
    Claude,   // Strong XML parsing, prefers semantic tags
    Gpt,      // Strong markdown parsing, prefers fenced blocks
    Gemini,   // Handles both XML and markdown well
    Generic,  // No model-specific tuning
}
```

Currently `target_model` does not affect output. It is reserved for SPEC_08 where it may influence attribute ordering, header depth, or other minor formatting choices.

---

## Renderers

### XmlRenderer

Wraps all output in `<context>...</context>`. Each block type maps to a specific XML element:

```text
┌───────────────────┬──────────────────────────────────────────┐
│ Block Type        │ XML Element                              │
├───────────────────┼──────────────────────────────────────────┤
│ Code              │ <code lang="X" path="Y">...</code>      │
│ Conversation      │ <turn role="X">...</turn>                │
│ FileTree          │ <tree root="X">...</tree>                │
│ ToolResult        │ <tool name="X" status="Y">...</tool>     │
│ Document          │ <doc title="X" format="Y">...</doc>      │
│ StructuredData    │ <data format="X">...</data>              │
│ Diff              │ <diff path="X">...</diff>                │
│ EmbeddingRef      │ <embed-ref model="X" />                  │
│ Image             │ <image type="X" alt="Y">...</image>      │
│ Extension         │ <ext ns="X" type="Y">...</ext>           │
│ Unknown           │ <!-- unknown block type 0xNN -->         │
└───────────────────┴──────────────────────────────────────────┘
```

**XML attribute escaping**: All attribute values pass through `xml_escape()` which replaces `&`, `<`, `>`, and `"` with their XML entity equivalents. This prevents malformed XML from file paths or tool names containing special characters.

**Summary rendering**: When a block has a summary, the XML renderer adds `summary="true"` to the element and uses the summary text instead of the full content:

```xml
<code lang="rust" path="src/main.rs" summary="true">
Entry point: CLI arg parsing, config loading, server startup.
</code>
```

### MarkdownRenderer

Uses conventional markdown formatting. No outer wrapper — blocks are separated by blank lines.

- Code blocks use `##` headers with fenced code blocks: `` ```rust ... ``` ``
- Conversation turns use bold role labels: `**User**: content`
- Tool results use `###` headers: `### Tool: name (status)`
- File trees use fenced blocks: `` ``` ... ``` ``
- Summary mode uses `(summary)` suffix: `## path (summary)`

### MinimalRenderer

Uses the fewest structural tokens. Single-line delimiters mark block boundaries.

- Code: `--- path [lang] ---`
- Conversation: `[role] content`
- Tools: `--- name [status] ---`
- Trees: `--- tree: root ---`
- Summary mode: `--- path [lang] (summary) ---`

---

## Shared Helpers

The renderers share several utility functions (declared `pub(crate)` in `render_xml.rs`):

| Function | Purpose |
|----------|---------|
| `lang_display_name(Lang) -> &str` | Converts `Lang::Rust` → `"rust"`, etc. |
| `role_display_name(Role) -> &str` | Converts `Role::User` → `"user"`, etc. |
| `status_display_name(Status) -> &str` | Converts `Status::Ok` → `"ok"`, etc. |
| `format_hint_display_name(FormatHint) -> &str` | Converts `FormatHint::Markdown` → `"markdown"`, etc. |
| `data_format_display_name(DataFormat) -> &str` | Converts `DataFormat::Json` → `"json"`, etc. |
| `media_type_display_name(MediaType) -> &str` | Converts `MediaType::Png` → `"png"`, etc. |
| `content_to_string(&[u8], usize) -> Result<String>` | UTF-8 validation with block index for error context |
| `render_file_tree_entries(&[FileEntry], depth)` | Recursive tree rendering with indentation |

These live in `render_xml.rs` because XML is the primary/default mode, but they're used by all three renderers.

---

## Error Types

```rust
pub enum DriverError {
    EmptyInput,
    UnsupportedBlockType { block_type: BlockType },
    InvalidContent { block_index: usize },
}
```

| Variant | Cause | Recovery |
|---------|-------|----------|
| `EmptyInput` | No renderable blocks after filtering | Check that blocks exist and match `include_types` |
| `UnsupportedBlockType` | Block type cannot be rendered (reserved) | Remove or filter the block |
| `InvalidContent` | Block body is not valid UTF-8 | Check encoder input — all text content must be UTF-8 |

`block_index` in `InvalidContent` refers to the block's position in the filtered (not original) list, which helps callers identify the problematic block.

---

## Phase 3 Stub: Token Budget Engine

**`budget.rs`** is currently a stub. When implemented (SPEC_08), it will:

- Estimate token counts per block using the target model's tokenizer (or a heuristic)
- Rank blocks by priority using Annotation blocks (`Priority::Critical` through `Priority::Background`)
- Perform a two-pass render: scan all headers/summaries first, then render within the token budget
- Automatically fall back to summary rendering for blocks that exceed remaining budget
- Emit placeholder stubs (type, path, size) for blocks with no summary when budget is exhausted

---

## Integration: Encode → Decode → Render

The full pipeline is tested end-to-end:

```rust
let payload = LcpEncoder::new()
    .add_code(Lang::Rust, "src/main.rs", b"fn main() { ... }")
    .with_summary("Entry point: prints hello.")
    .add_tool_result("ripgrep", Status::Ok, b"3 matches found.")
    .add_conversation(Role::User, b"Fix the timeout bug.")
    .add_conversation(Role::Assistant, b"I'll check the pool config.")
    .encode()?;

let decoded = LcpDecoder::decode(&payload)?;

let driver = DefaultDriver;
let config = DriverConfig {
    mode: OutputMode::Xml,
    target_model: Some(ModelFamily::Claude),
    include_types: None,
};

let text = driver.render(&decoded.blocks, &config)?;
// text is now model-ready XML, ready to inject into the context window
```

---

## Module Map

```
src/
├── lib.rs              → Re-exports DefaultDriver, LcpDriver, DriverConfig, OutputMode, etc.
├── config.rs           → DriverConfig, OutputMode, ModelFamily
├── driver.rs           → LcpDriver trait, DefaultDriver (12 tests)
├── render_xml.rs       → XmlRenderer + shared display helpers (4 tests)
├── render_markdown.rs  → MarkdownRenderer (3 tests)
├── render_minimal.rs   → MinimalRenderer (3 tests)
├── budget.rs           → Token budget engine (stub, SPEC_08)
└── error.rs            → DriverError enum

tests/
└── render_integration.rs → Full pipeline tests: encode → decode → render (2 tests)
```

## Build & Test

```bash
cargo build -p bcp-driver
cargo test -p bcp-driver
cargo clippy -p bcp-driver -- -W clippy::pedantic
cargo doc -p bcp-driver --no-deps

# Integration: encode → decode → render
cargo test -p bcp-driver --test render_integration
```
