# Driver / Renderer

<span class="badge badge-green">Complete</span> <span class="badge badge-blue">Phase 2</span>

> The driver is the layer between decoded blocks and the LLM's token input. It renders `Vec<Block>` into model-ready text in one of three output modes: XML, Markdown, or Minimal.

## Overview

The driver sits at the end of the LCP pipeline:

```
.lcp binary ──▶ bcp-decoder ──▶ Vec<Block> ──▶ bcp-driver ──▶ model-ready text ──▶ LLM
```

It is not a simple serializer — it is an **opinionated renderer** that makes decisions about how to present context to maximize model comprehension within a token budget (RFC §5.1). This implementation covers the core rendering path without the token budget engine (deferred to SPEC_08).

## Output Modes

Per RFC §5.4, the driver supports three output format modes:

| Mode | Target | Token Cost | Wrapper |
|------|--------|-----------|---------|
| **XML** | Claude-family models | Medium | `<context>...</context>` |
| **Markdown** | All models | Highest | None |
| **Minimal** | Budget-constrained | Lowest | None |

### XML Mode

Emits semantic XML elements wrapped in `<context>`. Optimized for Claude-family models which have strong XML comprehension.

```xml
<context>
<code lang="rust" path="src/main.rs">
fn main() {
    let config = Config::load()?;
}
</code>

<tool name="ripgrep" status="ok">
3 matches for 'ConnectionPool' across 2 files.
</tool>

<turn role="user">Fix the connection timeout bug.</turn>
<turn role="assistant">I'll examine the pool config...</turn>
</context>
```

### Markdown Mode

Emits conventional fenced code blocks and headers. Compatible with all models.

```text
## src/main.rs

~~~rust
fn main() {
    let config = Config::load()?;
}
~~~

### Tool: ripgrep (ok)

3 matches for 'ConnectionPool' across 2 files.

**User**: Fix the connection timeout bug.

**Assistant**: I'll examine the pool config...
```

### Minimal Mode

Single-line delimiters for maximum token efficiency.

```text
--- src/main.rs [rust] ---
fn main() {
    let config = Config::load()?;
}

--- ripgrep [ok] ---
3 matches for 'ConnectionPool' across 2 files.

[user] Fix the connection timeout bug.
[assistant] I'll examine the pool config...
```

## Block Type → Element Mapping

Every block type maps to a specific rendering element in each mode:

| Block Type | XML Element | Markdown | Minimal |
|------------|-------------|----------|---------|
| Code | `<code lang="X" path="Y">` | `## path` + fenced block | `--- path [lang] ---` |
| Conversation | `<turn role="X">` | `**Role**: content` | `[role] content` |
| FileTree | `<tree root="X">` | `### File Tree: root` | `--- tree: root ---` |
| ToolResult | `<tool name="X" status="Y">` | `### Tool: name (status)` | `--- name [status] ---` |
| Document | `<doc title="X" format="Y">` | `### Document: title [fmt]` | `--- title ---` |
| StructuredData | `<data format="X">` | Fenced block with format | `--- data [format] ---` |
| Diff | `<diff path="X">` | `### Diff: path` + diff fence | `--- diff: path ---` |
| Annotation | *(not rendered)* | *(not rendered)* | *(not rendered)* |
| EmbeddingRef | `<embed-ref model="X" />` | `*[Embedding ref: model]*` | `[embed-ref: model]` |
| Image | `<image type="X" alt="Y">` | `### Image (type): alt` | `--- image [type]: alt ---` |
| Extension | `<ext ns="X" type="Y">` | `### Extension: ns/type` | `--- ext: ns/type ---` |

## Summary Rendering

When a block has a summary and the driver renders it in summary mode, the summary replaces the full content:

| Mode | Output |
|------|--------|
| XML | `<code lang="rust" path="..." summary="true">{summary}</code>` |
| Markdown | `## src/main.rs (summary)\n\n{summary}` |
| Minimal | `--- src/main.rs [rust] (summary) ---\n{summary}` |

This is the foundation for the token budget engine (SPEC_08). When budget is constrained, low-priority blocks will automatically fall back to summary rendering.

## File Tree Rendering

All modes render file trees with consistent indentation:

```text
src/
  main.rs (1024 bytes)
  lib.rs (512 bytes)
  utils/
    helpers.rs (256 bytes)
```

The wrapper differs per mode (XML uses `<tree>`, Markdown uses a fenced block, Minimal uses `--- tree: ---`).

## Filtering

The driver applies two layers of filtering before rendering:

1. **Automatic suppression**: `Annotation` blocks (metadata-only) and `End` blocks (wire sentinels) are always excluded.
2. **`include_types` filter**: When set in `DriverConfig`, only blocks matching the specified types are rendered. All others are silently skipped.

If filtering leaves zero renderable blocks, the driver returns `DriverError::EmptyInput`.

## Configuration

```rust
pub struct DriverConfig {
    pub mode: OutputMode,                     // Xml | Markdown | Minimal
    pub target_model: Option<ModelFamily>,    // Claude | Gpt | Gemini | Generic
    pub include_types: Option<Vec<BlockType>>, // Optional allowlist
}
```

`target_model` is reserved for future model-specific tuning. Currently it does not affect output.
