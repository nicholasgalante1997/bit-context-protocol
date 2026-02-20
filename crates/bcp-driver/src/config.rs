use bcp_types::BlockType;

/// Configuration for the BCP driver.
///
/// Controls how decoded blocks are rendered into model-ready text.
/// The driver uses these settings to select the output format, apply
/// model-specific tuning, filter which block types appear in the
/// rendered output, and manage token budget constraints.
///
/// ```text
/// ┌────────────────┬────────────────────────────────────────────────────┐
/// │ Field          │ Purpose                                            │
/// ├────────────────┼────────────────────────────────────────────────────┤
/// │ mode           │ Selects XML, Markdown, or Minimal output format    │
/// │ target_model   │ Hints for model-specific formatting adjustments    │
/// │ include_types  │ Optional allowlist — only render matching blocks   │
/// │ token_budget   │ Approximate token limit for rendered output        │
/// │ verbosity      │ Full / Summary / Adaptive rendering mode           │
/// └────────────────┴────────────────────────────────────────────────────┘
/// ```
///
/// When `include_types` is `None`, all block types are rendered (except
/// `Annotation`, which is metadata-only and never produces visible output).
/// When `Some(vec)`, only blocks whose `BlockType` is in the list are
/// rendered; all others are silently skipped.
///
/// When `token_budget` is `Some(n)`, the driver uses a two-pass algorithm
/// (RFC §5.5) to fit blocks within the budget: high-priority blocks get
/// full content, while lower-priority blocks degrade to summaries or
/// placeholders. When `None`, all blocks render with full content.
pub struct DriverConfig {
    /// Output format mode. Determines the textual structure of the
    /// rendered output.
    pub mode: OutputMode,

    /// Model family hint. Affects minor formatting choices (e.g., XML
    /// attribute ordering for Claude compatibility, markdown header
    /// depth for GPT).
    pub target_model: Option<ModelFamily>,

    /// Block type filter. When set, only blocks of these types are
    /// rendered; all others are silently skipped.
    pub include_types: Option<Vec<BlockType>>,

    /// Approximate token budget. When set, the driver uses summaries
    /// and placeholders for low-priority blocks to fit within this
    /// limit. When `None`, all blocks render with full content.
    pub token_budget: Option<u32>,

    /// Verbosity mode for budget-aware rendering. Controls whether the
    /// driver renders full content, summaries, or auto-selects per
    /// block based on budget and priority.
    pub verbosity: Verbosity,
}

impl Default for DriverConfig {
    /// Default configuration: XML mode, no model hint, no type filter,
    /// no token budget, adaptive verbosity.
    ///
    /// XML mode is the default because it produces the most semantically
    /// structured output — Claude-family models parse it natively, and
    /// other models handle it well too. Adaptive verbosity means the
    /// driver will use the budget engine when a `token_budget` is set,
    /// and render everything in full otherwise.
    fn default() -> Self {
        Self {
            mode: OutputMode::Xml,
            target_model: None,
            include_types: None,
            token_budget: None,
            verbosity: Verbosity::default(),
        }
    }
}

/// Verbosity modes for budget-aware rendering (RFC §5.5).
///
/// Controls how the driver decides between full content and summary
/// rendering for each block. This interacts with `token_budget`:
///
/// ```text
/// ┌──────────┬────────────────────────────────────────────────────────┐
/// │ Mode     │ Behavior                                               │
/// ├──────────┼────────────────────────────────────────────────────────┤
/// │ Full     │ Always render full content. Ignore budget entirely.    │
/// │ Summary  │ Always render summaries where available, full content  │
/// │          │ otherwise. Budget is ignored.                          │
/// │ Adaptive │ Auto-select per block based on budget + priority.      │
/// │          │ Without a budget set, behaves like Full.               │
/// └──────────┴────────────────────────────────────────────────────────┘
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Verbosity {
    /// Always render full content, ignoring any token budget.
    Full,
    /// Always render summaries where available. Blocks without
    /// summaries fall back to full content. Budget is ignored.
    Summary,
    /// Auto-select per block based on budget and priority. This is the
    /// default mode. Without a `token_budget`, behaves like `Full`.
    #[default]
    Adaptive,
}

/// Output format modes per RFC §5.4.
///
/// Each mode represents a different tradeoff between semantic structure,
/// model compatibility, and token efficiency. The driver dispatches to
/// a different renderer based on this setting.
///
/// ```text
/// ┌──────────┬─────────────────────────────────────────────────────────┐
/// │ Mode     │ Description                                             │
/// ├──────────┼─────────────────────────────────────────────────────────┤
/// │ Xml      │ <code lang="rust" path="...">content</code>            │
/// │          │ Optimized for Claude-family models. Wraps all output    │
/// │          │ in <context>...</context>.                              │
/// ├──────────┼─────────────────────────────────────────────────────────┤
/// │ Markdown │ ```rust\n// src/main.rs\ncontent\n```                   │
/// │          │ Compatible with all models, more tokens.                │
/// ├──────────┼─────────────────────────────────────────────────────────┤
/// │ Minimal  │ --- src/main.rs [rust] ---\ncontent                     │
/// │          │ Maximum token efficiency, fewest structural tokens.     │
/// └──────────┴─────────────────────────────────────────────────────────┘
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Xml,
    Markdown,
    Minimal,
}

/// Model family hints for output tuning.
///
/// The driver may adjust minor formatting choices based on the target
/// model. For example, Claude models have strong XML comprehension, so
/// the XML renderer can use Claude-optimized attribute ordering. GPT
/// models handle markdown well, so the markdown renderer can use
/// GPT-friendly header conventions.
///
/// This is a hint, not a hard requirement — the output is valid regardless
/// of the target model setting.
///
/// ```text
/// ┌─────────┬────────────────────────────────────────────────────┐
/// │ Family  │ Notes                                              │
/// ├─────────┼────────────────────────────────────────────────────┤
/// │ Claude  │ Strong XML parsing, prefers semantic tags          │
/// │ Gpt     │ Strong markdown parsing, prefers fenced blocks     │
/// │ Gemini  │ Handles both XML and markdown well                 │
/// │ Generic │ No model-specific tuning applied                   │
/// └─────────┴────────────────────────────────────────────────────┘
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelFamily {
    Claude,
    Gpt,
    Gemini,
    Generic,
}
