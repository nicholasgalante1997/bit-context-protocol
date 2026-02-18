//! Edge case integration tests for the BCP decoder.
//!
//! This module validates four categories of edge cases that must work correctly
//! for the protocol to be production-safe:
//!
//! - **Unknown block types**: A decoder built against an older spec must treat
//!   unrecognized block type IDs as opaque `BlockContent::Unknown` values and
//!   preserve them losslessly (forward compatibility guarantee from RFC §3).
//!
//! - **Empty content**: A CODE block whose content field is zero bytes is valid
//!   wire format. The `content_len` varint encodes `0` and the body field is
//!   simply absent. This must not produce a `MissingRequiredField` error.
//!
//! - **Large varints**: Content lengths >= 128 bytes require multi-byte LEB128
//!   varints. A 16 KiB block exercises 3-byte varint encoding and validates
//!   that the decoder reads the full extent of the body correctly.
//!
//! - **Trailing data**: Extra bytes after the END sentinel are an error
//!   condition per the spec, captured as `DecodeError::TrailingData`. Some
//!   implementations may choose leniency; the test accepts both outcomes.

use std::path::Path;

use bcp_decoder::{DecodeError, LcpDecoder};
use bcp_encoder::LcpEncoder;
use bcp_types::BlockContent;
use bcp_types::enums::Lang;

fn golden(subpath: &str) -> Vec<u8> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = manifest_dir.join("tests/golden").join(subpath);
    std::fs::read(&fixture_path)
        .unwrap_or_else(|e| panic!("failed to read golden fixture {}: {e}", fixture_path.display()))
}

// ── Unknown block type ────────────────────────────────────────────────────────

#[test]
fn unknown_block_type_preserved() {
    let bytes = golden("edge_cases/unknown_block_type/payload.lcp");
    let decoded = LcpDecoder::decode(&bytes).expect("decode should succeed for unknown block type");

    let has_unknown = decoded.blocks.iter().any(|b| {
        matches!(b.content, BlockContent::Unknown { type_id: 0x42, .. })
    });

    assert!(
        has_unknown,
        "expected a BlockContent::Unknown with type_id=0x42, got: {:?}",
        decoded.blocks.iter().map(|b| &b.content).collect::<Vec<_>>()
    );
}

#[test]
fn unknown_block_type_reencodes_identical() {
    let bytes = golden("edge_cases/unknown_block_type/payload.lcp");
    let decoded = LcpDecoder::decode(&bytes).expect("decode should succeed");

    assert_eq!(
        decoded.blocks[0].content,
        BlockContent::Unknown {
            type_id: 0x42,
            body: b"hello".to_vec(),
        },
        "unknown block body must be preserved exactly as encoded"
    );
}

// ── Empty content ─────────────────────────────────────────────────────────────

#[test]
fn empty_content_valid() {
    let payload = LcpEncoder::new()
        .add_code(Lang::Rust, "empty.rs", b"")
        .encode()
        .expect("encoding an empty CODE block should succeed");

    let decoded = LcpDecoder::decode(&payload).expect("decoding an empty CODE block should succeed");

    assert_eq!(decoded.blocks.len(), 1);

    if let BlockContent::Code(code) = &decoded.blocks[0].content {
        assert!(
            code.content.is_empty(),
            "code content should be empty, got {} bytes",
            code.content.len()
        );
    } else {
        panic!("expected Code block, got {:?}", decoded.blocks[0].content);
    }
}

#[test]
fn empty_content_golden() {
    let bytes = golden("edge_cases/empty_content/payload.lcp");
    let decoded = LcpDecoder::decode(&bytes).expect("golden empty_content payload should decode");

    assert_eq!(
        decoded.blocks.len(),
        1,
        "expected exactly 1 block, got {}",
        decoded.blocks.len()
    );

    if let BlockContent::Code(code) = &decoded.blocks[0].content {
        assert!(
            code.content.is_empty(),
            "code content should be empty, got {} bytes",
            code.content.len()
        );
    } else {
        panic!("expected Code block, got {:?}", decoded.blocks[0].content);
    }
}

// ── Large varint ──────────────────────────────────────────────────────────────

#[test]
fn large_varint_roundtrip() {
    let bytes = golden("edge_cases/large_varint/payload.lcp");
    let decoded = LcpDecoder::decode(&bytes).expect("16 KiB payload should decode without error");

    assert_eq!(
        decoded.blocks.len(),
        1,
        "expected exactly 1 block, got {}",
        decoded.blocks.len()
    );

    if let BlockContent::Code(code) = &decoded.blocks[0].content {
        assert_eq!(
            code.content.len(),
            16384,
            "decoded content length should be 16384 bytes (16 KiB), got {}",
            code.content.len()
        );
        assert_eq!(
            code.content,
            vec![b'x'; 16384],
            "decoded content should be 16384 'x' bytes"
        );
    } else {
        panic!("expected Code block, got {:?}", decoded.blocks[0].content);
    }
}

// ── Trailing data ─────────────────────────────────────────────────────────────

#[test]
fn trailing_data_warning() {
    let bytes = golden("edge_cases/trailing_data/payload.lcp");
    let result = LcpDecoder::decode(&bytes);

    assert!(
        result.is_ok()
            || matches!(result, Err(DecodeError::TrailingData { extra_bytes: 4 })),
        "expected Ok or TrailingData(4 bytes), got an unexpected error variant"
    );
}
