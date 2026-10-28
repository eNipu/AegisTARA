//! Deterministic `aegis-model` compiler and linter gate for AI outputs (Task AI-02).
//!
//! Principle D6 (`docs/ai-backlog.md`): **"AI proposes. The engine computes. A
//! human decides."**
//!
//! Every synthetic training example and every runtime model completion passes
//! through [`Verifier`]:
//! 1. Parses the compact DSL into [`ItemNode`] or [`ThreatProposal`].
//! 2. Materializes a real [`aegis_model::AttackTree`] in the arena, checking
//!    node caps, DAG topological order, and non-empty gates.
//! 3. Resolves every step feasibility key against the active [`Methodology`]
//!    and runs [`aegis_model::evaluate`] (`MaxMin`) so the deterministic
//!    engine computes root feasibility and the `why_path` derivation trace.
//! 4. Applies typed [`Op`] records (`Provenance::ai`) onto an [`aegis_model::Project`]
//!    and verifies that [`Project::lint`] reports zero [`LintSeverity::Error`]
//!    findings.

use std::collections::BTreeMap;
use std::sync::Arc;

use aegis_model::library::ItemNode;
use aegis_model::{
    AssetId, AttackTree, DamageId, FeasibilityLevel, LintFinding, LintSeverity, MaxMin,
    Methodology, Node, NodeId, NodeIdx, Op, Project, Provenance, ThreatId, TreeId, evaluate,
};

use crate::dsl::{DslError, ThreatProposal, parse_threats_dsl, parse_tree_dsl};

/// Timestamp used when tagging verified AI operations in synthetic checks.
const AI_TIMESTAMP_MS: u64 = 1_730_000_000_000;

/// Why a candidate AI completion was rejected by the deterministic engine.
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    /// The DSL syntax was invalid or refused as `<|out_of_domain|>`.
    #[error("DSL parse error: {0}")]
    Dsl(#[from] DslError),
    /// The `aegis-model` domain model rejected an operation or tree edge.
    #[error("aegis-model error: {0}")]
    Model(#[from] aegis_model::ModelError),
    /// A step feasibility key does not exist in the active [`Methodology`].
    #[error("unknown feasibility key `{0}` for methodology `{1}`")]
    UnknownFeasibilityKey(String, String),
    /// A threat category key does not exist in the active [`Methodology`].
    #[error("unknown threat category `{0}` for methodology `{1}`")]
    UnknownThreatCategory(String, String),
    /// An impact spec (`category:level`) is malformed or unknown in the [`Methodology`].
    #[error("invalid impact rating `{0}` for methodology `{1}`")]
    InvalidImpact(String, String),
    /// The deterministic project linter reported one or more structural errors.
    #[error("project linter reported {count} error(s): {first}")]
    LintErrors {
        /// Number of error-severity findings.
        count: usize,
        /// First error code description.
        first: String,
    },
    /// A gate in a strict tree completion has fewer than 2 children.
    #[error("gate `{0}` has fewer than 2 children")]
    SingleChildGate(String),
    /// A gate has duplicate child labels (repetition loop).
    #[error("gate `{0}` contains duplicate child step `{1}`")]
    DuplicateSiblingStep(String, String),
    /// An unbound `{slot}` placeholder remained in a concrete completion.
    #[error("unbound template slot in `{0}`")]
    UnboundTemplateSlot(String),
    /// Every candidate in a Best-of-N batch was rejected by the verifier.
    #[error("all {0} candidate completions were rejected by the verifier")]
    AllCandidatesRejected(usize),
}

/// A verified candidate completion ranked by Best-of-N rejection sampling (Task AI-09).
#[derive(Clone, Debug, PartialEq)]
pub struct ScoredCandidate {
    /// Canonical DSL completion string.
    pub dsl: String,
    /// Deterministic verification summary (`node_count`, `root_feasibility_key`, `why_path`).
    pub verified: VerifiedTree,
    /// Combined ranking score (model prior + structural quality bonus).
    pub score: f64,
}

/// Summary returned when an attack tree DSL completion passes all checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedTree {
    /// Total nodes (`Gate` + `Step`) in the materialized [`AttackTree`].
    pub node_count: usize,
    /// Number of leaf `Step` nodes.
    pub step_count: usize,
    /// Root feasibility key computed by [`aegis_model::evaluate`] (`MaxMin`),
    /// if all steps carry a feasibility rating.
    pub root_feasibility_key: Option<String>,
    /// Labels along the deciding `why_path` from root to dominating leaf step.
    pub why_path_labels: Vec<String>,
}

/// Deterministic verifier backed by an [`Arc<Methodology>`].
#[derive(Clone, Debug)]
pub struct Verifier {
    methodology: Arc<Methodology>,
}

impl Verifier {
    /// Creates a verifier for the given [`Methodology`].
    #[must_use]
    pub const fn new(methodology: Arc<Methodology>) -> Self {
        Self { methodology }
    }

    /// Creates a verifier for the built-in ISO/SAE 21434 + UN R155 methodology.
    ///
    /// # Panics
    ///
    /// Never: the embedded ISO/SAE 21434 pack is tested in `aegis-model`.
    #[must_use]
    pub fn iso21434_r155() -> Self {
        let m = Methodology::iso21434_r155().expect("built-in ISO 21434 methodology is valid");
        Self::new(Arc::new(m))
    }

    /// Returns a reference to the underlying [`Methodology`].
    #[must_use]
    pub fn methodology(&self) -> &Arc<Methodology> {
        &self.methodology
    }

    /// Parses and verifies a compact DSL tree completion (`DraftTree`,
    /// `ExpandStep`, `BindSlots`, or `PredictSibling`).
    ///
    /// If the completion is a single leaf step (`PredictSibling`), this checks
    /// the step and its feasibility key; if it is a gate subtree, this builds
    /// the full [`AttackTree`], evaluates it with [`MaxMin`], and runs
    /// [`Project::lint`] inside a complete synthetic project context.
    ///
    /// # Errors
    ///
    /// Returns [`VerifyError`] if any syntax, vocabulary, evaluation, or lint
    /// rule is violated.
    pub fn verify_tree_dsl(&self, dsl: &str) -> Result<VerifiedTree, VerifyError> {
        let item_root = parse_tree_dsl(dsl)?;
        self.verify_item_node(&item_root)
    }

    /// Materializes an [`ItemNode`] into an [`AttackTree`], evaluates it with
    /// [`MaxMin`], and checks it with [`Project::lint`].
    ///
    /// # Errors
    ///
    /// Returns [`VerifyError`] if the tree or its ratings are invalid.
    pub fn verify_item_node(&self, item_root: &ItemNode) -> Result<VerifiedTree, VerifyError> {
        // A single leaf step (common in `PredictSibling`) can be verified by
        // wrapping it under a synthetic OR root gate so `Project::lint` and
        // `evaluate` run on a valid rooted tree.
        let owned_wrapper;
        let effective_root = if item_root.gate.is_none() {
            owned_wrapper = ItemNode {
                label: "Parent gate".into(),
                gate: Some(aegis_model::GateKind::Or),
                feasibility: None,
                children: vec![item_root.clone()],
            };
            &owned_wrapper
        } else {
            item_root
        };

        let (tree, step_feas, idx_feas) = self.build_attack_tree(effective_root)?;
        let node_count = if item_root.gate.is_none() {
            1
        } else {
            tree.len()
        };
        let step_count = step_feas.len();

        // If every leaf step in the subtree is rated, run `evaluate` (`MaxMin`)
        // to compute root feasibility and the deciding `why_path`.
        let mut root_feasibility_key = None;
        let mut why_path_labels = Vec::new();
        let all_steps_rated = count_leaves(effective_root) == idx_feas.len();
        if all_steps_rated && !idx_feas.is_empty() {
            let domain = MaxMin::<FeasibilityLevel>::new();
            let eval = evaluate(&tree, &domain, |idx| idx_feas.get(&idx).copied())?;
            let root_level = *eval.root_value();
            root_feasibility_key = self
                .methodology
                .feasibility_level_key(root_level)
                .map(ToOwned::to_owned);
            why_path_labels = eval
                .why_path(tree.root())
                .filter_map(|idx| tree.get(idx).map(|n| n.label().to_owned()))
                .collect();
        }

        // If this is not an unbound `{slot}` library template (or all steps are rated)
        // and the tree fits within `MAX_TREE_OP_NODES`, commit it into a full
        // `Project` with `Provenance::ai` and run `lint()`.
        let is_slot_template = has_template_slots(effective_root);
        if (!is_slot_template || all_steps_rated) && tree.len() <= aegis_model::MAX_TREE_OP_NODES {
            let findings = self.lint_tree_in_project(tree, step_feas)?;
            let errors: Vec<&LintFinding> = findings
                .iter()
                .filter(|f| f.severity() == LintSeverity::Error)
                .collect();
            if !errors.is_empty() {
                return Err(VerifyError::LintErrors {
                    count: errors.len(),
                    first: errors[0].code().to_string(),
                });
            }
        }

        Ok(VerifiedTree {
            node_count,
            step_count,
            root_feasibility_key,
            why_path_labels,
        })
    }

    /// Strict verification for full subtree completions (`DraftTree`, `ExpandStep`,
    /// `BindSlots`) and Best-of-N candidate filtering (Task AI-09).
    ///
    /// In addition to [`Self::verify_tree_dsl`], this enforces:
    /// - No unbound `{slot}` placeholders remain.
    /// - Every gate has at least 2 children (`SingleChildGate`).
    /// - Sibling nodes under any gate have distinct labels (`DuplicateSiblingStep`).
    ///
    /// # Errors
    ///
    /// Returns [`VerifyError`] if any structural or domain check fails.
    pub fn verify_tree_dsl_strict(&self, dsl: &str) -> Result<VerifiedTree, VerifyError> {
        let item_root = parse_tree_dsl(dsl)?;
        check_strict_item_node(&item_root)?;
        self.verify_item_node(&item_root)
    }

    /// Best-of-N Verifier-Guided Rejection Sampler (Task AI-09).
    ///
    /// Given `candidates` as `(dsl_completion, model_prior_score)` pairs:
    /// 1. Parses and verifies each candidate with [`Self::verify_tree_dsl_strict`]
    ///    (for rooted gate subtrees) or [`Self::verify_tree_dsl`] (for single-step
    ///    `PredictSibling` completions).
    /// 2. Rejects any candidate that fails DSL syntax, methodology vocabulary,
    ///    `MaxMin` evaluation, `Project::lint()`, or strict gate well-formedness.
    /// 3. Ranks surviving candidates by `model_prior_score` plus a small
    ///    structural completeness bonus.
    ///
    /// # Errors
    ///
    /// Returns [`VerifyError::AllCandidatesRejected`] if zero candidates survive.
    pub fn select_best_of_n(
        &self,
        candidates: &[(String, f64)],
    ) -> Result<Vec<ScoredCandidate>, VerifyError> {
        let mut survivors = Vec::new();
        for (dsl, prior) in candidates {
            let trimmed = dsl.trim();
            let verified_res = if trimmed.starts_with("<|gate:") {
                self.verify_tree_dsl_strict(trimmed)
            } else {
                let item = parse_tree_dsl(trimmed)?;
                if has_template_slots(&item) {
                    Err(VerifyError::UnboundTemplateSlot(item.label))
                } else {
                    self.verify_item_node(&item)
                }
            };
            if let Ok(verified) = verified_res {
                let path_len = u8::try_from(verified.why_path_labels.len().min(4)).unwrap_or(4);
                let completeness_bonus = if verified.root_feasibility_key.is_some() {
                    0.05
                } else {
                    0.0
                } + f64::from(path_len) * 0.01;
                survivors.push(ScoredCandidate {
                    dsl: trimmed.to_owned(),
                    verified,
                    score: prior + completeness_bonus,
                });
            }
        }

        if survivors.is_empty() {
            return Err(VerifyError::AllCandidatesRejected(candidates.len()));
        }

        survivors.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(survivors)
    }

    /// Parses and verifies a [`ThreatProposal`] DSL completion against the
    /// active [`Methodology`] and [`Project`] operations.
    ///
    /// # Errors
    ///
    /// Returns [`VerifyError`] if any category, impact rating, or operation fails.
    pub fn verify_threats_dsl(&self, dsl: &str) -> Result<Vec<ThreatProposal>, VerifyError> {
        let proposals = parse_threats_dsl(dsl)?;
        let m = &self.methodology;
        let mut project = Project::new(Arc::clone(m));
        let prov = Provenance::ai("aegis-core", AI_TIMESTAMP_MS);

        let asset_id = AssetId::new_v7();
        project.apply(Op::AddAsset {
            id: asset_id,
            name: "Verified Asset".into(),
            properties: m.all_properties(),
            provenance: prov.clone(),
        })?;

        for p in &proposals {
            let Some(category) = m.threat_category(&p.stride) else {
                return Err(VerifyError::UnknownThreatCategory(
                    p.stride.clone(),
                    m.key().to_owned(),
                ));
            };
            let Some((imp_cat_key, imp_lvl_key)) = p.impact.split_once(':') else {
                return Err(VerifyError::InvalidImpact(
                    p.impact.clone(),
                    m.key().to_owned(),
                ));
            };
            let Some(imp_cat) = m.impact_category(imp_cat_key.trim()) else {
                return Err(VerifyError::InvalidImpact(
                    p.impact.clone(),
                    m.key().to_owned(),
                ));
            };
            let Some(imp_lvl) = m.level(imp_cat, imp_lvl_key.trim()) else {
                return Err(VerifyError::InvalidImpact(
                    p.impact.clone(),
                    m.key().to_owned(),
                ));
            };

            let damage_id = DamageId::new_v7();
            project.apply(Op::AddDamageScenario {
                id: damage_id,
                name: p.damage_name.clone().into(),
                impacts: vec![(imp_cat, imp_lvl)],
                provenance: prov.clone(),
            })?;

            // Resolve any catalog refs that belong to schemes registered in the methodology
            // (for example `r155-annex5` refs like `4.1` or `7.2`).
            let mut catalog_refs = Vec::new();
            if let Some(r155) = m.catalog_scheme("r155-annex5") {
                for raw_id in &p.catalog_ids {
                    if !raw_id.starts_with("CAPEC-")
                        && !raw_id.starts_with("EMB3D-")
                        && let Ok(cref) = m.parse_catalog_ref(r155, raw_id)
                    {
                        catalog_refs.push(cref);
                    }
                }
            }

            let threat_id = ThreatId::new_v7();
            project.apply(Op::AddThreatScenario {
                id: threat_id,
                name: p.name.clone().into(),
                asset: asset_id,
                category,
                catalog_refs,
                damages: vec![damage_id],
                provenance: prov.clone(),
            })?;
        }

        Ok(proposals)
    }

    #[allow(clippy::type_complexity)]
    fn build_attack_tree(
        &self,
        root_item: &ItemNode,
    ) -> Result<
        (
            AttackTree,
            Vec<(NodeId, FeasibilityLevel)>,
            BTreeMap<NodeIdx, FeasibilityLevel>,
        ),
        VerifyError,
    > {
        let root_id = NodeId::new_v7();
        let root_node = match root_item.gate {
            Some(kind) => Node::gate(kind, root_item.label.as_str()),
            None => Node::step(root_item.label.as_str()),
        };
        let mut tree = AttackTree::new(root_id, root_node)?;
        let mut step_feas = Vec::new();
        let mut idx_feas = BTreeMap::new();

        let mut stack: Vec<(NodeIdx, &ItemNode)> = root_item
            .children
            .iter()
            .rev()
            .map(|c| (tree.root(), c))
            .collect();

        while let Some((parent_idx, item)) = stack.pop() {
            let node_id = NodeId::new_v7();
            let node = match item.gate {
                Some(kind) => Node::gate(kind, item.label.as_str()),
                None => Node::step(item.label.as_str()),
            };
            let idx = tree.add_child(parent_idx, node_id, node)?;
            if item.gate.is_none()
                && let Some(feas_key) = &item.feasibility
            {
                let Some(level) = self.methodology.feasibility_level(feas_key) else {
                    return Err(VerifyError::UnknownFeasibilityKey(
                        feas_key.clone(),
                        self.methodology.key().to_owned(),
                    ));
                };
                step_feas.push((node_id, level));
                idx_feas.insert(idx, level);
            }
            for child in item.children.iter().rev() {
                stack.push((idx, child));
            }
        }

        Ok((tree, step_feas, idx_feas))
    }

    fn lint_tree_in_project(
        &self,
        tree: AttackTree,
        step_feasibility: Vec<(NodeId, FeasibilityLevel)>,
    ) -> Result<Vec<LintFinding>, VerifyError> {
        let m = &self.methodology;
        let mut project = Project::new(Arc::clone(m));
        let prov = Provenance::ai("aegis-core", AI_TIMESTAMP_MS);

        let asset_id = AssetId::new_v7();
        project.apply(Op::AddAsset {
            id: asset_id,
            name: "ECU Asset".into(),
            properties: m.all_properties(),
            provenance: prov.clone(),
        })?;

        let first_cat = m
            .impact_categories()
            .next()
            .expect("methodology has at least one impact category");
        let first_lvl = m
            .levels(first_cat)
            .next()
            .expect("impact category has at least one level");
        let damage_id = DamageId::new_v7();
        project.apply(Op::AddDamageScenario {
            id: damage_id,
            name: "Loss of function".into(),
            impacts: vec![(first_cat, first_lvl)],
            provenance: prov.clone(),
        })?;

        let first_threat_cat = m
            .threat_categories()
            .next()
            .expect("methodology has at least one threat category");
        let threat_id = ThreatId::new_v7();
        project.apply(Op::AddThreatScenario {
            id: threat_id,
            name: "Synthetic Threat".into(),
            asset: asset_id,
            category: first_threat_cat,
            catalog_refs: Vec::new(),
            damages: vec![damage_id],
            provenance: prov.clone(),
        })?;

        let tree_id = TreeId::new_v7();
        project.apply(Op::AddAttackTree {
            id: tree_id,
            tree,
            step_feasibility,
            provenance: prov.clone(),
        })?;
        project.apply(Op::SetThreatAttackTree {
            threat: threat_id,
            tree: Some(tree_id),
            provenance: prov,
        })?;

        Ok(project.lint())
    }
}

fn count_leaves(node: &ItemNode) -> usize {
    if node.gate.is_none() {
        1
    } else {
        node.children.iter().map(count_leaves).sum()
    }
}

fn has_template_slots(node: &ItemNode) -> bool {
    if node.label.contains('{') && node.label.contains('}') {
        return true;
    }
    node.children.iter().any(has_template_slots)
}

fn check_strict_item_node(node: &ItemNode) -> Result<(), VerifyError> {
    if node.label.contains('{') && node.label.contains('}') {
        return Err(VerifyError::UnboundTemplateSlot(node.label.clone()));
    }
    if node.gate.is_some() {
        if node.children.len() < 2 {
            return Err(VerifyError::SingleChildGate(node.label.clone()));
        }
        let mut seen = std::collections::BTreeSet::new();
        for child in &node.children {
            let norm = child.label.trim().to_ascii_lowercase();
            if !seen.insert(norm) {
                return Err(VerifyError::DuplicateSiblingStep(
                    node.label.clone(),
                    child.label.clone(),
                ));
            }
            check_strict_item_node(child)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::format_tree_dsl;
    use aegis_model::library::LibraryPack;

    #[test]
    fn verifies_all_builtin_library_pack_items() {
        let pack = LibraryPack::builtin_demo();
        let verifier = Verifier::iso21434_r155();
        for item in pack.items() {
            let dsl = format_tree_dsl(&item.tree);
            let summary = verifier
                .verify_tree_dsl(&dsl)
                .expect("every built-in library item should pass verification");
            assert!(summary.node_count >= 1);
            assert!(summary.root_feasibility_key.is_some());
        }
    }

    #[test]
    fn rejects_invalid_feasibility_key() {
        let verifier = Verifier::iso21434_r155();
        let bad_dsl = "<|gate:or|>Root\n  <|step|>Step A<|feas|>impossible_key\n<|end|>";
        let err = verifier.verify_tree_dsl(bad_dsl).unwrap_err();
        assert!(matches!(err, VerifyError::UnknownFeasibilityKey(_, _)));
    }

    #[test]
    fn strict_verifier_rejects_unrated_single_child_and_duplicate_siblings() {
        let verifier = Verifier::iso21434_r155();

        let unrated = "<|gate:or|>Root\n  <|step|>Step A\n  <|step|>Step B<|feas|>medium\n<|end|>";
        let err_unrated = verifier.verify_tree_dsl_strict(unrated).unwrap_err();
        assert!(matches!(err_unrated, VerifyError::LintErrors { .. }));

        let single_child = "<|gate:and|>Root\n  <|step|>Only step<|feas|>high\n<|end|>";
        let err_single = verifier.verify_tree_dsl_strict(single_child).unwrap_err();
        assert!(matches!(err_single, VerifyError::SingleChildGate(_)));

        let dup_sibling = "<|gate:or|>Root\n  <|step|>Same step<|feas|>high\n  <|step|>Same step<|feas|>low\n<|end|>";
        let err_dup = verifier.verify_tree_dsl_strict(dup_sibling).unwrap_err();
        assert!(matches!(err_dup, VerifyError::DuplicateSiblingStep(_, _)));
    }

    #[test]
    fn best_of_n_rejection_sampler_filters_invalid_and_ranks_valid() {
        let verifier = Verifier::iso21434_r155();
        let candidates = vec![
            // High prior, but invalid feasibility key -> rejected
            (
                "<|gate:or|>Root\n  <|step|>Step A<|feas|>bogus\n  <|step|>Step B<|feas|>low\n<|end|>"
                    .to_owned(),
                0.99,
            ),
            // High prior, but single-child gate -> rejected
            (
                "<|gate:and|>Root\n  <|step|>Only one<|feas|>high\n<|end|>".to_owned(),
                0.95,
            ),
            // Valid candidate 1
            (
                "<|gate:or|>Root\n  <|step|>Step A<|feas|>medium\n  <|step|>Step B<|feas|>low\n<|end|>"
                    .to_owned(),
                0.80,
            ),
            // Valid candidate 2 with higher prior
            (
                "<|gate:seq_and|>Root\n  <|step|>Reach bus<|feas|>high\n  <|step|>Inject frame<|feas|>medium\n<|end|>"
                    .to_owned(),
                0.88,
            ),
        ];

        let survivors = verifier
            .select_best_of_n(&candidates)
            .expect("two valid candidates should survive");
        assert_eq!(survivors.len(), 2);
        assert!(survivors[0].dsl.contains("<|gate:seq_and|>Root"));
        assert_eq!(
            survivors[0].verified.root_feasibility_key.as_deref(),
            Some("medium")
        );
    }
}
