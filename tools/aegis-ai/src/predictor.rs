//! Pure-Rust Gear 1 Structural Tree & Ontology Inference Engine (Tasks AI-11 & AI-12).
//!
//! Loads the compact trained `artifacts/gear1_model.json` artifact and performs
//! sub-millisecond CPU inference (`< 0.1 ms/query`) with zero external ML runtime
//! dependencies. Every predicted candidate is filtered and ranked through
//! [`crate::verifier::Verifier::select_best_of_n`] so 100% of returned ghost-node
//! suggestions pass `aegis-model` validation.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::dataset::Gear1Example;
use crate::verifier::{ScoredCandidate, Verifier, VerifyError};

/// Serialized sparse linear softmax head in `gear1_model.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SparseLinearHead {
    /// Ordered class labels.
    pub classes: Vec<String>,
    /// Sparse feature weights mapping `feature_key -> Vec<(class_index, weight)>`.
    pub sparse_weights: BTreeMap<String, Vec<(usize, f64)>>,
}

impl SparseLinearHead {
    /// Computes normalized softmax probabilities over `self.classes` for `feats`.
    #[must_use]
    pub fn predict_probs(&self, feats: &[String]) -> Vec<f64> {
        let n = self.classes.len();
        if n == 0 {
            return Vec::new();
        }
        let mut logits = vec![0.0_f64; n];
        for f in feats {
            if let Some(pairs) = self.sparse_weights.get(f) {
                for &(idx, w) in pairs {
                    if idx < n {
                        logits[idx] += w;
                    }
                }
            }
        }
        let max_l = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mut exps = Vec::with_capacity(n);
        let mut sum = 0.0_f64;
        for l in logits {
            let e = (l - max_l).exp();
            sum += e;
            exps.push(e);
        }
        let denom = if sum > 0.0 { sum } else { 1.0 };
        for e in &mut exps {
            *e /= denom;
        }
        exps
    }

    /// Returns the top-`k` `(class_label, probability)` predictions.
    #[must_use]
    pub fn predict_top_k(&self, feats: &[String], k: usize) -> Vec<(String, f64)> {
        let probs = self.predict_probs(feats);
        let mut indexed: Vec<(usize, f64)> = probs.into_iter().enumerate().collect();
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        indexed
            .into_iter()
            .take(k)
            .map(|(i, p)| (self.classes[i].clone(), p))
            .collect()
    }
}

/// Top-level JSON artifact produced by `scripts/train_gear1_scratch.py`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gear1ModelArtifact {
    /// Model identifier (`aegis-core-gear1-structural-v1`).
    pub model_name: String,
    /// Architecture name.
    pub architecture: String,
    /// License identifier (`Apache-2.0`).
    pub license: String,
    /// Number of training rows used.
    pub train_examples: usize,
    /// Number of held-out evaluation rows used.
    pub eval_examples: usize,
    /// Multi-task prediction heads (`target_node_kind`, `target_feasibility`, `target_catalog`, `target_label`).
    pub heads: BTreeMap<String, SparseLinearHead>,
}

/// Input query for predicting the next attack-tree node or sibling step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gear1Query {
    /// Language code (`en` or `ja`).
    pub lang: String,
    /// Exposed asset interfaces (`can`, `ethernet`, `cellular`, ...).
    pub interfaces: Vec<String>,
    /// STRIDE threat category (`tampering`, `spoofing`, `denial-of-service`, ...).
    pub stride: String,
    /// Parent gate kind (`or`, `and`, `seq_and`, or `root`).
    pub parent_gate: String,
    /// Tree depth (`0` for root, `1+` for child nodes).
    pub depth: usize,
    /// 0-based sibling index under the parent gate.
    pub sibling_index: usize,
    /// Active ancestor path and sibling context (`"Root [SEQ_AND] -> Gate [OR] :: Sibling 1"`).
    pub context_text: String,
}

impl From<&Gear1Example> for Gear1Query {
    fn from(ex: &Gear1Example) -> Self {
        Self {
            lang: ex.lang.clone(),
            interfaces: ex.interfaces.clone(),
            stride: ex.stride.clone(),
            parent_gate: ex.parent_gate.clone(),
            depth: ex.depth,
            sibling_index: ex.sibling_index,
            context_text: ex.context_text.clone(),
        }
    }
}

/// Multi-head prediction output from [`Gear1Predictor`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gear1Prediction {
    /// Top-3 predicted structural node kinds (`step`, `or`, `and`, `seq_and`).
    pub node_kinds: Vec<(String, f64)>,
    /// Top-3 predicted feasibility ratings (`high`, `medium`, `low`, `gate`).
    pub feasibilities: Vec<(String, f64)>,
    /// Top-3 predicted UN R155 Annex 5 catalog IDs (`4.1`, `5.1`, ...).
    pub catalogs: Vec<(String, f64)>,
    /// Top-3 predicted concrete attack step/gate labels (with `{threat}` and `{id}` instantiated).
    pub labels: Vec<(String, f64)>,
    /// Verifier-approved Best-of-N DSL ghost-node completions.
    pub verified_dsl_candidates: Vec<String>,
}

/// Evaluation metrics computed by running [`Gear1Predictor`] on a dataset split.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvalReport {
    /// Number of evaluated examples.
    pub num_examples: usize,
    /// Average single-query CPU inference time in microseconds.
    pub avg_latency_us: f64,
    /// Fraction of queries whose top-ranked ghost-node completion passed `Verifier`.
    pub verifier_pass_rate: f64,
    /// Top-1 and Top-3 accuracy per prediction head.
    pub heads: BTreeMap<String, HeadEvalMetrics>,
}

/// Top-1 and Top-3 accuracy for a single head.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeadEvalMetrics {
    /// Number of target classes in this head.
    pub num_classes: usize,
    /// Top-1 accuracy (`0.0..=1.0`).
    pub top1: f64,
    /// Top-3 accuracy (`0.0..=1.0`).
    pub top3: f64,
}

/// Pure-Rust Gear 1 Inference Engine.
#[derive(Clone, Debug)]
pub struct Gear1Predictor {
    artifact: Gear1ModelArtifact,
}

impl Gear1Predictor {
    /// Loads a trained [`Gear1Predictor`] from a JSON file on disk.
    ///
    /// # Errors
    ///
    /// Returns [`std::io::Error`] if reading or parsing the JSON file fails.
    pub fn load_from_path(path: &Path) -> std::io::Result<Self> {
        let raw = fs::read_to_string(path)?;
        let artifact: Gear1ModelArtifact =
            serde_json::from_str(&raw).map_err(std::io::Error::other)?;
        Ok(Self { artifact })
    }

    /// Returns a reference to the underlying model artifact metadata.
    #[must_use]
    pub const fn artifact(&self) -> &Gear1ModelArtifact {
        &self.artifact
    }

    /// Runs multi-head inference and Best-of-N verifier filtering for `query`.
    ///
    /// # Errors
    ///
    /// Returns [`VerifyError`] if all synthesized candidate completions are rejected.
    pub fn predict(
        &self,
        query: &Gear1Query,
        verifier: &Verifier,
    ) -> Result<Gear1Prediction, VerifyError> {
        let feats = extract_features(query);

        let node_kinds = self
            .artifact
            .heads
            .get("target_node_kind")
            .map(|h| h.predict_top_k(&feats, 3))
            .unwrap_or_default();
        let feasibilities = self
            .artifact
            .heads
            .get("target_feasibility")
            .map(|h| h.predict_top_k(&feats, 3))
            .unwrap_or_default();
        let catalogs = self
            .artifact
            .heads
            .get("target_catalog")
            .map(|h| h.predict_top_k(&feats, 3))
            .unwrap_or_default();
        let raw_labels = self
            .artifact
            .heads
            .get("target_label")
            .map(|h| h.predict_top_k(&feats, 3))
            .unwrap_or_default();

        let labels: Vec<(String, f64)> = raw_labels
            .into_iter()
            .map(|(tpl, p)| (instantiate_label(&tpl, &query.context_text), p))
            .collect();

        // Pick the best concrete feasibility rating (`high`, `medium`, `low`)
        let best_step_feas = feasibilities
            .iter()
            .find(|(f, _)| f != "gate")
            .map_or("medium", |(f, _)| f.as_str());

        // Build candidate DSL completions for Best-of-N verifier filtering
        let mut raw_candidates: Vec<(String, f64)> = Vec::new();
        for (lbl, lbl_prob) in &labels {
            let clean_lbl = sanitize_concrete_label(lbl);
            // Candidate as a concrete rated leaf step (`PredictSibling` / ghost step)
            let step_dsl = format!("<|step|>{clean_lbl}<|feas|>{best_step_feas}\n<|end|>");
            raw_candidates.push((step_dsl, *lbl_prob));
        }

        let scored: Vec<ScoredCandidate> = verifier.select_best_of_n(&raw_candidates)?;
        let verified_dsl_candidates = scored.into_iter().map(|s| s.dsl).collect();

        Ok(Gear1Prediction {
            node_kinds,
            feasibilities,
            catalogs,
            labels,
            verified_dsl_candidates,
        })
    }

    /// Evaluates the predictor across a slice of [`Gear1Example`]s and verifies
    /// every top-ranked completion through `verifier`.
    #[must_use]
    pub fn evaluate_split(&self, examples: &[Gear1Example], verifier: &Verifier) -> EvalReport {
        if examples.is_empty() {
            return EvalReport {
                num_examples: 0,
                avg_latency_us: 0.0,
                verifier_pass_rate: 1.0,
                heads: BTreeMap::new(),
            };
        }

        let mut top1_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut top3_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut verified_ok = 0usize;

        let t0 = Instant::now();
        for ex in examples {
            let query = Gear1Query::from(ex);
            if let Ok(pred) = self.predict(&query, verifier) {
                if !pred.verified_dsl_candidates.is_empty() {
                    verified_ok += 1;
                }
                check_top_k(
                    &pred.node_kinds,
                    &ex.target_node_kind,
                    "target_node_kind",
                    &mut top1_counts,
                    &mut top3_counts,
                );
                check_top_k(
                    &pred.feasibilities,
                    &ex.target_feasibility,
                    "target_feasibility",
                    &mut top1_counts,
                    &mut top3_counts,
                );
                check_top_k(
                    &pred.catalogs,
                    &ex.target_catalog,
                    "target_catalog",
                    &mut top1_counts,
                    &mut top3_counts,
                );
                check_top_k(
                    &pred.labels,
                    &ex.target_label,
                    "target_label",
                    &mut top1_counts,
                    &mut top3_counts,
                );
            }
        }
        let elapsed_us = t0.elapsed().as_secs_f64() * 1_000_000.0;
        #[allow(clippy::cast_precision_loss)]
        let n_f64 = examples.len() as f64;

        let mut heads = BTreeMap::new();
        for key in [
            "target_node_kind",
            "target_feasibility",
            "target_catalog",
            "target_label",
        ] {
            let num_classes = self.artifact.heads.get(key).map_or(0, |h| h.classes.len());
            #[allow(clippy::cast_precision_loss)]
            let t1 = *top1_counts.get(key).unwrap_or(&0) as f64 / n_f64;
            #[allow(clippy::cast_precision_loss)]
            let t3 = *top3_counts.get(key).unwrap_or(&0) as f64 / n_f64;
            heads.insert(
                key.to_owned(),
                HeadEvalMetrics {
                    num_classes,
                    top1: (t1 * 10000.0).round() / 10000.0,
                    top3: (t3 * 10000.0).round() / 10000.0,
                },
            );
        }

        #[allow(clippy::cast_precision_loss)]
        let verifier_pass_rate = verified_ok as f64 / n_f64;
        EvalReport {
            num_examples: examples.len(),
            avg_latency_us: ((elapsed_us / n_f64) * 100.0).round() / 100.0,
            verifier_pass_rate: (verifier_pass_rate * 10000.0).round() / 10000.0,
            heads,
        }
    }
}

fn check_top_k(
    preds: &[(String, f64)],
    target: &str,
    key: &'static str,
    top1: &mut BTreeMap<&'static str, usize>,
    top3: &mut BTreeMap<&'static str, usize>,
) {
    if preds.first().is_some_and(|(c, _)| c == target) {
        *top1.entry(key).or_default() += 1;
    }
    if preds.iter().take(3).any(|(c, _)| c == target) {
        *top3.entry(key).or_default() += 1;
    }
}

/// Extracts `(threat_title, catalog_id)` from `"Execute <title> (<TID|CAPEC-xxx>)"`
/// at the start of `context_text`, matching `parse_context_slots` in Python.
#[must_use]
pub fn parse_context_slots(context_text: &str) -> (String, String) {
    let trimmed = context_text.trim();
    let Some(rest) = trimmed.strip_prefix("Execute ") else {
        return (String::new(), String::new());
    };
    // Look for ` (TID-` or ` (CAPEC-`
    for prefix in [" (TID-", " (CAPEC-"] {
        if let Some(open_pos) = rest.find(prefix) {
            let threat = rest[..open_pos].trim();
            let after_open = &rest[open_pos + 2..]; // starts at `TID-` or `CAPEC-`
            if let Some(close_pos) = after_open.find(')') {
                let cid = &after_open[..close_pos];
                if cid.split_once('-').is_some_and(|(_, digits)| {
                    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
                }) {
                    return (threat.to_owned(), cid.to_owned());
                }
            }
        }
    }
    (String::new(), String::new())
}

/// Replaces scenario-specific threat title and catalog ID with `{threat}` and `{id}`.
#[must_use]
pub fn canonicalize_label(label: &str, context_text: &str) -> String {
    let (threat, cid) = parse_context_slots(context_text);
    let mut out = label.to_owned();
    if !threat.is_empty() && out.contains(&threat) {
        out = out.replace(&threat, "{threat}");
    }
    if !cid.is_empty() && out.contains(&cid) {
        out = out.replace(&cid, "{id}");
    }
    out
}

/// Substitutes `{threat}` and `{id}` placeholders using `context_text`.
#[must_use]
pub fn instantiate_label(template: &str, context_text: &str) -> String {
    let (threat, cid) = parse_context_slots(context_text);
    let mut out = template.to_owned();
    if !threat.is_empty() {
        out = out.replace("{threat}", &threat);
    }
    if !cid.is_empty() {
        out = out.replace("{id}", &cid);
    }
    out
}

fn sanitize_concrete_label(label: &str) -> String {
    label
        .replace("{threat}", "target embedded")
        .replace("{id}", "TID-101")
        .replace("{bus}", "CAN")
        .replace("{ecu}", "target ECU")
        .replace("{function}", "actuator command")
}

/// Tokenizes lowercase ASCII words (`[a-z0-9_]+`) and CJK runs, matching `WORD_RE` in Python.
fn tokenize_words(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut cur_is_cjk = false;

    for ch in text.chars() {
        let is_ascii_word = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_';
        let is_cjk =
            ('\u{3040}'..='\u{30ff}').contains(&ch) || ('\u{4e00}'..='\u{9fff}').contains(&ch);
        if is_ascii_word {
            if !cur.is_empty() && cur_is_cjk {
                tokens.push(std::mem::take(&mut cur));
            }
            cur_is_cjk = false;
            cur.push(ch);
        } else if is_cjk {
            if !cur.is_empty() && !cur_is_cjk {
                tokens.push(std::mem::take(&mut cur));
            }
            cur_is_cjk = true;
            cur.push(ch);
        } else if !cur.is_empty() {
            tokens.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

/// Deterministic sparse feature extractor matching `extract_features` in `scripts/train_gear1_scratch.py` 1-to-1.
#[must_use]
pub fn extract_features(query: &Gear1Query) -> Vec<String> {
    let lang = &query.lang;
    let stride = &query.stride;
    let parent = &query.parent_gate;
    let depth = query.depth;
    let sib_idx = query.sibling_index.min(4);

    let mut feats = vec![
        "bias".to_owned(),
        format!("lang={lang}"),
        format!("stride={stride}"),
        format!("parent={parent}"),
        format!("depth={depth}"),
        format!("sib_idx={sib_idx}"),
        format!("parent_depth={parent}_{depth}"),
        format!("parent_sib={parent}_{sib_idx}"),
        format!("depth_sib={depth}_{sib_idx}"),
        format!("stride_parent={stride}_{parent}"),
        format!("stride_depth={stride}_{depth}"),
    ];

    let mut ifaces = query.interfaces.clone();
    ifaces.sort();
    let iface_joined = ifaces.join(",");
    if !iface_joined.is_empty() {
        feats.push(format!("ifaces={iface_joined}"));
        feats.push(format!("ifaces_stride={iface_joined}_{stride}"));
        feats.push(format!("ifaces_depth={iface_joined}_{depth}"));
    }
    for iface in &ifaces {
        feats.push(format!("iface={iface}"));
        feats.push(format!("iface_stride={iface}_{stride}"));
        feats.push(format!("iface_parent={iface}_{parent}"));
        feats.push(format!("iface_depth={iface}_{depth}"));
    }

    let ctx_raw = &query.context_text;
    let (_, cid) = parse_context_slots(ctx_raw);
    if cid.starts_with("TID-") {
        feats.push("src_family=emb3d".to_owned());
        feats.push(format!("src_family_depth=emb3d_{depth}"));
        feats.push(format!("src_family_sib=emb3d_{sib_idx}"));
    } else if cid.starts_with("CAPEC-") {
        feats.push("src_family=capec".to_owned());
        feats.push(format!("src_family_depth=capec_{depth}"));
        feats.push(format!("src_family_sib=capec_{sib_idx}"));
    } else {
        feats.push("src_family=core".to_owned());
    }

    let (path_part, sib_part) = ctx_raw.split_once(" :: ").unwrap_or((ctx_raw.as_str(), ""));
    let sib_none = usize::from(sib_part.trim().is_empty() || sib_part.trim() == "none");
    feats.push(format!("sib_none={sib_none}"));

    if let Some(last_anc) = path_part
        .rsplit("->")
        .map(str::trim)
        .find(|s| !s.is_empty())
    {
        for w in tokenize_words(&last_anc.to_lowercase())
            .into_iter()
            .take(12)
        {
            feats.push(format!("pw={w}"));
            feats.push(format!("pw_sib={w}_{sib_idx}"));
        }
    }

    for w in tokenize_words(&sib_part.to_lowercase())
        .into_iter()
        .take(18)
    {
        feats.push(format!("sw={w}"));
    }

    let ctx_canon = canonicalize_label(ctx_raw, ctx_raw).to_lowercase();
    let words = tokenize_words(&ctx_canon);
    let mut seen_w = BTreeSet::new();
    for w in &words {
        if seen_w.insert(w.clone()) {
            feats.push(format!("w={w}"));
            let chars: Vec<char> = w.chars().collect();
            if chars.len() >= 4 {
                let limit = (chars.len() - 2).min(6);
                for i in 0..limit {
                    let g3: String = chars[i..i + 3].iter().collect();
                    feats.push(format!("g3={g3}"));
                }
            }
        }
    }
    let bg_limit = words.len().saturating_sub(1).min(24);
    for i in 0..bg_limit {
        feats.push(format!("bg={}_{}", words[i], words[i + 1]));
    }

    feats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_slot_extraction_and_instantiation_round_trips() {
        let ctx = "Execute Power Consumption Analysis Side Channel (TID-101) [SEQ_AND] -> Exploit Power Consumption Analysis Side Channel weakness (TID-101) [OR] :: none";
        let (threat, cid) = parse_context_slots(ctx);
        assert_eq!(threat, "Power Consumption Analysis Side Channel");
        assert_eq!(cid, "TID-101");

        let concrete = "Trigger Power Consumption Analysis Side Channel condition on Zone Controller & HSM Hardware Module";
        let tpl = canonicalize_label(concrete, ctx);
        assert_eq!(
            tpl,
            "Trigger {threat} condition on Zone Controller & HSM Hardware Module"
        );
        assert_eq!(instantiate_label(&tpl, ctx), concrete);
    }

    #[test]
    fn gear1_predictor_synthesizes_verifier_approved_dsl_candidates() {
        let mut heads = BTreeMap::new();
        heads.insert(
            "target_node_kind".to_owned(),
            SparseLinearHead {
                classes: vec!["and".into(), "or".into(), "seq_and".into(), "step".into()],
                sparse_weights: BTreeMap::from([("bias".into(), vec![(3, 4.0)])]),
            },
        );
        heads.insert(
            "target_feasibility".to_owned(),
            SparseLinearHead {
                classes: vec!["gate".into(), "high".into(), "low".into(), "medium".into()],
                sparse_weights: BTreeMap::from([("bias".into(), vec![(3, 3.5), (1, 1.0)])]),
            },
        );
        heads.insert(
            "target_catalog".to_owned(),
            SparseLinearHead {
                classes: vec!["4.1".into(), "5.2".into()],
                sparse_weights: BTreeMap::from([("bias".into(), vec![(1, 2.5)])]),
            },
        );
        heads.insert(
            "target_label".to_owned(),
            SparseLinearHead {
                classes: vec![
                    "Trigger {threat} condition on Zone Controller & HSM Hardware Module".into(),
                    "Exploit physical side-channel or fault-injection weakness (CWE-1300/CWE-1240)"
                        .into(),
                ],
                sparse_weights: BTreeMap::from([("bias".into(), vec![(0, 3.0), (1, 2.0)])]),
            },
        );

        let predictor = Gear1Predictor {
            artifact: Gear1ModelArtifact {
                model_name: "test-gear1".into(),
                architecture: "sparse-test".into(),
                license: "Apache-2.0".into(),
                train_examples: 10,
                eval_examples: 2,
                heads,
            },
        };
        let query = Gear1Query {
            lang: "en".into(),
            interfaces: vec!["debug-port".into()],
            stride: "tampering".into(),
            parent_gate: "or".into(),
            depth: 2,
            sibling_index: 0,
            context_text: "Execute Electromagnetic Side Channel (TID-102) [SEQ_AND] -> Exploit Electromagnetic Side Channel weakness (TID-102) [OR] :: none".into(),
        };
        let verifier = Verifier::iso21434_r155();
        let pred = predictor
            .predict(&query, &verifier)
            .expect("prediction passes verifier");
        assert_eq!(pred.node_kinds[0].0, "step");
        assert_eq!(pred.feasibilities[0].0, "medium");
        assert_eq!(
            pred.labels[0].0,
            "Trigger Electromagnetic Side Channel condition on Zone Controller & HSM Hardware Module"
        );
        assert_eq!(pred.verified_dsl_candidates.len(), 2);
        assert!(pred.verified_dsl_candidates[0].contains("Electromagnetic Side Channel"));
    }
}
