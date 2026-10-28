//! Compact Domain-Specific Language (DSL) for `AegisTARA` AI models (Task AI-01).
//!
//! Small CPU-bound models generate tokens sequentially. Verbose JSON wastes
//! 60-70% of token budget on repeated braces, quotes, and field names. This
//! module defines a deterministic, line-oriented DSL with explicit control
//! tokens that maps 1-to-1 to [`aegis_model::library::ItemNode`],
//! [`aegis_model::AttackTree`], and threat scenario proposals:
//!
//! ```text
//! <|gate:seq_and|>Run modified code on the ECU over UDS
//!   <|step|>Reach the OBD port<|feas|>low
//!   <|gate:or|>Pass security access
//!     <|step|>Guess a weak diagnostic PIN<|feas|>low
//!     <|step|>Recover the seed-key secret from another unit<|feas|>high
//!   <|step|>Reflash the ECU with modified firmware<|feas|>medium
//! <|end|>
//! ```
//!
//! Every indentation level is two spaces (`  `) per tree depth. Any non-TARA
//! or out-of-scope query maps to the single refusal token `<|out_of_domain|>`.

use std::fmt::Write as _;

use aegis_model::GateKind;
use aegis_model::library::ItemNode;
use serde::{Deserialize, Serialize};

/// Special control tokens reserved for the `AegisTARA` tokenizer and grammar.
pub const SPECIAL_TOKENS: &[&str] = &[
    "<|system|>",
    "<|meth|>",
    "<|lang|>",
    "<|asset|>",
    "<|interfaces|>",
    "<|props|>",
    "<|threat|>",
    "<|stride|>",
    "<|cat|>",
    "<|damage|>",
    "<|impact|>",
    "<|tree_path|>",
    "<|siblings|>",
    "<|template|>",
    "<|slots|>",
    "<|task:predict_sibling|>",
    "<|task:expand_step|>",
    "<|task:draft_tree|>",
    "<|task:suggest_threats|>",
    "<|task:bind_slots|>",
    "<|gate:or|>",
    "<|gate:and|>",
    "<|gate:seq_and|>",
    "<|step|>",
    "<|feas|>",
    "<|out_of_domain|>",
    "<|end|>",
];

/// Token emitted immediately when a prompt is outside TARA and cybersecurity.
pub const OUT_OF_DOMAIN_TOKEN: &str = "<|out_of_domain|>";

/// End-of-completion sentinel token.
pub const END_TOKEN: &str = "<|end|>";

/// The specific TARA task a training or inference prompt asks the model to perform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// Predict the next child node (step or sub-gate) inside an open gate.
    PredictSibling,
    /// Expand a leaf attack step into a gate (`AND`, `OR`, `SEQ_AND`) and child steps.
    ExpandStep,
    /// Draft a complete candidate attack tree for a threat scenario and asset.
    DraftTree,
    /// Propose threat scenarios (with STRIDE, catalog IDs, and damage scenario) for an asset.
    SuggestThreats,
    /// Bind `{slot}` placeholders in an attack tree template to concrete project names.
    BindSlots,
    /// Refuse a non-cybersecurity / non-TARA prompt with `<|out_of_domain|>`.
    OutOfDomain,
}

impl TaskKind {
    /// Returns the special control token for this task.
    #[must_use]
    pub const fn control_token(self) -> &'static str {
        match self {
            Self::PredictSibling => "<|task:predict_sibling|>",
            Self::ExpandStep => "<|task:expand_step|>",
            Self::DraftTree => "<|task:draft_tree|>",
            Self::SuggestThreats => "<|task:suggest_threats|>",
            Self::BindSlots => "<|task:bind_slots|>",
            Self::OutOfDomain => "<|out_of_domain|>",
        }
    }

    /// Short `snake_case` key used in stats and filenames.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PredictSibling => "predict_sibling",
            Self::ExpandStep => "expand_step",
            Self::DraftTree => "draft_tree",
            Self::SuggestThreats => "suggest_threats",
            Self::BindSlots => "bind_slots",
            Self::OutOfDomain => "out_of_domain",
        }
    }
}

/// One proposed threat scenario emitted by [`TaskKind::SuggestThreats`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreatProposal {
    /// Threat scenario name.
    pub name: String,
    /// Threat category key in the methodology (e.g. `spoofing`, `tampering`).
    pub stride: String,
    /// Catalog reference IDs (e.g. `4.1`, `CAPEC-125`).
    pub catalog_ids: Vec<String>,
    /// Associated damage scenario title.
    pub damage_name: String,
    /// Proposed impact category and level (`safety:severe`, `operational:major`).
    pub impact: String,
}

/// Errors while parsing a DSL completion back into `aegis-model` structures.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DslError {
    /// The model refused the prompt as out of domain.
    #[error("prompt was refused as out of domain (<|out_of_domain|>)")]
    OutOfDomain,
    /// The completion is empty or missing any tree/threat lines.
    #[error("the DSL completion is empty")]
    Empty,
    /// The completion did not end with `<|end|>`.
    #[error("the DSL completion is missing the <|end|> sentinel")]
    MissingEndToken,
    /// Indentation is not a multiple of two spaces or jumps more than one level.
    #[error("invalid indentation on line {line}: {reason}")]
    BadIndentation {
        /// 1-based line number.
        line: usize,
        /// Explanation.
        reason: &'static str,
    },
    /// A line does not start with a recognized node or threat tag.
    #[error("unrecognized DSL tag on line {line}")]
    UnknownTag {
        /// 1-based line number.
        line: usize,
    },
    /// A node label or required field is empty.
    #[error("empty label or field on line {line}")]
    EmptyLabel {
        /// 1-based line number.
        line: usize,
    },
    /// A gate has a `<|feas|>` tag or a step has children.
    #[error("structural rule violated on line {line}: {reason}")]
    Structure {
        /// 1-based line number.
        line: usize,
        /// Explanation.
        reason: &'static str,
    },
}

/// Formats an [`ItemNode`] subtree into the compact `AegisTARA` DSL, ending
/// with `<|end|>`.
#[must_use]
pub fn format_tree_dsl(root: &ItemNode) -> String {
    let mut out = String::new();
    write_node_dsl(&mut out, root, 0);
    out.push_str(END_TOKEN);
    out
}

/// Formats a single [`ItemNode`] at indentation depth `depth`.
fn write_node_dsl(out: &mut String, node: &ItemNode, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    match node.gate {
        Some(GateKind::Or) => {
            let _ = writeln!(out, "<|gate:or|>{}", node.label);
        }
        Some(GateKind::And) => {
            let _ = writeln!(out, "<|gate:and|>{}", node.label);
        }
        Some(GateKind::SeqAnd) => {
            let _ = writeln!(out, "<|gate:seq_and|>{}", node.label);
        }
        None => {
            let _ = write!(out, "<|step|>{}", node.label);
            if let Some(feas) = &node.feasibility {
                let _ = write!(out, "<|feas|>{feas}");
            }
            out.push('\n');
        }
    }
    for child in &node.children {
        write_node_dsl(out, child, depth + 1);
    }
}

/// Parses a compact `AegisTARA` DSL completion back into an [`ItemNode`] tree.
///
/// # Errors
///
/// Returns [`DslError`] if the completion is `<|out_of_domain|>`, lacks
/// `<|end|>`, has malformed indentation, or violates gate/step structure.
pub fn parse_tree_dsl(input: &str) -> Result<ItemNode, DslError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(DslError::Empty);
    }
    if trimmed.starts_with(OUT_OF_DOMAIN_TOKEN) {
        return Err(DslError::OutOfDomain);
    }

    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut saw_end = false;

    for (idx, raw_line) in input.lines().enumerate() {
        let line_no = idx + 1;
        let line_trimmed_end = raw_line.trim_end();
        if line_trimmed_end.trim().is_empty() {
            continue;
        }
        if line_trimmed_end.trim() == END_TOKEN {
            saw_end = true;
            break;
        }
        if let Some(before_end) = line_trimmed_end.strip_suffix(END_TOKEN) {
            if !before_end.trim().is_empty() {
                lines.push((line_no, before_end));
            }
            saw_end = true;
            break;
        }
        lines.push((line_no, line_trimmed_end));
    }

    if !saw_end {
        return Err(DslError::MissingEndToken);
    }
    if lines.is_empty() {
        return Err(DslError::Empty);
    }

    let mut parsed_lines: Vec<(usize, usize, ItemNode)> = Vec::with_capacity(lines.len());
    for (line_no, raw) in lines {
        let leading_spaces = raw.bytes().take_while(|&b| b == b' ').count();
        if leading_spaces % 2 != 0 {
            return Err(DslError::BadIndentation {
                line: line_no,
                reason: "indentation must be a multiple of 2 spaces",
            });
        }
        let depth = leading_spaces / 2;
        let content = &raw[leading_spaces..];
        let node = parse_single_node_line(line_no, content)?;
        parsed_lines.push((line_no, depth, node));
    }

    if parsed_lines[0].1 != 0 {
        return Err(DslError::BadIndentation {
            line: parsed_lines[0].0,
            reason: "root node must be at depth 0",
        });
    }

    let mut cursor = 0;
    let root = assemble_subtree(&parsed_lines, &mut cursor, 0)?;
    if cursor != parsed_lines.len() {
        return Err(DslError::BadIndentation {
            line: parsed_lines[cursor].0,
            reason: "multiple root nodes at depth 0",
        });
    }
    Ok(root)
}

fn parse_single_node_line(line_no: usize, content: &str) -> Result<ItemNode, DslError> {
    if let Some(rest) = content.strip_prefix("<|gate:or|>") {
        parse_gate_rest(line_no, GateKind::Or, rest)
    } else if let Some(rest) = content.strip_prefix("<|gate:and|>") {
        parse_gate_rest(line_no, GateKind::And, rest)
    } else if let Some(rest) = content.strip_prefix("<|gate:seq_and|>") {
        parse_gate_rest(line_no, GateKind::SeqAnd, rest)
    } else if let Some(rest) = content.strip_prefix("<|step|>") {
        let (label_part, feas_part) = match rest.split_once("<|feas|>") {
            Some((l, f)) => (l.trim(), Some(f.trim())),
            None => (rest.trim(), None),
        };
        if label_part.is_empty() {
            return Err(DslError::EmptyLabel { line: line_no });
        }
        let feasibility = match feas_part {
            Some("") => return Err(DslError::EmptyLabel { line: line_no }),
            Some(f) => Some(f.to_owned()),
            None => None,
        };
        Ok(ItemNode {
            label: label_part.to_owned(),
            gate: None,
            feasibility,
            children: Vec::new(),
        })
    } else {
        Err(DslError::UnknownTag { line: line_no })
    }
}

fn parse_gate_rest(line_no: usize, kind: GateKind, rest: &str) -> Result<ItemNode, DslError> {
    if rest.contains("<|feas|>") {
        return Err(DslError::Structure {
            line: line_no,
            reason: "gates may not carry a <|feas|> rating",
        });
    }
    let label = rest.trim();
    if label.is_empty() {
        return Err(DslError::EmptyLabel { line: line_no });
    }
    Ok(ItemNode {
        label: label.to_owned(),
        gate: Some(kind),
        feasibility: None,
        children: Vec::new(),
    })
}

fn assemble_subtree(
    lines: &[(usize, usize, ItemNode)],
    cursor: &mut usize,
    expected_depth: usize,
) -> Result<ItemNode, DslError> {
    let (line_no, depth, mut node) = lines[*cursor].clone();
    if depth != expected_depth {
        return Err(DslError::BadIndentation {
            line: line_no,
            reason: "unexpected indentation jump",
        });
    }
    *cursor += 1;

    while *cursor < lines.len() {
        let (child_line, child_depth, _) = &lines[*cursor];
        if *child_depth <= expected_depth {
            break;
        }
        if *child_depth > expected_depth + 1 {
            return Err(DslError::BadIndentation {
                line: *child_line,
                reason: "child depth jumped by more than one level",
            });
        }
        if node.gate.is_none() {
            return Err(DslError::Structure {
                line: *child_line,
                reason: "a leaf <|step|> node cannot have indented children",
            });
        }
        let child = assemble_subtree(lines, cursor, expected_depth + 1)?;
        node.children.push(child);
    }

    if node.gate.is_some() && node.children.is_empty() {
        return Err(DslError::Structure {
            line: line_no,
            reason: "a gate node must have at least one child",
        });
    }
    Ok(node)
}

/// Formats a list of [`ThreatProposal`] values into the compact DSL.
#[must_use]
pub fn format_threats_dsl(proposals: &[ThreatProposal]) -> String {
    let mut out = String::new();
    for p in proposals {
        let cats = p.catalog_ids.join(",");
        let _ = writeln!(
            out,
            "<|threat|>{}<|stride|>{}<|cat|>{}<|damage|>{}<|impact|>{}",
            p.name, p.stride, cats, p.damage_name, p.impact
        );
    }
    out.push_str(END_TOKEN);
    out
}

/// Parses a [`TaskKind::SuggestThreats`] DSL completion into [`ThreatProposal`] items.
///
/// # Errors
///
/// Returns [`DslError`] if any line is malformed or missing required fields.
pub fn parse_threats_dsl(input: &str) -> Result<Vec<ThreatProposal>, DslError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(DslError::Empty);
    }
    if trimmed.starts_with(OUT_OF_DOMAIN_TOKEN) {
        return Err(DslError::OutOfDomain);
    }

    let mut proposals = Vec::new();
    let mut saw_end = false;

    for (idx, raw_line) in input.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line == END_TOKEN {
            saw_end = true;
            break;
        }
        let line = if let Some(stripped) = line.strip_suffix(END_TOKEN) {
            saw_end = true;
            stripped.trim()
        } else {
            line
        };
        if line.is_empty() {
            break;
        }
        let Some(rest) = line.strip_prefix("<|threat|>") else {
            return Err(DslError::UnknownTag { line: line_no });
        };
        let Some((name, rest)) = rest.split_once("<|stride|>") else {
            return Err(DslError::UnknownTag { line: line_no });
        };
        let Some((stride, rest)) = rest.split_once("<|cat|>") else {
            return Err(DslError::UnknownTag { line: line_no });
        };
        let Some((cats, rest)) = rest.split_once("<|damage|>") else {
            return Err(DslError::UnknownTag { line: line_no });
        };
        let Some((damage_name, impact)) = rest.split_once("<|impact|>") else {
            return Err(DslError::UnknownTag { line: line_no });
        };

        let name = name.trim();
        let stride = stride.trim();
        let damage_name = damage_name.trim();
        let impact = impact.trim();
        if name.is_empty() || stride.is_empty() || damage_name.is_empty() || impact.is_empty() {
            return Err(DslError::EmptyLabel { line: line_no });
        }
        let catalog_ids: Vec<String> = cats
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        if catalog_ids.is_empty() {
            return Err(DslError::EmptyLabel { line: line_no });
        }

        proposals.push(ThreatProposal {
            name: name.to_owned(),
            stride: stride.to_owned(),
            catalog_ids,
            damage_name: damage_name.to_owned(),
            impact: impact.to_owned(),
        });
        if saw_end {
            break;
        }
    }

    if !saw_end {
        return Err(DslError::MissingEndToken);
    }
    if proposals.is_empty() {
        return Err(DslError::Empty);
    }
    Ok(proposals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_tree_dsl() {
        let tree = ItemNode {
            label: "Run modified code on the ECU over UDS".into(),
            gate: Some(GateKind::SeqAnd),
            feasibility: None,
            children: vec![
                ItemNode {
                    label: "Reach the OBD port".into(),
                    gate: None,
                    feasibility: Some("low".into()),
                    children: vec![],
                },
                ItemNode {
                    label: "Pass security access".into(),
                    gate: Some(GateKind::Or),
                    feasibility: None,
                    children: vec![
                        ItemNode {
                            label: "Guess a weak diagnostic PIN".into(),
                            gate: None,
                            feasibility: Some("low".into()),
                            children: vec![],
                        },
                        ItemNode {
                            label: "Recover the seed-key secret from another unit".into(),
                            gate: None,
                            feasibility: Some("high".into()),
                            children: vec![],
                        },
                    ],
                },
            ],
        };

        let dsl = format_tree_dsl(&tree);
        let parsed = parse_tree_dsl(&dsl).expect("valid DSL should parse");
        assert_eq!(parsed, tree);
    }

    #[test]
    fn round_trip_threats_dsl() {
        let threats = vec![ThreatProposal {
            name: "Inject spoofed braking frames on CAN bus".into(),
            stride: "spoofing".into(),
            catalog_ids: vec!["4.1".into(), "CAPEC-151".into()],
            damage_name: "Unintended vehicle deceleration".into(),
            impact: "safety:severe".into(),
        }];
        let dsl = format_threats_dsl(&threats);
        let parsed = parse_threats_dsl(&dsl).expect("valid threats DSL should parse");
        assert_eq!(parsed, threats);
    }

    #[test]
    fn rejects_out_of_domain_and_malformed_dsl() {
        assert_eq!(
            parse_tree_dsl("<|out_of_domain|>"),
            Err(DslError::OutOfDomain)
        );
        assert_eq!(
            parse_tree_dsl("<|step|>Reach OBD port"),
            Err(DslError::MissingEndToken)
        );
        assert_eq!(
            parse_tree_dsl("<|gate:or|>Empty gate\n<|end|>"),
            Err(DslError::Structure {
                line: 1,
                reason: "a gate node must have at least one child",
            })
        );
    }
}
