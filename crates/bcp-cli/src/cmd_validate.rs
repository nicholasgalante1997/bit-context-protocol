/// Implementation of `bcp validate`.
///
/// Attempts a full structural decode of the BCP file and reports either a
/// series of success checkmarks (`✓`) or a diagnostic failure line (`✗`).
/// The command exits with code 0 on a valid file and code 1 on any error
/// (the main dispatcher in `main.rs` converts `Err` to exit code 1).
///
/// # Success output
///
/// ```text
/// ✓ Header: valid (BCP v1.0)
/// ✓ Blocks: 4 blocks parsed successfully
/// ✓ Sentinel: END block present
/// ✓ Integrity: all block bodies parse without error
/// ```
///
/// # Failure output
///
/// ```text
/// ✗ Error: invalid header — invalid magic number: expected 0x42435000, got 0xDEADBEEF
/// ```
///
/// # Validation steps
///
/// The validate command runs a single `BcpDecoder::decode` call, which
/// covers all four structural layers defined in RFC §4:
///
/// ```text
/// 1. Header      — magic number, version, reserved byte
/// 2. Decompression — whole-payload zstd (if compressed flag set)
/// 3. Block frames — block_type varint, flags byte, content_len varint, body
/// 4. Block bodies — TLV field deserialization for each typed block
/// ```
///
/// A file that passes all four steps is considered structurally valid.
/// Semantic validity (e.g. referential integrity between annotations and
/// their target blocks) is out of scope for the `PoC`.
use std::fs;

use anyhow::{Context, Result, anyhow};
use bcp_decoder::{DecodeError, BcpDecoder};

use crate::ValidateArgs;

/// Run the `bcp validate` command.
///
/// Prints a validation report to stdout and returns `Ok(())` on success.
/// On any structural error, prints a `✗` diagnostic to stdout and returns
/// `Err`, which the main dispatcher converts to exit code 1.
///
/// # Errors
///
/// Returns an error if the file cannot be read, or if the BCP payload
/// fails any structural validation check.
pub fn run(args: &ValidateArgs) -> Result<()> {
    let bytes =
        fs::read(&args.file).with_context(|| format!("cannot read {}", args.file.display()))?;

    match BcpDecoder::decode(&bytes) {
        Ok(decoded) => {
            let header = &decoded.header;
            println!(
                "✓ Header: valid (BCP v{}.{})",
                header.version_major, header.version_minor
            );
            println!(
                "✓ Blocks: {} block{} parsed successfully",
                decoded.blocks.len(),
                if decoded.blocks.len() == 1 { "" } else { "s" }
            );
            println!("✓ Sentinel: END block present");
            println!("✓ Integrity: all block bodies parse without error");
            Ok(())
        }

        Err(e) => {
            let diagnostic = decode_error_diagnostic(&e);
            println!("✗ Error: {diagnostic}");
            Err(anyhow!("validation failed"))
        }
    }
}

// ── Error formatting ──────────────────────────────────────────────────────────

/// Converts a `DecodeError` into a human-readable diagnostic string.
///
/// Maps each error variant to a message that mirrors the format shown in
/// `SPEC_09` §3, giving enough context for the user to locate the problem.
///
/// ```text
/// ┌──────────────────────────┬──────────────────────────────────────────┐
/// │ DecodeError variant      │ Diagnostic message prefix                │
/// ├──────────────────────────┼──────────────────────────────────────────┤
/// │ InvalidHeader            │ "invalid header — <inner error>"         │
/// │ MissingEndSentinel       │ "missing END sentinel"                   │
/// │ TrailingData             │ "trailing data after END ({n} bytes)"    │
/// │ MissingContentStore      │ "content-addressed block with no store"  │
/// │ UnresolvedReference      │ "unresolved BLAKE3 reference"            │
/// │ Wire / Type / Decompress │ "<error Display>"                        │
/// └──────────────────────────┴──────────────────────────────────────────┘
/// ```
fn decode_error_diagnostic(e: &DecodeError) -> String {
    match e {
        DecodeError::InvalidHeader(inner) => format!("invalid header — {inner}"),
        DecodeError::MissingEndSentinel => "missing END sentinel".to_string(),
        DecodeError::TrailingData { extra_bytes } => {
            format!("trailing data after END ({extra_bytes} unexpected bytes)")
        }
        DecodeError::MissingContentStore => {
            "content-addressed reference block found but no content store available".to_string()
        }
        DecodeError::UnresolvedReference { hash } => {
            let hex: String = hash
                .iter()
                .take(8)
                .fold(String::with_capacity(16), |mut s, b| {
                    use std::fmt::Write as _;
                    let _ = write!(s, "{b:02x}");
                    s
                });
            format!("unresolved BLAKE3 reference {hex}…")
        }
        other => other.to_string(),
    }
}
