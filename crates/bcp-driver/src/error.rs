use bcp_types::BlockType;

/// Errors that can occur during block rendering.
///
/// The driver validates its input before rendering. These errors represent
/// conditions that prevent a successful render — empty input, unsupported
/// block types, or invalid content within blocks.
///
/// ```text
/// ┌──────────────────────┬──────────────────────────────────────────────┐
/// │ Variant              │ Cause                                        │
/// ├──────────────────────┼──────────────────────────────────────────────┤
/// │ EmptyInput           │ No blocks provided to render                 │
/// │ UnsupportedBlockType │ Block type cannot be rendered in this mode   │
/// │ InvalidContent       │ Block body contains invalid UTF-8            │
/// └──────────────────────┴──────────────────────────────────────────────┘
/// ```
#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("no blocks to render")]
    EmptyInput,

    #[error("unsupported block type for rendering: {block_type:?}")]
    UnsupportedBlockType { block_type: BlockType },

    #[error("invalid UTF-8 in block content at index {block_index}")]
    InvalidContent { block_index: usize },
}
