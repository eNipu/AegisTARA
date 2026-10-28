//! DPO Preference Pair Builder for Structural Compliance (Task AI-10).
//!
//! Synthesizes compiler-verified Direct Preference Optimization (`DPO`) pairs
//! `(prompt, chosen, rejected, reject_reason)` from verified SFT examples:
//! - Every `chosen` completion is verified by [`Verifier`].
//! - Every `rejected` completion is deterministically mutated to exhibit a
//!   realistic small-model structural or ontology failure mode (unrated leaf
//!   steps, hallucinated feasibility scale keys, degenerate single-child gates,
//!   repetitive sibling steps, or invalid STRIDE/impact keys) and is confirmed
//!   to be rejected by [`Verifier`].

use aegis_model::library::ItemNode;
use serde::{Deserialize, Serialize};

use crate::dataset::SftExample;
use crate::dsl::{TaskKind, format_tree_dsl, parse_tree_dsl};
use crate::verifier::{Verifier, VerifyError};

/// A single compiler-verified preference pair for Direct Preference Optimization (Task AI-10).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DpoPair {
    /// Deterministic pair identifier.
    pub id: String,
    /// Task kind (`draft_tree`, `expand_step`, `predict_sibling`, `suggest_threats`, `bind_slots`).
    pub task: String,
    /// Language tag (`en` or `ja`).
    pub lang: String,
    /// Input prompt (identical to SFT prompt).
    pub prompt: String,
    /// Compiler-verified winning completion.
    pub chosen: String,
    /// Compiler-rejected losing completion.
    pub rejected: String,
    /// Exact [`VerifyError`] category and message that caught `rejected`.
    pub reject_reason: String,
}

/// Builds verified [`DpoPair`] records from a slice of verified [`SftExample`]s.
///
/// # Errors
///
/// Returns [`VerifyError`] if a `chosen` completion unexpectedly fails verification.
pub fn build_dpo_pairs(
    verifier: &Verifier,
    sft_examples: &[SftExample],
) -> Result<Vec<DpoPair>, VerifyError> {
    let mut pairs = Vec::new();

    for (idx, ex) in sft_examples.iter().enumerate() {
        match ex.task {
            TaskKind::DraftTree | TaskKind::ExpandStep | TaskKind::BindSlots => {
                // Skip unbound slot templates when building strict DPO pairs.
                if ex.completion.contains('{') && ex.completion.contains('}') {
                    continue;
                }
                verifier.verify_tree_dsl_strict(&ex.completion)?;
                let item = parse_tree_dsl(&ex.completion)?;

                // Rotate across the 4 structural tree mutation modes so the DPO
                // dataset balances all failure modes without bloating 4x.
                let candidates = [
                    mutate_unrated_leaf(&item),
                    mutate_hallucinated_feasibility(&item),
                    mutate_single_child_gate(&item),
                    mutate_duplicate_sibling(&item),
                ];

                // Always emit at least 2 distinct failure modes per rooted tree example.
                for mode_offset in [idx % 4, (idx + 1) % 4] {
                    if let Some((mode_tag, bad_dsl)) = &candidates[mode_offset]
                        && let Err(err) = verifier.verify_tree_dsl_strict(bad_dsl)
                    {
                        pairs.push(DpoPair {
                            id: format!("{}-dpo-{mode_tag}", ex.id),
                            task: ex.task.as_str().into(),
                            lang: ex.lang.clone(),
                            prompt: ex.prompt.clone(),
                            chosen: ex.completion.clone(),
                            rejected: bad_dsl.clone(),
                            reject_reason: format!("{mode_tag}: {err}"),
                        });
                    }
                }
            }
            TaskKind::PredictSibling => {
                // Sample 1 in 4 PredictSibling examples to keep DPO balanced across tasks.
                if idx % 4 != 0 || (ex.completion.contains('{') && ex.completion.contains('}')) {
                    continue;
                }
                let item = parse_tree_dsl(&ex.completion)?;
                if item.gate.is_none() {
                    let bad_dsl = if idx % 8 == 0 {
                        format!("<|step|>{}<|feas|>critical\n<|end|>", item.label)
                    } else {
                        format!("<|step|>{}\n<|end|>", item.label)
                    };
                    if let Err(err) = verifier.verify_tree_dsl(&bad_dsl) {
                        pairs.push(DpoPair {
                            id: format!("{}-dpo-step", ex.id),
                            task: ex.task.as_str().into(),
                            lang: ex.lang.clone(),
                            prompt: ex.prompt.clone(),
                            chosen: ex.completion.clone(),
                            rejected: bad_dsl,
                            reject_reason: format!("invalid_step: {err}"),
                        });
                    }
                }
            }
            TaskKind::SuggestThreats => {
                verifier.verify_threats_dsl(&ex.completion)?;
                let bad_dsl = ex
                    .completion
                    .replacen("<|stride|>tampering", "<|stride|>phishing", 1)
                    .replacen("<|stride|>spoofing", "<|stride|>social_engineering", 1)
                    .replacen(
                        "<|impact|>safety:severe",
                        "<|impact|>safety:catastrophic",
                        1,
                    )
                    .replacen(
                        "<|impact|>operational:major",
                        "<|impact|>operational:extreme",
                        1,
                    );
                if bad_dsl != ex.completion
                    && let Err(err) = verifier.verify_threats_dsl(&bad_dsl)
                {
                    pairs.push(DpoPair {
                        id: format!("{}-dpo-ontology", ex.id),
                        task: ex.task.as_str().into(),
                        lang: ex.lang.clone(),
                        prompt: ex.prompt.clone(),
                        chosen: ex.completion.clone(),
                        rejected: bad_dsl,
                        reject_reason: format!("invalid_ontology: {err}"),
                    });
                }
            }
            TaskKind::OutOfDomain => {}
        }
    }

    Ok(pairs)
}

/// Drops the feasibility rating from the first rated leaf step so `Project::lint()`
/// reports `AttackTreeStepUnrated`.
fn mutate_unrated_leaf(root: &ItemNode) -> Option<(&'static str, String)> {
    let mut clone = root.clone();
    if strip_first_leaf_feasibility(&mut clone) {
        Some(("unrated_step", format_tree_dsl(&clone)))
    } else {
        None
    }
}

fn strip_first_leaf_feasibility(node: &mut ItemNode) -> bool {
    if node.gate.is_none() {
        if node.feasibility.is_some() {
            node.feasibility = None;
            return true;
        }
        return false;
    }
    for child in &mut node.children {
        if strip_first_leaf_feasibility(child) {
            return true;
        }
    }
    false
}

/// Replaces the first leaf step's feasibility key with a non-ISO-21434 scale key.
fn mutate_hallucinated_feasibility(root: &ItemNode) -> Option<(&'static str, String)> {
    let mut clone = root.clone();
    if corrupt_first_leaf_feasibility(&mut clone) {
        Some(("hallucinated_feasibility", format_tree_dsl(&clone)))
    } else {
        None
    }
}

fn corrupt_first_leaf_feasibility(node: &mut ItemNode) -> bool {
    if node.gate.is_none() {
        node.feasibility = Some("critical_impossible".into());
        return true;
    }
    for child in &mut node.children {
        if corrupt_first_leaf_feasibility(child) {
            return true;
        }
    }
    false
}

/// Truncates a gate's children to 1 child (`SingleChildGate`).
fn mutate_single_child_gate(root: &ItemNode) -> Option<(&'static str, String)> {
    if root.gate.is_none() || root.children.is_empty() {
        return None;
    }
    let mut clone = root.clone();
    clone.children.truncate(1);
    Some(("single_child_gate", format_tree_dsl(&clone)))
}

/// Duplicates the first child under the root gate (`DuplicateSiblingStep`).
fn mutate_duplicate_sibling(root: &ItemNode) -> Option<(&'static str, String)> {
    if root.gate.is_none() || root.children.is_empty() {
        return None;
    }
    let mut clone = root.clone();
    let dup = clone.children[0].clone();
    clone.children.push(dup);
    Some(("duplicate_sibling_step", format_tree_dsl(&clone)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::CompiledDataset;

    #[test]
    fn builds_verified_dpo_pairs_across_all_failure_modes() {
        let verifier = Verifier::iso21434_r155();
        let ds = CompiledDataset::build_verified().expect("verified dataset builds");
        let pairs = build_dpo_pairs(&verifier, &ds.sft_examples[..120])
            .expect("DPO pair generation succeeds");
        assert!(pairs.len() >= 10);

        let mut reasons = std::collections::BTreeSet::new();
        for pair in &pairs {
            assert_ne!(pair.chosen, pair.rejected);
            let tag = pair.reject_reason.split(':').next().unwrap_or("");
            reasons.insert(tag.to_owned());
        }
        assert!(reasons.contains("unrated_step"));
        assert!(reasons.contains("hallucinated_feasibility"));
        assert!(reasons.contains("single_child_gate"));
        assert!(reasons.contains("duplicate_sibling_step"));
    }
}
