#!/usr/bin/env bash
# scripts/setup_env.sh
#
# Sets up the Python environment for BCP QLoRA experiments on DGX Spark.
#
# Prerequisites:
#   - Python 3.11+ installed
#   - uv installed (https://docs.astral.sh/uv/)
#   - CUDA 12.x drivers installed (pre-installed on DGX Spark)
#   - Ollama running with qwen3-coder-next:q8_0 pulled
#   - bcp CLI built: cargo build --release -p bcp-cli
#
# Usage:
#   bash scripts/setup_env.sh
#   source .venv/bin/activate

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

echo "=== Creating Python virtual environment (Python 3.12) ==="
# PyTorch CUDA wheels only support up to Python 3.13; pin to 3.12 which is
# available on the DGX Spark and has the broadest wheel compatibility.
uv venv .venv --python 3.12
source .venv/bin/activate

echo "=== Installing PyTorch with CUDA 12.8 support ==="
# PyTorch needs a special index for CUDA builds — install it first.
# The DGX Spark has a Blackwell GB10 (sm_121) which requires PyTorch 2.7+
# with CUDA 12.8 builds.
uv pip install "torch>=2.7.0" --index-url https://download.pytorch.org/whl/cu128

echo "=== Installing project dependencies ==="
# Installs everything from pyproject.toml in one pass
uv pip install -e .

echo "=== Verifying bcp CLI ==="
export PATH="/home/nicks-dgx/dev/.RFCs/bit-context-protocol/target/release:$PATH"
if command -v bcp &> /dev/null; then
    echo "bcp CLI found: $(bcp --version)"
else
    echo "ERROR: bcp CLI not found on PATH."
    echo "Build it first: cargo build --release -p bcp-cli"
    echo "Then add to PATH: export PATH=\"\$PWD/target/release:\$PATH\""
    exit 1
fi

echo "=== Verifying Ollama ==="
if loclaude models 2>/dev/null | grep -q "qwen3-coder-next"; then
    echo "Ollama verified: qwen3-coder-next model available"
else
    echo "WARNING: Could not verify Ollama models."
    echo "Ensure Ollama is running and qwen3-coder-next:q8_0 is pulled."
fi

echo "=== Verifying CUDA ==="
python -c "
import torch
if torch.cuda.is_available():
    print(f'CUDA available: {torch.cuda.get_device_name(0)}')
    print(f'Memory: {torch.cuda.get_device_properties(0).total_memory / 1e9:.1f} GB')
else:
    print('WARNING: CUDA not available. Training will be extremely slow on CPU.')
"

echo ""
echo "=== Setup complete ==="
echo "Activate with: source .venv/bin/activate"
