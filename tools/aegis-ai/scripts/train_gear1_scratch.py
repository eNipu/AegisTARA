#!/usr/bin/env python3
"""
Task AI-06 (Step 3): Train "Gear 1" Structural Attack Tree & Ontology Predictor
from Scratch on M2 Max.

Trains a compact multi-head classifier + template-aware step/gate predictor
from scratch on the verified `gear1/{train,valid,test}.jsonl` splits produced
by `aegis-ai generate`.

Feature extraction and template instantiation are designed to match the
pure-Rust inference engine in `tools/aegis-ai/src/predictor.rs` 1-to-1.
"""

from __future__ import annotations

import argparse
import json
import math
import random
import re
import time
from pathlib import Path

WORD_RE = re.compile(r"[a-z0-9_]+|[\u3040-\u30ff\u4e00-\u9fff]+")
ROOT_HEADER_RE = re.compile(r"^Execute\s+(.+?)\s+\(((?:TID|CAPEC)-\d+)\)")


def parse_context_slots(context_text: str) -> tuple[str, str]:
    """
    Extracts `(threat_title, catalog_id)` from the root gate prefix in
    `context_text` when present, e.g.:
      "Execute Power Consumption Analysis Side Channel (TID-101) [SEQ_AND] ..."
      -> ("Power Consumption Analysis Side Channel", "TID-101")
    """
    m = ROOT_HEADER_RE.match(context_text.strip())
    if not m:
        return "", ""
    return m.group(1).strip(), m.group(2).strip()


def canonicalize_label(label: str, context_text: str) -> str:
    """
    Replaces scenario-specific `{threat}` and `{id}` substrings in `label`
    with `{threat}` and `{id}` placeholders when those exact strings are
    already present in the active `context_text` root header.
    """
    threat, cid = parse_context_slots(context_text)
    out = label
    if threat and threat in out:
        out = out.replace(threat, "{threat}")
    if cid and cid in out:
        out = out.replace(cid, "{id}")
    return out


def instantiate_label(template: str, context_text: str) -> str:
    """
    Substitutes `{threat}` and `{id}` placeholders in a predicted template
    using the active `context_text` root header.
    """
    threat, cid = parse_context_slots(context_text)
    out = template
    if threat:
        out = out.replace("{threat}", threat)
    if cid:
        out = out.replace("{id}", cid)
    return out


def extract_features(row: dict) -> list[str]:
    """
    Deterministic sparse feature extractor shared between Python training and
    Rust inference (`src/predictor.rs`).
    """
    lang = row.get("lang", "en")
    stride = row.get("stride", "tampering")
    parent = row.get("parent_gate", "root")
    depth = int(row.get("depth", 0))
    sib_idx = min(int(row.get("sibling_index", 0)), 4)

    feats: list[str] = [
        "bias",
        f"lang={lang}",
        f"stride={stride}",
        f"parent={parent}",
        f"depth={depth}",
        f"sib_idx={sib_idx}",
        f"parent_depth={parent}_{depth}",
        f"parent_sib={parent}_{sib_idx}",
        f"depth_sib={depth}_{sib_idx}",
        f"stride_parent={stride}_{parent}",
        f"stride_depth={stride}_{depth}",
    ]

    ifaces = sorted(row.get("interfaces", []))
    iface_joined = ",".join(ifaces)
    if iface_joined:
        feats.append(f"ifaces={iface_joined}")
        feats.append(f"ifaces_stride={iface_joined}_{stride}")
        feats.append(f"ifaces_depth={iface_joined}_{depth}")
    for iface in ifaces:
        feats.append(f"iface={iface}")
        feats.append(f"iface_stride={iface}_{stride}")
        feats.append(f"iface_parent={iface}_{parent}")
        feats.append(f"iface_depth={iface}_{depth}")

    ctx_raw = row.get("context_text", "")
    _, cid = parse_context_slots(ctx_raw)
    if cid.startswith("TID-"):
        feats.append("src_family=emb3d")
        feats.append(f"src_family_depth=emb3d_{depth}")
        feats.append(f"src_family_sib=emb3d_{sib_idx}")
    elif cid.startswith("CAPEC-"):
        feats.append("src_family=capec")
        feats.append(f"src_family_depth=capec_{depth}")
        feats.append(f"src_family_sib=capec_{sib_idx}")
    else:
        feats.append("src_family=core")

    # Separate ancestor path and active siblings (`path :: siblings`)
    if " :: " in ctx_raw:
        path_part, sib_part = ctx_raw.split(" :: ", 1)
    else:
        path_part, sib_part = ctx_raw, ""

    feats.append(f"sib_none={1 if sib_part.strip() in ('', 'none') else 0}")

    # Words from the last ancestor gate (immediate parent label)
    anc_segments = [s.strip() for s in path_part.split("->") if s.strip()]
    if anc_segments:
        last_anc = anc_segments[-1].lower()
        for w in WORD_RE.findall(last_anc)[:12]:
            feats.append(f"pw={w}")
            feats.append(f"pw_sib={w}_{sib_idx}")

    # Words from sibling context
    sib_words = WORD_RE.findall(sib_part.lower())
    for w in sib_words[:18]:
        feats.append(f"sw={w}")

    # Full context words + character 3-grams
    ctx_canon = canonicalize_label(ctx_raw, ctx_raw).lower()
    words = WORD_RE.findall(ctx_canon)
    seen_w = set()
    for w in words:
        if w not in seen_w:
            seen_w.add(w)
            feats.append(f"w={w}")
            if len(w) >= 4:
                for i in range(min(len(w) - 2, 6)):
                    feats.append(f"g3={w[i:i+3]}")
    for i in range(min(len(words) - 1, 24)):
        feats.append(f"bg={words[i]}_{words[i+1]}")

    return feats


class MultiClassHead:
    """Linear softmax head trained from scratch with L2 regularization."""

    def __init__(self, classes: list[str]) -> None:
        self.classes = sorted(set(classes))
        self.class_to_idx = {c: i for i, c in enumerate(self.classes)}
        self.weights: dict[str, list[float]] = {}

    def _ensure_feat(self, feat: str) -> list[float]:
        w = self.weights.get(feat)
        if w is None:
            w = [0.0] * len(self.classes)
            self.weights[feat] = w
        return w

    def predict_probs(self, feats: list[str]) -> list[float]:
        n = len(self.classes)
        if n == 0:
            return []
        logits = [0.0] * n
        for f in feats:
            w = self.weights.get(f)
            if w is not None:
                for i, val in enumerate(w):
                    logits[i] += val
        max_l = max(logits)
        exps = [math.exp(l - max_l) for l in logits]
        total = sum(exps) or 1.0
        return [e / total for e in exps]

    def predict_top_k(self, feats: list[str], k: int = 3) -> list[tuple[str, float]]:
        probs = self.predict_probs(feats)
        ranked = sorted(enumerate(probs), key=lambda x: x[1], reverse=True)[:k]
        return [(self.classes[i], round(p, 4)) for i, p in ranked]

    def step(self, feats: list[str], target_class: str, lr: float, l2: float) -> float:
        target_idx = self.class_to_idx.get(target_class)
        if target_idx is None:
            return 0.0
        probs = self.predict_probs(feats)
        loss = -math.log(max(probs[target_idx], 1e-12))
        scale = lr / max(math.sqrt(len(feats)), 1.0)
        decay = 1.0 - lr * l2
        # Only update classes with non-negligible probability (> 2e-3) + target_idx
        active_indices = [
            (i, probs[i] * scale)
            for i in range(len(self.classes))
            if i == target_idx or probs[i] > 2e-3
        ]
        for f in feats:
            w = self._ensure_feat(f)
            for i, step_delta in active_indices:
                w[i] *= decay
                if i == target_idx:
                    w[i] += scale - step_delta
                else:
                    w[i] -= step_delta
        return loss

    def to_dict(self) -> dict:
        sparse: dict[str, list[list[float]]] = {}
        for k, vals in self.weights.items():
            pairs = [[i, round(v, 4)] for i, v in enumerate(vals) if abs(v) > 1.5e-2]
            if pairs:
                sparse[k] = pairs
        return {
            "classes": self.classes,
            "sparse_weights": sparse,
        }


def load_jsonl(path: Path) -> list[dict]:
    rows = []
    if not path.exists():
        return rows
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows


def evaluate_head(
    head: MultiClassHead,
    dataset: list[tuple[list[str], dict]],
    key: str,
    instantiate: bool = False,
) -> tuple[float, float]:
    if not dataset:
        return 0.0, 0.0
    top1 = 0
    top3 = 0
    for feats, row in dataset:
        preds = [c for c, _ in head.predict_top_k(feats, k=3)]
        if instantiate:
            ctx = row.get("context_text", "")
            preds = [instantiate_label(p, ctx) for p in preds]
        target = row[key]
        if preds and preds[0] == target:
            top1 += 1
        if target in preds:
            top3 += 1
    n = len(dataset)
    return top1 / n, top3 / n


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Train Gear 1 Structural Tree Predictor from scratch."
    )
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=Path("tools/aegis-ai/data/gear1"),
        help="Directory containing Gear 1 train/valid/test JSONL splits",
    )
    parser.add_argument(
        "--epochs",
        type=int,
        default=28,
        help="Number of training epochs (default: 28)",
    )
    parser.add_argument(
        "--lr",
        type=float,
        default=0.18,
        help="Initial learning rate (default: 0.18)",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("tools/aegis-ai/artifacts/gear1_model.json"),
        help="Output path for trained Gear 1 model JSON",
    )
    args = parser.parse_args()

    train_rows = load_jsonl(args.data_dir / "train.jsonl")
    valid_rows = load_jsonl(args.data_dir / "valid.jsonl")
    test_rows = load_jsonl(args.data_dir / "test.jsonl")

    if not train_rows:
        raise SystemExit(
            f"error: no training rows found in {args.data_dir}; run `cargo run -- generate` first."
        )

    eval_rows = valid_rows + test_rows if (valid_rows or test_rows) else train_rows

    # Add canonical template label to each row for training the label head
    for r in train_rows + eval_rows:
        r["target_label_template"] = canonicalize_label(
            r["target_label"], r.get("context_text", "")
        )

    all_rows = train_rows + eval_rows
    heads = {
        "target_node_kind": MultiClassHead([r["target_node_kind"] for r in all_rows]),
        "target_feasibility": MultiClassHead([r["target_feasibility"] for r in all_rows]),
        "target_catalog": MultiClassHead([r["target_catalog"] for r in all_rows]),
        "target_label": MultiClassHead([r["target_label_template"] for r in all_rows]),
    }

    train_data = [(extract_features(r), r) for r in train_rows]
    eval_data = [(extract_features(r), r) for r in eval_rows]

    rng = random.Random(42)
    t0 = time.perf_counter()
    for epoch in range(1, args.epochs + 1):
        # Cosine learning-rate schedule
        progress = (epoch - 1) / max(args.epochs - 1, 1)
        lr = args.lr * (0.15 + 0.85 * 0.5 * (1.0 + math.cos(math.pi * progress)))
        shuffled = list(train_data)
        rng.shuffle(shuffled)
        for feats, row in shuffled:
            heads["target_node_kind"].step(feats, row["target_node_kind"], lr=lr, l2=1e-4)
            heads["target_feasibility"].step(feats, row["target_feasibility"], lr=lr, l2=1e-4)
            heads["target_catalog"].step(feats, row["target_catalog"], lr=lr, l2=1e-4)
            heads["target_label"].step(feats, row["target_label_template"], lr=lr, l2=5e-5)
    elapsed_ms = (time.perf_counter() - t0) * 1000.0

    # Benchmark single-query CPU inference latency across all 4 heads
    sample_feats = train_data[0][0]
    t_inf0 = time.perf_counter()
    iters = 500
    for _ in range(iters):
        for h in heads.values():
            h.predict_top_k(sample_feats, k=3)
    avg_inf_ms = ((time.perf_counter() - t_inf0) * 1000.0) / iters

    metrics = {}
    for key, head in heads.items():
        eval_key = "target_label" if key == "target_label" else key
        is_lbl = key == "target_label"
        tr_top1, tr_top3 = evaluate_head(head, train_data, eval_key, instantiate=is_lbl)
        ev_top1, ev_top3 = evaluate_head(head, eval_data, eval_key, instantiate=is_lbl)
        metrics[key] = {
            "num_classes": len(head.classes),
            "train_top1": round(tr_top1, 4),
            "train_top3": round(tr_top3, 4),
            "eval_top1": round(ev_top1, 4),
            "eval_top3": round(ev_top3, 4),
        }

    artifact = {
        "model_name": "aegis-core-gear1-structural-v1",
        "architecture": "multi-head-sparse-ngram-template-softmax",
        "license": "Apache-2.0",
        "train_examples": len(train_rows),
        "eval_examples": len(eval_rows),
        "train_time_ms": round(elapsed_ms, 2),
        "avg_cpu_inference_ms": round(avg_inf_ms, 4),
        "metrics": metrics,
        "heads": {k: h.to_dict() for k, h in heads.items()},
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(artifact, ensure_ascii=False, indent=2), encoding="utf-8")
    size_kb = args.out.stat().st_size / 1024.0

    print(
        f"ok    trained Gear-1 Structural Tree Predictor from scratch in {elapsed_ms:.1f} ms "
        f"(model size: {size_kb:.1f} KB, CPU latency: {avg_inf_ms:.3f} ms/query)"
    )
    for key, m in metrics.items():
        print(
            f"      head {key:<20} classes={m['num_classes']:<4} "
            f"train_top1={m['train_top1']*100:5.1f}%  "
            f"eval_top1={m['eval_top1']*100:5.1f}%  "
            f"eval_top3={m['eval_top3']*100:5.1f}%"
        )


if __name__ == "__main__":
    main()
