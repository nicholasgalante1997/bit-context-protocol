#![no_main]

use libfuzzer_sys::fuzz_target;
use bcp_wire::header::{BcpHeader, HeaderFlags, HEADER_SIZE};

// Fuzz target: BcpHeader write->read roundtrip.
//
// Takes 1 byte of fuzz input as header flags, constructs a header,
// serializes it, deserializes it, and asserts the output matches.
fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // Only use the 2 defined flag bits to create a valid header
    let flags_byte = data[0] & 0b0000_0011;
    let header = BcpHeader::new(HeaderFlags::from_raw(flags_byte));

    let mut buf = [0u8; HEADER_SIZE];
    header.write_to(&mut buf).unwrap();

    let parsed = BcpHeader::read_from(&buf).unwrap();
    assert_eq!(parsed.version_major, header.version_major);
    assert_eq!(parsed.version_minor, header.version_minor);
    assert_eq!(parsed.flags, header.flags);
});
