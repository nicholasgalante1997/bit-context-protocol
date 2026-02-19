/// Implementation of `bcp inspect`.
///
/// Reads a BCP file, decodes all blocks, and prints a structured summary
/// to stdout. Optionally shows block body content (`--show-body`) or a raw
/// hex dump (`--show-hex`). When `--block N` is given, only the block at
/// index N is shown.
///
/// # Output format
///
/// ```text
/// Header: BCP v1.0, flags=0x00, 4 blocks
/// Block 0: CODE [rust] path="src/main.rs" (23 bytes)
///          Summary: "Entry point with CLI setup"
/// Block 1: CONVERSATION [user] (19 bytes)
/// Block 2: CONVERSATION [assistant] (31 bytes)
/// Block 3: ANNOTATION target=0 kind=priority value="high"
/// ---
/// END sentinel at offset 312
/// ```
use std::fs;

use anyhow::{Context, Result};
use bcp_decoder::BcpDecoder;
use bcp_types::block::BlockContent;
use bcp_types::enums::AnnotationKind;

use crate::InspectArgs;

/// Run the `bcp inspect` command.
///
/// # Errors
///
/// Returns an error if the file cannot be read or the BCP payload is
/// structurally invalid (malformed header, truncated blocks, etc.).
pub fn run(args: &InspectArgs) -> Result<()> {
    let bytes =
        fs::read(&args.file).with_context(|| format!("cannot read {}", args.file.display()))?;

    let decoded = BcpDecoder::decode(&bytes)
        .with_context(|| format!("failed to decode {}", args.file.display()))?;

    let header = &decoded.header;
    println!(
        "Header: BCP v{}.{}, flags=0x{:02X}, {} block{}",
        header.version_major,
        header.version_minor,
        header.flags.raw(),
        decoded.blocks.len(),
        if decoded.blocks.len() == 1 { "" } else { "s" }
    );

    for (idx, block) in decoded.blocks.iter().enumerate() {
        // When --block N is specified, skip all other indices.
        if let Some(target) = args.block
            && idx != target
        {
            continue;
        }

        let type_label = block_type_label(&block.content);
        let detail = block_detail(&block.content);
        let body_bytes = block_body_bytes(&block.content);

        println!("Block {idx}: {type_label}{detail} ({body_bytes} bytes)");

        if let Some(ref summary) = block.summary {
            println!("         Summary: {:?}", summary.text);
        }

        if args.show_body {
            let body = block_body_lossy(&block.content);
            let truncated: String = body.chars().take(80).collect();
            let ellipsis = if body.chars().count() > 80 { "…" } else { "" };
            println!("         Body:    {truncated}{ellipsis}");
        }

        if args.show_hex {
            let raw = block_body_raw(&block.content);
            println!("         Hex dump:");
            for (i, chunk) in raw.chunks(16).enumerate() {
                let offset = i * 16;
                let hex: String =
                    chunk
                        .iter()
                        .fold(String::with_capacity(chunk.len() * 3), |mut s, b| {
                            use std::fmt::Write as _;
                            if !s.is_empty() {
                                s.push(' ');
                            }
                            let _ = write!(s, "{b:02x}");
                            s
                        });
                let ascii: String = chunk
                    .iter()
                    .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
                    .collect();
                println!("           {offset:04x}  {hex:<48}  {ascii}");
            }
        }
    }

    // The END sentinel occupies the last 4 bytes of a minimal payload.
    // Show its byte offset (file length minus 4) as a convenience marker.
    println!("---");
    println!("END sentinel at offset {}", bytes.len().saturating_sub(4));

    Ok(())
}

// ── Block formatting helpers ──────────────────────────────────────────────────

/// Returns the uppercase type label (e.g. `"CODE"`, `"CONVERSATION"`).
fn block_type_label(content: &BlockContent) -> &'static str {
    match content {
        BlockContent::Code(_) => "CODE",
        BlockContent::Conversation(_) => "CONVERSATION",
        BlockContent::FileTree(_) => "FILE_TREE",
        BlockContent::ToolResult(_) => "TOOL_RESULT",
        BlockContent::Document(_) => "DOCUMENT",
        BlockContent::StructuredData(_) => "STRUCTURED_DATA",
        BlockContent::Diff(_) => "DIFF",
        BlockContent::Annotation(_) => "ANNOTATION",
        BlockContent::EmbeddingRef(_) => "EMBEDDING_REF",
        BlockContent::Image(_) => "IMAGE",
        BlockContent::Extension(_) => "EXTENSION",
        BlockContent::End => "END",
        BlockContent::Unknown { .. } => "UNKNOWN",
    }
}

/// Returns the human-readable inline detail string for a block, formatted
/// so it reads naturally after the type label on one line.
///
/// Examples:
/// - Code       → ` [rust] path="src/main.rs"`
/// - Conversation → ` [user]`
/// - `ToolResult` → ` [ripgrep] status=ok`
/// - Annotation → ` target=0 kind=priority value="high"`
fn block_detail(content: &BlockContent) -> String {
    match content {
        BlockContent::Code(c) => {
            let lang = format!("{:?}", c.lang).to_lowercase();
            format!(" [{lang}] path={:?}", c.path)
        }
        BlockContent::Conversation(c) => {
            let role = format!("{:?}", c.role).to_lowercase();
            format!(" [{role}]")
        }
        BlockContent::FileTree(t) => format!(" root={:?}", t.root_path),
        BlockContent::ToolResult(t) => {
            let status = format!("{:?}", t.status).to_lowercase();
            format!(" [{}] status={status}", t.tool_name)
        }
        BlockContent::Document(d) => format!(" title={:?}", d.title),
        BlockContent::StructuredData(s) => {
            let fmt = format!("{:?}", s.format).to_lowercase();
            format!(" format={fmt}")
        }
        BlockContent::Diff(d) => format!(" path={:?}", d.path),
        BlockContent::Annotation(a) => {
            let kind = annotation_kind_label(a.kind);
            let value = format_annotation_value(a.kind, &a.value);
            format!(" target={} kind={kind} value={value:?}", a.target_block_id)
        }
        BlockContent::EmbeddingRef(_) => " [embedding ref]".to_string(),
        BlockContent::Image(i) => {
            let media = format!("{:?}", i.media_type).to_lowercase();
            format!(" [{media}] alt={:?}", i.alt_text)
        }
        BlockContent::Extension(e) => {
            format!(" namespace={:?} type={:?}", e.namespace, e.type_name)
        }
        BlockContent::End => String::new(),
        BlockContent::Unknown { type_id, .. } => format!(" [0x{type_id:02X}]"),
    }
}

/// Returns the byte length of the primary content field of a block.
///
/// For most blocks this is the main `content` bytes field. For structured
/// types it is the inner data length. This is used in the `(N bytes)` display.
fn block_body_bytes(content: &BlockContent) -> usize {
    match content {
        BlockContent::Code(c) => c.content.len(),
        BlockContent::Conversation(c) => c.content.len(),
        BlockContent::FileTree(t) => t.entries.len() * 8, // approximate
        BlockContent::ToolResult(t) => t.content.len(),
        BlockContent::Document(d) => d.content.len(),
        BlockContent::StructuredData(s) => s.content.len(),
        BlockContent::Diff(d) => d.hunks.iter().map(|h| h.lines.len()).sum(),
        BlockContent::Annotation(a) => a.value.len(),
        BlockContent::EmbeddingRef(_) => 32,
        BlockContent::Image(i) => i.data.len(),
        BlockContent::Extension(e) => e.content.len(),
        BlockContent::End => 0,
        BlockContent::Unknown { body, .. } => body.len(),
    }
}

/// Returns the primary content bytes as UTF-8 lossy string, for `--show-body`.
fn block_body_lossy(content: &BlockContent) -> String {
    let bytes = block_body_raw(content);
    String::from_utf8_lossy(bytes).into_owned()
}

/// Returns the raw bytes of the primary content field, for `--show-hex`.
fn block_body_raw(content: &BlockContent) -> &[u8] {
    match content {
        BlockContent::Code(c) => &c.content,
        BlockContent::Conversation(c) => &c.content,
        BlockContent::ToolResult(t) => &t.content,
        BlockContent::Document(d) => &d.content,
        BlockContent::StructuredData(s) => &s.content,
        BlockContent::Annotation(a) => &a.value,
        BlockContent::Image(i) => &i.data,
        BlockContent::Extension(e) => &e.content,
        BlockContent::Unknown { body, .. } => body,
        _ => b"",
    }
}

/// Returns a display-friendly annotation kind name.
fn annotation_kind_label(kind: AnnotationKind) -> &'static str {
    match kind {
        AnnotationKind::Priority => "priority",
        AnnotationKind::Summary => "summary",
        AnnotationKind::Tag => "tag",
    }
}

/// Formats an annotation value for display.
///
/// For priority annotations, decodes the single byte to a priority name.
/// For all others, formats as UTF-8 lossy text.
fn format_annotation_value(kind: AnnotationKind, value: &[u8]) -> String {
    use bcp_types::enums::Priority;

    if kind == AnnotationKind::Priority
        && let Some(&byte) = value.first()
        && let Ok(p) = Priority::from_wire_byte(byte)
    {
        return format!("{p:?}").to_lowercase();
    }
    String::from_utf8_lossy(value).into_owned()
}
