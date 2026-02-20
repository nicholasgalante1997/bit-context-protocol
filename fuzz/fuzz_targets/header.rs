#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz target: BcpHeader::read_from with arbitrary bytes.
//
// Catches bugs in:
// - Magic byte validation
// - Version checking
// - Reserved byte enforcement
// - Truncated header handling
fuzz_target!(|data: &[u8]| {
    let _ = bcp_wire::header::BcpHeader::read_from(data);
});
