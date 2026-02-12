# SPEC_05 — Driver / Renderer

**Crate**: `lcp-driver`
**Phase**: 2 (Decode & Render)
**Prerequisites**: SPEC_01, SPEC_02, SPEC_04
**Dependencies**: `lcp-wire`, `lcp-types`, `lcp-decoder`

---

## Context

The driver is the layer between decoded blocks and the LLM's token input. It
is not a simple serializer — it is an opinionated renderer that makes decisions
about how to present context to maximize model comprehension within a token
budget. The driver takes a `Vec<Block>` (from the decoder) and emits
model-ready text in one of several output modes, as specified in RFC §5.

This spec covers the core rendering path without the token budget engine
(which is deferred to SPEC_08). The driver implemented here renders all
blocks unconditionally in the selected output mode.

---

## Requirements

### 1. Driver Configuration

```rust
/// Configuration for the LCP driver.
///
/// Controls how decoded blocks are rendered into model-ready text.
pub struct DriverConfig {
    /// Output format mode. Determines the textual structure of
    /// the rendered output.
    pub mode: OutputMode,

    /// Model family hint. Affects minor formatting choices
    /// (e.g., XML attribute ordering for Claude compatibility).
    pub target_model: Option<ModelFamily>,

    /// Block type filter. When set, only blocks of these types
    /// are rendered; all others are silently skipped.
    pub include_types: Option<Vec<BlockType>>,
}

/// Output format modes per RFC §5.4.
///
/// ┌──────────┬─────────────────────────────────────────────────────┐
/// │ Mode     │ Description                                        │
/// ├──────────┼─────────────────────────────────────────────────────┤
/// │ Xml      │ <code lang="rust" path="...">content</code>        │
/// │          │ Optimized for Claude-family models.                 │
/// ├──────────┼─────────────────────────────────────────────────────┤
/// │ Markdown │ ```rust\n// src/main.rs\ncontent\n```               │
/// │          │ Compatible with all models, more tokens.            │
/// ├──────────┼─────────────────────────────────────────────────────┤
/// │ Minimal  │ --- src/main.rs [rust] ---\ncontent                 │
/// │          │ Maximum token efficiency.                           │
/// └──────────┴─────────────────────────────────────────────────────┘
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Xml,
    Markdown,
    Minimal,
}

/// Model family hints for output tuning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelFamily {
    Claude,
    Gpt,
    Gemini,
    Generic,
}
```

### 2. Driver Trait

```rust
/// Core driver interface — renders decoded blocks into model-ready text.
pub trait LcpDriver {
    /// Render a complete set of decoded blocks into model-ready text.
    fn render(
        &self,
        blocks: &[Block],
        config: &DriverConfig,
    ) -> Result<String, DriverError>;
}
```

### 3. XML-Tagged Renderer

Per RFC §5.4 and §12.3, the XML mode wraps blocks in semantic XML elements.

```rust
/// XML-tagged renderer — emits <context>-wrapped XML elements.
///
/// Example output (from RFC §12.3):
///
///   <context>
///   <code lang="rust" path="src/main.rs" priority="high">
///   fn main() {
///       let config = Config::load()?;
///   }
///   </code>
///
///   <tool name="ripgrep" status="ok">
///   3 matches for 'ConnectionPool' across 2 files.
///   </tool>
///
///   <turn role="user">Fix the connection timeout bug.</turn>
///   <turn role="assistant">I'll examine the pool config...</turn>
///   </context>
pub struct XmlRenderer;

impl XmlRenderer {
    /// Render a single block to XML.
    ///
    /// Element mapping:
    /// ┌───────────────────┬────────────────────────────────────────┐
    /// │ Block Type        │ XML Element                            │
    /// ├───────────────────┼────────────────────────────────────────┤
    /// │ Code              │ <code lang="X" path="Y">...</code>     │
    /// │ Conversation      │ <turn role="X">...</turn>              │
    /// │ FileTree          │ <tree root="X">...</tree>              │
    /// │ ToolResult        │ <tool name="X" status="Y">...</tool>   │
    /// │ Document          │ <doc title="X" format="Y">...</doc>    │
    /// │ StructuredData    │ <data format="X">...</data>            │
    /// │ Diff              │ <diff path="X">...</diff>              │
    /// │ Annotation        │ (not rendered — metadata only)         │
    /// │ EmbeddingRef      │ <embed-ref model="X" />               │
    /// │ Image             │ <image type="X" alt="Y">...</image>    │
    /// │ Extension         │ <ext ns="X" type="Y">...</ext>         │
    /// └───────────────────┴────────────────────────────────────────┘
    fn render_block(&self, block: &Block) -> String { /* ... */ }
}
```

### 4. Markdown Renderer

```rust
/// Markdown renderer — emits conventional fenced code blocks and headers.
///
/// Example output:
///
///   ## src/main.rs
///
///   ```rust
///   fn main() {
///       let config = Config::load()?;
///   }
///   ```
///
///   ### Tool: ripgrep (ok)
///
///   3 matches for 'ConnectionPool' across 2 files.
///
///   **User**: Fix the connection timeout bug.
///
///   **Assistant**: I'll examine the pool config...
pub struct MarkdownRenderer;
```

### 5. Minimal Renderer

```rust
/// Minimal renderer — single-line delimiters for maximum token efficiency.
///
/// Example output:
///
///   --- src/main.rs [rust] ---
///   fn main() {
///       let config = Config::load()?;
///   }
///
///   --- ripgrep [ok] ---
///   3 matches for 'ConnectionPool' across 2 files.
///
///   [user] Fix the connection timeout bug.
///   [assistant] I'll examine the pool config...
pub struct MinimalRenderer;
```

### 6. Default Driver Implementation

```rust
/// Default driver that dispatches to the appropriate renderer
/// based on the configured output mode.
pub struct DefaultDriver;

impl LcpDriver for DefaultDriver {
    fn render(
        &self,
        blocks: &[Block],
        config: &DriverConfig,
    ) -> Result<String, DriverError> {
        // 1. Filter blocks by include_types (if set)
        // 2. Dispatch to the renderer for config.mode
        // 3. Join rendered blocks with appropriate separators
        // 4. Return the complete model-ready text
    }
}
```

### 7. Rendering Details

#### Code Block Rendering

| Mode     | Output                                                    |
|----------|-----------------------------------------------------------|
| XML      | `<code lang="rust" path="src/main.rs">\n{content}\n</code>` |
| Markdown | `## src/main.rs\n\n```rust\n{content}\n```\n`             |
| Minimal  | `--- src/main.rs [rust] ---\n{content}\n`                 |

#### Conversation Block Rendering

| Mode     | Output                                        |
|----------|-----------------------------------------------|
| XML      | `<turn role="user">{content}</turn>`           |
| Markdown | `**User**: {content}\n`                        |
| Minimal  | `[user] {content}\n`                           |

#### File Tree Rendering

All modes render the tree with indentation:

```
src/
  main.rs (1024 bytes)
  lib.rs (512 bytes)
  utils/
    helpers.rs (256 bytes)
```

| Mode     | Wrapper                                        |
|----------|------------------------------------------------|
| XML      | `<tree root="src/">\n{tree}\n</tree>`          |
| Markdown | `### File Tree: src/\n\n```\n{tree}\n```\n`    |
| Minimal  | `--- tree: src/ ---\n{tree}\n`                 |

#### Summary Rendering

When a block has a summary and the driver is in summary-only mode (SPEC_08),
the summary replaces the full content:

| Mode     | Output                                                     |
|----------|------------------------------------------------------------|
| XML      | `<code lang="rust" path="..." summary="true">{summary}</code>` |
| Markdown | `## src/main.rs (summary)\n\n{summary}\n`                  |
| Minimal  | `--- src/main.rs [rust] (summary) ---\n{summary}\n`        |

### 8. Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("no blocks to render")]
    EmptyInput,

    #[error("unsupported block type for rendering: {block_type:?}")]
    UnsupportedBlockType { block_type: BlockType },

    #[error("invalid UTF-8 in block content at index {block_index}")]
    InvalidContent { block_index: usize },
}
```

---

## File Structure

```
crates/lcp-driver/
├── Cargo.toml
└── src/
    ├── lib.rs              # Crate root: pub use DefaultDriver, LcpDriver
    ├── driver.rs           # LcpDriver trait, DefaultDriver, DriverConfig
    ├── render_xml.rs       # XmlRenderer
    ├── render_markdown.rs  # MarkdownRenderer
    ├── render_minimal.rs   # MinimalRenderer
    ├── budget.rs           # Token budget engine (stub in Phase 2, full in SPEC_08)
    └── error.rs            # DriverError
```

---

## Acceptance Criteria

- [ ] XML mode output for a CODE block matches the format in RFC §12.3
- [ ] XML mode wraps all output in `<context>...</context>`
- [ ] Markdown mode produces valid fenced code blocks with language hints
- [ ] Minimal mode uses single-line `--- path [lang] ---` delimiters
- [ ] All 11 block types render without errors in all three modes
- [ ] ANNOTATION blocks are not rendered as visible text (metadata only)
- [ ] `include_types` filter correctly omits non-matching blocks
- [ ] Conversation blocks render with correct role labels in all modes
- [ ] File trees render with correct indentation and size annotations
- [ ] Summary rendering produces correct output when summary is present
- [ ] Empty block list returns `DriverError::EmptyInput`

---

## Verification

```bash
cargo build -p lcp-driver
cargo test -p lcp-driver
cargo clippy -p lcp-driver -- -W clippy::pedantic
cargo doc -p lcp-driver --no-deps

# Integration: encode → decode → render
cargo test -p lcp-driver --test render_integration
```
