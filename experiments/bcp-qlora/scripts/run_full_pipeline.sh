#!/usr/bin/env bash
# scripts/run_full_pipeline.sh
#
# End-to-end pipeline: clone repos → generate data → prepare datasets →
#                      train model → evaluate → produce report
#
# Prerequisites:
#   - setup_env.sh has been run and .venv is activated
#   - Ollama is running with qwen3-coder-next:q8_0 and devstral-2:123b
#   - bcp CLI is on PATH
#
# Usage:
#   bash scripts/run_full_pipeline.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

source .venv/bin/activate

# Ensure bcp CLI is on PATH (built from the Rust workspace)
BCP_BIN="$(cd "$PROJECT_DIR/../.." && pwd)/target/release"
export PATH="$BCP_BIN:$PATH"

if ! command -v bcp &> /dev/null; then
    echo "ERROR: bcp CLI not found. Build it first:"
    echo "  cargo build --release -p bcp-cli"
    exit 1
fi

echo "=== Step 1: Clone source repositories ==="
echo "Cloning ~12 open-source repos for training data."
bash scripts/clone_repos.sh

echo ""
echo "=== Step 2: Generate training data (via Ollama) ==="
echo "This sends code through BCP render → Qwen3 Coder generates responses."
python -m src.generate_data \
    --config configs/generation.yaml \
    --output data/raw/ \
    --limit 1250

echo ""
echo "=== Step 3: Prepare HuggingFace datasets ==="
echo "Converting JSONL → HuggingFace Dataset with train/val/test splits."
python -m src.prepare_dataset \
    --input data/raw/ \
    --output data/processed/

echo ""
echo "=== Step 4: Train QLoRA adapter (BCP Minimal) ==="
echo "Fine-tuning Llama 3.1 8B with QLoRA on BCP Minimal context."
echo "Estimated time: 2-4 hours for 3 epochs on 4,000 examples."
python -m src.train \
    --config configs/qlora_tier1.yaml \
    --dataset data/processed/context_bcp_minimal \
    --variant bcp_minimal \
    --output outputs/checkpoints/bcp_minimal

echo ""
echo "=== Step 5: Evaluate ==="
echo "Running evaluation matrix (5 configurations × test set)."
python -m src.evaluate \
    --base-model meta-llama/Llama-3.1-8B-Instruct \
    --adapter outputs/checkpoints/bcp_minimal/final \
    --test-data data/processed/ \
    --output outputs/eval/

echo ""
echo "=== Pipeline Complete ==="
echo ""
echo "Results: outputs/eval/results.json"
echo "Report:  outputs/eval/ (see comparison table above)"
echo ""
echo "Next steps:"
echo "  - If PASS: BCP Minimal preserves comprehension at fewer tokens"
echo "  - If FAIL: Try BCP XML mode or increase training data"
echo "  - Tier 2: Add hybrid binary tokens (see SPEC_13 §Tier 2)"
