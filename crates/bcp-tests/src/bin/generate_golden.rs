//! Golden fixture generator for the BCP conformance test suite.
//!
//! This binary creates all fixture files under `tests/golden/`. Run it once
//! after making wire-format changes to regenerate the committed binary
//! payloads. Snapshot files (`.snap`) are updated separately via `cargo insta
//! review` after running the conformance tests.
//!
//! # Usage
//!
//! ```bash
//! cargo run --bin generate_golden -p bcp-tests
//! ```
//!
//! # Generated fixtures
//!
//! | Directory              | Contents                                    |
//! |------------------------|---------------------------------------------|
//! | simple_code            | Single CODE block (Rust)                    |
//! | conversation           | USER + ASSISTANT turns                      |
//! | mixed_blocks           | CODE + CONVERSATION + TOOL_RESULT + FILE_TREE |
//! | with_summaries         | CODE + TOOL_RESULT, each with a summary     |
//! | compressed_blocks      | Per-block zstd compression                  |
//! | compressed_payload     | Whole-payload zstd compression              |
//! | content_addressed      | Two identical CODE blocks deduplicated       |
//! | budget_constrained     | CRITICAL + NORMAL + BACKGROUND priorities   |
//! | all_block_types        | One of each block type (11 types)           |
//! | edge_cases/empty_content   | CODE block with empty content           |
//! | edge_cases/large_varint    | CODE block with content >= 16 KB        |
//! | edge_cases/unknown_block_type | Handcrafted: type_id=0x42 block      |
//! | edge_cases/trailing_data   | Valid payload + 4 extra bytes           |

#![allow(clippy::pedantic)]

use std::path::Path;
use std::sync::Arc;

use bcp_encoder::{BcpEncoder, MemoryContentStore};
use bcp_types::diff::DiffHunk;
use bcp_types::enums::{
    AnnotationKind, DataFormat, FormatHint, Lang, MediaType, Priority, Role, Status,
};
use bcp_types::file_tree::{FileEntry, FileEntryKind};

fn main() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let golden_dir = manifest_dir.join("tests/golden");

    generate_simple_code(&golden_dir);
    generate_conversation(&golden_dir);
    generate_mixed_blocks(&golden_dir);
    generate_with_summaries(&golden_dir);
    generate_compressed_blocks(&golden_dir);
    generate_compressed_payload(&golden_dir);
    generate_content_addressed(&golden_dir);
    generate_budget_constrained(&golden_dir);
    generate_all_block_types(&golden_dir);
    generate_edge_cases(&golden_dir);

    println!("All golden fixtures written to {}", golden_dir.display());
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn write_file(path: &Path, data: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create_dir_all");
    }
    std::fs::write(path, data).expect("write_file");
    println!("  wrote {}", path.display());
}

fn write_manifest(dir: &Path, json: &str) {
    write_file(&dir.join("manifest.json"), json.as_bytes());
}

fn payload_path(dir: &Path) -> std::path::PathBuf {
    dir.join("payload.bcp")
}

// ── Fixture generators ────────────────────────────────────────────────────────

fn generate_simple_code(golden: &Path) {
    let dir = golden.join("simple_code");
    write_manifest(
        &dir,
        r#"{
  "description": "Single CODE block containing a minimal Rust main function.",
  "blocks": [
    {
      "type": "code",
      "lang": "rust",
      "path": "src/main.rs",
      "content": "fn main() {\n    println!(\"Hello, BCP!\");\n}"
    }
  ]
}"#,
    );

    let path = "src/main.rs";
    let content = b"fn main() {\n    println!(\"Hello, BCP!\");\n}";
    let payload = BcpEncoder::new()
        .add_code(Lang::Rust, path, content)
        .encode()
        .expect("encode simple_code");
    write_file(&payload_path(&dir), &payload);
}

fn generate_conversation(golden: &Path) {
    let dir = golden.join("conversation");
    write_manifest(
        &dir,
        r#"{
  "description": "Two CONVERSATION blocks: a USER turn followed by an ASSISTANT reply.",
  "blocks": [
    { "type": "conversation", "role": "user", "content": "Fix the connection timeout bug." },
    { "type": "conversation", "role": "assistant", "content": "I'll examine the pool configuration and trace the timeout path." }
  ]
}"#,
    );

    let payload = BcpEncoder::new()
        .add_conversation(Role::User, b"Fix the connection timeout bug.")
        .add_conversation(
            Role::Assistant,
            b"I'll examine the pool configuration and trace the timeout path.",
        )
        .encode()
        .expect("encode conversation");
    write_file(&payload_path(&dir), &payload);
}

fn generate_mixed_blocks(golden: &Path) {
    let dir = golden.join("mixed_blocks");
    write_manifest(
        &dir,
        r#"{
  "description": "Four block types: CODE + CONVERSATION + TOOL_RESULT + FILE_TREE.",
  "blocks": [
    { "type": "code", "lang": "rust", "path": "src/lib.rs", "content": "pub fn add(a: u32, b: u32) -> u32 { a + b }" },
    { "type": "conversation", "role": "user", "content": "Add overflow protection." },
    { "type": "tool_result", "tool_name": "cargo_test", "status": "ok", "content": "test result: ok. 3 passed; 0 failed" },
    { "type": "file_tree", "root": "src/", "entries": [
        { "name": "lib.rs", "kind": "file", "size": 45 },
        { "name": "main.rs", "kind": "file", "size": 120 }
    ]}
  ]
}"#,
    );

    let lib_path = "src/lib.rs";
    let lib_content = b"pub fn add(a: u32, b: u32) -> u32 { a + b }";
    let lib_name = "lib.rs";
    let main_name = "main.rs";

    let payload = BcpEncoder::new()
        .add_code(Lang::Rust, lib_path, lib_content)
        .add_conversation(Role::User, b"Add overflow protection.")
        .add_tool_result(
            "cargo_test",
            Status::Ok,
            b"test result: ok. 3 passed; 0 failed",
        )
        .add_file_tree(
            "src/",
            vec![
                FileEntry {
                    name: lib_name.to_string(),
                    kind: FileEntryKind::File,
                    size: 45,
                    children: vec![],
                },
                FileEntry {
                    name: main_name.to_string(),
                    kind: FileEntryKind::File,
                    size: 120,
                    children: vec![],
                },
            ],
        )
        .encode()
        .expect("encode mixed_blocks");
    write_file(&payload_path(&dir), &payload);
}

fn generate_with_summaries(golden: &Path) {
    let dir = golden.join("with_summaries");
    write_manifest(
        &dir,
        r#"{
  "description": "CODE and TOOL_RESULT blocks, each carrying a summary sub-block.",
  "blocks": [
    {
      "type": "code", "lang": "python", "path": "server.py",
      "content": "import asyncio\n\nasync def serve():\n    pass",
      "summary": "Async HTTP server entry point."
    },
    {
      "type": "tool_result", "tool_name": "ripgrep", "status": "ok",
      "content": "src/main.rs:42: ConnectionPool::new()",
      "summary": "1 match for ConnectionPool across 1 file."
    }
  ]
}"#,
    );

    let server_path = "server.py";
    let server_content = b"import asyncio\n\nasync def serve():\n    pass";
    let rg_content = b"src/main.rs:42: ConnectionPool::new()";

    let payload = BcpEncoder::new()
        .add_code(Lang::Python, server_path, server_content)
        .with_summary("Async HTTP server entry point.").unwrap()
        .add_tool_result("ripgrep", Status::Ok, rg_content)
        .with_summary("1 match for ConnectionPool across 1 file.").unwrap()
        .encode()
        .expect("encode with_summaries");
    write_file(&payload_path(&dir), &payload);
}

fn generate_compressed_blocks(golden: &Path) {
    let dir = golden.join("compressed_blocks");
    let long_code = "fn placeholder() {}\n".repeat(20);
    write_manifest(
        &dir,
        r#"{
  "description": "CODE block with per-block zstd compression (COMPRESSED flag set on the block).",
  "blocks": [
    { "type": "code", "lang": "rust", "path": "src/big.rs", "compress": true,
      "content": "fn placeholder() {}  (repeated 20 times)" }
  ]
}"#,
    );

    let big_path = "src/big.rs";
    let payload = BcpEncoder::new()
        .add_code(Lang::Rust, big_path, long_code.as_bytes())
        .with_compression().unwrap()
        .encode()
        .expect("encode compressed_blocks");
    write_file(&payload_path(&dir), &payload);
}

fn generate_compressed_payload(golden: &Path) {
    let dir = golden.join("compressed_payload");
    write_manifest(
        &dir,
        r#"{
  "description": "CODE + CONVERSATION with whole-payload zstd compression (header COMPRESSED flag set).",
  "blocks": [
    { "type": "code", "lang": "typescript", "path": "index.ts", "content": "export const VERSION = '1.0.0';" },
    { "type": "conversation", "role": "user", "content": "What does this export?" }
  ],
  "compress_payload": true
}"#,
    );

    let ts_path = "index.ts";
    let ts_content = b"export const VERSION = '1.0.0';";
    let payload = BcpEncoder::new()
        .add_code(Lang::TypeScript, ts_path, ts_content)
        .add_conversation(Role::User, b"What does this export?")
        .compress_payload()
        .encode()
        .expect("encode compressed_payload");
    write_file(&payload_path(&dir), &payload);
}

fn generate_content_addressed(golden: &Path) {
    let dir = golden.join("content_addressed");

    let store = Arc::new(MemoryContentStore::new());
    let shared_content = b"fn shared() -> u32 { 42 }";
    let path_a = "src/a.rs";
    let path_b = "src/b.rs";

    let payload = BcpEncoder::new()
        .set_content_store(Arc::clone(&store) as Arc<dyn bcp_types::content_store::ContentStore>)
        .auto_dedup()
        .add_code(Lang::Rust, path_a, shared_content)
        .add_code(Lang::Rust, path_b, shared_content)
        .encode()
        .expect("encode content_addressed");
    write_file(&payload_path(&dir), &payload);

    let hash = blake3::hash(shared_content);
    let entries = vec![format!(
        r#"  "{}": "{}""#,
        hex::encode(hash.as_bytes()),
        hex::encode(shared_content)
    )];

    write_manifest(
        &dir,
        r#"{
  "description": "Two identical CODE blocks deduplicated via BLAKE3 content addressing.",
  "blocks": [
    { "type": "code", "lang": "rust", "path": "src/a.rs", "content": "fn shared() -> u32 { 42 }" },
    { "type": "code", "lang": "rust", "path": "src/b.rs", "content": "fn shared() -> u32 { 42 }" }
  ],
  "auto_dedup": true
}"#,
    );

    let content_store_json = format!("{{\n{}\n}}\n", entries.join(",\n"));
    write_file(
        &dir.join("content_store.json"),
        content_store_json.as_bytes(),
    );
}

fn generate_budget_constrained(golden: &Path) {
    let dir = golden.join("budget_constrained");
    write_manifest(
        &dir,
        r#"{
  "description": "Three CODE blocks with CRITICAL, NORMAL, and BACKGROUND priorities for budget testing.",
  "blocks": [
    { "type": "code", "lang": "rust", "path": "critical.rs",
      "content": "// CRITICAL: must always be included\nfn critical_path() { todo!() }",
      "priority": "critical" },
    { "type": "code", "lang": "rust", "path": "normal.rs",
      "content": "// NORMAL priority: included when budget allows\nfn normal_path() {}",
      "priority": "normal" },
    { "type": "code", "lang": "rust", "path": "background.rs",
      "content": "// BACKGROUND: omitted first under budget pressure\nfn background() {}",
      "priority": "background" }
  ]
}"#,
    );

    let critical_path = "critical.rs";
    let normal_path = "normal.rs";
    let background_path = "background.rs";

    let payload = BcpEncoder::new()
        .add_code(
            Lang::Rust,
            critical_path,
            b"// CRITICAL: must always be included\nfn critical_path() { todo!() }",
        )
        .with_priority(Priority::Critical).unwrap()
        .add_code(
            Lang::Rust,
            normal_path,
            b"// NORMAL priority: included when budget allows\nfn normal_path() {}",
        )
        .with_priority(Priority::Normal).unwrap()
        .add_code(
            Lang::Rust,
            background_path,
            b"// BACKGROUND: omitted first under budget pressure\nfn background() {}",
        )
        .with_priority(Priority::Background).unwrap()
        .encode()
        .expect("encode budget_constrained");
    write_file(&payload_path(&dir), &payload);
}

fn generate_all_block_types(golden: &Path) {
    let dir = golden.join("all_block_types");
    write_manifest(
        &dir,
        r##"{
  "description": "One block of each of the 11 semantic block types.",
  "blocks": [
    { "type": "code", "lang": "go", "path": "main.go", "content": "package main" },
    { "type": "conversation", "role": "system", "content": "You are a helpful assistant." },
    { "type": "file_tree", "root": "/", "entries": [{ "name": "main.go", "kind": "file", "size": 12 }] },
    { "type": "tool_result", "tool_name": "ls", "status": "ok", "content": "main.go" },
    { "type": "document", "title": "README", "content": "# BCP", "format_hint": "markdown" },
    { "type": "structured_data", "format": "json", "content": "{\"version\":1}" },
    { "type": "diff", "path": "main.go", "hunks": [{ "old_start": 1, "new_start": 1, "lines": "-package old\n+package main\n" }] },
    { "type": "annotation", "target_block_id": 0, "kind": "tag", "value": "entry-point" },
    { "type": "embedding_ref", "vector_id": "vec-001", "source_hash": "aabbcc...(32 bytes)", "model": "text-embedding-3-small" },
    { "type": "image", "media_type": "png", "alt_text": "Logo", "data": "(1x1 PNG bytes)" },
    { "type": "extension", "namespace": "com.example", "type_name": "custom", "content": "hello" }
  ]
}"##,
    );

    // Minimal 1x1 PNG (67 bytes).
    let tiny_png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR length + type
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1 dimensions
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, // bit depth, color, crc
        0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, // IDAT length + type
        0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, // compressed pixel data
        0x00, 0x00, 0x02, 0x00, 0x01, 0xE2, 0x21, 0xBC, // IDAT crc
        0x33, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, // IEND length + type
        0x44, 0xAE, 0x42, 0x60, 0x82, // IEND data + crc
    ];

    let source_hash = vec![0xAA; 32];
    let go_file = ["main", ".go"].concat();
    let go_content = ["package", " main"].concat();
    let json_content = ["{", r#""version":1}"#].concat();
    let diff_old = ["-package old\n+package", " main\n"].concat();

    let payload = BcpEncoder::new()
        .add_code(Lang::Go, &go_file, go_content.as_bytes())
        .add_conversation(Role::System, b"You are a helpful assistant.")
        .add_file_tree(
            "/",
            vec![FileEntry {
                name: go_file.clone(),
                kind: FileEntryKind::File,
                size: 12,
                children: vec![],
            }],
        )
        .add_tool_result("ls", Status::Ok, go_file.as_bytes())
        .add_document("README", "# BCP".as_bytes(), FormatHint::Markdown)
        .add_structured_data(DataFormat::Json, json_content.as_bytes())
        .add_diff(
            &go_file,
            vec![DiffHunk {
                old_start: 1,
                new_start: 1,
                lines: diff_old.into_bytes(),
            }],
        )
        .add_annotation(0, AnnotationKind::Tag, b"entry-point")
        .add_embedding_ref(b"vec-001", &source_hash, "text-embedding-3-small")
        .add_image(MediaType::Png, "Logo", tiny_png)
        .add_extension("com.example", "custom", b"hello")
        .encode()
        .expect("encode all_block_types");
    write_file(&payload_path(&dir), &payload);
}

fn generate_edge_cases(golden: &Path) {
    generate_edge_empty_content(golden);
    generate_edge_large_varint(golden);
    generate_edge_unknown_block_type(golden);
    generate_edge_trailing_data(golden);
}

fn generate_edge_empty_content(golden: &Path) {
    let dir = golden.join("edge_cases/empty_content");
    write_manifest(
        &dir,
        r#"{
  "description": "CODE block with empty content field. Tests that zero-length bytes are valid.",
  "blocks": [
    { "type": "code", "lang": "rust", "path": "empty.rs", "content": "" }
  ]
}"#,
    );

    let empty_path = "empty.rs";
    let payload = BcpEncoder::new()
        .add_code(Lang::Rust, empty_path, b"")
        .encode()
        .expect("encode empty_content");
    write_file(&payload_path(&dir), &payload);
}

fn generate_edge_large_varint(golden: &Path) {
    let dir = golden.join("edge_cases/large_varint");
    let large_content = b"x".repeat(16 * 1024);
    write_manifest(
        &dir,
        r#"{
  "description": "CODE block with 16 KiB content. content_len uses a 3-byte LEB128 varint.",
  "blocks": [
    { "type": "code", "lang": "rust", "path": "large.rs", "content": "x (repeated 16384 times)" }
  ]
}"#,
    );

    let large_path = "large.rs";
    let payload = BcpEncoder::new()
        .add_code(Lang::Rust, large_path, &large_content)
        .encode()
        .expect("encode large_varint");
    write_file(&payload_path(&dir), &payload);
}

fn generate_edge_unknown_block_type(golden: &Path) {
    use bcp_wire::block_frame::{BlockFlags, BlockFrame};
    use bcp_wire::header::{HEADER_SIZE, HeaderFlags, BcpHeader};

    let dir = golden.join("edge_cases/unknown_block_type");

    // Handcraft a valid payload using the wire types so that varints are
    // encoded correctly. block_type=0xFF (END) must be LEB128-encoded as
    // [0xFF, 0x01], not a raw 0xFF byte.
    let mut payload = vec![0u8; HEADER_SIZE];
    BcpHeader::new(HeaderFlags::NONE)
        .write_to(&mut payload)
        .expect("write header");

    let unknown = BlockFrame {
        block_type: 0x42,
        flags: BlockFlags::NONE,
        body: b"hello".to_vec(),
    };
    unknown.write_to(&mut payload).expect("write unknown block");

    let end = BlockFrame {
        block_type: 0xFF,
        flags: BlockFlags::NONE,
        body: Vec::new(),
    };
    end.write_to(&mut payload).expect("write END sentinel");

    write_file(&payload_path(&dir), &payload);
}

fn generate_edge_trailing_data(golden: &Path) {
    let dir = golden.join("edge_cases/trailing_data");

    let t_path = "t.rs";
    let mut payload = BcpEncoder::new()
        .add_code(Lang::Rust, t_path, b"// trailing")
        .encode()
        .expect("encode for trailing_data base");

    // Append garbage bytes after the END sentinel.
    payload.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);

    write_file(&payload_path(&dir), &payload);
}
