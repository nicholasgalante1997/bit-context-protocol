#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz target: BlockContent::decode_body per-block-type body deserialization.
//
// Input format:
//   - First byte: block_type (0x01-0xFE, or 0xFF for End)
//   - Remaining bytes: body to pass to decode_body
//
// Catches bugs in:
// - TLV field parsing for each block type
// - Missing required fields
// - Invalid enum values
// - UTF-8 validation
// - Type-specific decoding edge cases
fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let block_type_wire = data[0];
    let body = &data[1..];

    let block_type = match block_type_wire {
        0x01 => bcp_types::block_type::BlockType::Code,
        0x02 => bcp_types::block_type::BlockType::Conversation,
        0x03 => bcp_types::block_type::BlockType::FileTree,
        0x04 => bcp_types::block_type::BlockType::ToolResult,
        0x05 => bcp_types::block_type::BlockType::Document,
        0x06 => bcp_types::block_type::BlockType::StructuredData,
        0x07 => bcp_types::block_type::BlockType::Diff,
        0x08 => bcp_types::block_type::BlockType::Annotation,
        0x09 => bcp_types::block_type::BlockType::EmbeddingRef,
        0x0A => bcp_types::block_type::BlockType::Image,
        0xFE => bcp_types::block_type::BlockType::Extension,
        0xFF => bcp_types::block_type::BlockType::End,
        _ => bcp_types::block_type::BlockType::Unknown(block_type_wire),
    };

    let _ = bcp_types::block::BlockContent::decode_body(&block_type, body);
});
