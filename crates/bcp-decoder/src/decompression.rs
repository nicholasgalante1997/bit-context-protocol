// Zstd decompression wrapper.
//
// This module is a **Phase 2 stub**. When the encoder supports per-block
// or whole-payload zstd compression (header flag bit 0, block flag bit 1),
// this module will provide the inverse operation.
//
// When implemented, this module will provide:
//   - `decompress(data: &[u8]) -> Result<Vec<u8>, DecodeError>`
//   - Dictionary-aware decompression for common languages
