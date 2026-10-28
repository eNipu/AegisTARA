#!/bin/sh
# Task AI-08: Fine-tune and fuse Gear 2 ("Aegis-Core") on Apple Silicon (M2 Max)
# using Apple's MLX framework (`mlx-lm`).
#
# Default base model: `Qwen/Qwen2.5-0.5B` (Apache-2.0, supports English + Japanese
# per Decision D13). You can override with `BASE_MODEL=HuggingFaceTB/SmolLM2-360M`
# for an even smaller English-first model.
#
# Usage:
#   sh tools/aegis-ai/scripts/train_gear2_mlx.sh
set -eu

repo_root=$(cd "$(dirname "$0")/../../.." && pwd)
data_dir="$repo_root/tools/aegis-ai/data/mlx"
out_dir="$repo_root/tools/aegis-ai/artifacts/gear2"
base_model="${BASE_MODEL:-Qwen/Qwen2.5-0.5B}"
iters="${ITERS:-400}"

if [ ! -f "$data_dir/train.jsonl" ]; then
    echo "Generating compiler-verified dataset splits first..."
    cargo run --manifest-path "$repo_root/tools/aegis-ai/Cargo.toml" -- generate --out-dir "$repo_root/tools/aegis-ai/data"
fi

if ! python3 -c "import mlx_lm" >/dev/null 2>&1; then
    echo "note: mlx-lm is not installed in the active Python environment."
    echo "      To install on your Apple M2 Max:"
    echo "        python3 -m venv .venv-ai && . .venv-ai/bin/activate"
    echo "        pip install mlx mlx-lm"
    echo "      Then re-run: sh tools/aegis-ai/scripts/train_gear2_mlx.sh"
    exit 0
fi

mkdir -p "$out_dir"

echo "1/2 Fine-tuning $base_model on verified TARA DSL ($data_dir)..."
python3 -m mlx_lm.lora \
    --model "$base_model" \
    --train \
    --data "$data_dir" \
    --iters "$iters" \
    --batch-size 4 \
    --num-layers 16 \
    --learning-rate 2e-4 \
    --adapter-path "$out_dir/adapters"

echo "2/2 Fusing adapter weights and quantizing to 4-bit for fast local inference..."
python3 -m mlx_lm.fuse \
    --model "$base_model" \
    --adapter-path "$out_dir/adapters" \
    --save-path "$out_dir/aegis-core-q4" \
    --de-quantize

echo "ok    Gear-2 model saved to $out_dir/aegis-core-q4"
