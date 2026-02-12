# SPEC_07 — Content Addressing (BLAKE3)

**Crate**: `lcp-encoder`, `lcp-decoder` (modifications)
**Phase**: 3 (Advanced Features)
**Prerequisites**: SPEC_01, SPEC_02, SPEC_03, SPEC_04
**Dependencies**: `lcp-wire`, `lcp-types`, `blake3`

---

## Context

LCP supports content-addressed blocks per RFC §4.7. When a block's
`IS_REFERENCE` flag (bit 2 of `block_flags`) is set, the body contains a
32-byte BLAKE3 hash instead of inline content. This enables deduplication:
if the same file appears in multiple tool results, it is stored once and
referenced by hash. The decoder resolves references against a content store
at decode time.

For the PoC, the content store is an in-memory `HashMap<[u8; 32], Vec<u8>>`.
A production implementation would use a persistent store (disk or remote).

---

## Requirements

### 1. Content Store Trait

```rust
/// Content store for resolving hash-referenced block bodies.
///
/// The store maps BLAKE3 hashes (32 bytes) to raw block body bytes.
/// Implementations can be in-memory, file-backed, or networked.
pub trait ContentStore: Send + Sync {
    /// Retrieve content by its BLAKE3 hash.
    ///
    /// Returns None if the hash is not found in the store.
    fn get(&self, hash: &[u8; 32]) -> Option<Vec<u8>>;

    /// Store content and return its BLAKE3 hash.
    ///
    /// If the content already exists (same hash), this is a no-op.
    fn put(&self, content: &[u8]) -> [u8; 32];

    /// Check whether a hash exists in the store.
    fn contains(&self, hash: &[u8; 32]) -> bool;
}
```

### 2. In-Memory Content Store

```rust
use std::collections::HashMap;
use std::sync::RwLock;

/// In-memory content store backed by a HashMap.
///
/// Suitable for the PoC and testing. Not persisted across runs.
pub struct MemoryContentStore {
    store: RwLock<HashMap<[u8; 32], Vec<u8>>>,
}

impl MemoryContentStore {
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
        }
    }

    /// Return the number of unique entries in the store.
    pub fn len(&self) -> usize { /* ... */ }

    /// Return the total bytes stored.
    pub fn total_bytes(&self) -> usize { /* ... */ }
}

impl ContentStore for MemoryContentStore {
    fn get(&self, hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.store.read().ok()?.get(hash).cloned()
    }

    fn put(&self, content: &[u8]) -> [u8; 32] {
        let hash: [u8; 32] = blake3::hash(content).into();
        let mut store = self.store.write().expect("content store lock poisoned");
        store.entry(hash).or_insert_with(|| content.to_vec());
        hash
    }

    fn contains(&self, hash: &[u8; 32]) -> bool {
        self.store.read().ok().map_or(false, |s| s.contains_key(hash))
    }
}
```

### 3. Encoder Integration

```rust
impl LcpEncoder {
    /// Enable content addressing for the last added block.
    ///
    /// The block body will be stored in the content store and
    /// replaced with a 32-byte BLAKE3 hash in the payload. The
    /// IS_REFERENCE flag (bit 2) will be set in block_flags.
    ///
    /// Requires a content store to be configured on the encoder.
    pub fn with_content_addressing(&mut self) -> &mut Self { /* ... */ }

    /// Set the content store for this encoder.
    ///
    /// When content addressing is enabled on a block, its body
    /// bytes are stored here and replaced with the hash.
    pub fn set_content_store(
        &mut self,
        store: Arc<dyn ContentStore>,
    ) -> &mut Self { /* ... */ }

    /// Enable automatic deduplication: any block body that already
    /// exists in the content store is automatically replaced with
    /// a reference.
    pub fn auto_dedup(&mut self) -> &mut Self { /* ... */ }
}
```

### 4. Reference Block Wire Format

```rust
/// When a block's IS_REFERENCE flag is set, the body contains
/// exactly 32 bytes: the BLAKE3 hash of the actual content.
///
/// Wire layout of a reference block:
///   ┌──────────────────────────────────────┐
///   │ block_type  (varint)                 │
///   │ block_flags (uint8, bit 2 set)       │
///   │ content_len (varint, always 32)      │
///   │ blake3_hash [32 bytes]               │
///   └──────────────────────────────────────┘
///
/// The decoder reads the 32-byte hash and looks it up in the
/// content store to retrieve the actual body bytes, which are
/// then parsed as usual.
pub const REFERENCE_BODY_SIZE: usize = 32;
```

### 5. Decoder Integration

```rust
/// Decoder configuration extension for content addressing.
impl LcpDecoder {
    /// Decode a payload with content store for reference resolution.
    ///
    /// When a block's IS_REFERENCE flag is set, the 32-byte hash
    /// body is looked up in the content store. If found, the resolved
    /// bytes replace the hash and are parsed normally. If not found,
    /// a DecodeError::UnresolvedReference is returned.
    pub fn decode_with_store(
        payload: &[u8],
        store: &dyn ContentStore,
    ) -> Result<DecodedPayload, DecodeError> {
        // Implementation
    }
}
```

### 6. Error Types (additions)

```rust
/// Added to DecodeError:
#[error("unresolved content reference: hash {hash} not found in content store")]
UnresolvedReference { hash: String },
```

---

## File Structure

Changes to existing crates:

```
crates/lcp-encoder/src/content_store.rs  # Full implementation (was stub)
crates/lcp-decoder/src/decoder.rs        # Add decode_with_store method
```

New shared crate (optional, or inline in encoder/decoder):

```
crates/lcp-encoder/src/content_store.rs
  ├── ContentStore trait
  ├── MemoryContentStore
  └── BLAKE3 hashing utilities
```

---

## Acceptance Criteria

- [ ] `blake3::hash(content)` produces deterministic 32-byte hashes
- [ ] `MemoryContentStore::put` followed by `get` returns identical content
- [ ] `MemoryContentStore::put` with identical content returns the same hash (dedup)
- [ ] Encoder with `with_content_addressing` stores body in content store
- [ ] Encoder with `with_content_addressing` writes 32-byte hash as block body
- [ ] Encoder sets IS_REFERENCE flag (bit 2) on content-addressed blocks
- [ ] Decoder with content store resolves reference blocks to original content
- [ ] Decoder without content store returns `UnresolvedReference` for reference blocks
- [ ] Auto-dedup mode automatically references duplicate blocks
- [ ] Round-trip: encode(with refs) → decode(with store) is identical to encode(without refs) → decode

---

## Verification

```bash
cargo test -p lcp-encoder -p lcp-decoder -- content_address
cargo clippy -p lcp-encoder -p lcp-decoder -- -W clippy::pedantic

# Dedup test: encode same file twice, verify only one copy in store
cargo test -p lcp-encoder -- dedup --nocapture
```
