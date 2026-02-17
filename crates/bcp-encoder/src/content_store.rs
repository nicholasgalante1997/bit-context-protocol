use std::collections::HashMap;
use std::sync::RwLock;

use bcp_types::content_store::ContentStore;

/// In-memory content store backed by a `HashMap`.
///
/// Suitable for the PoC and testing. Not persisted across runs.
/// Uses [`RwLock`] for interior mutability so that the
/// [`ContentStore`] trait methods (which take `&self`) can mutate
/// the internal map safely across threads.
///
/// # Concurrency
///
/// Read operations (`get`, `contains`) acquire a read lock.
/// Write operations (`put`) acquire a write lock. Multiple
/// concurrent readers are allowed; writers are exclusive.
///
/// # Example
///
/// ```rust
/// use bcp_encoder::MemoryContentStore;
/// use bcp_types::ContentStore;
///
/// let store = MemoryContentStore::new();
/// let data = b"fn main() {}";
/// let hash = store.put(data);
/// assert_eq!(store.get(&hash).unwrap(), data);
/// assert!(store.contains(&hash));
/// assert_eq!(store.len(), 1);
/// ```
pub struct MemoryContentStore {
    store: RwLock<HashMap<[u8; 32], Vec<u8>>>,
}

impl MemoryContentStore {
    /// Create an empty in-memory content store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
        }
    }

    /// Return the number of unique entries in the store.
    #[must_use]
    pub fn len(&self) -> usize {
        self.store
            .read()
            .expect("content store lock poisoned")
            .len()
    }

    /// Return `true` if the store contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the total bytes stored across all entries.
    ///
    /// This counts only the content bytes, not the 32-byte hash
    /// keys or `HashMap` overhead.
    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.store
            .read()
            .expect("content store lock poisoned")
            .values()
            .map(Vec::len)
            .sum()
    }
}

impl Default for MemoryContentStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentStore for MemoryContentStore {
    fn get(&self, hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.store
            .read()
            .expect("content store lock poisoned")
            .get(hash)
            .cloned()
    }

    fn put(&self, content: &[u8]) -> [u8; 32] {
        let hash: [u8; 32] = blake3::hash(content).into();
        let mut store = self.store.write().expect("content store lock poisoned");
        store.entry(hash).or_insert_with(|| content.to_vec());
        hash
    }

    fn contains(&self, hash: &[u8; 32]) -> bool {
        self.store
            .read()
            .expect("content store lock poisoned")
            .contains_key(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_roundtrip() {
        let store = MemoryContentStore::new();
        let data = b"fn main() { println!(\"hello\"); }";
        let hash = store.put(data);
        let retrieved = store.get(&hash).expect("should find stored content");
        assert_eq!(retrieved, data);
    }

    #[test]
    fn put_returns_deterministic_hash() {
        let store = MemoryContentStore::new();
        let data = b"deterministic content";
        let hash1 = store.put(data);
        let hash2 = store.put(data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn dedup_stores_only_once() {
        let store = MemoryContentStore::new();
        let data = b"duplicate content";
        store.put(data);
        store.put(data);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn contains_returns_true_for_stored_hash() {
        let store = MemoryContentStore::new();
        let data = b"some content";
        let hash = store.put(data);
        assert!(store.contains(&hash));
    }

    #[test]
    fn contains_returns_false_for_unknown_hash() {
        let store = MemoryContentStore::new();
        let fake_hash = [0u8; 32];
        assert!(!store.contains(&fake_hash));
    }

    #[test]
    fn get_returns_none_for_unknown_hash() {
        let store = MemoryContentStore::new();
        let fake_hash = [0xFF; 32];
        assert!(store.get(&fake_hash).is_none());
    }

    #[test]
    fn len_and_total_bytes() {
        let store = MemoryContentStore::new();
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());
        assert_eq!(store.total_bytes(), 0);

        store.put(b"hello"); // 5 bytes
        store.put(b"world"); // 5 bytes
        assert_eq!(store.len(), 2);
        assert!(!store.is_empty());
        assert_eq!(store.total_bytes(), 10);
    }

    #[test]
    fn blake3_hash_is_32_bytes() {
        let store = MemoryContentStore::new();
        let hash = store.put(b"test");
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn different_content_produces_different_hashes() {
        let store = MemoryContentStore::new();
        let hash1 = store.put(b"content A");
        let hash2 = store.put(b"content B");
        assert_ne!(hash1, hash2);
    }
}
