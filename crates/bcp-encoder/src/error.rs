use bcp_wire::WireError;

/// Errors specific to zstd compression and decompression.
///
/// These are surfaced when per-block or whole-payload compression
/// is enabled and the zstd codec encounters an issue, or when
/// decompressed output exceeds safety limits.
///
/// ```text
///   CompressionError
///   ├── CompressFailed      ← zstd encoder returned an error
///   ├── DecompressFailed    ← zstd decoder returned an error
///   └── DecompressionBomb   ← decompressed size exceeds safety limit
/// ```
#[derive(Debug, thiserror::Error)]
pub enum CompressionError {
    #[error("zstd compression failed: {0}")]
    CompressFailed(String),

    #[error("zstd decompression failed: {0}")]
    DecompressFailed(String),

    #[error("decompressed size {actual} exceeds limit {limit}")]
    DecompressionBomb { actual: usize, limit: usize },
}

/// Errors that can occur during LCP payload encoding.
///
/// The encoder validates structural constraints (non-empty payload,
/// block size limits, summary targeting) and propagates lower-level
/// wire, I/O, and compression errors from the serialization layer.
///
/// Error hierarchy:
///
/// ```text
///   EncodeError
///   ├── EmptyPayload         ← no blocks were added before .encode()
///   ├── BlockTooLarge        ← single block body exceeds size limit
///   ├── InvalidSummaryTarget ← with_summary called with no preceding block
///   ├── MissingContentStore  ← content addressing enabled without a store
///   ├── Compression(…)       ← from zstd compress/decompress
///   ├── Wire(WireError)      ← from bcp-wire serialization
///   └── Io(std::io::Error)   ← from underlying I/O writes
/// ```
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    #[error("no blocks have been added to the encoder")]
    EmptyPayload,

    #[error("block body exceeds maximum size ({size} bytes, limit {limit})")]
    BlockTooLarge { size: usize, limit: usize },

    #[error("with_summary called but no blocks have been added yet")]
    InvalidSummaryTarget,

    #[error("content addressing requires a content store (call set_content_store first)")]
    MissingContentStore,

    #[error(transparent)]
    Compression(#[from] CompressionError),

    #[error(transparent)]
    Wire(#[from] WireError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
