//! Combinatorial Attack Tree Masking and Verified Dataset Compiler (Task AI-04).
//!
//! Turns [`aegis_model::library::LibraryPack::builtin_demo`] and the bilingual
//! seed bank in [`crate::corpus`] into:
//! 1. **Gear 2 (`mlx/*.jsonl`)**: Prompt/completion JSONL records in the
//!    `mlx-lm` format (`{"prompt": "...", "completion": "..."}`), verified by
//!    [`crate::verifier::Verifier`].
//! 2. **Gear 1 (`gear1/*.jsonl`)**: Structured feature/target records for
//!    training the from-scratch 20M-parameter Structural Tree & Ontology
//!    Predictor (`next_node_kind`, `parent_gate`, `feasibility`, `stride`,
//!    `catalog_id`, `context_tokens`).
//! 3. **Vocabulary corpus (`vocab_corpus.txt`)**: Pure cybersecurity and TARA
//!    text used by `scripts/prune_vocab.py` to strip non-TARA tokens from open
//!    models.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use aegis_model::GateKind;
use aegis_model::library::{Facet, ItemKind, ItemNode, LibraryItem, LibraryPack};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::corpus::{
    OpenCatalogScenario, SeedScenario, open_catalog_scenarios, out_of_domain_prompts,
    seed_scenarios, slot_binding_seeds,
};
use crate::dpo::{DpoPair, build_dpo_pairs};
use crate::dsl::{
    END_TOKEN, OUT_OF_DOMAIN_TOKEN, TaskKind, ThreatProposal, format_threats_dsl, format_tree_dsl,
};
use crate::verifier::{Verifier, VerifyError};

/// System instruction prepended to every Gear 2 training and inference prompt.
pub const SYSTEM_PROMPT: &str = "<|system|>AegisTARA-Core: emit only valid TARA DSL (<|gate:...|> / <|step|> / <|threat|>) or <|out_of_domain|>.";

/// One verified SFT example for Gear 2 (`mlx-lm` compatible JSONL).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SftExample {
    /// Deterministic SHA-256 hex digest of `(task, prompt, completion)`.
    pub id: String,
    /// Task category.
    pub task: TaskKind,
    /// Language tag (`en` or `ja`).
    pub lang: String,
    /// Source item or scenario identifier.
    pub source_id: String,
    /// Input prompt ending with the task control token.
    pub prompt: String,
    /// Target DSL completion ending with `<|end|>` or `<|out_of_domain|>`.
    pub completion: String,
}

/// Minimal `{"prompt": ..., "completion": ...}` row expected by `mlx_lm.lora`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MlxRow<'a> {
    /// Prompt text.
    pub prompt: &'a str,
    /// Completion text.
    pub completion: &'a str,
}

/// One structured training record for the from-scratch Gear 1 Tree Predictor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gear1Example {
    /// Deterministic ID.
    pub id: String,
    /// Language (`en` or `ja`).
    pub lang: String,
    /// Asset interfaces (`can`, `obd`, `cellular`, `ethernet`, `debug-port`, ...).
    pub interfaces: Vec<String>,
    /// STRIDE category (`spoofing`, `tampering`, `denial-of-service`, ...).
    pub stride: String,
    /// Parent gate type (`or`, `and`, `seq_and`, or `root`).
    pub parent_gate: String,
    /// Depth of the target node in the attack tree (0 = root).
    pub depth: usize,
    /// 0-based index among the parent gate's children.
    pub sibling_index: usize,
    /// Concatenated path labels from root to parent plus prior siblings.
    pub context_text: String,
    /// Target structural node kind (`step`, `or`, `and`, `seq_and`).
    pub target_node_kind: String,
    /// Target feasibility key (`low`, `medium`, `high`, or `gate`).
    pub target_feasibility: String,
    /// Target primary catalog ID (`CAPEC-125`, `4.1`, etc.).
    pub target_catalog: String,
    /// Target node label text.
    pub target_label: String,
}

/// Aggregate statistics of a compiled dataset.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetStats {
    /// Total verified Gear 2 SFT examples.
    pub total_sft: usize,
    /// Total Gear 1 structural predictor examples.
    pub total_gear1: usize,
    /// Total verified DPO preference pairs (Task AI-10).
    pub total_dpo: usize,
    /// Count of SFT examples in the train split.
    pub train_count: usize,
    /// Count of SFT examples in the validation split.
    pub valid_count: usize,
    /// Count of SFT examples in the test split.
    pub test_count: usize,
    /// Examples per [`TaskKind`].
    pub by_task: BTreeMap<String, usize>,
    /// Examples per language (`en`, `ja`).
    pub by_lang: BTreeMap<String, usize>,
    /// Gear 1 examples per target node kind (`step`, `or`, `and`, `seq_and`).
    pub gear1_by_node_kind: BTreeMap<String, usize>,
}

/// Compiled dataset holding verified Gear 1, Gear 2 SFT, and DPO examples.
#[derive(Clone, Debug)]
pub struct CompiledDataset {
    /// All verified Gear 2 SFT examples.
    pub sft_examples: Vec<SftExample>,
    /// All Gear 1 structural examples.
    pub gear1_examples: Vec<Gear1Example>,
    /// All verified DPO preference pairs.
    pub dpo_pairs: Vec<DpoPair>,
    /// Summary statistics.
    pub stats: DatasetStats,
}

impl CompiledDataset {
    /// Builds and compiler-verifies the complete dataset from the built-in
    /// library pack (`library-demo.json`), the ISO 21434 + UN R155 methodology
    /// pack, and the bilingual seed bank in [`crate::corpus`].
    ///
    /// Builds and compiler-verifies the English-focused dataset from the
    /// built-in library pack (`library-demo.json`), the ISO 21434 + UN R155
    /// methodology pack, the curated English seed bank in [`crate::corpus`],
    /// and the open-source MITRE EMB3D + CAPEC catalog (`packs/open_cyber_en.json`).
    ///
    /// Every generated tree or threat completion is checked by [`Verifier`];
    /// if any example fails verification, this returns [`VerifyError`].
    ///
    /// # Errors
    ///
    /// Returns [`VerifyError`] if any synthesized completion fails the
    /// `aegis-model` compiler/linter check.
    pub fn build_verified() -> Result<Self, VerifyError> {
        Self::build_verified_with_options(false)
    }

    /// Builds and compiler-verifies the dataset, optionally including the
    /// Japanese (`ja`) seed bank alongside the primary English (`en`) corpus.
    ///
    /// # Errors
    ///
    /// Returns [`VerifyError`] if any synthesized completion fails verification.
    pub fn build_verified_with_options(include_japanese: bool) -> Result<Self, VerifyError> {
        let verifier = Verifier::iso21434_r155();
        let pack = LibraryPack::builtin_demo();
        let scenarios = seed_scenarios();
        let open_scenarios = open_catalog_scenarios();

        let mut sft = Vec::new();
        let mut gear1 = Vec::new();

        // 1. Expand built-in library pack items (`crates/aegis-model/packs/library-demo.json`).
        for item in pack.items() {
            compile_library_item(&verifier, item, include_japanese, &mut sft, &mut gear1)?;
        }

        // 2. Expand curated seed scenarios (`src/corpus.rs`).
        for scen in scenarios
            .iter()
            .filter(|s| include_japanese || s.lang == "en")
        {
            compile_seed_scenario(&verifier, scen, &mut sft, &mut gear1)?;
        }

        // 3. Expand MITRE EMB3D + MITRE CAPEC open-catalog scenarios (`packs/open_cyber_en.json`).
        for open_scen in &open_scenarios {
            compile_open_catalog_scenario(&verifier, open_scen, &mut sft, &mut gear1)?;
        }
        compile_open_catalog_threat_groups(&verifier, &open_scenarios, &mut sft)?;

        // 4. Expand template slot-binding examples (`{bus}`, `{ecu}`, `{function}`).
        for item in pack.items().iter().filter(|i| i.kind == ItemKind::Template) {
            for binding in slot_binding_seeds()
                .into_iter()
                .filter(|b| include_japanese || b.lang == "en")
            {
                compile_slot_binding(
                    &verifier,
                    item,
                    binding.lang,
                    binding.bus,
                    binding.ecu,
                    binding.function,
                    &mut sft,
                )?;
            }
        }

        // 5. Add out-of-domain refusal examples (`<|out_of_domain|>`).
        for (idx, prompt_text) in out_of_domain_prompts().iter().enumerate() {
            let lang = if prompt_text.is_ascii() { "en" } else { "ja" };
            if !include_japanese && lang != "en" {
                continue;
            }
            let prompt = format!("{SYSTEM_PROMPT}\n{prompt_text}");
            let completion = OUT_OF_DOMAIN_TOKEN.to_owned();
            push_sft(
                &mut sft,
                TaskKind::OutOfDomain,
                lang,
                &format!("ood-{idx}"),
                prompt,
                completion,
            );
        }

        // Deduplicate by deterministic ID while preserving stable order.
        let mut seen_sft = std::collections::BTreeSet::new();
        sft.retain(|ex| seen_sft.insert(ex.id.clone()));

        let mut seen_g1 = std::collections::BTreeSet::new();
        gear1.retain(|ex| seen_g1.insert(ex.id.clone()));

        let dpo_pairs = build_dpo_pairs(&verifier, &sft)?;
        let stats = compute_stats(&sft, &gear1, &dpo_pairs);
        Ok(Self {
            sft_examples: sft,
            gear1_examples: gear1,
            dpo_pairs,
            stats,
        })
    }

    /// Writes `mlx/{train,valid,test}.jsonl`, `gear1/{train,valid,test}.jsonl`,
    /// `dpo/{train,valid,test}.jsonl`, `sft_full.jsonl`, `vocab_corpus.txt`, and
    /// `stats.json` under `out_dir`.
    ///
    /// # Errors
    ///
    /// Returns [`std::io::Error`] if directory creation or file writing fails.
    pub fn write_to_dir(&self, out_dir: &Path) -> std::io::Result<()> {
        let mlx_dir = out_dir.join("mlx");
        let gear1_dir = out_dir.join("gear1");
        let dpo_dir = out_dir.join("dpo");
        fs::create_dir_all(&mlx_dir)?;
        fs::create_dir_all(&gear1_dir)?;
        fs::create_dir_all(&dpo_dir)?;

        let mut mlx_train = String::new();
        let mut mlx_valid = String::new();
        let mut mlx_test = String::new();
        let mut sft_full = String::new();
        let mut vocab_corpus = String::new();

        for ex in &self.sft_examples {
            let full_line = serde_json::to_string(ex).map_err(std::io::Error::other)?;
            sft_full.push_str(&full_line);
            sft_full.push('\n');

            let row = MlxRow {
                prompt: &ex.prompt,
                completion: &ex.completion,
            };
            let mlx_line = serde_json::to_string(&row).map_err(std::io::Error::other)?;
            match split_bucket(&ex.id) {
                Split::Train => {
                    mlx_train.push_str(&mlx_line);
                    mlx_train.push('\n');
                }
                Split::Valid => {
                    mlx_valid.push_str(&mlx_line);
                    mlx_valid.push('\n');
                }
                Split::Test => {
                    mlx_test.push_str(&mlx_line);
                    mlx_test.push('\n');
                }
            }

            if ex.task != TaskKind::OutOfDomain {
                vocab_corpus.push_str(&ex.prompt);
                vocab_corpus.push('\n');
                vocab_corpus.push_str(&ex.completion);
                vocab_corpus.push('\n');
            }
        }

        fs::write(mlx_dir.join("train.jsonl"), mlx_train)?;
        fs::write(mlx_dir.join("valid.jsonl"), mlx_valid)?;
        fs::write(mlx_dir.join("test.jsonl"), mlx_test)?;
        fs::write(out_dir.join("sft_full.jsonl"), sft_full)?;
        fs::write(out_dir.join("vocab_corpus.txt"), vocab_corpus)?;

        let mut g1_train = String::new();
        let mut g1_valid = String::new();
        let mut g1_test = String::new();
        for ex in &self.gear1_examples {
            let line = serde_json::to_string(ex).map_err(std::io::Error::other)?;
            match split_bucket(&ex.id) {
                Split::Train => {
                    g1_train.push_str(&line);
                    g1_train.push('\n');
                }
                Split::Valid => {
                    g1_valid.push_str(&line);
                    g1_valid.push('\n');
                }
                Split::Test => {
                    g1_test.push_str(&line);
                    g1_test.push('\n');
                }
            }
        }
        fs::write(gear1_dir.join("train.jsonl"), g1_train)?;
        fs::write(gear1_dir.join("valid.jsonl"), g1_valid)?;
        fs::write(gear1_dir.join("test.jsonl"), g1_test)?;

        let mut dpo_train = String::new();
        let mut dpo_valid = String::new();
        let mut dpo_test = String::new();
        for pair in &self.dpo_pairs {
            let line = serde_json::to_string(pair).map_err(std::io::Error::other)?;
            match split_bucket(&pair.id) {
                Split::Train => {
                    dpo_train.push_str(&line);
                    dpo_train.push('\n');
                }
                Split::Valid => {
                    dpo_valid.push_str(&line);
                    dpo_valid.push('\n');
                }
                Split::Test => {
                    dpo_test.push_str(&line);
                    dpo_test.push('\n');
                }
            }
        }
        fs::write(dpo_dir.join("train.jsonl"), dpo_train)?;
        fs::write(dpo_dir.join("valid.jsonl"), dpo_valid)?;
        fs::write(dpo_dir.join("test.jsonl"), dpo_test)?;

        let stats_json =
            serde_json::to_string_pretty(&self.stats).map_err(std::io::Error::other)?;
        fs::write(out_dir.join("stats.json"), stats_json)?;

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Split {
    Train,
    Valid,
    Test,
}

/// Deterministic 80% train / 10% valid / 10% test split based on hash byte.
fn split_bucket(id_hex: &str) -> Split {
    let byte = u8::from_str_radix(id_hex.get(0..2).unwrap_or("00"), 16).unwrap_or(0);
    if byte < 204 {
        Split::Train
    } else if byte < 230 {
        Split::Valid
    } else {
        Split::Test
    }
}

fn compile_library_item(
    verifier: &Verifier,
    item: &LibraryItem,
    include_japanese: bool,
    sft: &mut Vec<SftExample>,
    gear1: &mut Vec<Gear1Example>,
) -> Result<(), VerifyError> {
    let interfaces = item
        .facets
        .get(&Facet::Interface)
        .cloned()
        .unwrap_or_else(|| vec!["can".into()]);
    let assets = item
        .facets
        .get(&Facet::Asset)
        .cloned()
        .unwrap_or_else(|| vec!["ecu".into()]);
    let strides = item
        .facets
        .get(&Facet::Stride)
        .cloned()
        .unwrap_or_else(|| vec!["tampering".into()]);
    let catalogs = item
        .facets
        .get(&Facet::Catalog)
        .cloned()
        .unwrap_or_else(|| vec!["4.1".into()]);

    let asset_str = assets.join(",");
    let iface_str = interfaces.join(",");
    let stride_str = &strides[0];
    let cat_str = catalogs.join(",");

    // Verify and add `DraftTree` / `ExpandStep` for non-leaf library items.
    if item.tree.gate.is_some() {
        let completion = format_tree_dsl(&item.tree);
        verifier.verify_tree_dsl(&completion)?;

        let prompt_en = format!(
            "{SYSTEM_PROMPT}\n<|meth|>iso21434-r155<|lang|>en<|asset|>{asset_str}<|interfaces|>{iface_str}\n<|threat|>{}<|stride|>{stride_str}<|cat|>{cat_str}\n{}",
            item.name,
            TaskKind::DraftTree.control_token()
        );
        push_sft(
            sft,
            TaskKind::DraftTree,
            "en",
            &item.id,
            prompt_en,
            completion.clone(),
        );

        // If the library item has a Japanese alias and Japanese is enabled, also emit a bilingual prompt pair.
        if include_japanese && let Some(ja_alias) = item.aliases.first() {
            let prompt_ja = format!(
                "{SYSTEM_PROMPT}\n<|meth|>iso21434-r155<|lang|>ja<|asset|>{asset_str}<|interfaces|>{iface_str}\n<|threat|>{ja_alias}<|stride|>{stride_str}<|cat|>{cat_str}\n{}",
                TaskKind::DraftTree.control_token()
            );
            push_sft(
                sft,
                TaskKind::DraftTree,
                "ja",
                &item.id,
                prompt_ja,
                completion,
            );
        }

        // Combinatorial masking across all gates and subtrees across canonical ECU contexts.
        for ecu_ctx in [
            asset_str.as_str(),
            "Electronic Brake Controller (EBC)",
            "Telematics Control Unit (TCU)",
            "Central Vehicle Gateway",
        ] {
            let mut path = Vec::new();
            harvest_tree_cuts(
                verifier,
                &format!("{}-{}", item.id, ecu_ctx),
                "en",
                ecu_ctx,
                &interfaces,
                stride_str,
                &cat_str,
                &item.tree,
                &mut path,
                sft,
                gear1,
            )?;
        }
    } else {
        // Single leaf item: verify and record in Gear 1.
        let completion = format_tree_dsl(&item.tree);
        verifier.verify_tree_dsl(&completion)?;
        push_gear1(
            gear1,
            "en",
            &interfaces,
            stride_str,
            "root",
            0,
            0,
            &item.name,
            &item.tree,
            &catalogs[0],
        );
    }

    Ok(())
}

fn compile_seed_scenario(
    verifier: &Verifier,
    scen: &SeedScenario,
    sft: &mut Vec<SftExample>,
    gear1: &mut Vec<Gear1Example>,
) -> Result<(), VerifyError> {
    let iface_str = scen.interfaces.join(",");
    let props_str = scen.properties.join(",");
    let cat_str = scen.catalog_ids.join(",");
    let ifaces_vec: Vec<String> = scen.interfaces.iter().map(|&s| s.to_owned()).collect();

    // 1. `SuggestThreats`: Asset + Interfaces + Properties -> Threat Proposals.
    if !scen.threat_proposals.is_empty() {
        let threats_dsl = format_threats_dsl(&scen.threat_proposals);
        verifier.verify_threats_dsl(&threats_dsl)?;
        let prompt = format!(
            "{SYSTEM_PROMPT}\n<|meth|>{}<|lang|>{}<|asset|>{}<|interfaces|>{iface_str}<|props|>{props_str}\n{}",
            scen.methodology_key,
            scen.lang,
            scen.asset_name,
            TaskKind::SuggestThreats.control_token()
        );
        push_sft(
            sft,
            TaskKind::SuggestThreats,
            scen.lang,
            scen.id,
            prompt,
            threats_dsl,
        );
    }

    // 2. `DraftTree`: Threat + Asset + Interfaces -> Full Attack Tree.
    let tree_dsl = format_tree_dsl(&scen.tree);
    verifier.verify_tree_dsl(&tree_dsl)?;
    let draft_prompt = format!(
        "{SYSTEM_PROMPT}\n<|meth|>{}<|lang|>{}<|asset|>{}<|interfaces|>{iface_str}\n<|threat|>{}<|stride|>{}<|cat|>{cat_str}<|damage|>{}<|impact|>{}\n{}",
        scen.methodology_key,
        scen.lang,
        scen.asset_name,
        scen.threat_name,
        scen.stride,
        scen.damage_name,
        scen.impact,
        TaskKind::DraftTree.control_token()
    );
    push_sft(
        sft,
        TaskKind::DraftTree,
        scen.lang,
        scen.id,
        draft_prompt,
        tree_dsl,
    );

    // 3. Combinatorial cuts (`PredictSibling` and `ExpandStep`) + Gear 1 rows across canonical ECU variants.
    for (variant_idx, ecu_ctx) in [
        scen.asset_name,
        "Electronic Brake Controller (EBC)",
        "ADAS Compute Module",
        "Zone Gateway Controller",
    ]
    .into_iter()
    .enumerate()
    {
        let mut path = Vec::new();
        harvest_tree_cuts(
            verifier,
            &format!("{}-v{variant_idx}", scen.id),
            scen.lang,
            ecu_ctx,
            &ifaces_vec,
            scen.stride,
            &cat_str,
            &scen.tree,
            &mut path,
            sft,
            gear1,
        )?;
    }

    Ok(())
}

fn compile_open_catalog_scenario(
    verifier: &Verifier,
    scen: &OpenCatalogScenario,
    sft: &mut Vec<SftExample>,
    gear1: &mut Vec<Gear1Example>,
) -> Result<(), VerifyError> {
    let iface_str = scen.interfaces.join(",");
    let cat_str = scen.catalog_ids.join(",");

    let tree_dsl = format_tree_dsl(&scen.tree);
    verifier.verify_tree_dsl(&tree_dsl)?;
    let draft_prompt = format!(
        "{SYSTEM_PROMPT}\n<|meth|>{}<|lang|>{}<|asset|>{}<|interfaces|>{iface_str}\n<|threat|>{}<|stride|>{}<|cat|>{cat_str}<|damage|>{}<|impact|>{}\n{}",
        scen.methodology_key,
        scen.lang,
        scen.asset_name,
        scen.threat_name,
        scen.stride,
        scen.damage_name,
        scen.impact,
        TaskKind::DraftTree.control_token()
    );
    push_sft(
        sft,
        TaskKind::DraftTree,
        &scen.lang,
        &scen.id,
        draft_prompt,
        tree_dsl,
    );

    let mut path = Vec::new();
    harvest_tree_cuts(
        verifier,
        &scen.id,
        &scen.lang,
        &scen.asset_name,
        &scen.interfaces,
        &scen.stride,
        &cat_str,
        &scen.tree,
        &mut path,
        sft,
        gear1,
    )?;

    Ok(())
}

fn compile_open_catalog_threat_groups(
    verifier: &Verifier,
    scenarios: &[OpenCatalogScenario],
    sft: &mut Vec<SftExample>,
) -> Result<(), VerifyError> {
    // Group open-catalog threats into windows of 3 per asset + source to build
    // rich SuggestThreats examples grounded in MITRE EMB3D and CAPEC.
    let mut by_asset: BTreeMap<(&str, &str), Vec<&OpenCatalogScenario>> = BTreeMap::new();
    for s in scenarios {
        by_asset
            .entry((s.asset_name.as_str(), s.source.as_str()))
            .or_default()
            .push(s);
    }

    for ((asset_name, source_name), group) in by_asset {
        for (chunk_idx, window) in group.chunks(3).enumerate() {
            if window.is_empty() {
                continue;
            }
            let first = window[0];
            let iface_str = first.interfaces.join(",");
            let props_str = first.properties.join(",");
            let proposals: Vec<ThreatProposal> = window
                .iter()
                .map(|w| ThreatProposal {
                    name: w.threat_name.clone(),
                    stride: w.stride.clone(),
                    catalog_ids: w.catalog_ids.clone(),
                    damage_name: w.damage_name.clone(),
                    impact: w.impact.clone(),
                })
                .collect();
            let threats_dsl = format_threats_dsl(&proposals);
            verifier.verify_threats_dsl(&threats_dsl)?;
            let prompt = format!(
                "{SYSTEM_PROMPT}\n<|meth|>{}<|lang|>{}<|asset|>{asset_name}<|interfaces|>{iface_str}<|props|>{props_str}<|source|>{source_name}#{chunk_idx}\n{}",
                first.methodology_key,
                first.lang,
                TaskKind::SuggestThreats.control_token()
            );
            push_sft(
                sft,
                TaskKind::SuggestThreats,
                &first.lang,
                &format!("open-threats-{source_name}-{chunk_idx}"),
                prompt,
                threats_dsl,
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn harvest_tree_cuts(
    verifier: &Verifier,
    source_id: &str,
    lang: &str,
    asset_name: &str,
    interfaces: &[String],
    stride: &str,
    catalog_str: &str,
    node: &ItemNode,
    ancestors: &mut Vec<(GateKind, String)>,
    sft: &mut Vec<SftExample>,
    gear1: &mut Vec<Gear1Example>,
) -> Result<(), VerifyError> {
    let iface_str = interfaces.join(",");
    let primary_cat = catalog_str.split(',').next().unwrap_or("4.1");

    let parent_gate_name = ancestors.last().map_or("root", |(k, _)| gate_kind_str(*k));
    let root_or_anc_context = if ancestors.is_empty() {
        format!("{} [ROOT]", node.label)
    } else {
        format_ancestor_path(ancestors)
    };
    push_gear1(
        gear1,
        lang,
        interfaces,
        stride,
        parent_gate_name,
        ancestors.len(),
        0,
        &root_or_anc_context,
        node,
        primary_cat,
    );

    let Some(gate_kind) = node.gate else {
        return Ok(());
    };

    // If this is a non-root gate, create an `ExpandStep` training pair where
    // the model is asked to expand `node.label` into this gate and its children.
    if !ancestors.is_empty() {
        let sub_dsl = format_tree_dsl(node);
        verifier.verify_tree_dsl(&sub_dsl)?;
        let tree_path = format_ancestor_path(ancestors);
        let expand_prompt = format!(
            "{SYSTEM_PROMPT}\n<|meth|>iso21434-r155<|lang|>{lang}<|asset|>{asset_name}<|interfaces|>{iface_str}<|stride|>{stride}\n<|tree_path|>{tree_path}\n<|step|>{}\n{}",
            node.label,
            TaskKind::ExpandStep.control_token()
        );
        push_sft(
            sft,
            TaskKind::ExpandStep,
            lang,
            source_id,
            expand_prompt,
            sub_dsl,
        );
    }

    ancestors.push((gate_kind, node.label.clone()));
    let current_path = format_ancestor_path(ancestors);

    // For every child `i` under this gate:
    // 1. Prefix cut: given children `0..i`, predict child `i`.
    // 2. Leave-one-out cut (when gate has >= 2 children): given all siblings except `i`, predict `i`.
    for (i, target_child) in node.children.iter().enumerate() {
        let target_dsl = format_tree_dsl(target_child);
        verifier.verify_tree_dsl(&target_dsl)?;

        let prefix_siblings: Vec<String> = node.children[..i]
            .iter()
            .enumerate()
            .map(|(idx, c)| format!("{}. {}", idx + 1, c.label))
            .collect();
        let sib_text = if prefix_siblings.is_empty() {
            "none".to_owned()
        } else {
            prefix_siblings.join(" | ")
        };

        let prefix_prompt = format!(
            "{SYSTEM_PROMPT}\n<|meth|>iso21434-r155<|lang|>{lang}<|asset|>{asset_name}<|interfaces|>{iface_str}<|stride|>{stride}<|cat|>{catalog_str}\n<|tree_path|>{current_path}\n<|siblings|>{sib_text}\n{}",
            TaskKind::PredictSibling.control_token()
        );
        push_sft(
            sft,
            TaskKind::PredictSibling,
            lang,
            source_id,
            prefix_prompt,
            target_dsl.clone(),
        );

        let context_for_g1 = format!("{current_path} :: {sib_text}");
        push_gear1(
            gear1,
            lang,
            interfaces,
            stride,
            gate_kind_str(gate_kind),
            ancestors.len(),
            i,
            &context_for_g1,
            target_child,
            primary_cat,
        );

        if node.children.len() >= 2 {
            let other_siblings: Vec<String> = node
                .children
                .iter()
                .enumerate()
                .filter(|&(idx, _)| idx != i)
                .map(|(idx, c)| format!("{}. {}", idx + 1, c.label))
                .collect();
            let loo_sib_text = other_siblings.join(" | ");
            let loo_prompt = format!(
                "{SYSTEM_PROMPT}\n<|meth|>iso21434-r155<|lang|>{lang}<|asset|>{asset_name}<|interfaces|>{iface_str}<|stride|>{stride}<|cat|>{catalog_str}\n<|tree_path|>{current_path}\n<|siblings|>{loo_sib_text}\n{}",
                TaskKind::PredictSibling.control_token()
            );
            push_sft(
                sft,
                TaskKind::PredictSibling,
                lang,
                source_id,
                loo_prompt,
                target_dsl,
            );
            let loo_context_g1 = format!("{current_path} :: {loo_sib_text}");
            push_gear1(
                gear1,
                lang,
                interfaces,
                stride,
                gate_kind_str(gate_kind),
                ancestors.len(),
                other_siblings.len(),
                &loo_context_g1,
                target_child,
                primary_cat,
            );
        }
    }

    // Recurse into child gates.
    for child in &node.children {
        if child.gate.is_some() {
            harvest_tree_cuts(
                verifier,
                source_id,
                lang,
                asset_name,
                interfaces,
                stride,
                catalog_str,
                child,
                ancestors,
                sft,
                gear1,
            )?;
        }
    }

    ancestors.pop();
    Ok(())
}

fn compile_slot_binding(
    verifier: &Verifier,
    template_item: &LibraryItem,
    lang: &str,
    bus: &str,
    ecu: &str,
    function: &str,
    sft: &mut Vec<SftExample>,
) -> Result<(), VerifyError> {
    let bound_tree = bind_node_slots(&template_item.tree, bus, ecu, function);
    let completion = format_tree_dsl(&bound_tree);
    verifier.verify_tree_dsl(&completion)?;

    let template_dsl = format_tree_dsl(&template_item.tree)
        .trim_end_matches(END_TOKEN)
        .trim()
        .to_owned();
    let prompt = format!(
        "{SYSTEM_PROMPT}\n<|meth|>iso21434-r155<|lang|>{lang}\n<|template|>\n{template_dsl}\n<|slots|>bus={bus} | ecu={ecu} | function={function}\n{}",
        TaskKind::BindSlots.control_token()
    );
    push_sft(
        sft,
        TaskKind::BindSlots,
        lang,
        &template_item.id,
        prompt,
        completion,
    );
    Ok(())
}

fn bind_node_slots(node: &ItemNode, bus: &str, ecu: &str, function: &str) -> ItemNode {
    let label = node
        .label
        .replace("{bus}", bus)
        .replace("{ecu}", ecu)
        .replace("{function}", function);
    let children = node
        .children
        .iter()
        .map(|c| bind_node_slots(c, bus, ecu, function))
        .collect();
    ItemNode {
        label,
        gate: node.gate,
        feasibility: node.feasibility.clone(),
        attack_type: None,
        children,
    }
}

fn format_ancestor_path(ancestors: &[(GateKind, String)]) -> String {
    if ancestors.is_empty() {
        return "root".to_owned();
    }
    ancestors
        .iter()
        .map(|(k, label)| format!("{} [{}]", label, gate_kind_str(*k).to_uppercase()))
        .collect::<Vec<_>>()
        .join(" -> ")
}

const fn gate_kind_str(kind: GateKind) -> &'static str {
    match kind {
        GateKind::Or => "or",
        GateKind::And => "and",
        GateKind::SeqAnd => "seq_and",
    }
}

fn push_sft(
    out: &mut Vec<SftExample>,
    task: TaskKind,
    lang: &str,
    source_id: &str,
    prompt: String,
    completion: String,
) {
    let id = hash_hex(&format!("{}:{prompt}:{completion}", task.as_str()));
    out.push(SftExample {
        id,
        task,
        lang: lang.to_owned(),
        source_id: source_id.to_owned(),
        prompt,
        completion,
    });
}

#[allow(clippy::too_many_arguments)]
fn push_gear1(
    out: &mut Vec<Gear1Example>,
    lang: &str,
    interfaces: &[String],
    stride: &str,
    parent_gate: &str,
    depth: usize,
    sibling_index: usize,
    context_text: &str,
    target_node: &ItemNode,
    catalog_id: &str,
) {
    let target_node_kind = match target_node.gate {
        Some(k) => gate_kind_str(k).to_owned(),
        None => "step".to_owned(),
    };
    let target_feasibility = target_node
        .feasibility
        .clone()
        .unwrap_or_else(|| "gate".into());
    let id = hash_hex(&format!(
        "g1:{lang}:{stride}:{parent_gate}:{depth}:{sibling_index}:{context_text}:{}",
        target_node.label
    ));
    out.push(Gear1Example {
        id,
        lang: lang.to_owned(),
        interfaces: interfaces.to_vec(),
        stride: stride.to_owned(),
        parent_gate: parent_gate.to_owned(),
        depth,
        sibling_index,
        context_text: context_text.to_owned(),
        target_node_kind,
        target_feasibility,
        target_catalog: catalog_id.to_owned(),
        target_label: target_node.label.clone(),
    });
}

fn hash_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        })
}

fn compute_stats(sft: &[SftExample], gear1: &[Gear1Example], dpo: &[DpoPair]) -> DatasetStats {
    let mut stats = DatasetStats {
        total_sft: sft.len(),
        total_gear1: gear1.len(),
        total_dpo: dpo.len(),
        ..DatasetStats::default()
    };
    for ex in sft {
        match split_bucket(&ex.id) {
            Split::Train => stats.train_count += 1,
            Split::Valid => stats.valid_count += 1,
            Split::Test => stats.test_count += 1,
        }
        *stats
            .by_task
            .entry(ex.task.as_str().to_owned())
            .or_default() += 1;
        *stats.by_lang.entry(ex.lang.clone()).or_default() += 1;
    }
    for g1 in gear1 {
        *stats
            .gear1_by_node_kind
            .entry(g1.target_node_kind.clone())
            .or_default() += 1;
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_verifies_full_dataset() {
        let dataset = CompiledDataset::build_verified()
            .expect("all generated English dataset examples must pass aegis-model verification");
        assert!(dataset.stats.total_sft > 1500);
        assert!(dataset.stats.total_gear1 > 1500);
        assert!(dataset.stats.by_task.contains_key("predict_sibling"));
        assert!(dataset.stats.by_task.contains_key("expand_step"));
        assert!(dataset.stats.by_task.contains_key("draft_tree"));
        assert!(dataset.stats.by_task.contains_key("suggest_threats"));
        assert!(dataset.stats.by_task.contains_key("bind_slots"));
        assert!(dataset.stats.by_task.contains_key("out_of_domain"));
        assert!(dataset.stats.by_lang.contains_key("en"));
        assert!(!dataset.stats.by_lang.contains_key("ja"));

        let bilingual = CompiledDataset::build_verified_with_options(true)
            .expect("bilingual dataset must also pass verification");
        assert!(bilingual.stats.by_lang.contains_key("ja"));
    }
}
