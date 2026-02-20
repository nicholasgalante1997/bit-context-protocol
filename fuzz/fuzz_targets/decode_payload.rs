#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz target: Full BCP decoder entry point.
//
// Calls `BcpDecoder::decode(data)` on arbitrary input bytes.
// Catches bugs in:
// - Header validation (magic, version, reserved byte)
// - Block frame iteration (varint parsing, flags, body lengths)
// - Summary extraction
// - Per-block body deserialization (TLV parsing)
// - Whole-payload decompression
// - END sentinel handling
// - Trailing data detection
fuzz_target!(|data: &[u8]| {
    let _ = bcp_decoder::BcpDecoder::decode(data);
});
