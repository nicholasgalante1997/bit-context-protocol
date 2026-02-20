#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz target: TLV field header + value parsing.
//
// Exercises the low-level field codec in bcp-types:
// - decode_field_header
// - decode_varint_value
// - decode_bytes_value
// - skip_field
//
// Catches bugs in:
// - Malformed field tags
// - Wire type confusion
// - Length prefix overflows
// - Truncated field values
fuzz_target!(|data: &[u8]| {
    let mut cursor = 0;
    while cursor < data.len() {
        let buf = &data[cursor..];
        let Ok((header, header_len)) = bcp_types::fields::decode_field_header(buf) else {
            break;
        };
        cursor += header_len;

        let remaining = &data[cursor..];
        let advance = match header.wire_type {
            bcp_types::fields::FieldWireType::Varint => {
                bcp_types::fields::decode_varint_value(remaining)
                    .map(|(_, len)| len)
            }
            bcp_types::fields::FieldWireType::Bytes | bcp_types::fields::FieldWireType::Nested => {
                bcp_types::fields::decode_bytes_value(remaining)
                    .map(|(_, len)| len)
            }
        };

        match advance {
            Ok(len) => cursor += len,
            Err(_) => break,
        }
    }
});
