# SPEC_08 — Token Budget Engine

**Crate**: `lcp-driver` (modifications)
**Phase**: 3 (Advanced Features)
**Prerequisites**: SPEC_02, SPEC_04, SPEC_05
**Dependencies**: `lcp-types`, `lcp-decoder`, `lcp-driver`

---

## Context

The token budget engine is the intelligence layer of the driver. When a
token budget is specified, the driver performs a two-pass decode (RFC §5.5):
first scanning all block headers and summaries to estimate token counts, then
ranking blocks by priority and rendering full content or summaries based on
what fits within the budget.

This is what distinguishes LCP from a dumb serialization format. The budget
engine enables graceful degradation: the model always sees a complete picture
of what context is available, even if some blocks are rendered as summaries.

For the PoC, token estimation uses a character-count heuristic (÷4 for
English text, ÷3 for code) rather than a real tokenizer. The estimator
is pluggable so a real tokenizer can be swapped in later.

---

## Requirements

### 1. Driver Configuration Extension

```rust
/// Extended driver configuration with token budget support.
pub struct DriverConfig {
    // ... existing fields from SPEC_05 ...

    /// Approximate token budget. When set, the driver will use
    /// summaries for low-priority blocks to fit within this budget.
    /// When None, all blocks are rendered with full content.
    pub token_budget: Option<u32>,

    /// Verbosity mode for budget-aware rendering.
    pub verbosity: Verbosity,
}

/// Verbosity modes for the token budget engine.
///
/// ┌──────────┬────────────────────────────────────────────────────┐
/// │ Mode     │ Behavior                                           │
/// ├──────────┼────────────────────────────────────────────────────┤
/// │ Full     │ Always render full content. Ignore budget.         │
/// │ Summary  │ Always render summaries (if available).            │
/// │ Adaptive │ Auto-select per block based on budget + priority.  │
/// └──────────┴────────────────────────────────────────────────────┘
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verbosity {
    Full,
    Summary,
    Adaptive,
}
```

### 2. Token Estimator

```rust
/// Token count estimator. Pluggable: the PoC uses a heuristic,
/// but a real tokenizer (tiktoken, etc.) can be swapped in.
pub trait TokenEstimator: Send + Sync {
    /// Estimate the token count for a text string.
    fn estimate(&self, text: &str) -> u32;
}

/// Heuristic token estimator based on character count.
///
/// Approximation:
///   - English prose: chars / 4
///   - Code: chars / 3 (shorter tokens on average)
///   - Minimum: 1 token per non-empty string
pub struct HeuristicEstimator;

impl TokenEstimator for HeuristicEstimator {
    fn estimate(&self, text: &str) -> u32 {
        if text.is_empty() {
            return 0;
        }
        // Heuristic: ~4 chars per token for English text
        let estimate = text.len() as u32 / 4;
        estimate.max(1)
    }
}

/// Code-aware estimator that uses different ratios for code vs. prose.
pub struct CodeAwareEstimator;

impl TokenEstimator for CodeAwareEstimator {
    fn estimate(&self, text: &str) -> u32 {
        if text.is_empty() {
            return 0;
        }
        // Detect code: if >30% of lines start with whitespace, treat as code
        let lines: Vec<&str> = text.lines().collect();
        let indented = lines.iter().filter(|l| l.starts_with(' ') || l.starts_with('\t')).count();
        let ratio = if lines.is_empty() { 4 } else if indented * 100 / lines.len() > 30 { 3 } else { 4 };
        let estimate = text.len() as u32 / ratio;
        estimate.max(1)
    }
}
```

### 3. Two-Pass Budget Algorithm

```rust
/// The budget engine performs a two-pass process:
///
/// Pass 1 — Scan:
///   For each block, compute:
///     - full_tokens:    estimated tokens for full content rendering
///     - summary_tokens: estimated tokens for summary rendering (0 if no summary)
///     - priority:       from ANNOTATION blocks or default (Normal)
///     - block_index:    position in the block sequence
///
/// Pass 2 — Budget Allocation:
///   1. Sort blocks by priority (Critical first, Background last).
///      Within same priority, preserve original order.
///   2. Walk through sorted blocks, subtracting from remaining budget:
///      a. CRITICAL blocks: always full content (subtract full_tokens)
///      b. HIGH blocks: full content if budget allows, else summary
///      c. NORMAL blocks: full content if budget allows, else summary
///      d. LOW blocks: summary if budget allows, else placeholder
///      e. BACKGROUND blocks: one-line reference if budget allows, else omit
///   3. If a block has no summary and budget is exhausted, emit a
///      metadata placeholder: "[block_type: path, N tokens omitted]"
///
/// The result is a Vec<RenderDecision> that tells the renderer
/// what to emit for each block.

/// Decision for how to render a single block.
pub enum RenderDecision {
    /// Render the full block content.
    Full,
    /// Render the summary text only.
    Summary,
    /// Render a one-line metadata placeholder.
    Placeholder {
        block_type: BlockType,
        description: String,
        omitted_tokens: u32,
    },
    /// Omit the block entirely (Background blocks over budget).
    Omit,
}

/// Block metadata collected during the scan pass.
struct BlockBudgetInfo {
    block_index: usize,
    priority: Priority,
    full_tokens: u32,
    summary_tokens: u32,
    has_summary: bool,
}

/// Run the two-pass budget algorithm.
///
/// Returns a RenderDecision for each block in the original order.
fn allocate_budget(
    blocks: &[Block],
    budget: u32,
    estimator: &dyn TokenEstimator,
    mode: OutputMode,
) -> Vec<RenderDecision> {
    // Implementation
}
```

### 4. Priority Resolution

Priorities come from ANNOTATION blocks targeting other blocks:

```rust
/// Resolve priorities for all blocks.
///
/// Walk through the block list. For each ANNOTATION block with
/// kind=Priority, apply its priority value to the target block.
/// Blocks without an annotation default to Priority::Normal.
///
/// ANNOTATION blocks themselves are not rendered (they are metadata).
fn resolve_priorities(blocks: &[Block]) -> Vec<Priority> {
    // Implementation
}
```

### 5. Placeholder Rendering

When a block is over budget and has no summary:

```rust
/// Render a metadata placeholder for an omitted block.
///
/// Format per output mode:
///   XML:      <omitted type="code" path="src/main.rs" tokens="823" />
///   Markdown: _[Omitted: code src/main.rs, ~823 tokens]_
///   Minimal:  [omitted: code src/main.rs ~823tok]
fn render_placeholder(
    block: &Block,
    omitted_tokens: u32,
    mode: OutputMode,
) -> String {
    // Implementation
}
```

### 6. Budget-Aware Driver Integration

```rust
impl DefaultDriver {
    fn render(
        &self,
        blocks: &[Block],
        config: &DriverConfig,
    ) -> Result<String, DriverError> {
        let decisions = match (config.token_budget, config.verbosity) {
            (Some(budget), Verbosity::Adaptive) => {
                allocate_budget(blocks, budget, &self.estimator, config.mode)
            }
            (_, Verbosity::Summary) => {
                // All blocks: Summary if available, else Full
                blocks.iter().map(|b| {
                    if b.summary.is_some() {
                        RenderDecision::Summary
                    } else {
                        RenderDecision::Full
                    }
                }).collect()
            }
            _ => {
                // Full verbosity or no budget: render everything
                blocks.iter().map(|_| RenderDecision::Full).collect()
            }
        };

        // Render each block according to its decision
        // ...
    }
}
```

---

## File Structure

Changes to existing crate:

```
crates/lcp-driver/src/
├── budget.rs       # Full implementation (was stub)
│   ├── TokenEstimator trait
│   ├── HeuristicEstimator
│   ├── CodeAwareEstimator
│   ├── RenderDecision enum
│   ├── allocate_budget()
│   └── resolve_priorities()
├── driver.rs       # Updated to use budget engine
└── placeholder.rs  # Placeholder rendering (new file)
```

---

## Acceptance Criteria

- [ ] With no budget set, all blocks render with full content
- [ ] With Verbosity::Full, budget is ignored and all blocks render fully
- [ ] With Verbosity::Summary, all blocks with summaries render as summaries
- [ ] CRITICAL blocks always render full content even when over budget
- [ ] HIGH blocks prefer full content but fall back to summary under pressure
- [ ] LOW blocks are summarized before NORMAL blocks
- [ ] BACKGROUND blocks are omitted first (before summarizing anything)
- [ ] Blocks without summaries get metadata placeholders when over budget
- [ ] Placeholder format matches the expected output for each rendering mode
- [ ] `HeuristicEstimator` produces reasonable estimates (within 2x of actual for English text)
- [ ] Total rendered token count does not exceed budget by more than one CRITICAL block's worth
- [ ] Priority resolution correctly reads ANNOTATION blocks
- [ ] Blocks without priority annotations default to Normal

---

## Verification

```bash
cargo test -p lcp-driver -- budget
cargo clippy -p lcp-driver -- -W clippy::pedantic

# Budget allocation test: 5 blocks with mixed priorities, tight budget
cargo test -p lcp-driver -- budget_allocation --nocapture
```
