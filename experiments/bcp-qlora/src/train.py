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

    # Step 1: Load tokenizer
    print(f"Loading tokenizer from {config['base_model']}...")
    tokenizer = AutoTokenizer.from_pretrained(config["base_model"])
    tokenizer.pad_token = tokenizer.eos_token
    tokenizer.padding_side = "right"

    # Step 2: Load model in 4-bit
    print(f"Loading model {config['base_model']} in 4-bit quantization...")
    bnb_config = build_quantization_config(config["qlora"])
    model = AutoModelForCausalLM.from_pretrained(
        config["base_model"],
        quantization_config=bnb_config,
        device_map="auto",
        torch_dtype=torch.bfloat16,
    )

    model = prepare_model_for_kbit_training(model)

    # Step 3: Add LoRA adapters
    print("Adding LoRA adapters...")
    lora_config = build_lora_config(config["lora"])
    model = get_peft_model(model, lora_config)
    model.print_trainable_parameters()

    # Step 4: Load dataset
    print(f"Loading dataset from {args.dataset}...")
    dataset = load_from_disk(args.dataset)
    print(f"  Train: {len(dataset['train'])} examples")
    print(f"  Validation: {len(dataset['validation'])} examples")

    # Step 5: Configure training
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
        report_to="none",
        max_steps=args.max_steps if args.max_steps else -1,
    )

    # Step 6: Train
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

    # Step 7: Save final adapter
    final_path = args.output + "/final"
    print(f"Saving final adapter to {final_path}...")
    trainer.save_model(final_path)
    print("Training complete.")


if __name__ == "__main__":
    main()
