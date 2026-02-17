/// Size of a BLAKE3 hash in bytes — the body of a reference block.
///
/// When a block's `IS_REFERENCE` flag (bit 2 of `block_flags`) is set,
/// the body contains exactly this many bytes: the BLAKE3 hash of the
/// actual content. The decoder resolves the hash against a
/// [`ContentStore`] to retrieve the original body.
///
/// ```text
/// ┌──────────────────────────────────────┐
/// │ block_type  (varint)                 │
/// │ block_flags (uint8, bit 2 set)       │
/// │ content_len (varint, always 32)      │
/// │ blake3_hash [32 bytes]               │
/// └──────────────────────────────────────┘
/// ```
pub const REFERENCE_BODY_SIZE: usize = 32;

/// Content store for resolving hash-referenced block bodies.
///
/// The store maps BLAKE3 hashes (32 bytes) to raw block body bytes.
/// Implementations can be in-memory, file-backed, or networked.
///
/// All methods take `&self` — implementations that need interior
/// mutability (e.g. `MemoryContentStore` in `bcp-encoder`) use
/// synchronization primitives like [`std::sync::RwLock`].
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` to support concurrent
/// encoding/decoding in multi-threaded runtimes.
///
/// # Wire Format
///
/// When a block is content-addressed, its on-wire body is replaced
/// with the 32-byte BLAKE3 hash of the original body. The block's
/// `IS_REFERENCE` flag (bit 2) signals this substitution. At decode
/// time, the hash is looked up in the content store to retrieve the
/// original bytes, which are then parsed normally.
///
/// ```text
/// Encode path:
///   body bytes ──▶ BLAKE3 hash ──▶ store.put(body) ──▶ write hash as body
///
/// Decode path:
///   read 32-byte hash ──▶ store.get(hash) ──▶ original body ──▶ decode_body()
/// ```
pub trait ContentStore: Send + Sync {
    /// Retrieve content by its BLAKE3 hash.
    ///
    /// Returns `None` if the hash is not found in the store.
    fn get(&self, hash: &[u8; 32]) -> Option<Vec<u8>>;

    /// Store content and return its BLAKE3 hash.
    ///
    /// If the content already exists (same hash), this is a no-op
    /// and the existing hash is returned. The store deduplicates
    /// automatically.
    fn put(&self, content: &[u8]) -> [u8; 32];

    /// Check whether a hash exists in the store without retrieving
    /// the content.
    fn contains(&self, hash: &[u8; 32]) -> bool;
}
