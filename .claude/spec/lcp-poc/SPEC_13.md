# SPEC_13 — QLoRA Fine-Tuning: BCP Comprehension Validation

**Location**: `experiments/bcp-qlora/`
**Phase**: 6 (Validation — Model Comprehension)
**Prerequisites**: SPEC_01 through SPEC_11 (complete BCP reference implementation + tiktoken benchmarks)
**Hardware**: NVIDIA DGX Spark (Grace CPU + Blackwell GPU, 128GB unified memory)
**Dependencies**: `bcp-encoder`, `bcp-decoder`, `bcp-driver`, `bcp-cli`, Python 3.11+, PyTorch, transformers, peft, bitsandbytes, trl, datasets, Ollama (for data generation)

---

## Context

BCP's token efficiency claims (RFC §6) have been validated at the tokenizer level by
SPEC_11's tiktoken benchmarks: the format produces measurably fewer BPE tokens than
equivalent markdown for identical semantic content. But fewer tokens is only half the
value proposition. The harder question is: **does a model comprehend BCP-rendered
context as well as (or better than) raw markdown, despite using fewer tokens?**

If the answer is yes, BCP delivers a strict improvement — same comprehension, fewer
tokens, more room in the context window. If the answer is no, the token savings are
meaningless because the model can't use the compressed format effectively.

This spec defines a QLoRA fine-tuning experiment on the DGX Spark that answers this
question empirically. The experiment:

1. Generates synthetic training data: (BCP-rendered context, expected coding/tool-use
   response) pairs sourced from real code repositories, using a locally-hosted coding
   model (Qwen3 Coder via Ollama) for response generation — zero API cost
2. Fine-tunes Llama 3.1 8B with QLoRA (4-bit quantization, low-rank adapters)
3. Evaluates the fine-tuned model against a held-out test set, comparing:
   - Fine-tuned model + BCP Minimal input (fewest tokens)
   - Fine-tuned model + BCP XML input (Claude-optimized)
   - Base model + raw markdown input (baseline)
4. Produces a comprehension parity report with quantitative metrics

The DGX Spark's 128GB unified memory (shared between Grace CPU and Blackwell GPU)
eliminates the typical VRAM ceiling. Llama 3.1 8B in 4-bit quantization requires
~5GB for weights, ~2-4GB for optimizer states, and ~1-2GB for activations — well
within budget. Training completes in hours, not days.

### Tier Architecture

This spec implements **Tier 1 (Text Decode Validation)** with an explicit extension
path to two follow-on tiers:

```
Tier 1 (this spec)                    Tier 2 (fast-follow)          Tier 3 (research)
─────────────────────────             ────────────────────          ─────────────────
BCP text decode output                Hybrid binary/text            Native binary
(XML, Markdown, Minimal)              tokens for structure          ingestion
    │                                     │                             │
    ▼                                     ▼                             ▼
Fine-tune on rendered text    →    Extend tokenizer with       →   Pretraining-scale
Compare vs markdown baseline       special tokens for BCP          collaboration with
                                   block types (0x01, 0x02)        model providers
                                   + TLV field markers
    │                                     │                             │
    ▼                                     ▼                             ▼
Proves: format preserves          Proves: models can learn      Proves: models can
comprehension at fewer tokens      binary structure markers      consume BCP directly
                                                                 without text decode
    │
    └── Foundation laid by this spec:
        - Data pipeline (reusable for Tier 2/3 data generation)
        - Evaluation harness (reusable metrics + comparison framework)
        - Training infrastructure (QLoRA config, DGX Spark setup)
        - Dataset schema (extensible for hybrid/binary examples)
```

Tier 2 extends the tokenizer vocabulary with special tokens mapping to BCP wire
format constants (block type IDs, field tag IDs, flag bytes) and fine-tunes on
hybrid-annotated context where binary structure tokens replace rendered delimiters.
Tier 3 requires pretraining-level compute and is documented as a research direction
only.

---

## Design Decisions

### DD-13-01: Base Model Selection

**Question**: Which model to fine-tune?

**Option A**: Llama 3.1 8B Instruct
- Pros: Strong code comprehension baseline, well-studied QLoRA behavior, 8K default context (extendable), extensive community tooling. Fits comfortably in 128GB with QLoRA.
- Cons: Meta license restrictions for some commercial uses.

**Option B**: Mistral 7B v0.3 Instruct
- Pros: Strong coding performance per parameter, Apache 2.0 license, sliding window attention.
- Cons: Slightly less community QLoRA documentation, smaller context window default.

**Option C**: Qwen 2.5 7B Instruct
- Pros: Strong multilingual + code performance, Apache 2.0 license, 32K context.
- Cons: Less QLoRA community tooling, tokenizer differences may affect BPE comparisons.

**Decision**: **Option A — Llama 3.1 8B Instruct**

**Rationale**: Best-studied QLoRA target with the most reproducible training recipes.
The instruct variant provides a strong baseline for code comprehension tasks, making
it easier to isolate BCP's effect vs general capability. The 128GB unified memory
on the Spark leaves ample headroom. Tier 2/3 experiments can test alternative models
once the pipeline is validated.

### DD-13-02: Training Framework

**Question**: Which training framework for QLoRA?

**Option A**: Raw `transformers` + `peft` + `trl`
- Pros: Full control, no abstraction overhead, direct access to all hyperparameters, easier to customize for Tier 2 tokenizer extension.
- Cons: More boilerplate, must handle data collation/logging manually.

**Option B**: Axolotl
- Pros: YAML-driven config, handles QLoRA setup automatically, built-in evaluation.
- Cons: Opinionated abstractions may conflict with custom data pipeline, harder to extend for Tier 2 tokenizer modifications, additional dependency.

**Decision**: **Option A — Raw transformers + peft + trl**

**Rationale**: The experiment requires custom data formatting (BCP-rendered context
in the prompt), custom evaluation metrics (comprehension comparison across formats),
and a clear extension path to Tier 2 tokenizer modification. The `SFTTrainer` from
`trl` provides supervised fine-tuning with QLoRA support while keeping the training
loop transparent. Direct `peft` access will be critical for Tier 2 when we need to
extend the tokenizer and resize embeddings.

### DD-13-03: Synthetic Data Generation Strategy

**Question**: How to generate training data?

**Option A**: Claude API for response generation
- Pros: Highest quality responses, understands code deeply, can generate nuanced
  tool-use responses.
- Cons: API cost (~$36 for 5,000 examples at Haiku tier, ~$135 at Sonnet tier).
  Requires API key, subject to rate limits.

**Option B**: Local model via Ollama (Qwen3 Coder Next Q8, 79GB)
- Pros: Zero cost, no rate limits, frontier-class code comprehension, already cached
  on DGX Spark, runs entirely on local hardware. Full control over generation
  parameters.
- Cons: Slightly lower quality than Claude Sonnet for nuanced explanations.
  Consumes GPU memory during generation (cannot train simultaneously).

**Option C**: Local model via Ollama (devstral-2:123b or gpt-oss:120b)
- Pros: Zero cost, large parameter count, strong general reasoning.
- Cons: Slower generation than Qwen3 Coder (larger model), not code-specialized.

**Decision**: **Option B — Ollama + Qwen3 Coder Next Q8 (local)**

**Rationale**: Zero API cost makes experimentation free. The model is already cached
on the DGX Spark (`qwen3-coder-next:q8_0`, 79GB). Qwen3 Coder is a frontier-class
coding model — purpose-built for code comprehension, review, and generation tasks,
which aligns directly with our training data domain. The Q8 quantization preserves
near-full quality. The data pipeline uses Ollama's OpenAI-compatible API, so
switching to a cloud provider or different local model is a one-line config change.

**Available models on DGX Spark** (for reference/fallback):

```
qwen3-coder-next:q8_0       78.99GB   ← PRIMARY (code-specialized, Q8 quality)
qwen3-coder-next:latest     48.19GB   ← Fallback (smaller quantization)
devstral-2:123b              69.75GB   ← Alternative (general + code)
gpt-oss:120b                 60.88GB   ← Alternative (general purpose)
qwen2.5-coder:32b           18.49GB   ← Lightweight fallback
qwen3-coder:30b             17.28GB   ← Lightweight fallback
deepseek-coder:33b          17.53GB   ← Lightweight fallback
```

### DD-13-04: Evaluation Methodology

**Question**: How to measure comprehension parity?

**Option A**: Human evaluation (blind A/B comparison)
- Pros: Most reliable signal for response quality.
- Cons: Slow, expensive, not reproducible, small sample sizes.

**Option B**: LLM-as-judge (local model evaluates responses)
- Pros: Scalable, reproducible, zero cost with Ollama. Can use a different model
  than the one that generated the data to reduce bias.
- Cons: Local judge may have format preferences. Lower judge quality than Claude.

**Option C**: Automated metrics (BLEU, ROUGE, exact match, pass@k)
- Pros: Fully automated, reproducible, no cost.
- Cons: Poor correlation with actual code quality for open-ended tasks.

**Option D**: Hybrid — automated metrics + LLM-as-judge (local)
- Pros: Automated metrics for fast iteration, LLM-as-judge for final evaluation.
  Zero cost when using Ollama for judging.
- Cons: More complex pipeline.

**Decision**: **Option D — Hybrid evaluation (all local)**

**Rationale**: Use automated metrics (ROUGE-L for open-ended responses) during
training iteration, then run LLM-as-judge on the final held-out test set for the
comprehension parity report. The judge uses a *different* model than the data
generator (e.g., generate with `qwen3-coder-next:q8_0`, judge with
`devstral-2:123b`) to avoid self-evaluation bias. The judge prompt evaluates
correctness and helpfulness without seeing the input format, preventing
format-preference bias. If a more authoritative evaluation is needed later, the
pipeline supports swapping in a cloud API judge with a config change.

---

## Requirements

### 1. Project Structure

The experiment lives outside the Rust workspace in `experiments/bcp-qlora/`.
Python project managed with `uv` (or `pip` + `venv`).

```
experiments/bcp-qlora/
├── pyproject.toml              # Python project config (uv/pip compatible)
├── README.md                   # Experiment overview + reproduction steps
├── configs/
│   ├── qlora_tier1.yaml        # QLoRA training hyperparameters
│   └── generation.yaml         # Data generation config (model, repos, counts)
├── src/
│   ├── __init__.py
│   ├── generate_data.py        # Synthetic data generation pipeline
│   ├── prepare_dataset.py      # Raw data → HuggingFace Dataset conversion
│   ├── train.py                # QLoRA fine-tuning script
│   ├── evaluate.py             # Evaluation harness (metrics + LLM-as-judge)
│   ├── bcp_render.py           # BCP CLI wrapper for rendering context
│   ├── markdown_baseline.py    # Equivalent markdown builder (mirrors SPEC_11)
│   └── ollama_client.py        # Ollama OpenAI-compatible API wrapper
├── data/
│   ├── raw/                    # Generated (context, response) pairs
│   │   ├── code_tasks/         # Code comprehension tasks
│   │   └── tool_tasks/         # Tool-use interpretation tasks
│   ├── processed/              # HuggingFace Dataset format (train/val/test splits)
│   └── repos/                  # Cloned source repos for context extraction
├── outputs/
│   ├── checkpoints/            # QLoRA adapter checkpoints
│   ├── eval/                   # Evaluation results (JSON + tables)
│   └── report/                 # Final comprehension parity report
└── scripts/
    ├── setup_env.sh            # DGX Spark environment setup
    ├── clone_repos.sh          # Clone source repos for data generation
    └── run_full_pipeline.sh    # End-to-end: generate → train → evaluate
```

### 2. Environment Setup

```bash
#!/usr/bin/env bash
# scripts/setup_env.sh
#
# Sets up the Python environment for BCP QLoRA experiments on DGX Spark.
#
# What this script does:
#   1. Creates an isolated Python virtual environment (.venv/)
#   2. Installs PyTorch with CUDA support (for GPU-accelerated training)
#   3. Installs HuggingFace ecosystem (transformers, peft, trl, datasets)
#   4. Installs bitsandbytes (enables 4-bit quantization for QLoRA)
#   5. Installs evaluation dependencies (ROUGE scoring)
#   6. Verifies the bcp CLI binary is available on PATH
#   7. Verifies Ollama is running and the data generation model is available
#
# Prerequisites:
#   - Python 3.11+ installed
#   - CUDA 12.x drivers installed (pre-installed on DGX Spark)
#   - Ollama running with qwen3-coder-next:q8_0 pulled
#   - bcp CLI built: cargo build --release -p bcp-cli
#
# Usage:
#   bash scripts/setup_env.sh
#   source .venv/bin/activate

set -euo pipefail

echo "=== Creating Python virtual environment ==="
python3 -m venv .venv
source .venv/bin/activate

echo "=== Installing PyTorch with CUDA 12.4 support ==="
# PyTorch is the deep learning framework that runs on the GPU.
# The --index-url points to NVIDIA CUDA 12.4 builds specifically.
pip install torch==2.5.* --index-url https://download.pytorch.org/whl/cu124

echo "=== Installing HuggingFace training ecosystem ==="
# transformers: loads pre-trained models (Llama 3.1 8B) and tokenizers
# peft: Parameter-Efficient Fine-Tuning — implements LoRA/QLoRA adapters
# bitsandbytes: 4-bit quantization (shrinks 8B model from ~16GB to ~5GB)
# trl: Transformer Reinforcement Learning — provides SFTTrainer for
#      supervised fine-tuning with built-in QLoRA support
# datasets: HuggingFace's data loading library (handles train/val/test splits)
# accelerate: abstracts GPU/multi-GPU setup, handles device placement
pip install transformers>=4.46.0
pip install peft>=0.13.0
pip install bitsandbytes>=0.44.0
pip install trl>=0.12.0
pip install datasets>=3.0.0
pip install accelerate>=1.0.0

echo "=== Installing data generation dependencies ==="
# openai: OpenAI-compatible Python client — used to talk to Ollama's
#         local API (Ollama exposes an OpenAI-compatible endpoint at
#         localhost:11434/v1). NOT used for OpenAI cloud API.
# pyyaml: reads our YAML config files (generation.yaml, qlora_tier1.yaml)
pip install openai>=1.0.0
pip install pyyaml>=6.0

echo "=== Installing evaluation dependencies ==="
# rouge-score: computes ROUGE-L metric (measures text overlap between
#              generated responses and reference responses)
# scikit-learn: general ML utilities used for classification metrics
pip install rouge-score>=0.1.2
pip install scikit-learn>=1.5.0

echo "=== Verifying bcp CLI ==="
# The bcp CLI is built from the Rust workspace. It must be on PATH
# for the data pipeline to encode/decode BCP payloads.
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
# Ollama runs as a service (usually in Docker on the DGX Spark).
# We use the 'loclaude models' command to verify it's accessible.
# The data generation script talks to Ollama via its OpenAI-compatible
# API at http://localhost:11434/v1
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
    print(f'Memory: {torch.cuda.get_device_properties(0).total_mem / 1e9:.1f} GB')
else:
    print('WARNING: CUDA not available. Training will be extremely slow on CPU.')
"

echo ""
echo "=== Setup complete ==="
echo "Activate with: source .venv/bin/activate"
```

```toml
# pyproject.toml
#
# Python project configuration for the BCP QLoRA experiment.
# Install with: pip install -e .
# Or just use the setup_env.sh script which installs dependencies directly.

[project]
name = "bcp-qlora"
version = "0.1.0"
requires-python = ">=3.11"
description = "QLoRA fine-tuning experiment to validate BCP comprehension parity"
dependencies = [
    "torch>=2.5.0",
    "transformers>=4.46.0",
    "peft>=0.13.0",
    "bitsandbytes>=0.44.0",
    "trl>=0.12.0",
    "datasets>=3.0.0",
    "accelerate>=1.0.0",
    "openai>=1.0.0",        # Used for Ollama's OpenAI-compatible API
    "pyyaml>=6.0",
    "rouge-score>=0.1.2",
    "scikit-learn>=1.5.0",
]

# These let you run scripts as: python -m src.generate_data (etc.)
[project.scripts]
generate-data = "src.generate_data:main"
prepare-dataset = "src.prepare_dataset:main"
train = "src.train:main"
evaluate = "src.evaluate:main"
```

### 3. Data Generation Pipeline

The pipeline generates (context, task, response) triples where context is real code
from open-source repositories, rendered through BCP's encode → decode → render
pipeline. Response generation uses Qwen3 Coder Next (Q8) running locally via Ollama
— zero API cost.

```
Data generation flow:

  Source repos ──► Extract code files ──► Build BCP payload ──► Render in 3 modes
       │                                      │                       │
       │                                      │                  ┌────┴────┐
       ▼                                      ▼                  ▼         ▼
  Clone ~15 repos              BCP encoder (via bcp-cli)    BCP Minimal  BCP XML
  Filter by language                  │                     BCP Markdown
  Select interesting files            │                          │
       │                              ▼                          │
       ▼                     Build markdown baseline             │
  Generate task prompts       (naive + realistic)                │
  (code review, bug fix,             │                           │
   explain function,                 ▼                           │
   interpret tool output)   ┌────────────────────────┐           │
       │                    │  Ollama API call        │◄──────────┘
       │                    │  (qwen3-coder-next:q8_0)│
       ▼                    │  generate ideal response│
  Attach task to context    │  for each context+task  │
                            └───────────┬────────────┘
                                        │
                                        ▼
                                 Save as JSONL:
                                 {
                                   "context_bcp_minimal": "...",
                                   "context_bcp_xml": "...",
                                   "context_bcp_markdown": "...",
                                   "context_raw_markdown": "...",
                                   "task": "...",
                                   "response": "...",
                                   "metadata": { ... }
                                 }
```

#### 3.1 Ollama Client

The data generation pipeline talks to Ollama through its OpenAI-compatible API.
This means the same client code works for Ollama (local, free) or any
OpenAI-compatible cloud API (if you ever want to switch).

```python
# src/ollama_client.py

"""
Ollama client for response generation using the OpenAI-compatible API.

=== What is Ollama? ===
Ollama is a local model server that runs LLMs on your machine. On the DGX Spark,
it runs in a Docker container and exposes an API at http://localhost:11434.

=== Why OpenAI-compatible API? ===
Ollama provides an endpoint at /v1/chat/completions that matches the OpenAI API
format exactly. This means we can use the standard `openai` Python package to
talk to our local models. If you ever want to switch to a cloud API (OpenAI,
Anthropic via proxy, etc.), you only change the base_url and api_key.

=== Available models on this DGX Spark ===
  qwen3-coder-next:q8_0  (79GB) — Primary: frontier code model, Q8 quantization
  devstral-2:123b         (70GB) — Alternative: strong general + code
  gpt-oss:120b            (61GB) — Alternative: general purpose
  qwen2.5-coder:32b      (18GB) — Lightweight fallback

Usage:
    client = OllamaClient(model="qwen3-coder-next:q8_0")
    response = client.generate("Review this code...", temperature=0.3)
"""

from openai import OpenAI


class OllamaClient:
    """
    Wrapper around Ollama's OpenAI-compatible API.

    The DGX Spark runs Ollama in Docker, typically accessible at localhost:11434.
    The /v1 endpoint speaks the OpenAI chat completions protocol.

    Attributes:
        client: The OpenAI Python client, pointed at Ollama's local server.
        model: Which Ollama model to use (must be already pulled/cached).
    """

    def __init__(
        self,
        model: str = "qwen3-coder-next:q8_0",
        base_url: str = "http://localhost:11434/v1",
    ):
        # The OpenAI client doesn't need a real API key for Ollama,
        # but the library requires _something_ in the field.
        self.client = OpenAI(base_url=base_url, api_key="ollama")
        self.model = model

    def generate(
        self,
        prompt: str,
        system_prompt: str = "",
        temperature: float = 0.3,
        max_tokens: int = 2048,
    ) -> str:
        """
        Generate a response from the local model.

        Args:
            prompt: The user message (context + task).
            system_prompt: Optional system instructions for the model.
            temperature: Controls randomness. 0.0 = deterministic, 1.0 = creative.
                         We use 0.3 for consistent, correct code responses.
            max_tokens: Maximum length of the generated response in tokens.

        Returns:
            The model's response text.

        Raises:
            openai.APIConnectionError: If Ollama is not running.
            openai.APIError: If the model is not available.
        """
        messages = []
        if system_prompt:
            messages.append({"role": "system", "content": system_prompt})
        messages.append({"role": "user", "content": prompt})

        response = self.client.chat.completions.create(
            model=self.model,
            messages=messages,
            temperature=temperature,
            max_tokens=max_tokens,
        )

        return response.choices[0].message.content

    def is_available(self) -> bool:
        """
        Check if Ollama is running and the model is accessible.
        Call this before starting a long data generation run.
        """
        try:
            self.client.models.list()
            return True
        except Exception:
            return False
```

#### 3.2 Source Repository Selection

```yaml
# configs/generation.yaml
#
# Configuration for the synthetic data generation pipeline.
# This file controls which repos to clone, what files to extract,
# how many examples to generate, and which model to use.

# ─── Source Repositories ───
# These repos provide real code for training data.
# The pipeline clones them, extracts source files, and uses those
# files as context in the training examples.
source_repos:

  # Rust projects — systems programming, CLI tools
  - url: "https://github.com/BurntSushi/ripgrep"
    languages: ["rust"]
    max_files: 20
    description: "Fast regex search tool — good for CLI patterns, file I/O"

  - url: "https://github.com/sharkdp/hyperfine"
    languages: ["rust"]
    max_files: 15
    description: "CLI benchmarking tool — command-line arg parsing, statistics"

  - url: "https://github.com/Schniz/fnm"
    languages: ["rust"]
    max_files: 15
    description: "Fast Node.js version manager — async I/O, platform abstractions"

  # TypeScript projects — web libraries, type systems
  - url: "https://github.com/colinhacks/zod"
    languages: ["typescript"]
    max_files: 15
    description: "Schema validation library — complex type system, generics"

  - url: "https://github.com/facebook/react"
    languages: ["typescript", "javascript"]
    max_files: 20
    description: "UI framework — component patterns, hooks, reconciler internals"

  - url: "https://github.com/nodejs/undici"
    languages: ["typescript", "javascript"]
    max_files: 15
    description: "HTTP client — networking, streams, protocol implementation"

  # Python projects — web frameworks, validation
  - url: "https://github.com/tiangolo/fastapi"
    languages: ["python"]
    max_files: 15
    description: "Web framework — dependency injection, OpenAPI, async handlers"

  - url: "https://github.com/pydantic/pydantic"
    languages: ["python"]
    max_files: 15
    description: "Data validation — complex types, serialization, metaclasses"

  # Go projects — systems programming, networking
  - url: "https://github.com/junegunn/fzf"
    languages: ["go"]
    max_files: 15
    description: "Fuzzy finder — TUI rendering, algorithm implementation"

  - url: "https://github.com/containerd/nerdctl"
    languages: ["go"]
    max_files: 15
    description: "Container CLI — Docker-compatible, complex CLI patterns"

  - url: "https://github.com/charmbracelet/bubbletea"
    languages: ["go"]
    max_files: 15
    description: "TUI framework — Elm architecture in Go, event handling"

  # BCP's own codebase — meta: train on the thing we're testing
  - url: "https://github.com/nicholasgalante1997/bit-context-protocol"
    languages: ["rust"]
    max_files: 20
    description: "BCP reference implementation — our own codebase"

# ─── File Selection Filters ───
# Controls which source files are included. Files outside these
# bounds are skipped to keep training examples focused.
file_selection:
  min_lines: 20          # Skip trivial files (just imports, empty modules)
  max_lines: 500         # Skip huge files (won't fit in model context window)
  skip_patterns:         # Glob patterns to exclude
    - "**/test*"         # Test files (we want production code)
    - "**/vendor/**"     # Vendored dependencies
    - "**/*.generated.*" # Auto-generated code
    - "**/node_modules/**"
    - "**/target/**"     # Rust build artifacts
    - "**/.git/**"

# ─── Task Generation ───
# Controls how many examples to generate and what types of tasks.
task_generation:
  target_count: 5000     # Total (context, task, response) triples to generate
  split:                 # How to divide examples into train/validation/test
    train: 0.8           # 4000 examples — used for QLoRA fine-tuning
    val: 0.1             # 500 examples  — used to monitor training (eval_loss)
    test: 0.1            # 500 examples  — used for final comprehension evaluation

  # Task types define what we ask the model to do with the code context.
  # Each has a weight (probability of being selected) and a prompt template.
  task_types:
    - type: "code_review"
      weight: 0.25
      description: "Ask the model to review code for bugs and improvements"
      prompt_template: |
        Review the following code and identify any bugs, performance issues,
        or improvements. Be specific about line numbers and provide fixes.

    - type: "explain_function"
      weight: 0.20
      description: "Ask the model to explain what code does"
      prompt_template: |
        Explain what this code does. Describe the algorithm, key data
        structures, and any edge cases it handles.

    - type: "bug_fix"
      weight: 0.20
      description: "Ask the model to find and fix a bug"
      prompt_template: |
        The following code has a bug. Identify the bug and provide a
        corrected version with an explanation of what was wrong.

    - type: "add_feature"
      weight: 0.15
      description: "Ask the model to add error handling or a small feature"
      prompt_template: |
        Add error handling to this code. The function should return
        descriptive errors instead of panicking or silently failing.

    - type: "tool_result_interpretation"
      weight: 0.20
      description: "Ask the model to interpret tool output (search, tests, lints)"
      prompt_template: |
        Given the tool output below, explain what was found and suggest
        next steps. The tool output includes search results, diagnostics,
        or test output.

# ─── Model Configuration ───
# Which model to use for generating training data responses.
# Uses Ollama's OpenAI-compatible API (http://localhost:11434/v1).
model:
  provider: "ollama"                      # "ollama" for local, "openai" for cloud
  name: "qwen3-coder-next:q8_0"          # Model name as shown in `ollama list`
  base_url: "http://localhost:11434/v1"   # Ollama's OpenAI-compatible endpoint
  max_tokens: 2048                        # Max response length
  temperature: 0.3                        # Low temp = consistent, correct responses
  system_prompt: |
    You are an expert software engineer. Provide clear, accurate, and
    actionable responses to coding tasks. When reviewing code, cite
    specific line numbers. When fixing bugs, explain the root cause.
    When explaining code, be concise but thorough.

# ─── Judge Model Configuration ───
# Used during evaluation (not data generation). A DIFFERENT model
# from the data generator to avoid self-evaluation bias.
judge_model:
  provider: "ollama"
  name: "devstral-2:123b"                # Different from generator model
  base_url: "http://localhost:11434/v1"
  max_tokens: 16
  temperature: 0.0                        # Deterministic judging
```

#### 3.3 Data Generation Script

```python
# src/generate_data.py

"""
Synthetic data generation for BCP comprehension fine-tuning.

=== What This Script Does ===
This is the first step in the pipeline. It creates the training data that will
be used to fine-tune Llama 3.1 8B. The output is a set of JSONL files where
each line is one training example containing:
  - The same code context rendered in 4 different formats (BCP Minimal, XML,
    Markdown, and raw markdown)
  - A task prompt (e.g., "review this code", "explain this function")
  - An ideal response generated by Qwen3 Coder via Ollama

=== How It Works ===
1. Clones open-source repos (ripgrep, zod, fastapi, etc.)
2. Extracts source files that match our size/language filters
3. For each file group:
   a. Builds a BCP payload using the `bcp encode` CLI command
   b. Renders the payload in all 3 BCP modes (Minimal, XML, Markdown)
   c. Also builds a traditional markdown baseline for comparison
4. Picks a random task type (code review, bug fix, explain, etc.)
5. Sends (context + task) to Qwen3 Coder via Ollama to generate a response
6. Saves everything as a JSONL line with all 4 context variants

=== Why All 4 Formats? ===
We generate all 4 context variants for the SAME code + task, so we can later
train on one format (BCP Minimal) and compare against another (raw markdown).
The response is generated once (using BCP XML, the richest format) and reused
across all variants — this ensures the "correct answer" is format-independent.

=== Prerequisites ===
  - bcp CLI on PATH (cargo build --release -p bcp-cli)
  - Ollama running with qwen3-coder-next:q8_0
  - Source repos cloned (run scripts/clone_repos.sh first)

Usage:
  python -m src.generate_data --config configs/generation.yaml --output data/raw/
  python -m src.generate_data --config configs/generation.yaml --output data/raw/ --limit 50
"""

import argparse
import json
import subprocess
import tempfile
import random
import uuid
import time
from pathlib import Path

import yaml

from src.ollama_client import OllamaClient
from src.bcp_render import render_files_as_bcp
from src.markdown_baseline import build_naive_markdown


def load_config(config_path: str) -> dict:
    """Load and return the YAML configuration file."""
    return yaml.safe_load(Path(config_path).read_text())


def discover_source_files(repos_dir: Path, repo_config: dict, file_config: dict) -> list[dict]:
    """
    Walk a cloned repo directory and find source files matching our filters.

    Args:
        repos_dir: Path to data/repos/ where cloned repos live.
        repo_config: One entry from source_repos (url, languages, max_files).
        file_config: The file_selection config (min_lines, max_lines, skip_patterns).

    Returns:
        List of dicts, each with keys: path, language, content, line_count.
    """
    # Extract repo name from URL: "https://github.com/BurntSushi/ripgrep" → "ripgrep"
    repo_name = repo_config["url"].rstrip("/").split("/")[-1]
    repo_path = repos_dir / repo_name

    if not repo_path.exists():
        print(f"  WARNING: Repo not found at {repo_path}, skipping")
        return []

    # Map file extensions to language names
    ext_to_lang = {
        ".rs": "rust", ".ts": "typescript", ".tsx": "typescript",
        ".js": "javascript", ".jsx": "javascript",
        ".py": "python", ".go": "go",
    }

    # Invert: which extensions should we look for?
    target_exts = set()
    for ext, lang in ext_to_lang.items():
        if lang in repo_config["languages"]:
            target_exts.add(ext)

    files = []
    for file_path in sorted(repo_path.rglob("*")):
        # Skip directories
        if not file_path.is_file():
            continue

        # Skip files that don't match our target languages
        if file_path.suffix not in target_exts:
            continue

        # Skip files matching exclusion patterns
        rel_path = str(file_path.relative_to(repo_path))
        if any(file_path.match(pat) for pat in file_config.get("skip_patterns", [])):
            continue

        # Read and filter by line count
        try:
            content = file_path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, PermissionError):
            continue

        line_count = content.count("\n") + 1
        if line_count < file_config["min_lines"] or line_count > file_config["max_lines"]:
            continue

        files.append({
            "path": rel_path,
            "language": ext_to_lang[file_path.suffix],
            "content": content,
            "line_count": line_count,
        })

        # Respect max_files limit per repo
        if len(files) >= repo_config["max_files"]:
            break

    return files


def select_task(task_types: list[dict]) -> dict:
    """
    Randomly select a task type based on configured weights.

    The weights don't need to sum to 1.0 — random.choices normalizes them.
    For example, weights [0.25, 0.20, 0.20, 0.15, 0.20] mean code_review
    is slightly more likely to be selected.
    """
    weights = [t["weight"] for t in task_types]
    return random.choices(task_types, weights=weights, k=1)[0]


def generate_example(
    client: OllamaClient,
    files: list[dict],
    task: dict,
    config: dict,
) -> dict | None:
    """
    Generate one training example.

    Steps:
      1. Render the files through BCP in all 3 modes
      2. Build a markdown baseline
      3. Send (BCP XML context + task) to the model for response generation
         (we use XML because it's the richest format — best for the model
          to understand the context and generate a good response)
      4. Package everything into a JSONL-compatible dict

    Returns:
        A dict with all context variants + task + response, or None if
        generation failed.
    """
    try:
        # Step 1: Render through BCP in all modes
        bcp_minimal = render_files_as_bcp(files, mode="minimal")
        bcp_xml = render_files_as_bcp(files, mode="xml")
        bcp_markdown = render_files_as_bcp(files, mode="markdown")

        # Step 2: Build markdown baseline
        raw_markdown = build_naive_markdown(files)

        # Step 3: Generate response using the richest context format
        prompt = f"{bcp_xml}\n\n{task['prompt_template']}"
        response = client.generate(
            prompt=prompt,
            system_prompt=config["model"]["system_prompt"],
            temperature=config["model"]["temperature"],
            max_tokens=config["model"]["max_tokens"],
        )

        # Step 4: Package the example
        return {
            "id": str(uuid.uuid4()),
            "context_bcp_minimal": bcp_minimal,
            "context_bcp_xml": bcp_xml,
            "context_bcp_markdown": bcp_markdown,
            "context_raw_markdown": raw_markdown,
            "task": task["prompt_template"],
            "task_type": task["type"],
            "response": response,
            "metadata": {
                "source_files": [f["path"] for f in files],
                "languages": list(set(f["language"] for f in files)),
                "block_types": ["code"],  # Extended for tool-use tasks
                "generator_model": config["model"]["name"],
                "tier": 1,
            },
        }
    except Exception as e:
        print(f"  ERROR generating example: {e}")
        return None


def main():
    parser = argparse.ArgumentParser(
        description="Generate synthetic training data for BCP QLoRA fine-tuning."
    )
    parser.add_argument(
        "--config", required=True,
        help="Path to generation.yaml config file"
    )
    parser.add_argument(
        "--output", required=True,
        help="Output directory for JSONL files (e.g., data/raw/)"
    )
    parser.add_argument(
        "--limit", type=int, default=None,
        help="Generate at most N examples (for testing the pipeline)"
    )
    args = parser.parse_args()

    config = load_config(args.config)
    output_dir = Path(args.output)
    output_dir.mkdir(parents=True, exist_ok=True)

    # Initialize Ollama client
    client = OllamaClient(
        model=config["model"]["name"],
        base_url=config["model"]["base_url"],
    )
    if not client.is_available():
        print("ERROR: Ollama is not running or model is not available.")
        print(f"Expected model: {config['model']['name']}")
        print("Start Ollama and pull the model, then retry.")
        return

    # Discover source files from all repos
    repos_dir = Path("data/repos")
    all_files_by_repo = {}
    for repo_cfg in config["source_repos"]:
        repo_name = repo_cfg["url"].rstrip("/").split("/")[-1]
        files = discover_source_files(repos_dir, repo_cfg, config["file_selection"])
        print(f"  {repo_name}: {len(files)} files found")
        all_files_by_repo[repo_name] = files

    # Main generation loop
    target = args.limit or config["task_generation"]["target_count"]
    output_file = output_dir / "examples.jsonl"
    generated = 0

    print(f"\nGenerating {target} examples → {output_file}")
    print(f"Using model: {config['model']['name']}")

    with open(output_file, "a") as f:
        while generated < target:
            # Pick a random repo, then 1-3 random files from it
            repo_name = random.choice(list(all_files_by_repo.keys()))
            repo_files = all_files_by_repo[repo_name]
            if not repo_files:
                continue

            num_files = random.randint(1, min(3, len(repo_files)))
            selected_files = random.sample(repo_files, num_files)

            # Pick a random task type
            task = select_task(config["task_generation"]["task_types"])

            # Generate the example
            example = generate_example(client, selected_files, task, config)
            if example is None:
                continue

            # Write to JSONL (one JSON object per line)
            f.write(json.dumps(example) + "\n")
            generated += 1

            if generated % 50 == 0:
                print(f"  {generated}/{target} examples generated")

    print(f"\nDone. {generated} examples written to {output_file}")


if __name__ == "__main__":
    main()
```

#### 3.4 BCP Render Helper

```python
# src/bcp_render.py

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
    # Each block in the manifest describes one BCP block to encode.
    # See: bcp encode --help, or SPEC_09 for the manifest format.
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

    # Write manifest to a temp file. We use delete=False because
    # we need the file to exist when bcp reads it.
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".json", delete=False
    ) as manifest_file:
        json.dump(manifest, manifest_file)
        manifest_path = manifest_file.name

    # Generate a matching .bcp file path
    bcp_path = manifest_path.replace(".json", ".bcp")

    try:
        # Step 1: Encode the manifest into a .bcp binary file
        subprocess.run(
            ["bcp", "encode", manifest_path, "-o", bcp_path],
            check=True,           # Raise exception on non-zero exit
            capture_output=True,  # Don't print to our stdout
        )

        # Step 2: Decode the .bcp file into text in the specified mode
        result = subprocess.run(
            ["bcp", "decode", bcp_path, "--mode", mode],
            check=True,
            capture_output=True,
            text=True,            # Return stdout as string, not bytes
        )

        return result.stdout

    finally:
        # Clean up temp files
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
                See configs/generation.yaml for the schema.
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
```

#### 3.5 Markdown Baseline Builder

```python
# src/markdown_baseline.py

"""
Markdown baseline builders for comparison with BCP-rendered output.

=== Why Do We Need This? ===
To measure whether BCP improves comprehension, we need a baseline: "how well
does the model understand the SAME code when presented as traditional markdown?"
This module builds that markdown baseline.

=== Two Baselines ===
We build two markdown variants to bracket the range of real-world formatting:

1. Naive Markdown: Triple-backtick code fences with language tags.
   This is how most tools dump code into prompts.
   ```rust
   // src/main.rs
   fn main() { ... }
   ```

2. Realistic Agent Markdown: XML-style tags + JSON envelopes, mimicking
   how Claude Code actually formats context (more verbose = more tokens).

The BCP Minimal format should use fewer tokens than BOTH baselines while
preserving the same semantic content.
"""


def build_naive_markdown(files: list[dict]) -> str:
    """
    Build naive markdown — triple backticks with language tags.

    This is the most common way tools present code to models.
    Each file gets a fenced code block with a comment showing the path.

    Args:
        files: List of file dicts with keys: language, path, content.

    Returns:
        A string of markdown-formatted code blocks.
    """
    parts = []
    for f in files:
        lang = f["language"]
        path = f["path"]
        content = f["content"]
        # The // comment prefix works for most languages.
        # Python would use #, but for token counting purposes
        # the exact comment syntax doesn't meaningfully affect results.
        parts.append(f"```{lang}\n// {path}\n{content}\n```\n")
    return "\n".join(parts)


def build_realistic_markdown(files: list[dict]) -> str:
    """
    Build realistic agent markdown — mimics how Claude Code formats context.

    This includes XML-style tags, full paths repeated in multiple positions,
    and language identifiers in both the tag and the code fence. This is
    intentionally verbose because that's what models actually receive today.

    Args:
        files: List of file dicts with keys: language, path, content.

    Returns:
        A string of XML-tagged, fenced code blocks.
    """
    parts = ["<context>\n"]
    for f in files:
        lang = f["language"]
        path = f["path"]
        content = f["content"]
        parts.append(
            f'<source path="{path}" language="{lang}">\n'
            f"```{lang}\n{content}\n```\n"
            f"</source>\n\n"
        )
    parts.append("</context>\n")
    return "".join(parts)
```

#### 3.6 Tool-Use Task Generation

Tool-use tasks include BCP payloads with mixed block types: code, tool results,
conversation turns, and structured data.

```
Tool-use task examples:

  Task: "interpret_search_results"
  Context blocks:
    - CODE: src/pool.rs (connection pool implementation)
    - TOOL_RESULT: ripgrep output ("3 matches for 'timeout' in 2 files")
    - CONVERSATION: user asks "Why is the connection timing out?"
  Expected response: Identify timeout configuration, suggest fixes

  Task: "diagnose_test_failure"
  Context blocks:
    - CODE: src/handler.rs (HTTP handler)
    - TOOL_RESULT: cargo test output (1 failure, assertion error)
    - STRUCTURED_DATA: config.toml (app configuration)
  Expected response: Identify root cause from test output, propose fix

  Task: "review_diff"
  Context blocks:
    - CODE: src/auth.rs (before changes)
    - DIFF: proposed changes to auth.rs
    - TOOL_RESULT: clippy warnings on the diff
  Expected response: Review the diff, address clippy warnings, suggest improvements
```

### 4. Dataset Preparation

Convert raw JSONL into HuggingFace `Dataset` format with train/val/test splits.

```python
# src/prepare_dataset.py

"""
Convert raw JSONL to HuggingFace Dataset with chat-template formatting.

=== What This Script Does ===
Takes the JSONL files produced by generate_data.py and converts them into
HuggingFace Dataset objects that the training script (train.py) can load.

=== What is a HuggingFace Dataset? ===
It's a standardized data format used by the transformers/trl ecosystem.
Think of it like a typed DataFrame — it stores your training examples in
an efficient columnar format with train/validation/test splits. The
SFTTrainer (used in train.py) expects data in this format.

=== Chat Template Format ===
Each training example is formatted as a chat conversation:
  [
    {"role": "user",      "content": "<rendered context>\\n\\n<task prompt>"},
    {"role": "assistant", "content": "<ideal response>"}
  ]

The SFTTrainer uses the model's chat template (Llama 3.1's template) to
convert this into the exact token sequence the model expects during training.

=== Four Dataset Variants ===
We create one Dataset per context format. This lets us train separate
adapters for each format (or train on one and evaluate on others):
  - context_bcp_minimal:  BCP Minimal rendered context (fewest tokens)
  - context_bcp_xml:      BCP XML rendered context (richest structure)
  - context_bcp_markdown:  BCP Markdown rendered context
  - context_raw_markdown:  Traditional markdown (baseline for comparison)

Usage:
  python -m src.prepare_dataset --input data/raw/ --output data/processed/
"""

from datasets import Dataset, DatasetDict
from pathlib import Path
import argparse
import json


# These are the context format variants we generate datasets for.
# Each corresponds to a key in the JSONL files produced by generate_data.py.
CONTEXT_VARIANTS = [
    "context_bcp_minimal",
    "context_bcp_xml",
    "context_bcp_markdown",
    "context_raw_markdown",
]


def format_chat(context: str, task: str, response: str) -> dict:
    """
    Format one example as a chat conversation for SFTTrainer.

    The SFTTrainer expects a "messages" field containing a list of
    role/content dicts. It uses the model's chat template to convert
    this into tokens.

    Args:
        context: The rendered code context (in one of the 4 formats).
        task: The task prompt (e.g., "Review this code...").
        response: The ideal response generated by Qwen3 Coder.

    Returns:
        Dict with a "messages" key containing the chat conversation.
    """
    return {
        "messages": [
            {"role": "user", "content": f"{context}\n\n{task}"},
            {"role": "assistant", "content": response},
        ]
    }


def load_raw_data(input_dir: Path) -> list[dict]:
    """
    Load all JSONL files from the input directory.

    JSONL = JSON Lines format. Each line in the file is one complete
    JSON object (one training example). This format is standard in
    ML pipelines because it's easy to stream and append to.
    """
    examples = []
    for jsonl_file in sorted(input_dir.rglob("*.jsonl")):
        print(f"  Loading {jsonl_file}")
        with open(jsonl_file) as f:
            for line_num, line in enumerate(f, 1):
                line = line.strip()
                if not line:
                    continue
                try:
                    examples.append(json.loads(line))
                except json.JSONDecodeError as e:
                    print(f"  WARNING: Skipping malformed line {line_num} in {jsonl_file}: {e}")
    print(f"  Loaded {len(examples)} total examples")
    return examples


def build_datasets(examples: list[dict]) -> dict[str, DatasetDict]:
    """
    Build one DatasetDict per context variant, each with train/val/test splits.

    A DatasetDict is a dict of Datasets keyed by split name:
      {"train": Dataset, "validation": Dataset, "test": Dataset}

    The split is deterministic: first 80% = train, next 10% = val, last 10% = test.
    This means running the script twice on the same input produces identical splits.
    """
    datasets = {}

    for variant in CONTEXT_VARIANTS:
        print(f"  Building dataset for variant: {variant}")

        formatted = []
        for ex in examples:
            if variant not in ex:
                print(f"  WARNING: Example {ex.get('id', '?')} missing {variant}, skipping")
                continue
            formatted.append(
                format_chat(ex[variant], ex["task"], ex["response"])
            )

        # Deterministic split by position (no shuffling — shuffle during training)
        n = len(formatted)
        train_end = int(n * 0.8)
        val_end = int(n * 0.9)

        datasets[variant] = DatasetDict({
            "train": Dataset.from_list(formatted[:train_end]),
            "validation": Dataset.from_list(formatted[train_end:val_end]),
            "test": Dataset.from_list(formatted[val_end:]),
        })

        print(f"    train: {train_end}, val: {val_end - train_end}, test: {n - val_end}")

    return datasets


def main():
    parser = argparse.ArgumentParser(
        description="Convert raw JSONL to HuggingFace Datasets for training."
    )
    parser.add_argument(
        "--input", required=True,
        help="Directory containing JSONL files from generate_data.py"
    )
    parser.add_argument(
        "--output", required=True,
        help="Output directory for HuggingFace Datasets"
    )
    args = parser.parse_args()

    print("Loading raw data...")
    examples = load_raw_data(Path(args.input))

    print("\nBuilding datasets...")
    datasets = build_datasets(examples)

    print("\nSaving to disk...")
    output_dir = Path(args.output)
    for variant, dataset_dict in datasets.items():
        save_path = output_dir / variant
        dataset_dict.save_to_disk(str(save_path))
        print(f"  Saved {variant} → {save_path}")

    print("\nDone.")


if __name__ == "__main__":
    main()
```

#### 4.1 Dataset Schema (JSONL)

Each line in the raw JSONL files follows this schema. The schema is designed to be
extensible for Tier 2 (hybrid tokens) and Tier 3 (binary) by adding new context
variant fields.

```json
{
  "id": "uuid-v4",
  "context_bcp_minimal": "--- src/main.rs [rust] ---\nfn main() { ... }\n",
  "context_bcp_xml": "<code lang=\"rust\" path=\"src/main.rs\">\nfn main() { ... }\n</code>",
  "context_bcp_markdown": "```rust\n// src/main.rs\nfn main() { ... }\n```",
  "context_raw_markdown": "```rust\n// src/main.rs\nfn main() { ... }\n```\n\n### Tool Result (ripgrep):\n...",
  "task": "Review the following code and identify any bugs...",
  "task_type": "code_review",
  "response": "The code has a potential issue on line 42 where...",
  "metadata": {
    "source_files": ["src/main.rs", "src/search.rs"],
    "languages": ["rust"],
    "block_types": ["code", "tool_result", "conversation"],
    "generator_model": "qwen3-coder-next:q8_0",
    "tier": 1
  }
}
```

### 5. QLoRA Training

```yaml
# configs/qlora_tier1.yaml
#
# QLoRA training hyperparameters for BCP comprehension fine-tuning.
#
# === What is QLoRA? ===
# QLoRA = Quantized Low-Rank Adaptation. It's a memory-efficient fine-tuning
# technique that:
#   1. Quantizes the base model to 4-bit (shrinks 16GB model to ~5GB)
#   2. Adds small trainable "adapter" matrices (LoRA) to each layer
#   3. Only trains the adapter weights (0.5-2% of total parameters)
#   4. At inference, the adapter is merged back into the base model
#
# This lets us fine-tune an 8B parameter model on a single GPU using
# ~10-15GB of memory total, well within the DGX Spark's 128GB.
#
# === Key Parameters Explained ===
# - r (rank): Size of the low-rank adapter matrices. Higher = more capacity
#   but more memory. 16 is the standard starting point for 7-13B models.
# - lora_alpha: Scaling factor. The effective learning rate for LoRA is
#   (alpha/r) * base_lr. With alpha=32, r=16, scaling = 2.0x.
# - target_modules: Which model layers get LoRA adapters. We target all
#   attention projections (q,k,v,o) AND the MLP layers (gate, up, down)
#   for maximum adaptation capacity.
# - bits=4: Use NormalFloat4 quantization (the "Q" in "QLoRA"). This is
#   a special 4-bit format optimized for normally-distributed weights.

# ─── Model ───
base_model: "meta-llama/Llama-3.1-8B-Instruct"
model_dtype: "bfloat16"  # bf16 is standard on modern GPUs (Blackwell, Ampere+)

# ─── QLoRA Configuration ───
# These settings control how the base model is quantized (compressed).
qlora:
  bits: 4                         # 4-bit quantization (NF4)
  quant_type: "nf4"               # NormalFloat4 — best for QLoRA
  double_quant: true              # Also quantize the quantization constants
  compute_dtype: "bfloat16"       # Do math in bf16 (fast + precise enough)

# ─── LoRA Configuration ───
# These settings control the trainable adapter matrices.
lora:
  r: 16                           # Rank of adapter matrices (capacity knob)
  lora_alpha: 32                  # Scaling factor (alpha/r = 2.0)
  lora_dropout: 0.05              # Small dropout for regularization
  target_modules:                 # Which layers get adapters:
    - "q_proj"                    #   Query projection (attention)
    - "k_proj"                    #   Key projection (attention)
    - "v_proj"                    #   Value projection (attention)
    - "o_proj"                    #   Output projection (attention)
    - "gate_proj"                 #   Gate projection (MLP / feed-forward)
    - "up_proj"                   #   Up projection (MLP)
    - "down_proj"                 #   Down projection (MLP)
  bias: "none"                    # Don't add bias terms to adapters
  task_type: "CAUSAL_LM"          # Causal language modeling (next-token prediction)

# ─── Training ───
# Standard training hyperparameters.
training:
  num_epochs: 3                   # Full passes over the training data
  per_device_train_batch_size: 4  # Examples processed per GPU per step
  gradient_accumulation_steps: 4  # Accumulate 4 mini-batches before updating
                                  # Effective batch size = 4 * 4 = 16
  learning_rate: 2.0e-4           # 0.0002 — standard for QLoRA
  lr_scheduler_type: "cosine"     # Cosine decay (smooth LR reduction)
  warmup_ratio: 0.03              # Warm up LR for first 3% of steps
  weight_decay: 0.001             # L2 regularization (prevents overfitting)
  max_seq_length: 4096            # Max tokens per example (code + response)
  bf16: true                      # Use bf16 mixed precision
  gradient_checkpointing: true    # Trade compute for memory (slower but less RAM)

# ─── Logging & Checkpointing ───
logging:
  output_dir: "outputs/checkpoints"
  logging_steps: 10               # Print loss every 10 steps
  save_steps: 100                 # Save checkpoint every 100 steps
  eval_steps: 100                 # Evaluate on validation set every 100 steps
  save_total_limit: 3             # Keep only the 3 most recent checkpoints

# ─── Evaluation During Training ───
# Monitors validation loss to detect overfitting.
eval:
  eval_strategy: "steps"          # Evaluate every N steps (not every epoch)
  metric_for_best_model: "eval_loss"  # Lower val loss = better model
```

```python
# src/train.py

"""
QLoRA fine-tuning for BCP comprehension validation.

=== What This Script Does ===
Loads Llama 3.1 8B Instruct, quantizes it to 4-bit, adds LoRA adapter
layers, and fine-tunes on BCP-rendered coding context. The result is a
small adapter checkpoint (~100-200MB) that modifies the base model's
behavior when loaded.

=== How QLoRA Fine-Tuning Works (step by step) ===
1. Load the base model (Llama 3.1 8B) in 4-bit quantized form (~5GB)
2. Freeze all base model weights (they don't change during training)
3. Insert small trainable LoRA adapter matrices into each target layer
4. For each training batch:
   a. Feed the input through the model (forward pass)
   b. Compute loss: how far off is the model's prediction from the target?
   c. Compute gradients: which adapter weights should change, and how much?
   d. Update adapter weights (backward pass + optimizer step)
5. After training, save only the adapter weights (not the full model)

=== Memory Budget on DGX Spark (128GB unified) ===
  Base model (4-bit):     ~5 GB
  LoRA adapters:          ~0.2 GB
  Optimizer states:       ~2-4 GB
  Activations + batch:    ~2-4 GB
  ─────────────────────────────
  Total:                  ~10-15 GB  (well within 128GB)

=== What The Output Is ===
The output is a LoRA adapter checkpoint directory containing:
  - adapter_model.safetensors  (the trained weights, ~100-200MB)
  - adapter_config.json        (LoRA configuration)
These are loaded on top of the base model at inference time.

Usage:
  python -m src.train \\
    --config configs/qlora_tier1.yaml \\
    --dataset data/processed/context_bcp_minimal \\
    --output outputs/checkpoints/bcp_minimal

  # Train on a different context format:
  python -m src.train \\
    --config configs/qlora_tier1.yaml \\
    --dataset data/processed/context_raw_markdown \\
    --variant raw_markdown \\
    --output outputs/checkpoints/raw_markdown

  # Quick smoke test (10 steps only):
  python -m src.train \\
    --config configs/qlora_tier1.yaml \\
    --dataset data/processed/context_bcp_minimal \\
    --output outputs/checkpoints/smoke_test \\
    --max-steps 10
"""

import argparse
from pathlib import Path

import torch
import yaml
from datasets import load_from_disk
from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training
from transformers import (
    AutoModelForCausalLM,
    AutoTokenizer,
    BitsAndBytesConfig,
    TrainingArguments,
)
from trl import SFTTrainer


def load_config(path: str) -> dict:
    """Load YAML config file and return as dict."""
    return yaml.safe_load(Path(path).read_text())


def build_quantization_config(qlora_cfg: dict) -> BitsAndBytesConfig:
    """
    Build the bitsandbytes config for 4-bit quantization.

    This tells HuggingFace how to load the model in quantized form:
      - load_in_4bit: Use 4-bit weights (instead of 16-bit)
      - bnb_4bit_quant_type: "nf4" = NormalFloat4, optimized for normal distributions
      - bnb_4bit_use_double_quant: Also quantize the quantization constants
      - bnb_4bit_compute_dtype: Do the actual math in bf16 (4-bit is storage only)
    """
    return BitsAndBytesConfig(
        load_in_4bit=True,
        bnb_4bit_quant_type=qlora_cfg["quant_type"],
        bnb_4bit_use_double_quant=qlora_cfg["double_quant"],
        bnb_4bit_compute_dtype=getattr(torch, qlora_cfg["compute_dtype"]),
    )


def build_lora_config(lora_cfg: dict) -> LoraConfig:
    """
    Build the LoRA (Low-Rank Adaptation) config.

    LoRA inserts small trainable matrices into specific layers of the model.
    Instead of training all 8 billion parameters, we train ~50 million
    adapter parameters (0.6% of total) — much faster, much less memory.
    """
    return LoraConfig(
        r=lora_cfg["r"],
        lora_alpha=lora_cfg["lora_alpha"],
        lora_dropout=lora_cfg["lora_dropout"],
        target_modules=lora_cfg["target_modules"],
        bias=lora_cfg["bias"],
        task_type=lora_cfg["task_type"],
    )


def main():
    parser = argparse.ArgumentParser(
        description="Fine-tune Llama 3.1 8B with QLoRA on BCP-rendered context."
    )
    parser.add_argument("--config", required=True, help="Path to qlora_tier1.yaml")
    parser.add_argument("--dataset", required=True, help="Path to HuggingFace Dataset directory")
    parser.add_argument("--output", required=True, help="Output directory for checkpoints")
    parser.add_argument(
        "--variant", default="bcp_minimal",
        choices=["bcp_minimal", "bcp_xml", "bcp_markdown", "raw_markdown"],
        help="Which context format variant to train on (default: bcp_minimal)"
    )
    parser.add_argument(
        "--max-steps", type=int, default=None,
        help="Stop after N steps (for smoke testing). Overrides num_epochs."
    )
    args = parser.parse_args()

    config = load_config(args.config)

    # ── Step 1: Load tokenizer ──
    # The tokenizer converts text to token IDs that the model understands.
    # We use the same tokenizer as the base model (Llama 3.1's tokenizer).
    print(f"Loading tokenizer from {config['base_model']}...")
    tokenizer = AutoTokenizer.from_pretrained(config["base_model"])
    tokenizer.pad_token = tokenizer.eos_token  # Llama doesn't have a pad token
    tokenizer.padding_side = "right"           # Pad on the right for causal LM

    # ── Step 2: Load model in 4-bit ──
    # This loads the full 8B parameter model but stores weights in 4-bit,
    # reducing memory from ~16GB to ~5GB.
    print(f"Loading model {config['base_model']} in 4-bit quantization...")
    bnb_config = build_quantization_config(config["qlora"])
    model = AutoModelForCausalLM.from_pretrained(
        config["base_model"],
        quantization_config=bnb_config,
        device_map="auto",           # Automatically place layers on GPU
        torch_dtype=torch.bfloat16,  # bf16 for non-quantized operations
    )

    # Prepare the quantized model for LoRA training.
    # This freezes the base weights and enables gradient computation
    # through the quantized layers.
    model = prepare_model_for_kbit_training(model)

    # ── Step 3: Add LoRA adapters ──
    # This inserts small trainable matrices into the model.
    # Only these adapter weights will be updated during training.
    print("Adding LoRA adapters...")
    lora_config = build_lora_config(config["lora"])
    model = get_peft_model(model, lora_config)
    model.print_trainable_parameters()  # Shows: "trainable params: X / total: Y (Z%)"

    # ── Step 4: Load dataset ──
    print(f"Loading dataset from {args.dataset}...")
    dataset = load_from_disk(args.dataset)
    print(f"  Train: {len(dataset['train'])} examples")
    print(f"  Validation: {len(dataset['validation'])} examples")

    # ── Step 5: Configure training ──
    train_cfg = config["training"]
    log_cfg = config["logging"]
    training_args = TrainingArguments(
        output_dir=args.output,
        num_train_epochs=train_cfg["num_epochs"],
        per_device_train_batch_size=train_cfg["per_device_train_batch_size"],
        gradient_accumulation_steps=train_cfg["gradient_accumulation_steps"],
        learning_rate=train_cfg["learning_rate"],
        lr_scheduler_type=train_cfg["lr_scheduler_type"],
        warmup_ratio=train_cfg["warmup_ratio"],
        weight_decay=train_cfg["weight_decay"],
        bf16=train_cfg["bf16"],
        gradient_checkpointing=train_cfg["gradient_checkpointing"],
        logging_steps=log_cfg["logging_steps"],
        save_steps=log_cfg["save_steps"],
        eval_steps=log_cfg.get("eval_steps", log_cfg["save_steps"]),
        save_total_limit=log_cfg["save_total_limit"],
        eval_strategy=config["eval"]["eval_strategy"],
        metric_for_best_model=config["eval"]["metric_for_best_model"],
        load_best_model_at_end=True,
        report_to="none",  # Don't report to wandb/tensorboard
        max_steps=args.max_steps if args.max_steps else -1,
    )

    # ── Step 6: Train ──
    # SFTTrainer (Supervised Fine-Tuning Trainer) handles:
    #   - Applying the chat template to format examples
    #   - Masking the user prompt so loss is only computed on the response
    #   - Gradient accumulation, checkpointing, evaluation
    print("Starting training...")
    trainer = SFTTrainer(
        model=model,
        tokenizer=tokenizer,
        args=training_args,
        train_dataset=dataset["train"],
        eval_dataset=dataset["validation"],
        max_seq_length=train_cfg["max_seq_length"],
    )

    trainer.train()

    # ── Step 7: Save final adapter ──
    final_path = args.output + "/final"
    print(f"Saving final adapter to {final_path}...")
    trainer.save_model(final_path)
    print("Training complete.")


if __name__ == "__main__":
    main()
```

### 6. Evaluation Harness

The evaluation answers one question: **does the model comprehend BCP-rendered
context as well as raw markdown?**

```
Evaluation matrix:

  ┌────────────────────────────┬─────────────────────┬──────────────────┐
  │ Model                      │ Context Format      │ Label            │
  ├────────────────────────────┼─────────────────────┼──────────────────┤
  │ Base Llama 3.1 8B          │ Raw markdown        │ baseline         │
  │ Fine-tuned (BCP Minimal)   │ BCP Minimal         │ ft_minimal       │
  │ Fine-tuned (BCP Minimal)   │ BCP XML             │ ft_xml           │
  │ Fine-tuned (BCP Minimal)   │ Raw markdown        │ ft_markdown      │
  │ Base Llama 3.1 8B          │ BCP Minimal         │ base_minimal     │
  ├────────────────────────────┼─────────────────────┼──────────────────┤
  │ (Optional additional runs) │                     │                  │
  │ Fine-tuned (BCP XML)       │ BCP XML             │ ft_xml_on_xml    │
  │ Fine-tuned (Raw MD)        │ Raw markdown        │ ft_md_on_md      │
  └────────────────────────────┴─────────────────────┴──────────────────┘

  Primary comparison:
    ft_minimal vs baseline
    "Does the fine-tuned model with BCP Minimal match or beat
     the base model with raw markdown?"

  Token efficiency comparison:
    ft_minimal tokens vs baseline tokens
    "How many fewer tokens does the BCP Minimal context use?"

  Format transfer test:
    ft_minimal on XML vs ft_minimal on Minimal
    "Does training on Minimal generalize to XML format?"
```

```python
# src/evaluate.py

"""
Evaluation harness for BCP comprehension parity.

=== What This Script Does ===
Runs the evaluation matrix: loads the base model and fine-tuned adapter,
generates responses for each (model, context format) combination, and
computes metrics to determine if BCP-rendered context preserves model
comprehension compared to raw markdown.

=== Evaluation Metrics ===
Two types of metrics are computed:

1. ROUGE-L (automated, fast, free):
   Measures the longest common subsequence between the generated response
   and the reference response. A score of 1.0 = perfect overlap, 0.0 = no
   overlap. This is a rough proxy for "did the model say the right thing?"
   ROUGE-L is imperfect (two correct responses can have low overlap if
   phrased differently) but it's fast and free.

2. LLM-as-Judge (model-based, slower, free with Ollama):
   A different local model (devstral-2:123b) reads the task, reference
   response, and candidate response, then rates quality 1-5. This captures
   semantic correctness that ROUGE misses. We use a DIFFERENT model than
   the data generator (Qwen3 Coder) to avoid self-evaluation bias.

=== The Key Question ===
If ft_minimal ROUGE-L is within 0.05 of baseline ROUGE-L, AND ft_minimal
uses fewer input tokens, then BCP delivers strictly more efficient context
with comparable comprehension. That's the PASS condition.

Usage:
  python -m src.evaluate \\
    --base-model meta-llama/Llama-3.1-8B-Instruct \\
    --adapter outputs/checkpoints/bcp_minimal/final \\
    --test-data data/processed/ \\
    --output outputs/eval/

  # Quick smoke test (5 examples):
  python -m src.evaluate \\
    --base-model meta-llama/Llama-3.1-8B-Instruct \\
    --adapter outputs/checkpoints/bcp_minimal/final \\
    --test-data data/processed/ \\
    --output outputs/eval/smoke/ \\
    --limit 5
"""

import json
import argparse
from pathlib import Path

import torch
import yaml
from peft import PeftModel
from transformers import AutoModelForCausalLM, AutoTokenizer, BitsAndBytesConfig
from rouge_score import rouge_scorer

from src.ollama_client import OllamaClient


def load_model_with_adapter(base_model: str, adapter_path: str | None):
    """
    Load the base model, optionally with a QLoRA adapter merged in.

    When adapter_path is None, this loads the vanilla base model (for baseline).
    When adapter_path is provided, it loads the base model + merges the LoRA
    adapter weights, giving you the fine-tuned model.

    Args:
        base_model: HuggingFace model ID (e.g., "meta-llama/Llama-3.1-8B-Instruct")
        adapter_path: Path to LoRA adapter directory, or None for base model.

    Returns:
        The loaded model (on GPU, in 4-bit quantization).
    """
    bnb_config = BitsAndBytesConfig(
        load_in_4bit=True,
        bnb_4bit_quant_type="nf4",
        bnb_4bit_use_double_quant=True,
        bnb_4bit_compute_dtype=torch.bfloat16,
    )
    model = AutoModelForCausalLM.from_pretrained(
        base_model,
        quantization_config=bnb_config,
        device_map="auto",
        torch_dtype=torch.bfloat16,
    )
    if adapter_path:
        model = PeftModel.from_pretrained(model, adapter_path)
    return model


def generate_response(model, tokenizer, prompt: str, max_new_tokens: int = 2048) -> str:
    """
    Generate a response from the model given a prompt.

    Uses the model's chat template to format the prompt correctly,
    then generates tokens autoregressively.

    Args:
        model: The loaded language model.
        tokenizer: The model's tokenizer.
        prompt: The user message to send to the model.
        max_new_tokens: Maximum number of tokens to generate.

    Returns:
        The generated response text (without the prompt).
    """
    # Format as a chat message using the model's built-in template
    messages = [{"role": "user", "content": prompt}]
    input_ids = tokenizer.apply_chat_template(
        messages, return_tensors="pt", add_generation_prompt=True
    ).to(model.device)

    # Generate without computing gradients (inference only)
    with torch.no_grad():
        output = model.generate(
            input_ids,
            max_new_tokens=max_new_tokens,
            temperature=0.1,      # Near-deterministic for reproducible evaluation
            do_sample=True,
            pad_token_id=tokenizer.eos_token_id,
        )

    # Extract only the generated tokens (remove the prompt tokens)
    response_ids = output[0][input_ids.shape[1]:]
    return tokenizer.decode(response_ids, skip_special_tokens=True)


class ComprehensionEvaluator:
    """
    Evaluates model comprehension across context formats.

    This is the core of the evaluation — it runs one (model, format) combination
    and computes all metrics.

    Attributes:
        scorer: The ROUGE-L scorer (from google-research/rouge-score).
        judge: Ollama client for LLM-as-judge evaluation.
    """

    def __init__(self, judge_config: dict | None = None):
        """
        Args:
            judge_config: Config for the LLM judge model (from generation.yaml).
                          If None, LLM-as-judge scores are skipped.
        """
        # ROUGE-L measures the longest common subsequence between two texts.
        # use_stemmer=True normalizes words (e.g., "running" → "run").
        self.scorer = rouge_scorer.RougeScorer(["rougeL"], use_stemmer=True)

        self.judge = None
        if judge_config:
            self.judge = OllamaClient(
                model=judge_config["name"],
                base_url=judge_config["base_url"],
            )

    def compute_rouge_l(self, prediction: str, reference: str) -> float:
        """
        Compute ROUGE-L F1 score between prediction and reference.

        ROUGE-L finds the longest common subsequence (LCS) between two texts
        and computes precision (what fraction of the prediction is in the LCS)
        and recall (what fraction of the reference is in the LCS), then returns
        their F1 harmonic mean.

        Returns:
            Float between 0.0 (no overlap) and 1.0 (perfect match).
        """
        scores = self.scorer.score(reference, prediction)
        return scores["rougeL"].fmeasure

    def llm_judge_score(self, task: str, response: str, reference: str) -> int:
        """
        Use a local LLM as a judge to rate response quality 1-5.

        The judge does NOT see the context format — only the task, reference
        response, and candidate response. This prevents the judge from having
        a preference for one format over another.

        Uses a DIFFERENT model (devstral-2:123b) than the data generator
        (qwen3-coder-next) to avoid self-evaluation bias.

        Returns:
            Integer 1-5, or -1 if judging failed/unavailable.
        """
        if not self.judge:
            return -1

        judge_prompt = f"""Rate the following response to a coding task on a scale of 1-5.

Task: {task}

Reference (ideal) response:
{reference}

Candidate response:
{response}

Rating criteria:
  5 = Correct, complete, well-explained, matches or exceeds reference quality
  4 = Mostly correct, minor omissions or imprecisions
  3 = Partially correct, significant gaps but demonstrates understanding
  2 = Largely incorrect or incomplete, some relevant content
  1 = Incorrect, irrelevant, or empty

Respond with only a single integer (1-5)."""

        try:
            result = self.judge.generate(
                prompt=judge_prompt,
                temperature=0.0,
                max_tokens=16,
            )
            return int(result.strip()[0])  # Take first character as the score
        except (ValueError, IndexError):
            return -1

    def evaluate_run(
        self,
        model,
        tokenizer,
        test_examples: list[dict],
        context_key: str,
        label: str,
    ) -> dict:
        """
        Run evaluation for one (model, context format) combination.

        Iterates over all test examples, generates a response for each,
        and computes metrics.

        Args:
            model: The loaded model (base or fine-tuned).
            tokenizer: The model's tokenizer.
            test_examples: List of test examples (dicts with context variants).
            context_key: Which context variant to use (e.g., "context_bcp_minimal").
            label: Human-readable label for this run (e.g., "ft_minimal").

        Returns:
            Dict with aggregated metrics and per-example results.
        """
        results = []
        for i, ex in enumerate(test_examples):
            if i % 10 == 0:
                print(f"    [{label}] {i}/{len(test_examples)}...")

            prompt = f"{ex[context_key]}\n\n{ex['task']}"
            prediction = generate_response(model, tokenizer, prompt)
            input_tokens = len(tokenizer.encode(prompt))

            rouge_l = self.compute_rouge_l(prediction, ex["response"])
            judge_score = self.llm_judge_score(ex["task"], prediction, ex["response"])

            results.append({
                "id": ex.get("id", ""),
                "task_type": ex.get("task_type", ""),
                "rouge_l": rouge_l,
                "judge_score": judge_score,
                "input_tokens": input_tokens,
                "output_tokens": len(tokenizer.encode(prediction)),
            })

        # Aggregate metrics
        n = len(results)
        judge_scores = [r["judge_score"] for r in results if r["judge_score"] > 0]

        return {
            "label": label,
            "context_format": context_key,
            "n_examples": n,
            "mean_rouge_l": sum(r["rouge_l"] for r in results) / n,
            "mean_judge_score": sum(judge_scores) / max(1, len(judge_scores)),
            "mean_input_tokens": sum(r["input_tokens"] for r in results) / n,
            "per_example": results,
        }


def print_comparison_table(runs: list[dict]):
    """
    Print the comprehension parity report.

    This is the final output — a table comparing all (model, format)
    combinations with their metrics and a PASS/FAIL verdict.
    """
    print()
    print("=" * 85)
    print("BCP Comprehension Parity Report")
    print("=" * 85)
    print()
    print(f"{'Label':<25} {'Format':<22} {'ROUGE-L':>8} {'Judge':>6} {'Tokens':>8} {'vs Base':>8}")
    print("-" * 85)

    # Find baseline token count for comparison
    baseline_tokens = None
    for run in runs:
        if run["label"] == "baseline":
            baseline_tokens = run["mean_input_tokens"]

    for run in runs:
        # Calculate token savings vs baseline
        token_delta = ""
        if baseline_tokens and run["label"] != "baseline":
            pct = (1.0 - run["mean_input_tokens"] / baseline_tokens) * 100.0
            token_delta = f"{pct:+.1f}%"

        judge_str = (
            f"{run['mean_judge_score']:.1f}"
            if run["mean_judge_score"] > 0
            else "N/A"
        )

        print(
            f"{run['label']:<25} "
            f"{run['context_format']:<22} "
            f"{run['mean_rouge_l']:>8.3f} "
            f"{judge_str:>6} "
            f"{run['mean_input_tokens']:>8.0f} "
            f"{token_delta:>8}"
        )

    print("-" * 85)
    print()

    # ── Verdict ──
    ft_minimal = next((r for r in runs if r["label"] == "ft_minimal"), None)
    baseline = next((r for r in runs if r["label"] == "baseline"), None)
    if ft_minimal and baseline:
        rouge_delta = ft_minimal["mean_rouge_l"] - baseline["mean_rouge_l"]
        token_savings = (
            (1.0 - ft_minimal["mean_input_tokens"] / baseline["mean_input_tokens"]) * 100.0
        )

        print("VERDICT:")
        if rouge_delta >= -0.05:
            print(f"  PASS: BCP Minimal comprehension is within tolerance of baseline")
            print(f"         ROUGE-L delta: {rouge_delta:+.3f} (threshold: -0.05)")
            print(f"         Token savings: {token_savings:.1f}%")
            print(f"  -> BCP delivers {token_savings:.0f}% fewer tokens with comparable comprehension")
        else:
            print(f"  FAIL: BCP Minimal comprehension below baseline")
            print(f"         ROUGE-L delta: {rouge_delta:+.3f} (threshold: -0.05)")
            print(f"         Consider: try BCP XML mode or increase training data")


def main():
    parser = argparse.ArgumentParser(
        description="Evaluate BCP comprehension parity across context formats."
    )
    parser.add_argument("--base-model", required=True, help="HuggingFace model ID")
    parser.add_argument("--adapter", required=True, help="Path to LoRA adapter checkpoint")
    parser.add_argument("--test-data", required=True, help="Path to processed datasets directory")
    parser.add_argument("--output", required=True, help="Output directory for results")
    parser.add_argument("--limit", type=int, default=None, help="Max examples to evaluate")
    parser.add_argument(
        "--judge-config", default="configs/generation.yaml",
        help="Path to config with judge model settings"
    )
    args = parser.parse_args()

    output_dir = Path(args.output)
    output_dir.mkdir(parents=True, exist_ok=True)

    # Load judge config
    judge_config = None
    if Path(args.judge_config).exists():
        full_config = yaml.safe_load(Path(args.judge_config).read_text())
        judge_config = full_config.get("judge_model")

    evaluator = ComprehensionEvaluator(judge_config=judge_config)

    # Load tokenizer (shared across all model variants)
    print(f"Loading tokenizer from {args.base_model}...")
    tokenizer = AutoTokenizer.from_pretrained(args.base_model)
    tokenizer.pad_token = tokenizer.eos_token

    # Load test examples (we need all context variants)
    print("Loading test examples...")
    # Load from the raw JSONL to get all variants in one place
    raw_dir = Path(args.test_data).parent / "raw"
    test_examples = []
    for jsonl_file in sorted(raw_dir.rglob("*.jsonl")):
        with open(jsonl_file) as f:
            for line in f:
                test_examples.append(json.loads(line.strip()))

    # Use only the test split (last 10%)
    n = len(test_examples)
    test_start = int(n * 0.9)
    test_examples = test_examples[test_start:]

    if args.limit:
        test_examples = test_examples[:args.limit]

    print(f"  {len(test_examples)} test examples")

    runs = []

    # ── Run 1: Baseline (base model + raw markdown) ──
    print("\n=== Evaluating: baseline (base model + raw markdown) ===")
    base_model = load_model_with_adapter(args.base_model, adapter_path=None)
    runs.append(evaluator.evaluate_run(
        base_model, tokenizer, test_examples, "context_raw_markdown", "baseline"
    ))

    # ── Run 2: base_minimal (base model + BCP Minimal) ──
    print("\n=== Evaluating: base_minimal (base model + BCP Minimal) ===")
    runs.append(evaluator.evaluate_run(
        base_model, tokenizer, test_examples, "context_bcp_minimal", "base_minimal"
    ))

    # Free base model memory before loading fine-tuned
    del base_model
    torch.cuda.empty_cache()

    # ── Run 3: ft_minimal (fine-tuned + BCP Minimal) ──
    print("\n=== Evaluating: ft_minimal (fine-tuned + BCP Minimal) ===")
    ft_model = load_model_with_adapter(args.base_model, adapter_path=args.adapter)
    runs.append(evaluator.evaluate_run(
        ft_model, tokenizer, test_examples, "context_bcp_minimal", "ft_minimal"
    ))

    # ── Run 4: ft_xml (fine-tuned + BCP XML) ──
    print("\n=== Evaluating: ft_xml (fine-tuned + BCP XML) ===")
    runs.append(evaluator.evaluate_run(
        ft_model, tokenizer, test_examples, "context_bcp_xml", "ft_xml"
    ))

    # ── Run 5: ft_markdown (fine-tuned + raw markdown) ──
    print("\n=== Evaluating: ft_markdown (fine-tuned + raw markdown) ===")
    runs.append(evaluator.evaluate_run(
        ft_model, tokenizer, test_examples, "context_raw_markdown", "ft_markdown"
    ))

    # ── Print results ──
    print_comparison_table(runs)

    # ── Save results ──
    results_path = output_dir / "results.json"
    with open(results_path, "w") as f:
        # Remove per_example data for the summary file (it's large)
        summary = [{k: v for k, v in r.items() if k != "per_example"} for r in runs]
        json.dump(summary, f, indent=2)
    print(f"\nResults saved to {results_path}")


if __name__ == "__main__":
    main()
```

### 7. End-to-End Pipeline Script

```bash
#!/usr/bin/env bash
# scripts/run_full_pipeline.sh
#
# End-to-end pipeline: clone repos → generate data → prepare datasets →
#                      train model → evaluate → produce report
#
# This script runs the entire experiment from scratch. It takes several hours
# (mostly the training step). You can also run each step individually — see
# the individual script commands below.
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

echo "=== Step 1: Clone source repositories ==="
echo "Cloning ~15 open-source repos for training data."
echo "These provide real code that gets rendered through BCP."
bash scripts/clone_repos.sh

echo ""
echo "=== Step 2: Generate training data (via Ollama) ==="
echo "This sends code through BCP render → Qwen3 Coder generates responses."
echo "Estimated time: 2-3 hours for 5,000 examples (local, no API cost)."
python -m src.generate_data \
    --config configs/generation.yaml \
    --output data/raw/

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
echo "This loads both base and fine-tuned models and generates responses."
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
```

---

## Tier 2 Extension Path: Hybrid Binary/Text Tokens

This section documents the design for Tier 2, which extends the tokenizer with
special tokens for BCP wire format constants. **Not implemented in this spec —
reserved for fast-follow.**

```
Tier 2 special tokens (mapped from BCP wire format):

  Token              BCP Wire Constant    Meaning
  ──────────────────────────────────────────────────
  <BCP_CODE>         0x01                 CODE block start
  <BCP_CONV>         0x02                 CONVERSATION block start
  <BCP_TREE>         0x03                 FILE_TREE block start
  <BCP_TOOL>         0x04                 TOOL_RESULT block start
  <BCP_DOC>          0x05                 DOCUMENT block start
  <BCP_DATA>         0x06                 STRUCTURED_DATA block start
  <BCP_DIFF>         0x07                 DIFF block start
  <BCP_ANNO>         0x08                 ANNOTATION block start
  <BCP_EMBED>        0x09                 EMBEDDING_REF block start
  <BCP_IMG>          0x0A                 IMAGE block start
  <BCP_END>          0xFF                 Block end / stream sentinel
  <BCP_FIELD>        (TLV tag prefix)     Field delimiter
  <BCP_SUMMARY>      (flag bit 0)         Summary sub-block marker
  <BCP_LANG_RUST>    Lang::Rust           Language: Rust
  <BCP_LANG_TS>      Lang::TypeScript     Language: TypeScript
  <BCP_LANG_PY>      Lang::Python         Language: Python
  <BCP_LANG_GO>      Lang::Go             Language: Go
  <BCP_ROLE_USER>    Role::User           Conversation role: user
  <BCP_ROLE_ASST>    Role::Assistant      Conversation role: assistant
  <BCP_STATUS_OK>    Status::Ok           Tool result: success
  <BCP_STATUS_ERR>   Status::Error        Tool result: error

  Example hybrid-encoded context:

    <BCP_CODE><BCP_LANG_RUST><BCP_FIELD>src/main.rs<BCP_FIELD>
    fn main() {
        let config = Config::load()?;
        server::start(config).await?;
    }
    <BCP_END>
    <BCP_TOOL>ripgrep<BCP_STATUS_OK><BCP_FIELD>
    3 matches for 'ConnectionPool' across 2 files.
    <BCP_END>
```

Tier 2 implementation requires:
1. Extend tokenizer: `tokenizer.add_special_tokens({"additional_special_tokens": [...]})`
2. Resize model embeddings: `model.resize_token_embeddings(len(tokenizer))`
3. New render mode in `bcp-driver`: `OutputMode::Hybrid` that emits special token markers
4. New data generation: render context in hybrid mode, generate responses
5. Fine-tune with the extended tokenizer

The Tier 1 infrastructure (data pipeline, training script, evaluation harness) is
designed to support this extension with minimal changes — the `context_key` pattern
in the evaluation harness and the JSONL schema's extensible variant fields enable
adding `context_bcp_hybrid` without restructuring.

---

## Tier 3 Extension Path: Native Binary Ingestion

Documented as a research direction only. **Requires pretraining-scale compute.**

Native binary ingestion means the model consumes raw BCP wire format bytes without
any text decode step. This requires:

1. A byte-level or sub-word tokenizer that includes BCP structural bytes in its
   vocabulary
2. Pretraining (not fine-tuning) on a corpus that includes raw BCP payloads paired
   with expected outputs
3. Collaboration with model providers to integrate BCP awareness into training runs

The Tier 1 evaluation harness can measure Tier 3 if a model with native binary
support becomes available — add `context_bcp_binary` (raw bytes, base64-encoded or
hex-encoded for transport) to the JSONL schema and run the same evaluation matrix.

---

## File Structure

```
experiments/bcp-qlora/
├── pyproject.toml
├── README.md
├── configs/
│   ├── qlora_tier1.yaml
│   └── generation.yaml
├── src/
│   ├── __init__.py
│   ├── generate_data.py        # Ollama → (context, task, response) triples
│   ├── prepare_dataset.py      # JSONL → HuggingFace Dataset (4 variants)
│   ├── train.py                # QLoRA fine-tuning with SFTTrainer
│   ├── evaluate.py             # Evaluation matrix + comprehension parity report
│   ├── bcp_render.py           # bcp-cli subprocess wrapper (encode → decode)
│   ├── markdown_baseline.py    # Naive + realistic markdown builders
│   └── ollama_client.py        # Ollama OpenAI-compatible API wrapper
├── data/
│   ├── raw/                    # Generated JSONL files
│   │   ├── code_tasks/
│   │   └── tool_tasks/
│   ├── processed/              # HuggingFace Dataset (train/val/test per variant)
│   └── repos/                  # Cloned source repositories
├── outputs/
│   ├── checkpoints/            # QLoRA adapter weights
│   ├── eval/                   # Evaluation results + comparison table
│   └── report/                 # Final comprehension parity report
└── scripts/
    ├── setup_env.sh            # DGX Spark environment + dependency install
    ├── clone_repos.sh          # Clone source repos for data generation
    └── run_full_pipeline.sh    # End-to-end pipeline script
```

---

## Acceptance Criteria

### Data Pipeline
- [ ] `generate_data.py` produces ≥2,000 (context, task, response) triples as JSONL
- [ ] Each JSONL line contains all 4 context variants (bcp_minimal, bcp_xml, bcp_markdown, raw_markdown)
- [ ] At least 20% of examples include tool-use tasks (tool_result + conversation blocks)
- [ ] Context is rendered via `bcp` CLI (not hand-constructed)
- [ ] `prepare_dataset.py` produces HuggingFace `DatasetDict` with train/val/test splits
- [ ] Data split is deterministic (same input → same split)
- [ ] Data generation uses Ollama locally (zero API cost)

### Training
- [ ] `train.py` loads Llama 3.1 8B Instruct in 4-bit quantization on DGX Spark
- [ ] QLoRA adapter trains with rank=16, alpha=32, targeting all attention + MLP layers
- [ ] Training completes in <6 hours on DGX Spark for 3 epochs on 4,000 examples
- [ ] Validation loss decreases monotonically (no divergence)
- [ ] Final adapter checkpoint is saved and loadable

### Evaluation
- [ ] Evaluation matrix runs all 5 primary configurations (see §6 table)
- [ ] ROUGE-L scores computed for all configurations
- [ ] LLM-as-judge scores computed for all configurations (devstral-2:123b via Ollama)
- [ ] Input token counts recorded per configuration
- [ ] Comprehension parity report printed with comparison table
- [ ] Verdict printed: PASS if ROUGE-L delta ≥ -0.05, FAIL otherwise

### Comprehension Parity (the actual experiment result)
- [ ] `ft_minimal` ROUGE-L is within 0.05 of `baseline` ROUGE-L
- [ ] `ft_minimal` uses measurably fewer input tokens than `baseline` (target: ≥15% fewer)
- [ ] `ft_minimal` LLM-as-judge mean score ≥ 3.5 / 5.0

### Infrastructure
- [ ] `setup_env.sh` installs all dependencies on DGX Spark
- [ ] `run_full_pipeline.sh` runs end-to-end without manual intervention
- [ ] `bcp` CLI binary is available and functional (built from workspace)
- [ ] No training data or model weights are committed to git (only configs and scripts)
- [ ] `data/` and `outputs/` are in `.gitignore`
- [ ] Ollama running with `qwen3-coder-next:q8_0` (data gen) and `devstral-2:123b` (judge)

### Documentation
- [ ] Every Python file has a module-level docstring explaining what it does and why
- [ ] Every function has a docstring explaining arguments, return values, and behavior
- [ ] YAML config files have inline comments explaining every parameter
- [ ] README.md provides a complete walkthrough for someone new to ML/fine-tuning
- [ ] Key ML concepts (QLoRA, tokenizer, ROUGE-L, chat templates) are explained inline

### Tier 2/3 Readiness
- [ ] JSONL schema documented with extension fields for `context_bcp_hybrid` and `context_bcp_binary`
- [ ] Tier 2 special token table documented with BCP wire constant mappings
- [ ] Evaluation harness accepts arbitrary `context_key` parameter (not hardcoded to Tier 1 variants)
- [ ] Training script supports `--variant` flag for selecting context format

---

## Verification

```bash
# ─── Prerequisites ───
cd experiments/bcp-qlora
bash scripts/setup_env.sh
source .venv/bin/activate

# ─── Verify infrastructure ───
bcp --version                                           # BCP CLI
python -c "import torch; print(torch.cuda.get_device_name(0))"  # GPU
loclaude models                                         # Ollama models

# ─── Clone source repos ───
bash scripts/clone_repos.sh

# ─── Smoke test: generate 50 examples ───
python -m src.generate_data \
    --config configs/generation.yaml \
    --output data/raw/ \
    --limit 50

# ─── Smoke test: prepare datasets ───
python -m src.prepare_dataset \
    --input data/raw/ \
    --output data/processed/

# ─── Smoke test: train 10 steps ───
python -m src.train \
    --config configs/qlora_tier1.yaml \
    --dataset data/processed/context_bcp_minimal \
    --output outputs/checkpoints/smoke_test \
    --max-steps 10

# ─── Smoke test: evaluate 5 examples ───
python -m src.evaluate \
    --base-model meta-llama/Llama-3.1-8B-Instruct \
    --adapter outputs/checkpoints/smoke_test/final \
    --test-data data/processed/ \
    --output outputs/eval/smoke_test/ \
    --limit 5

# ─── Full pipeline (hours — run when ready) ───
bash scripts/run_full_pipeline.sh
```

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Llama 3.1 8B downloads fail on DGX Spark (network restrictions) | Medium | Blocks all training | Pre-download model to local cache; support `--model-path` flag for local weights |
| Ollama not running or model not available | Medium | Blocks data generation | `setup_env.sh` verifies Ollama connectivity; `ollama_client.py` has `is_available()` check with clear error messages |
| Qwen3 Coder generates low-quality responses | Low | Weak training data | Q8 quantization preserves near-full quality; fall back to `devstral-2:123b` (123B params) or `gpt-oss:120b` via config change |
| BCP Minimal format genuinely hurts comprehension | Low | Invalidates thesis | Report the result honestly; pivot to XML mode which has richer structure markers |
| Training diverges or overfits | Medium | Wasted compute | Early stopping on validation loss; gradient checkpointing; conservative learning rate |
| 4,000 training examples insufficient for meaningful fine-tuning | Medium | Weak signal | Start with 2,000 and measure; scale to 10,000+ (free with local generation) |
| DGX Spark CUDA/driver compatibility issues with bitsandbytes | Low | Blocks QLoRA | Fall back to 8-bit quantization; test bitsandbytes compatibility in setup_env.sh |
| ROUGE-L is too noisy to detect comprehension differences | Medium | Inconclusive | LLM-as-judge provides a second signal; per-task-type breakdown reveals patterns |
| Evaluation compares fine-tuned vs base (not apples-to-apples) | High | Misleading | Also train `ft_md_on_md` (fine-tuned on raw markdown) as a fair comparison; report both |
| Ollama GPU memory conflict during data gen + training | Medium | OOM crash | Data generation and training are sequential steps (not parallel); pipeline script enforces this |

---

## Rollback Plan

### Full Rollback
- Delete `experiments/bcp-qlora/` directory
- No Rust code is modified; all existing crates remain functional
- No workspace configuration is changed
- Clean rollback: `rm -rf experiments/bcp-qlora`

### Partial Rollback (keep data pipeline, remove training)
- Delete `outputs/` directory (checkpoints, eval results)
- Keep `data/raw/` and `configs/` for future use
- Data generation pipeline remains independently useful for Tier 2

### Model Weights
- QLoRA adapters are stored in `outputs/checkpoints/` (not committed to git)
- Base model is cached in HuggingFace cache directory (`~/.cache/huggingface/`)
- Neither affects the repository state
