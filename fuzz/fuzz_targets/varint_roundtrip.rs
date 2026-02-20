#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz target: varint encode->decode roundtrip.
//
// Takes 8 bytes of fuzz input, interprets as a u64, encodes it as a
// LEB128 varint, then decodes it and asserts the value matches.
fuzz_target!(|data: &[u8]| {
    if data.len() < 8 {
        return;
    }
    let value = u64::from_le_bytes(data[..8].try_into().unwrap());

    let mut buf = [0u8; 10];
    let encoded_len = bcp_wire::varint::encode_varint(value, &mut buf);

    let (decoded, decoded_len) = bcp_wire::varint::decode_varint(&buf[..encoded_len]).unwrap();
    assert_eq!(decoded, value);
    assert_eq!(decoded_len, encoded_len);
});
