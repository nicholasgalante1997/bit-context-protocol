"""
BCP CLI wrapper for rendering code files through the BCP pipeline.

=== What This Does ===
Takes a list of source files and runs them through the BCP encode → decode
pipeline using the bcp CLI binary. This is how we generate the BCP-rendered
context variants for training data.

=== The Pipeline ===
  1. Build a JSON manifest describing the blocks to encode
  2. Write the manifest to a temp file
  3. Run: bcp encode <manifest.json> -o <output.bcp>
  4. Run: bcp decode <output.bcp> --mode <xml|markdown|minimal>
  5. Return the decoded text

=== Why Shell Out to bcp CLI? ===
The bcp encoder/decoder are Rust crates. Rather than building Python bindings
(complex) or WASM bindings (experimental), we use the CLI as the integration
point. This is the same approach used by the MCP server (SPEC_12). The overhead
is ~5-10ms per invocation, which is negligible for data generation.

Prerequisites:
  bcp CLI must be on PATH: cargo build --release -p bcp-cli
"""

import json
import subprocess
import tempfile
from pathlib import Path


def render_files_as_bcp(files: list[dict], mode: str = "xml") -> str:
    """
    Encode files as a BCP payload and render in the specified mode.

    Args:
        files: List of file dicts with keys: language, path, content.
        mode: BCP render mode — "xml", "markdown", or "minimal".

    Returns:
        The rendered text output from bcp decode.

    Raises:
        subprocess.CalledProcessError: If bcp encode or decode fails.
        FileNotFoundError: If bcp binary is not on PATH.
    """
    # Build the JSON manifest that bcp encode expects.
    manifest = {
        "blocks": [
            {
                "type": "code",
                "lang": f["language"],
                "path": f["path"],
                "content": f["content"],
            }
            for f in files
        ]
    }

    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".json", delete=False
    ) as manifest_file:
        json.dump(manifest, manifest_file)
        manifest_path = manifest_file.name

    bcp_path = manifest_path.replace(".json", ".bcp")

    try:
        # Step 1: Encode the manifest into a .bcp binary file
        subprocess.run(
            ["bcp", "encode", manifest_path, "-o", bcp_path],
            check=True,
            capture_output=True,
        )

        # Step 2: Decode the .bcp file into text in the specified mode
        result = subprocess.run(
            ["bcp", "decode", bcp_path, "--mode", mode],
            check=True,
            capture_output=True,
            text=True,
        )

        return result.stdout

    finally:
        Path(manifest_path).unlink(missing_ok=True)
        Path(bcp_path).unlink(missing_ok=True)


def render_mixed_blocks_as_bcp(blocks: list[dict], mode: str = "xml") -> str:
    """
    Encode mixed block types (code + tool results + conversation) as BCP.

    This is used for tool-use tasks where the context includes more than
    just code files. The blocks list follows the same manifest format that
    bcp encode expects.

    Args:
        blocks: List of block dicts. Each must have a "type" key.
        mode: BCP render mode.

    Returns:
        The rendered text output.
    """
    manifest = {"blocks": blocks}

    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".json", delete=False
    ) as manifest_file:
        json.dump(manifest, manifest_file)
        manifest_path = manifest_file.name

    bcp_path = manifest_path.replace(".json", ".bcp")

    try:
        subprocess.run(
            ["bcp", "encode", manifest_path, "-o", bcp_path],
            check=True,
            capture_output=True,
        )

        result = subprocess.run(
            ["bcp", "decode", bcp_path, "--mode", mode],
            check=True,
            capture_output=True,
            text=True,
        )

        return result.stdout

    finally:
        Path(manifest_path).unlink(missing_ok=True)
        Path(bcp_path).unlink(missing_ok=True)
