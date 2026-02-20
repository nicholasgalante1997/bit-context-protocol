#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz target: decode_varint LEB128 codec.
//
// Catches bugs in:
// - VarintTooLong (>10 continuation bytes)
// - Zero-length input
// - Maximum value edge cases (u64::MAX)
// - Malformed continuation bits
fuzz_target!(|data: &[u8]| {
    let _ = bcp_wire::varint::decode_varint(data);
});
