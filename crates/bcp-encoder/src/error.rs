use bcp_wire::WireError;

/// Errors that can occur during LCP payload encoding.
///
/// The encoder validates structural constraints (non-empty payload,
/// block size limits, summary targeting) and propagates lower-level
/// wire and I/O errors from the serialization layer.
///
/// Error hierarchy:
///
/// ```text
///   EncodeError
///   ├── EmptyPayload         ← no blocks were added before .encode()
///   ├── BlockTooLarge        ← single block body exceeds size limit
///   ├── InvalidSummaryTarget ← with_summary called with no preceding block
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

    #[error(transparent)]
    Wire(#[from] WireError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
