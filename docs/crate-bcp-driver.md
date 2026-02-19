# bcp-driver

<span class="badge badge-green">Complete</span> <span class="badge badge-blue">Phase 3</span>

> The rendering layer. Takes decoded `Vec<Block>` from `bcp-decoder` and produces model-ready text in XML, Markdown, or Minimal output modes. Includes the **Token Budget Engine** — a two-pass algorithm that fits blocks within a token limit by degrading lower-priority content to summaries, placeholders, or omissions.

## Crate Info

| Field | Value |
|-------|-------|
| Path | `crates/bcp-driver/` |
| Spec | [SPEC_05](driver.md), [SPEC_08](budget.md) |
| Dependencies | `bcp-wire`, `bcp-types`, `thiserror` |
| Dev Dependencies | `bcp-encoder`, `bcp-decoder` (integration tests) |

---

## Purpose and Role in the Protocol

The driver is the consumption endpoint of the BCP pipeline. Where the decoder converts binary bytes into typed `Block` structs, the driver converts those structs into the text that an LLM actually reads:

```
.bcp binary ──▶ bcp-decoder ──▶ Vec<Block> ──▶ bcp-driver ──▶ model-ready text ──▶ LLM
                                                    │
                                              DriverConfig
                                              (mode, verbosity, token_budget, include_types)
```

The RFC (Section 5.1) describes the driver as "not a simple deserializer — it is an opinionated renderer that makes decisions about how to present context to maximize model comprehension within a token budget." The key insight is that the same set of blocks can be rendered in fundamentally different ways depending on the target model and available budget:

- **XML mode** wraps blocks in semantic elements like `<code lang="rust" path="src/main.rs">`, optimized for Claude-family models that have strong XML comprehension built into their training.
- **Markdown mode** produces conventional fenced code blocks and headers, compatible with every model but using more tokens for structural overhead.
- **Minimal mode** uses single-line delimiters like `--- src/main.rs [rust] ---`, achieving maximum token efficiency at the cost of less semantic structure.

The driver also handles **budget-aware degradation**: when a `token_budget` is set, the budget engine resolves per-block priorities (from ANNOTATION blocks), estimates token costs, and selects the optimal rendering for each block — full content, summary, placeholder, or omission — to maximize information density within the budget.

---

## Core Trait: `BcpDriver`

The public interface is a single-method trait, keeping the contract minimal and easy to implement for custom drivers:

```rust
pub trait BcpDriver {
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

The standard implementation that ships with the crate. It performs a three-step pipeline:

```text
┌─────────────┐     ┌────────────────────┐     ┌──────────────────┐
│ &[Block]    │────▶│ 1. Filter          │────▶│ XmlRenderer      │
│             │     │ 2. Budget decisions │     │ MarkdownRenderer │
│             │     │ 3. Dispatch        │     │ MinimalRenderer  │
└─────────────┘     └────────────────────┘     └──────────────────┘
                           │                          │
                     DriverConfig                String output
```

### Step 1: Filter

```
Input blocks ──▶ Remove Annotation blocks (metadata-only, never rendered)
             ──▶ Remove End blocks (wire sentinels, not content)
             ──▶ Apply include_types filter (if set in config)
             ──▶ Track original_indices (for annotation → block mapping)
             ──▶ If zero blocks remain → return DriverError::EmptyInput
```

Annotations are the BCP protocol's mechanism for attaching metadata to other blocks (priority hints, tags, summaries). They're never rendered as visible text — their data is consumed by the budget engine during the scan pass. The driver suppresses them unconditionally.

The `original_indices` vector maps each filtered block's position back to its index in the original unfiltered block list. This is essential for the budget engine to correctly resolve annotation targets.

### Step 2: Budget Decisions

Based on `config.verbosity` and `config.token_budget`, the driver computes a `RenderDecision` per block:

```text
┌──────────────────────────────────┬──────────────────────────────────┐
│ (token_budget, verbosity)        │ Decision Strategy                │
├──────────────────────────────────┼──────────────────────────────────┤
│ (_, Summary)                     │ Summary where available, else    │
│                                  │ Full. Budget is ignored.         │
├──────────────────────────────────┼──────────────────────────────────┤
│ (Some(budget), Adaptive)         │ Run the full budget engine.      │
│                                  │ Per-block decisions based on     │
│                                  │ priority + budget remaining.     │
├──────────────────────────────────┼──────────────────────────────────┤
│ (None, Adaptive) or (_, Full)    │ All blocks render Full.          │
│                                  │ Budget is ignored.               │
└──────────────────────────────────┴──────────────────────────────────┘
```

### Step 3: Dispatch

The selected renderer receives `(block, decision)` pairs and uses the decision-aware rendering path (`render_all_with_decisions`). This is a single code path for all configurations — the renderer doesn't need to know about budgets or priorities.

| `OutputMode` | Renderer | Wrapper |
|-------------|----------|---------|
| `Xml` | `XmlRenderer` | `<context>...</context>` |
| `Markdown` | `MarkdownRenderer` | None |
| `Minimal` | `MinimalRenderer` | None |

---

## Token Budget Engine

The budget engine implements RFC §5.5's two-pass algorithm for fitting blocks within a token limit. It lives entirely in `budget.rs` and is invoked only when `config.token_budget` is `Some(n)` and `config.verbosity` is `Adaptive`.

### Algorithm Overview

```text
all_blocks ──▶ resolve_priorities() ──▶ HashMap<u32, Priority>
                                               │
filtered ──────────────────────────────▶ scan_blocks()
                                               │
                                         Vec<BlockBudgetInfo>
                                               │
                                        allocate_budget()
                                               │
                                        Vec<RenderDecision>
```

**Pass 1: Scan** — Walk all blocks, extract priority annotations into a `HashMap<target_block_id, Priority>`. Then for each filtered block, estimate full-content and summary token costs using a `TokenEstimator`.

**Pass 2: Allocate** — Sort blocks by priority (Critical first, Background last). Walk sorted blocks, greedily subtracting from remaining budget. Each block gets the best possible rendering within its priority's degradation path.

### Priority Resolution

Priorities come from ANNOTATION blocks with `AnnotationKind::Priority`. Each annotation targets a block by its zero-based index in the original (unfiltered) stream. If multiple annotations target the same block, the last one wins. Blocks without an annotation default to `Priority::Normal`.

```text
blocks[0]: Code("main.rs")
blocks[1]: Annotation(target=0, priority=Critical)  ← main.rs gets Critical
blocks[2]: Code("lib.rs")                           ← no annotation, defaults to Normal
```

### Priority Degradation Paths

Each priority level has a specific degradation path — the sequence of `RenderDecision` variants the engine tries as budget runs out:

```text
┌────────────┬──────────────────────────────────────────────────┐
│ Priority   │ Degradation path                                 │
├────────────┼──────────────────────────────────────────────────┤
│ Critical   │ Full (always, even over budget)                  │
│ High       │ Full → Summary → Full (forced, over budget)      │
│ Normal     │ Full → Summary → Placeholder                     │
│ Low        │ Summary → Placeholder                            │
│ Background │ Placeholder → Omit                               │
└────────────┴──────────────────────────────────────────────────┘
```

Key design choices:

- **Critical** blocks always render in full — they represent content the user explicitly marked as essential. Budget violation is acceptable.
- **High** blocks are similar but will use summaries if available. If no summary exists and budget is exhausted, they still render full (like Critical, but with a preference for budget compliance).
- **Normal** blocks (the default) try full content, then summary, then placeholder. This is the bread-and-butter of budget management.
- **Low** blocks never get full content — they start at summary and degrade to placeholder.
- **Background** blocks only get placeholders (which cost ~10 tokens). If even that won't fit, they're omitted entirely.

### Token Estimation

The `TokenEstimator` trait is pluggable — it allows swapping in a real tokenizer (tiktoken, etc.) without changing the budget algorithm:

```rust
pub trait TokenEstimator: Send + Sync {
    fn estimate(&self, text: &str) -> u32;
}
```

Two implementations ship with the crate:

```text
┌──────────────────────┬────────┬─────────────────────────────────┐
│ Estimator            │ Ratio  │ Best for                        │
├──────────────────────┼────────┼─────────────────────────────────┤
│ HeuristicEstimator   │ ÷ 4   │ Quick approximation, all text   │
│ CodeAwareEstimator   │ ÷ 3/4 │ Mixed code + prose payloads     │
└──────────────────────┴────────┴─────────────────────────────────┘
```

- `HeuristicEstimator`: `chars / 4`, minimum 1. Matches the rule-of-thumb that English prose averages ~4 characters per token. Systematically underestimates code.
- `CodeAwareEstimator`: `chars / 3` for code (>30% of non-empty lines indented), `chars / 4` for prose. This is the default used by `DefaultDriver`. Code produces more tokens per character due to short identifiers and punctuation.

### RenderDecision

The budget engine produces one `RenderDecision` per filtered block:

```text
┌─────────────┬──────────────────────────────────────────────────┐
│ Variant     │ Behavior                                         │
├─────────────┼──────────────────────────────────────────────────┤
│ Full        │ Render complete block content (ignore summary)   │
│ Summary     │ Render summary text only                         │
│ Placeholder │ Emit a compact omission notice with metadata     │
│ Omit        │ Skip the block entirely (no output)              │
└─────────────┴──────────────────────────────────────────────────┘
```

### Numeric Example

Consider 3 blocks with a budget of 150 tokens:

```
Block A: Critical, full=100tok, no summary
Block B: Normal, full=80tok, summary=10tok
Block C: Background, full=60tok, no summary
```

Allocation (sorted by priority — Critical first):

1. **A (Critical)**: Full → budget 150−100 = 50 remaining
2. **B (Normal)**: Full costs 80, over budget. Summary costs 10, fits → Summary. Budget 50−10 = 40 remaining
3. **C (Background)**: Placeholder costs 10, fits → Placeholder. Budget 40−10 = 30 remaining

Result: `[Full, Summary, Placeholder{omitted=60}]`

---

## Placeholders

When a block is reduced to a placeholder, the driver emits a compact notice that tells the model what was omitted. The format varies by output mode:

```text
┌──────────┬──────────────────────────────────────────────────────┐
│ Mode     │ Output                                               │
├──────────┼──────────────────────────────────────────────────────┤
│ Xml      │ <omitted type="code" desc="src/main.rs" tokens="823"/>│
│ Markdown │ _[Omitted: code src/main.rs, ~823 tokens]_           │
│ Minimal  │ [omitted: code src/main.rs ~823tok]                   │
└──────────┴──────────────────────────────────────────────────────┘
```

Placeholders cost ~10-15 tokens regardless of the omitted block's size. They let the model know that context exists without paying the full token cost — if the model needs the omitted content, the consuming application can re-render with a larger budget or higher priority.

---

## DriverConfig

```rust
pub struct DriverConfig {
    pub mode: OutputMode,
    pub target_model: Option<ModelFamily>,
    pub include_types: Option<Vec<BlockType>>,
    pub token_budget: Option<u32>,
    pub verbosity: Verbosity,
}
```

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `mode` | `OutputMode` | `Xml` | Selects XML, Markdown, or Minimal rendering |
| `target_model` | `Option<ModelFamily>` | `None` | Model-specific formatting hints (reserved for future use) |
| `include_types` | `Option<Vec<BlockType>>` | `None` | When set, only render blocks of these types |
| `token_budget` | `Option<u32>` | `None` | Approximate token limit for rendered output |
| `verbosity` | `Verbosity` | `Adaptive` | Full / Summary / Adaptive rendering mode |

### Verbosity

```text
┌──────────┬────────────────────────────────────────────────────────┐
│ Mode     │ Behavior                                               │
├──────────┼────────────────────────────────────────────────────────┤
│ Full     │ Always render full content. Ignore budget entirely.    │
│ Summary  │ Always render summaries where available, full content  │
│          │ otherwise. Budget is ignored.                          │
│ Adaptive │ Auto-select per block based on budget + priority.      │
│          │ Without a budget set, behaves like Full.               │
└──────────┴────────────────────────────────────────────────────────┘
```

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

Currently `target_model` does not affect output. It is reserved for future use where it may influence attribute ordering, header depth, or other minor formatting choices.

---

## Renderers

All three renderers share a common pattern:

- `render_all(&[&Block])` — legacy entry point (auto-uses summary if present)
- `render_all_with_decisions(&[(&Block, &RenderDecision)])` — budget-aware entry point
- `render_block_inner(block, index, use_summary: bool)` — shared core logic

The `render_all_with_decisions` method is the primary rendering path. It handles all four `RenderDecision` variants: Full renders the complete content, Summary renders the summary text, Placeholder emits a compact notice, and Omit skips the block.

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

**XML attribute escaping**: All attribute values pass through `xml_escape()` which replaces `&`, `<`, `>`, and `"` with their XML entity equivalents.

**Summary rendering**: When a block renders with `RenderDecision::Summary`, the XML renderer adds `summary="true"` to the element and uses the summary text instead of the full content:

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

## Integration: Encode → Decode → Render

The full pipeline, including budget-aware rendering:

```rust
use bcp_encoder::BcpEncoder;
use bcp_decoder::BcpDecoder;
use bcp_driver::{DefaultDriver, DriverConfig, BcpDriver, OutputMode, Verbosity};
use bcp_types::enums::{Lang, Priority, Role, Status};

let payload = BcpEncoder::new()
    .add_code(Lang::Rust, "src/main.rs", b"fn main() { ... }")
    .with_summary("Entry point: prints hello.")
    .with_priority(Priority::Critical)
    .add_tool_result("ripgrep", Status::Ok, b"3 matches found.")
    .add_conversation(Role::User, b"Fix the timeout bug.")
    .encode()?;

let decoded = BcpDecoder::decode(&payload)?;

let driver = DefaultDriver;

// Render with a 500-token budget — Critical block gets full content,
// other blocks degrade to summaries or placeholders as needed.
let config = DriverConfig {
    mode: OutputMode::Xml,
    token_budget: Some(500),
    verbosity: Verbosity::Adaptive,
    ..DriverConfig::default()
};

let text = driver.render(&decoded.blocks, &config)?;
// text is now budget-optimized XML, ready for the LLM's context window
```

---

## Module Map

```
src/
├── lib.rs              → Re-exports DefaultDriver, BcpDriver, DriverConfig, OutputMode,
│                         Verbosity, RenderDecision, TokenEstimator, etc.
├── config.rs           → DriverConfig, OutputMode, ModelFamily, Verbosity
├── driver.rs           → BcpDriver trait, DefaultDriver (13 tests)
├── render_xml.rs       → XmlRenderer + shared display helpers (4 tests)
├── render_markdown.rs  → MarkdownRenderer (3 tests)
├── render_minimal.rs   → MinimalRenderer (3 tests)
├── budget.rs           → Token budget engine: RenderDecision, TokenEstimator,
│                         HeuristicEstimator, CodeAwareEstimator, priority resolution,
│                         scan/allocate algorithm (26 tests)
├── placeholder.rs      → Placeholder rendering per output mode (4 tests)
└── error.rs            → DriverError enum

tests/
└── render_integration.rs → Full pipeline tests: encode → decode → render (6 tests)
```

## Build & Test

```bash
cargo build -p bcp-driver
cargo test -p bcp-driver
cargo clippy -p bcp-driver -- -W clippy::pedantic
cargo doc -p bcp-driver --no-deps

# Budget-specific tests
cargo test -p bcp-driver -- budget

# Integration: encode → decode → render (including budget roundtrips)
cargo test -p bcp-driver --test render_integration
```
