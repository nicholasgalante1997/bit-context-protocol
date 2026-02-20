#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz target: BlockFrame::read_from frame parsing.
//
// Catches bugs in:
// - Varint overflow in block_type/content_len
// - Truncated frames
// - Oversized body lengths
// - Flags parsing
fuzz_target!(|data: &[u8]| {
    let _ = bcp_wire::block_frame::BlockFrame::read_from(data);
});
