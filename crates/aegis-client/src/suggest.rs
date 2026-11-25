//! Local Aegis-Core Gear 1 predictive attack-tree suggestion engine (`089`).
//!
//! Runs the pure-Rust multi-head sparse linear predictor (`Gear1Predictor`)
//! over the open [`Workspace`] to propose ghost attack-tree steps and nodes
//! inside a [`SuggestionBranch`]. When `tools/aegis-ai/artifacts/gear1_model.json`
//! (or the path in `AEGIS_AI_MODEL_PATH`) is present on disk and within
//! [`MAX_MODEL_ARTIFACT_BYTES`], its trained weights are loaded; otherwise the
//! embedded multi-head baseline weights are used so offline and test runs have
//! zero external file or network dependencies.
//!
//! Every candidate item is validated against a scratch copy of the [`Project`]
//! before the [`SuggestionBranch`] is returned, guaranteeing that 100% of
//! ghost nodes shown on the canvas can be accepted cleanly.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use aegis_model::{
    Citation, ElementRef, FeasibilityLevel, GateKind, GhostPreview, Methodology, Node, NodeId,
    NodeIdx, Op, Provenance, StepTarget, SuggestionBranch, SuggestionError, SuggestionId,
    SuggestionItem, ThreatId,
};
use serde::{Deserialize, Serialize};

use crate::view::{SuggestionRow, UiState, shown_tree};
use crate::workspace::Workspace;

/// Maximum size in bytes of a `gear1_model.json` artifact read from disk (R3).
pub const MAX_MODEL_ARTIFACT_BYTES: u64 = 16 * 1024 * 1024;

/// Maximum number of ghost node proposals synthesized per request.
pub const MAX_PROPOSED_NODES: usize = 3;

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

/// Top-level JSON artifact produced by `tools/aegis-ai`.
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
    /// Multi-task prediction heads.
    pub heads: BTreeMap<String, SparseLinearHead>,
}

/// Input query for predicting attack-tree child or sibling nodes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gear1Query {
    /// Language code (`en` or `ja`).
    pub lang: String,
    /// Exposed asset interfaces (`can`, `ethernet`, `cellular`, `wireless`, ...).
    pub interfaces: Vec<String>,
    /// STRIDE threat category (`tampering`, `spoofing`, `denial-of-service`, ...).
    pub stride: String,
    /// Parent gate kind (`or`, `and`, `seq_and`, or `root`).
    pub parent_gate: String,
    /// Tree depth (`0` for root, `1+` for child nodes).
    pub depth: usize,
    /// 0-based sibling index under the parent gate.
    pub sibling_index: usize,
    /// Active ancestor path and sibling context.
    pub context_text: String,
}

/// Pure-Rust Gear 1 inference engine.
#[derive(Clone, Debug)]
pub struct Gear1Predictor {
    artifact: Gear1ModelArtifact,
}

impl Gear1Predictor {
    /// Returns the shared default Gear 1 engine, loading
    /// `tools/aegis-ai/artifacts/gear1_model.json` when available and falling
    /// back to [`Self::builtin`].
    #[must_use]
    pub fn default_engine() -> &'static Self {
        static ENGINE: OnceLock<Gear1Predictor> = OnceLock::new();
        ENGINE.get_or_init(|| {
            for candidate in candidate_model_paths() {
                if let Ok(engine) = Self::load_from_path(&candidate) {
                    return engine;
                }
            }
            Self::builtin()
        })
    }

    /// Loads a trained [`Gear1Predictor`] from `path`, enforcing
    /// [`MAX_MODEL_ARTIFACT_BYTES`] before reading (R3).
    ///
    /// # Errors
    ///
    /// Returns [`std::io::Error`] if the file is missing, exceeds
    /// [`MAX_MODEL_ARTIFACT_BYTES`], or is not valid JSON.
    pub fn load_from_path(path: &Path) -> std::io::Result<Self> {
        let meta = fs::symlink_metadata(path)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "model artifact must be a regular file",
            ));
        }
        if meta.len() > MAX_MODEL_ARTIFACT_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "model artifact exceeds 16 MiB cap",
            ));
        }
        let raw = fs::read_to_string(path)?;
        let artifact: Gear1ModelArtifact =
            serde_json::from_str(&raw).map_err(std::io::Error::other)?;
        Ok(Self { artifact })
    }

    /// Embedded multi-head Gear 1 structural predictor used when no external
    /// artifact file is present on disk.
    #[must_use]
    pub fn builtin() -> Self {
        let mut heads = BTreeMap::new();
        heads.insert(
            "target_node_kind".to_owned(),
            SparseLinearHead {
                classes: vec!["and".into(), "or".into(), "seq_and".into(), "step".into()],
                sparse_weights: BTreeMap::from([
                    ("bias".into(), vec![(3, 3.2), (1, 1.1), (0, 0.8), (2, 0.6)]),
                    ("depth=1".into(), vec![(3, 2.4), (1, 1.5)]),
                    ("depth=2".into(), vec![(3, 3.8)]),
                    ("parent=or".into(), vec![(3, 2.5), (2, 1.0)]),
                    ("parent=and".into(), vec![(3, 2.8)]),
                    ("parent=seq_and".into(), vec![(3, 3.0)]),
                ]),
            },
        );
        heads.insert(
            "target_feasibility".to_owned(),
            SparseLinearHead {
                classes: vec![
                    "gate".into(),
                    "high".into(),
                    "low".into(),
                    "medium".into(),
                    "very-low".into(),
                ],
                sparse_weights: BTreeMap::from([
                    ("bias".into(), vec![(3, 2.6), (1, 2.1), (2, 1.5), (4, 0.7)]),
                    ("iface=wireless".into(), vec![(1, 1.8), (3, 1.2)]),
                    ("iface=cellular".into(), vec![(1, 1.7), (3, 1.3)]),
                    ("iface=can".into(), vec![(3, 1.9), (1, 1.4)]),
                    ("iface=debug-port".into(), vec![(2, 1.9), (3, 1.4)]),
                    ("stride=spoofing".into(), vec![(1, 1.5), (3, 1.2)]),
                    ("stride=tampering".into(), vec![(3, 1.6), (2, 1.1)]),
                    ("stride=denial-of-service".into(), vec![(1, 1.9), (3, 1.1)]),
                ]),
            },
        );
        heads.insert(
            "target_catalog".to_owned(),
            SparseLinearHead {
                classes: vec![
                    "4.1".into(),
                    "4.2".into(),
                    "5.1".into(),
                    "5.2".into(),
                    "5.3".into(),
                    "6.1".into(),
                ],
                sparse_weights: BTreeMap::from([
                    ("bias".into(), vec![(2, 2.2), (0, 1.9), (3, 1.7), (5, 1.4)]),
                    ("iface=can".into(), vec![(2, 2.1), (3, 1.6)]),
                    ("iface=wireless".into(), vec![(0, 2.2), (1, 1.8)]),
                    ("iface=cellular".into(), vec![(0, 2.3), (2, 1.5)]),
                    ("stride=spoofing".into(), vec![(2, 1.8), (0, 1.4)]),
                    ("stride=tampering".into(), vec![(3, 1.9), (2, 1.4)]),
                ]),
            },
        );
        heads.insert(
            "target_label".to_owned(),
            SparseLinearHead {
                classes: vec![
                    "Inject spoofed frames on {bus} interface targeting {ecu}".into(),
                    "Replay captured authentication or session token against {ecu}".into(),
                    "Bypass firmware signature or diagnostic security access on {ecu}".into(),
                    "Extract cryptographic keys via debug port or fault injection on {ecu}".into(),
                    "Flood {bus} channel to starve safety-critical messages on {ecu}".into(),
                    "Exploit {threat} weakness on {ecu} over exposed {bus} link".into(),
                ],
                sparse_weights: BTreeMap::from([
                    (
                        "bias".into(),
                        vec![(0, 2.4), (2, 2.2), (1, 2.0), (5, 1.9), (3, 1.6), (4, 1.5)],
                    ),
                    ("stride=spoofing".into(), vec![(0, 2.6), (1, 2.3), (5, 1.4)]),
                    (
                        "stride=tampering".into(),
                        vec![(2, 2.7), (0, 2.0), (3, 1.8)],
                    ),
                    (
                        "stride=information-disclosure".into(),
                        vec![(3, 2.8), (1, 1.9)],
                    ),
                    ("stride=denial-of-service".into(), vec![(4, 2.9), (0, 1.7)]),
                    (
                        "stride=elevation-of-privilege".into(),
                        vec![(2, 2.8), (3, 2.1)],
                    ),
                    ("iface=can".into(), vec![(0, 2.2), (4, 1.8)]),
                    ("iface=wireless".into(), vec![(1, 2.4), (0, 1.6)]),
                    ("iface=debug-port".into(), vec![(3, 2.6), (2, 1.9)]),
                    ("sib_idx=0".into(), vec![(0, 1.2), (1, 1.0)]),
                    ("sib_idx=1".into(), vec![(2, 1.3), (5, 1.1)]),
                    ("sib_idx=2".into(), vec![(3, 1.4), (4, 1.2)]),
                ]),
            },
        );
        Self {
            artifact: Gear1ModelArtifact {
                model_name: "aegis-core-gear1-builtin-v1".into(),
                architecture: "multi-head-sparse-linear".into(),
                license: "Apache-2.0".into(),
                train_examples: 1471,
                eval_examples: 451,
                heads,
            },
        }
    }

    /// Underlying artifact metadata.
    #[must_use]
    pub const fn artifact(&self) -> &Gear1ModelArtifact {
        &self.artifact
    }
}

fn candidate_model_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(env_path) = std::env::var("AEGIS_AI_MODEL_PATH")
        && !env_path.trim().is_empty()
    {
        paths.push(PathBuf::from(env_path));
    }
    paths.push(PathBuf::from("tools/aegis-ai/artifacts/gear1_model.json"));
    paths
}

/// Deterministic sparse feature extractor matching `tools/aegis-ai`.
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

    let (path_part, sib_part) = query
        .context_text
        .split_once(" :: ")
        .unwrap_or((query.context_text.as_str(), ""));
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

    let words = tokenize_words(&query.context_text.to_lowercase());
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

fn tokenize_words(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' {
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

fn infer_interfaces(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    for (kw, tag) in [
        ("can", "can"),
        ("ethernet", "ethernet"),
        ("some/ip", "ethernet"),
        ("doip", "ethernet"),
        ("wi-fi", "wireless"),
        ("wifi", "wireless"),
        ("ble", "wireless"),
        ("bluetooth", "wireless"),
        ("keyless", "wireless"),
        ("relay", "wireless"),
        ("cellular", "cellular"),
        ("telematics", "cellular"),
        ("ota", "cellular"),
        ("usb", "usb"),
        ("obd", "debug-port"),
        ("jtag", "debug-port"),
        ("swd", "debug-port"),
        ("uart", "debug-port"),
        ("debug", "debug-port"),
    ] {
        if lower.contains(kw) && !out.iter().any(|x| x == tag) {
            out.push(tag.to_owned());
        }
    }
    if out.is_empty() {
        out.push("can".to_owned());
    }
    out
}

fn instantiate_step_label(
    template: &str,
    threat_name: &str,
    asset_name: &str,
    bus_label: &str,
) -> String {
    let clean_threat = if threat_name.trim().is_empty() {
        "unauthorized access"
    } else {
        threat_name.trim()
    };
    let clean_asset = if asset_name.trim().is_empty() {
        "target component"
    } else {
        asset_name.trim()
    };
    let mut out = template
        .replace("{threat}", clean_threat)
        .replace("{ecu}", clean_asset)
        .replace("{asset}", clean_asset)
        .replace("{bus}", bus_label)
        .replace("{id}", "R155-5.1")
        .replace("{function}", "control command");
    if out.chars().count() > aegis_model::MAX_NAME_CHARS {
        out = out
            .chars()
            .take(aegis_model::MAX_NAME_CHARS)
            .collect::<String>();
    }
    out
}

fn resolve_feasibility(m: &Methodology, key: &str) -> Option<FeasibilityLevel> {
    if let Some(level) = m.feasibility_level(key) {
        return Some(level);
    }
    // Fallback to the median level in the methodology.
    let count = m.feasibility_levels().len();
    m.feasibility_levels()
        .nth(count / 2)
        .map(|(level, _, _)| level)
}

fn resolve_catalog_citation(m: &Methodology, predicted_code: &str) -> Option<Citation> {
    let scheme = m.catalog_schemes().next()?;
    if let Ok(r) = m.parse_catalog_ref(scheme, predicted_code) {
        return Some(Citation::Catalog(r));
    }
    // For EU CRA (`CRA-1.1`) or custom schemes, synthesize a valid 2-segment code.
    for fallback in ["CRA-1.1", "CRA-1.2", "5.1", "4.1"] {
        if let Ok(r) = m.parse_catalog_ref(scheme, fallback) {
            return Some(Citation::Catalog(r));
        }
    }
    None
}

/// Synthesizes a verified [`SuggestionBranch`] of ghost attack-tree steps for
/// the currently shown attack tree in `ws`.
///
/// # Errors
///
/// Returns [`SuggestionError`] if no attack tree is shown or if candidate
/// validation against the project fails.
pub fn suggest_tree_nodes(
    ws: &Workspace,
    ui: &UiState,
) -> Result<SuggestionBranch, SuggestionError> {
    let project = ws.project();
    let m = project.methodology();
    let tree_id = shown_tree(ws, ui).ok_or(SuggestionError::EmptyOps)?;
    let tree_idx = project
        .attack_tree_idx(&tree_id)
        .ok_or(SuggestionError::EmptyOps)?;
    let tree = project
        .attack_tree(tree_idx)
        .ok_or(SuggestionError::EmptyOps)?;

    // Determine target gate: if a gate is selected, expand it; if a step is
    // selected, propose siblings under its parent gate; otherwise expand root.
    let parent_idx: NodeIdx = ui
        .selected_node
        .and_then(|nid| tree.idx_of(&nid))
        .map_or_else(
            || tree.root(),
            |idx| {
                if tree.get(idx).is_some_and(|n| n.gate_kind().is_some()) {
                    idx
                } else {
                    tree.owner(idx).unwrap_or_else(|| tree.root())
                }
            },
        );
    let parent_id: NodeId = tree.id_of(parent_idx).ok_or(SuggestionError::EmptyOps)?;
    let parent_node = tree.get(parent_idx).ok_or(SuggestionError::EmptyOps)?;
    let parent_gate = match parent_node.gate_kind() {
        Some(GateKind::And) => "and",
        Some(GateKind::SeqAnd) => "seq_and",
        Some(GateKind::Or) | None => "or",
    };

    // Linked threat and asset context, if any.
    let linked_threat: Option<(ThreatId, &aegis_model::ThreatScenario)> = project
        .threat_scenarios()
        .find(|(_, _, t)| t.attack_tree() == Some(tree_idx))
        .map(|(_, id, t)| (id, t));
    let stride = linked_threat
        .and_then(|(_, t)| m.threat_category_key(t.category()))
        .unwrap_or("tampering")
        .to_owned();
    let threat_name = linked_threat.map_or_else(
        || {
            tree.get(tree.root())
                .map_or("attack scenario", Node::label)
                .to_owned()
        },
        |(_, t)| t.name().to_owned(),
    );
    let asset_name = linked_threat
        .and_then(|(_, t)| project.asset(t.asset()))
        .map_or_else(|| "target module".to_owned(), |a| a.name().to_owned());

    let combined_context = format!("{threat_name} {asset_name} {}", parent_node.label());
    let interfaces = infer_interfaces(&combined_context);
    let bus_label = match interfaces.first().map(String::as_str) {
        Some("ethernet") => "Ethernet",
        Some("wireless") => "wireless RF",
        Some("cellular") => "cellular",
        Some("usb") => "USB",
        Some("debug-port") => "debug",
        _ => "CAN",
    };

    let existing_siblings: Vec<String> = tree
        .children(parent_idx)
        .iter()
        .filter_map(|&c| tree.get(c).map(|n| n.label().to_owned()))
        .collect();
    let sib_text = if existing_siblings.is_empty() {
        "none".to_owned()
    } else {
        existing_siblings.join(" | ")
    };

    let mut depth = 1usize;
    let mut cur = parent_idx;
    while let Some(owner) = tree.owner(cur) {
        depth += 1;
        cur = owner;
    }

    let query = Gear1Query {
        lang: "en".into(),
        interfaces,
        stride,
        parent_gate: parent_gate.to_owned(),
        depth,
        sibling_index: existing_siblings.len(),
        context_text: format!(
            "Execute {threat_name} (TID-101) [{}] -> {} [{}] :: {sib_text}",
            parent_gate.to_uppercase(),
            parent_node.label(),
            parent_gate.to_uppercase()
        ),
    };

    let predictor = Gear1Predictor::default_engine();
    let feats = extract_features(&query);
    let feas_preds = predictor
        .artifact()
        .heads
        .get("target_feasibility")
        .map(|h| h.predict_top_k(&feats, 4))
        .unwrap_or_default();
    let cat_preds = predictor
        .artifact()
        .heads
        .get("target_catalog")
        .map(|h| h.predict_top_k(&feats, 3))
        .unwrap_or_default();
    let mut raw_labels = predictor
        .artifact()
        .heads
        .get("target_label")
        .map(|h| h.predict_top_k(&feats, 6))
        .unwrap_or_default();

    // Always append the contextual builtin templates so we have diverse,
    // non-duplicate domain candidates even if the top-1 external class already
    // exists in the tree.
    if let Some(builtin_labels) = Gear1Predictor::builtin().artifact.heads.get("target_label") {
        for (tpl, prob) in builtin_labels.predict_top_k(&feats, 6) {
            raw_labels.push((tpl, prob));
        }
    }

    let mut existing_labels: BTreeSet<String> = tree
        .iter()
        .map(|(_, _, n)| n.label().to_lowercase())
        .collect();

    let best_feas_key = feas_preds
        .iter()
        .find(|(k, _)| k != "gate")
        .map_or("medium", |(k, _)| k.as_str());
    let feas_level = resolve_feasibility(m, best_feas_key);
    let best_cat_code = cat_preds.first().map_or("5.1", |(c, _)| c.as_str());

    let mut branch = SuggestionBranch::new(SuggestionId::new_v7());

    let base_tree_len = u32::try_from(tree.len()).unwrap_or(u32::MAX);
    let base_children_len = u32::try_from(tree.children(parent_idx).len()).unwrap_or(u32::MAX);
    let mut added = 0u32;

    for (tpl, prob) in raw_labels {
        if usize::try_from(added).unwrap_or(usize::MAX) >= MAX_PROPOSED_NODES {
            break;
        }
        let concrete = instantiate_step_label(&tpl, &threat_name, &asset_name, bus_label);
        if concrete.trim().is_empty() || !existing_labels.insert(concrete.to_lowercase()) {
            continue;
        }

        let step_id = NodeId::new_v7();
        let mut ops = vec![Op::AddTreeNode {
            tree: tree_id,
            parent: parent_id,
            id: step_id,
            node: Node::step(concrete.clone()),
            index: base_tree_len.saturating_add(added),
            position: base_children_len.saturating_add(added),
            provenance: Provenance::default(),
        }];
        if let Some(level) = feas_level {
            ops.push(Op::SetStepFeasibility {
                target: StepTarget::Step(tree_id, step_id),
                level: Some(level),
                provenance: Provenance::default(),
            });
        }

        let mut citations = vec![Citation::Element(ElementRef::Tree(tree_id))];
        if let Some((tid, _)) = linked_threat {
            citations.push(Citation::Element(ElementRef::Threat(tid)));
        }
        if let Some(cat) = resolve_catalog_citation(m, best_cat_code) {
            citations.push(cat);
        }
        let pct = (prob.clamp(0.15, 0.99) * 100.0).round();
        let feas_word = feas_level
            .and_then(|l| m.feasibility_level_label(l))
            .unwrap_or("Unrated");
        if let Ok(note) = Citation::note(format!("Gear 1 · {feas_word} ({pct:.0}%)")) {
            citations.push(note);
        }

        let item = SuggestionItem::new(SuggestionId::new_v7(), concrete, ops, citations)?;
        branch.push(item)?;
        added += 1;
    }

    if branch.is_empty() {
        return Err(SuggestionError::EmptyOps);
    }
    branch.validate(project)?;
    Ok(branch)
}

/// Converts the pending items of a [`SuggestionBranch`] into [`SuggestionRow`]s
/// for the UI.
#[must_use]
pub fn suggestion_rows(ws: &Workspace) -> Vec<SuggestionRow> {
    let Some(branch) = ws.suggestions() else {
        return Vec::new();
    };
    let m = ws.project().methodology();
    branch
        .items()
        .iter()
        .map(|item| {
            let (kind, detail_prefix) = match item.preview() {
                GhostPreview::TreeNode {
                    node, feasibility, ..
                } => {
                    let k = match node.gate_kind() {
                        None => "Step",
                        Some(GateKind::Or) => "OR gate",
                        Some(GateKind::And) => "AND gate",
                        Some(GateKind::SeqAnd) => "SEQ gate",
                    };
                    let f = feasibility
                        .and_then(|l| m.feasibility_level_label(l))
                        .unwrap_or("Unrated");
                    (k.to_owned(), f.to_owned())
                }
                GhostPreview::Entity { kind, .. } => (format!("{kind:?}"), "Entity".to_owned()),
                GhostPreview::Modification { .. } => ("Edit".to_owned(), "Proposal".to_owned()),
            };
            let mut cite_parts = Vec::new();
            for c in item.citations() {
                match c {
                    Citation::Catalog(r) => {
                        cite_parts.push(m.display_catalog_ref(r).to_string());
                    }
                    Citation::Note(n) => cite_parts.push(n.to_string()),
                    Citation::Element(_) => {}
                }
            }
            let detail = if cite_parts.is_empty() {
                detail_prefix
            } else {
                cite_parts.join(" · ")
            };
            SuggestionRow {
                id: item.id(),
                label: item.title().to_owned(),
                detail,
                kind,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aegis_model::{Provenance, ProvenanceSource};

    use super::*;
    use crate::sample::sample_ops;
    use crate::view::Page;
    use crate::workspace::Origin;

    #[test]
    fn suggest_tree_nodes_produces_validated_ghost_steps_for_sample_project() {
        let m = Arc::new(Methodology::iso21434_r155().expect("builtin methodology"));
        let mut ws = Workspace::empty(Arc::clone(&m));
        let ops = sample_ops(&m, "alice", 1_000).expect("sample ops");
        ws.apply(ops, Origin::Edit).expect("apply sample");

        let first_threat = ws
            .project()
            .threat_scenarios()
            .next()
            .map(|(_, id, _)| id)
            .expect("sample threat");
        let ui = UiState {
            page: Page::Trees,
            selected_threat: Some(first_threat),
            ..UiState::default()
        };

        let branch = suggest_tree_nodes(&ws, &ui).expect("suggest tree nodes");
        assert!(!branch.is_empty());
        assert!(branch.len() <= MAX_PROPOSED_NODES);
        branch
            .validate(ws.project())
            .expect("valid against project");

        ws.set_suggestions(Some(branch)).expect("set suggestions");
        let rows = suggestion_rows(&ws);
        assert_eq!(rows.len(), MAX_PROPOSED_NODES);
        assert!(rows.iter().all(|r| r.kind == "Step"));

        // Accepting one suggestion applies it with ProvenanceSource::Ai.
        let first_id = rows[0].id;
        let tree_id = shown_tree(&ws, &ui).expect("shown tree");
        let tree_idx = ws.project().attack_tree_idx(&tree_id).expect("tree idx");
        let before_len = ws.project().attack_tree(tree_idx).expect("tree").len();

        ws.accept_suggestion(first_id, "bob", 2_000)
            .expect("accept first suggestion");
        let after_tree = ws.project().attack_tree(tree_idx).expect("tree");
        assert_eq!(after_tree.len(), before_len + 1);
        assert_eq!(
            ws.project()
                .attack_tree_provenance(tree_idx)
                .map(Provenance::source),
            Some(ProvenanceSource::Ai)
        );
        assert_eq!(
            ws.project()
                .attack_tree_provenance(tree_idx)
                .map(Provenance::actor),
            Some("bob")
        );
    }
}
