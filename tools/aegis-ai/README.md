# `aegis-ai`: In-House Compact TARA & Attack Tree Model Subproject

This subproject builds **Aegis-Core**, an in-house, domain-restricted AI engine for `AegisTARA` designed to:

1. **Autocompiler / Predictive Attack Tree Completion:** Predict missing sibling steps, expand leaf steps into `AND` / `OR` / `SEQ_AND` subtrees, draft candidate attack trees from threat scenarios, and bind template slots (`{ecu}`, `{bus}`, `{function}`).
2. **Related TARA Components:** Propose threat scenarios (with STRIDE categories and UN R155 Annex 5 / MITRE CAPEC / MITRE EMB3D / CWE citations) and damage scenarios from assets, security properties, and interfaces.
3. **Strict Domain Purity:** Hold **only** cybersecurity and TARA knowledge and immediately refuse out-of-domain prompts with `<|out_of_domain|>`.
4. **CPU-First Speed & Compact Size:** Run in **~93 µs (0.093 ms) on CPU** for structural/catalog/ghost-node predictions (Gear 1, **~4.8 MB**) and **~120 ms on CPU** for open-ended prose/subtree drafting (Gear 2, **~95–210 MB** 4-bit quantized), trained locally on an **Apple M2 Max (32 GB unified memory)**.
5. **Deterministic Engine Guarantee (`D6`, `D12`):** Every training example and runtime output is validated through `crates/aegis-model` (`AttackTree`, `Methodology`, `evaluate`, `Project::lint`). The AI never computes risk or feasibility scores; the deterministic engine does.

---

## Subproject Task Breakdown (Backlog & Status)

The roadmap is broken down into **12 verifiable tasks** across 5 milestones:

| ID | Task | Deliverable | Status |
|---|---|---|---|
| **AI-01** | **Compact TARA DSL & Parser** | `src/dsl.rs`: Token-efficient domain language (`<|meth|>`, `<|asset|>`, `<|gate:or|>`, `<|gate:seq_and|>`, `<|step|>`, `<|out_of_domain|>`) + bidirectional conversion to/from `aegis_model::library::ItemNode` and `AttackTree`. | **Done** |
| **AI-02** | **Deterministic `aegis-model` Verifier Gate** | `src/verifier.rs`: Compiler-in-the-loop gate that checks every candidate tree/threat against `AttackTree` DAG rules, `Methodology` vocabularies, `evaluate(MaxMin)`, and `Project::lint()`. | **Done** |
| **AI-03** | **English-First + MITRE EMB3D/CAPEC Corpus** | `src/corpus.rs`, `scripts/fetch_open_cyber_catalogs.py`, `packs/open_cyber_en.json`: Curated ISO/SAE 21434 + UN R155 trees plus 166 open-source MITRE EMB3D v2.0.1 and MITRE CAPEC STIX 2.1 scenarios (`ja` supported via `build_verified_with_options(true)`). | **Done** |
| **AI-04** | **Combinatorial Tree Masking & Split Exporter** | `src/dataset.rs`: Cuts complete trees at every gate, leaf, and sibling position (`3,222` SFT examples, `2,668` Gear-1 structural rows, `1,598` DPO pairs) and exports verified `train.jsonl`, `valid.jsonl`, and `test.jsonl`. | **Done** |
| **AI-05** | **Dataset & Verification CLI (`aegis-ai`)** | `src/main.rs`: CLI (`generate`, `stats`, `verify-dsl`, `predict`, `eval`) to build datasets, inspect distributions, run live ghost-node predictions, and benchmark accuracy. | **Done** |
| **AI-06** | **From-Scratch Gear 1 Structural Predictor** | `scripts/train_gear1_scratch.py`: Trains a template-aware multi-head sparse softmax model (`4.8 MB`) predicting `target_node_kind` (`91.9%` Top-1 / `100%` Top-3), `target_feasibility` (`95.8%` Top-1 / `100%` Top-3), `target_catalog` (`99.8%` Top-1 / `100%` Top-3), and `target_label` (`79.6%` Top-1 / `95.0%` Top-3). | **Done** |
| **AI-07** | **Tokenizer Vocabulary Surgery & Domain Pruner** | `scripts/prune_vocab.py`: Builds a pure cybersecurity/TARA vocabulary (`1,937` tokens, `29` special DSL tokens) from `vocab_corpus.txt`. | **Done** |
| **AI-08** | **Apple `MLX` SFT / QLoRA Pipeline (Gear 2)** | `scripts/train_gear2_mlx.sh`: One-command training script for `HuggingFaceTB/SmolLM2-135M/360M` or `Qwen/Qwen2.5-0.5B` (Apache-2.0) using Apple Silicon Metal (`mlx-lm`) on `mlx/train.jsonl`. | **Done** |
| **AI-09** | **Best-of-N Verifier-Guided Rejection Sampler** | `src/verifier.rs` (`verify_tree_dsl_strict`, `select_best_of_n`): Filters out any candidate that fails DSL syntax, vocabulary, `MaxMin`, `Project::lint()`, single-child gate rules, or duplicate-sibling checks. | **Done** |
| **AI-10** | **DPO Preference Pair Builder** | `src/dpo.rs`: Synthesizes `1,598` verified `(prompt, chosen, rejected, reject_reason)` pairs across 5 structural and ontology failure modes into `data/dpo/{train,valid,test}.jsonl`. | **Done** |
| **AI-11** | **Pure-Rust Gear 1 CPU Inference Engine** | `src/predictor.rs`: Loads `artifacts/gear1_model.json` in pure Rust and executes multi-head prediction + Best-of-N verifier filtering in **93.2 µs/query** on CPU with a **100.0% verifier pass rate**. | **Done** |
| **AI-12** | **End-to-End Evaluation & Prediction CLI** | `src/main.rs` (`predict`, `eval`): Evaluates held-out test splits and emits verifier-approved ghost-node completions ready for `Provenance::ai` insertion. | **Done** |

---

## Quick Start on Your MacBook Pro (Apple M2 Max, 32 GB)

### 1. Fetch Open-Source MITRE Catalogs & Generate Verified Datasets

From the repository root:

```sh
python3 tools/aegis-ai/scripts/fetch_open_cyber_catalogs.py
cargo test --manifest-path tools/aegis-ai/Cargo.toml
cargo run --release --manifest-path tools/aegis-ai/Cargo.toml -- generate --out-dir tools/aegis-ai/data
cargo run --release --manifest-path tools/aegis-ai/Cargo.toml -- stats
```

### 2. Prune Domain Vocabulary & Train Gear 1 From Scratch

```sh
python3 tools/aegis-ai/scripts/prune_vocab.py
python3 tools/aegis-ai/scripts/train_gear1_scratch.py --epochs 20
```

### 3. Benchmark & Run Pure-Rust CPU Inference (`eval` & `predict`)

```sh
cargo run --release --manifest-path tools/aegis-ai/Cargo.toml -- eval
cargo run --release --manifest-path tools/aegis-ai/Cargo.toml -- predict \
  --interfaces "debug-port,jtag" --stride "tampering" --parent-gate "or" --depth 2 --sibling-index 1 \
  "Execute Power Consumption Analysis Side Channel (TID-101) [SEQ_AND] -> Exploit Power Consumption Analysis Side Channel weakness (TID-101) [OR] :: Trigger Power Consumption Analysis Side Channel condition on Zone Controller & HSM Hardware Module"
```

### 4. Fine-Tune & Fuse Gear 2 (`SmolLM2-135M/360M` or `Qwen2.5-0.5B`) with Apple `MLX`

```sh
sh tools/aegis-ai/scripts/train_gear2_mlx.sh
```
