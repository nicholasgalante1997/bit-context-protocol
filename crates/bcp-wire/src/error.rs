// TODO
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    /// Varint encoding exceeded 10 bytes without terminating.
    #[error("varint too long: exceeded 10-byte limit")]
    VarintTooLong,

    /// Input ended before a complete varint or header could be read.
    #[error("unexpected end of input at offset {offset}")]
    UnexpectedEof { offset: usize },

    /// Magic number did not match "LCP\0".
    #[error("invalid magic number: expected 0x4C435000, got {found:#010X}")]
    InvalidMagic { found: u32 },

    /// Unsupported format version.
    #[error("unsupported version {major}.{minor}")]
    UnsupportedVersion { major: u8, minor: u8 },

    /// Reserved field was non-zero.
    #[error("reserved field at offset {offset} was {value:#04X}, expected 0x00")]
    ReservedNonZero { offset: usize, value: u8 },

    /// I/O error during read or write.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

// NOTE Summary
// #[derive(thiserror::Error)] — this generates the impl std::error::Error and impl Display for you.
// Each #[error("...")] attribute becomes the Display output.
// Without thiserror, you'd be writing ~40 lines of boilerplate trait impls by hand.
// #[error(transparent)] on the Io variant —
// this means when someone prints this error,
// it delegates entirely to the inner std::io::Error's Display.
// Combined with #[from], it means any function returning Result<T, WireError> can use the ? operator
// on std::io calls and the conversion happens automatically.
// The {found:#010X} syntax — that's Rust's format string for hex.
// # adds the 0x prefix, 010 means pad to 10 characters (including the 0x),
// X is uppercase hex. So an invalid magic 0x00001234 prints as 0x00001234.
// Good for debugging binary formats.
// Why each variant carries context — UnexpectedEof { offset }
// tells you where in the byte stream things went wrong.
// This is critical when debugging binary payloads.
// The offset is the byte position from the start of the input where the read failed.
