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
   overlap.

2. LLM-as-Judge (model-based, slower, free with Ollama):
   A different local model (devstral-2:123b) reads the task, reference
   response, and candidate response, then rates quality 1-5.

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

    Args:
        model: The loaded language model.
        tokenizer: The model's tokenizer.
        prompt: The user message to send to the model.
        max_new_tokens: Maximum number of tokens to generate.

    Returns:
        The generated response text (without the prompt).
    """
    messages = [{"role": "user", "content": prompt}]
    input_ids = tokenizer.apply_chat_template(
        messages, return_tensors="pt", add_generation_prompt=True
    ).to(model.device)

    with torch.no_grad():
        output = model.generate(
            input_ids,
            max_new_tokens=max_new_tokens,
            temperature=0.1,
            do_sample=True,
            pad_token_id=tokenizer.eos_token_id,
        )

    response_ids = output[0][input_ids.shape[1]:]
    return tokenizer.decode(response_ids, skip_special_tokens=True)


class ComprehensionEvaluator:
    """
    Evaluates model comprehension across context formats.

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

        Returns:
            Float between 0.0 (no overlap) and 1.0 (perfect match).
        """
        scores = self.scorer.score(reference, prediction)
        return scores["rougeL"].fmeasure

    def llm_judge_score(self, task: str, response: str, reference: str) -> int:
        """
        Use a local LLM as a judge to rate response quality 1-5.

        The judge does NOT see the context format — only the task, reference
        response, and candidate response.

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
            return int(result.strip()[0])
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

        Args:
            model: The loaded model (base or fine-tuned).
            tokenizer: The model's tokenizer.
            test_examples: List of test examples (dicts with context variants).
            context_key: Which context variant to use.
            label: Human-readable label for this run.

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
    """Print the comprehension parity report."""
    print()
    print("=" * 85)
    print("BCP Comprehension Parity Report")
    print("=" * 85)
    print()
    print(f"{'Label':<25} {'Format':<22} {'ROUGE-L':>8} {'Judge':>6} {'Tokens':>8} {'vs Base':>8}")
    print("-" * 85)

    baseline_tokens = None
    for run in runs:
        if run["label"] == "baseline":
            baseline_tokens = run["mean_input_tokens"]

    for run in runs:
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

    judge_config = None
    if Path(args.judge_config).exists():
        full_config = yaml.safe_load(Path(args.judge_config).read_text())
        judge_config = full_config.get("judge_model")

    evaluator = ComprehensionEvaluator(judge_config=judge_config)

    print(f"Loading tokenizer from {args.base_model}...")
    tokenizer = AutoTokenizer.from_pretrained(args.base_model)
    tokenizer.pad_token = tokenizer.eos_token

    print("Loading test examples...")
    raw_dir = Path(args.test_data).parent / "raw"
    test_examples = []
    for jsonl_file in sorted(raw_dir.rglob("*.jsonl")):
        with open(jsonl_file) as f:
            for line in f:
                test_examples.append(json.loads(line.strip()))

    n = len(test_examples)
    test_start = int(n * 0.9)
    test_examples = test_examples[test_start:]

    if args.limit:
        test_examples = test_examples[:args.limit]

    print(f"  {len(test_examples)} test examples")

    runs = []

    # Run 1: Baseline (base model + raw markdown)
    print("\n=== Evaluating: baseline (base model + raw markdown) ===")
    base_model = load_model_with_adapter(args.base_model, adapter_path=None)
    runs.append(evaluator.evaluate_run(
        base_model, tokenizer, test_examples, "context_raw_markdown", "baseline"
    ))

    # Run 2: base_minimal (base model + BCP Minimal)
    print("\n=== Evaluating: base_minimal (base model + BCP Minimal) ===")
    runs.append(evaluator.evaluate_run(
        base_model, tokenizer, test_examples, "context_bcp_minimal", "base_minimal"
    ))

    # Free base model memory before loading fine-tuned
    del base_model
    torch.cuda.empty_cache()

    # Run 3: ft_minimal (fine-tuned + BCP Minimal)
    print("\n=== Evaluating: ft_minimal (fine-tuned + BCP Minimal) ===")
    ft_model = load_model_with_adapter(args.base_model, adapter_path=args.adapter)
    runs.append(evaluator.evaluate_run(
        ft_model, tokenizer, test_examples, "context_bcp_minimal", "ft_minimal"
    ))

    # Run 4: ft_xml (fine-tuned + BCP XML)
    print("\n=== Evaluating: ft_xml (fine-tuned + BCP XML) ===")
    runs.append(evaluator.evaluate_run(
        ft_model, tokenizer, test_examples, "context_bcp_xml", "ft_xml"
    ))

    # Run 5: ft_markdown (fine-tuned + raw markdown)
    print("\n=== Evaluating: ft_markdown (fine-tuned + raw markdown) ===")
    runs.append(evaluator.evaluate_run(
        ft_model, tokenizer, test_examples, "context_raw_markdown", "ft_markdown"
    ))

    print_comparison_table(runs)

    results_path = output_dir / "results.json"
    with open(results_path, "w") as f:
        summary = [{k: v for k, v in r.items() if k != "per_example"} for r in runs]
        json.dump(summary, f, indent=2)
    print(f"\nResults saved to {results_path}")


if __name__ == "__main__":
    main()
