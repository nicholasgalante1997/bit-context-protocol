use bcp_types::error::TypeError;
use bcp_wire::WireError;

/// Errors that can occur during LCP payload decoding.
///
/// The decoder validates at multiple levels: header integrity, block
/// frame structure, TLV body fields, stream termination, decompression,
/// and content store resolution. Each error variant captures enough
/// context for meaningful diagnostics.
///
/// Error hierarchy:
///
/// ```text
///   DecodeError
///   ├── InvalidHeader(WireError)   ← magic, version, or reserved byte wrong
///   ├── BlockTooLarge              ← single block body exceeds size limit
///   ├── MissingField               ← required TLV field absent in block body
///   ├── InvalidUtf8                ← string field contains non-UTF-8 bytes
///   ├── MissingEndSentinel         ← payload ran out without END block
///   ├── TrailingData               ← extra bytes after END sentinel
///   ├── DecompressFailed           ← zstd decompression error
///   ├── DecompressionBomb          ← decompressed size exceeds safety limit
///   ├── UnresolvedReference        ← BLAKE3 hash not found in content store
///   ├── MissingContentStore        ← IS_REFERENCE block but no store provided
///   ├── Type(TypeError)            ← from bcp-types body deserialization
///   ├── Wire(WireError)            ← from bcp-wire frame parsing
///   └── Io(std::io::Error)         ← from underlying I/O reads
/// ```
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// The 8-byte file header failed validation.
    ///
    /// This wraps a [`WireError`] from `LcpHeader::read_from` — the
    /// inner error distinguishes between bad magic, unsupported version,
    /// and non-zero reserved byte.
    #[error("invalid header: {0}")]
    InvalidHeader(WireError),

    /// A block body exceeds the maximum allowed size.
    #[error("block body too large: {size} bytes at offset {offset}")]
    BlockTooLarge { size: usize, offset: usize },

    /// A required field was missing from a known block type's body.
    ///
    /// This provides richer context than the underlying
    /// [`TypeError::MissingRequiredField`] by including the block type name
    /// and the field's wire ID.
    #[error("required field {field_name} (id={field_id}) missing in {block_type} block")]
    MissingField {
        block_type: &'static str,
        field_name: &'static str,
        field_id: u64,
    },

    /// A string field contained invalid UTF-8 bytes.
    #[error("invalid UTF-8 in field {field_name} of {block_type} block")]
    InvalidUtf8 {
        block_type: &'static str,
        field_name: &'static str,
    },

    /// The payload ended without an END sentinel block (type=0xFF).
    ///
    /// Every valid LCP payload must terminate with an END block. If the
    /// byte stream is exhausted before encountering one, the payload is
    /// considered truncated.
    #[error("payload does not end with END sentinel")]
    MissingEndSentinel,

    /// Extra bytes were found after the END sentinel.
    ///
    /// Per the spec, this is a warning-level condition — the payload
    /// decoded successfully, but the trailing data may indicate
    /// corruption or a buggy encoder. The decoder captures this as an
    /// error variant so callers can decide how to handle it.
    #[error("unexpected data after END sentinel ({extra_bytes} bytes)")]
    TrailingData { extra_bytes: usize },

    /// Zstd decompression failed.
    ///
    /// Returned when a block's `COMPRESSED` flag (bit 1) or the header's
    /// `COMPRESSED` flag (bit 0) is set and the zstd decoder cannot parse
    /// the compressed data. Common causes: truncated input, corrupt frame,
    /// or non-zstd data with the flag erroneously set.
    #[error("zstd decompression failed: {0}")]
    DecompressFailed(String),

    /// Decompressed data exceeds the safety limit.
    ///
    /// Prevents decompression bombs — payloads crafted to decompress into
    /// vastly larger output. The `limit` is the caller-configured maximum
    /// (default: 16 MiB per block, 256 MiB for whole-payload).
    #[error("decompressed size {actual} exceeds limit {limit}")]
    DecompressionBomb { actual: usize, limit: usize },

    /// A block has the `IS_REFERENCE` flag set but its 32-byte BLAKE3
    /// hash was not found in the content store.
    ///
    /// This means the content was previously content-addressed during
    /// encoding but the corresponding data was not provided to the
    /// decoder's content store.
    #[error("unresolved reference: BLAKE3 hash not found in content store")]
    UnresolvedReference { hash: [u8; 32] },

    /// A block has the `IS_REFERENCE` flag but no content store was
    /// provided to the decoder.
    ///
    /// Use [`LcpDecoder::decode_with_store`] instead of
    /// [`LcpDecoder::decode`] when decoding payloads that contain
    /// content-addressed blocks.
    #[error("block has IS_REFERENCE flag but no content store was provided")]
    MissingContentStore,

    /// A body deserialization error from `bcp-types`.
    ///
    /// This covers missing required fields, unknown wire types, and
    /// invalid enum values encountered while parsing TLV fields within
    /// a block body.
    #[error(transparent)]
    Type(#[from] TypeError),

    /// A wire-level framing error from `bcp-wire`.
    ///
    /// Surfaces when a block frame's varint is malformed, the body
    /// length exceeds the remaining bytes, or other structural issues.
    #[error(transparent)]
    Wire(#[from] WireError),

    /// An I/O error from the underlying reader (streaming decoder).
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
