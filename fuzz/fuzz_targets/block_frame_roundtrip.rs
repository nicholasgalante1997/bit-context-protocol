#![no_main]

use libfuzzer_sys::fuzz_target;
use bcp_wire::block_frame::{BlockFlags, BlockFrame, block_type};

// Fuzz target: BlockFrame write->read roundtrip.
//
// Input format:
//   byte 0: block_type
//   byte 1: flags
//   bytes 2..: body
//
// Constructs a BlockFrame, serializes it, deserializes it, and asserts
// the output matches the input.
fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }

    let bt = data[0];
    if bt == block_type::END {
        return;
    }

    let flags = BlockFlags::from_raw(data[1]);
    let body = data[2..].to_vec();

    let frame = BlockFrame {
        block_type: bt,
        flags,
        body,
    };

    let mut wire = Vec::new();
    frame.write_to(&mut wire).unwrap();

    let (parsed, consumed) = BlockFrame::read_from(&wire).unwrap().unwrap();
    assert_eq!(parsed.block_type, frame.block_type);
    assert_eq!(parsed.flags, frame.flags);
    assert_eq!(parsed.body, frame.body);
    assert_eq!(consumed, wire.len());
});
