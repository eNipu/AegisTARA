//! Attack trees as an indented text outline, for editing in the app.
//!
//! The outline is the simplest editor that covers every tree the model can
//! hold (AND, OR and sequential AND gates, shared steps), without a drawing
//! tool:
//!
//! ```text
//! OR Execute a forged remote command
//!   SEQ Attack the cellular link
//!     - Set up a rogue base station
//!     - Downgrade the connection to 2G
//!   AND Compromise the backend credentials
//!     - Phish an operator account
//!     - Bypass the multi-factor prompt
//!   - Exploit the debug port
//! ```
//!
//! Rules:
//!
//! - One node per line. A line indented deeper than the line above is its
//!   child. The first line is the root.
//! - `OR `, `AND ` and `SEQ ` (sequential AND) start a gate. `- ` starts a
//!   step; a line with no prefix is a step too.
//! - **Lines with the same text are the same node.** That is how a step is
//!   shared between branches (the tree is a DAG). The children of a shared
//!   gate are written under its first line only.
//! - Nodes keep their identity (and so their ratings, factors and controls)
//!   when their text and kind stay the same. A renamed node is a new node.
//!
//! Parsing is O(lines x depth); a tree has at most [`MAX_ATTACK_TREE_NODES`]
//! nodes, so the text is at most a few kilobytes.

use std::collections::HashMap;
use std::fmt::Write as _;

use aegis_model::{AttackTree, GateKind, MAX_ATTACK_TREE_NODES, Node, NodeId, NodeIdx};

/// Longest node text, in characters.
pub const MAX_LABEL_CHARS: usize = 256;
/// Longest outline accepted, in bytes (R3): enough for the largest tree.
pub const MAX_OUTLINE_BYTES: usize = 64 * 1024;

/// Why an outline is not a tree. Lines count from 1.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OutlineError {
    /// No line has text.
    #[error("the outline is empty")]
    Empty,
    /// The text is longer than [`MAX_OUTLINE_BYTES`].
    #[error("the outline is too long")]
    TooLong,
    /// The first line is indented.
    #[error("line {0}: the first line is the root and must not be indented")]
    RootIndented(usize),
    /// A second line at the root's level.
    #[error("line {0}: there can be only one root; indent this line under it")]
    SecondRoot(usize),
    /// A line under a step.
    #[error("line {0}: a step cannot have children; make its parent a gate (OR, AND or SEQ)")]
    UnderStep(usize),
    /// A gate prefix with no text.
    #[error("line {0}: the node has no text")]
    NoText(usize),
    /// Text longer than [`MAX_LABEL_CHARS`].
    #[error("line {0}: the text is longer than 256 characters")]
    LabelTooLong(usize),
    /// The same text used for a gate and a step, or two gate kinds.
    #[error("line {0}: this text is used before for a different kind of node")]
    KindConflict(usize),
    /// A shared gate with children under a second occurrence.
    #[error("line {0}: write the children of a shared gate under its first line only")]
    SharedRedefined(usize),
    /// Sharing made a node its own ancestor.
    #[error("line {0}: this makes a node part of itself (a cycle)")]
    Cycle(usize),
    /// A node twice under the same parent.
    #[error("line {0}: this node is already a child of the same parent")]
    DuplicateChild(usize),
    /// More than [`MAX_ATTACK_TREE_NODES`] distinct nodes.
    #[error("an attack tree has at most {max} nodes")]
    TooManyNodes {
        /// The limit.
        max: usize,
    },
}

const PREFIXES: [(&str, Option<GateKind>); 4] = [
    ("OR ", Some(GateKind::Or)),
    ("AND ", Some(GateKind::And)),
    ("SEQ ", Some(GateKind::SeqAnd)),
    ("- ", None),
];

fn prefix(kind: Option<GateKind>) -> &'static str {
    match kind {
        Some(GateKind::Or) => "OR ",
        Some(GateKind::And) => "AND ",
        Some(GateKind::SeqAnd) => "SEQ ",
        None => "- ",
    }
}

/// Writes a tree as an outline. A shared node is written each time it is
/// reached; its children only the first time. Time O(V + E).
#[must_use]
pub fn to_outline(tree: &AttackTree) -> String {
    let mut out = String::new();
    let mut seen = vec![false; tree.len()];
    write_node(tree, tree.root(), 0, &mut seen, &mut out);
    out
}

fn write_node(tree: &AttackTree, idx: NodeIdx, depth: usize, seen: &mut [bool], out: &mut String) {
    let Some(node) = tree.get(idx) else {
        return;
    };
    let _ = writeln!(
        out,
        "{}{}{}",
        "  ".repeat(depth),
        prefix(node.gate_kind()),
        node.label()
    );
    let first = !std::mem::replace(&mut seen[idx.index()], true);
    if first {
        for &child in tree.children(idx) {
            write_node(tree, child, depth + 1, seen, out);
        }
    }
}

struct Line<'a> {
    number: usize,
    indent: usize,
    kind: Option<GateKind>,
    label: &'a str,
}

fn parse_line(number: usize, raw: &str) -> Result<Option<Line<'_>>, OutlineError> {
    let text = raw.trim_end();
    let body = text.trim_start();
    if body.is_empty() {
        return Ok(None);
    }
    // A tab counts as two spaces.
    let indent = text[..text.len() - body.len()]
        .chars()
        .map(|c| if c == '\t' { 2 } else { 1 })
        .sum();
    let (kind, label) = PREFIXES
        .iter()
        .find_map(|(p, k)| body.strip_prefix(p).map(|rest| (*k, rest.trim())))
        .unwrap_or((None, body));
    if label.is_empty() || matches!(body, "OR" | "AND" | "SEQ" | "-") {
        return Err(OutlineError::NoText(number));
    }
    if label.chars().count() > MAX_LABEL_CHARS {
        return Err(OutlineError::LabelTooLong(number));
    }
    Ok(Some(Line {
        number,
        indent,
        kind,
        label,
    }))
}

/// Reads an outline into a tree. Nodes whose text and kind match a node of
/// `previous` keep that node's ID; others get new IDs.
///
/// # Errors
///
/// The first [`OutlineError`], with its line.
pub fn parse_outline(
    text: &str,
    previous: Option<&AttackTree>,
) -> Result<AttackTree, OutlineError> {
    if text.len() > MAX_OUTLINE_BYTES {
        return Err(OutlineError::TooLong);
    }
    let mut lines = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        if let Some(line) = parse_line(i + 1, raw)? {
            lines.push(line);
        }
    }
    let Some(first) = lines.first() else {
        return Err(OutlineError::Empty);
    };
    if first.indent != 0 {
        return Err(OutlineError::RootIndented(first.number));
    }

    // Old IDs by (label, kind).
    let old: HashMap<(&str, Option<GateKind>), NodeId> = previous
        .map(|t| {
            t.iter()
                .map(|(_, id, node)| ((node.label(), node.gate_kind()), id))
                .collect()
        })
        .unwrap_or_default();
    let id_for = |label: &str, kind: Option<GateKind>| {
        old.get(&(label, kind))
            .copied()
            .unwrap_or_else(NodeId::new_v7)
    };
    let node_of = |label: &str, kind: Option<GateKind>| match kind {
        Some(k) => Node::gate(k, label),
        None => Node::step(label),
    };

    let mut tree = AttackTree::new(
        id_for(first.label, first.kind),
        node_of(first.label, first.kind),
    )
    .map_err(|_| OutlineError::Empty)?;
    // Text to (node, kind, whether its children were written).
    let mut by_label: HashMap<&str, (NodeIdx, Option<GateKind>, bool)> = HashMap::new();
    by_label.insert(first.label, (tree.root(), first.kind, true));
    // The open ancestors: (indent, node, kind, accepts children here).
    let mut stack: Vec<(usize, NodeIdx, Option<GateKind>, bool)> =
        vec![(0, tree.root(), first.kind, true)];

    for line in &lines[1..] {
        while stack
            .last()
            .is_some_and(|(indent, ..)| *indent >= line.indent)
        {
            stack.pop();
        }
        // The root has indent 0 and every later line has more, so only a
        // second line at indent 0 empties the stack.
        let Some(&(_, parent, parent_kind, accepts)) = stack.last() else {
            return Err(OutlineError::SecondRoot(line.number));
        };
        if parent_kind.is_none() {
            return Err(OutlineError::UnderStep(line.number));
        }
        if !accepts {
            return Err(OutlineError::SharedRedefined(line.number));
        }
        let (idx, fresh) = match by_label.get(line.label) {
            Some(&(idx, kind, _)) => {
                if kind != line.kind {
                    return Err(OutlineError::KindConflict(line.number));
                }
                if tree.children(parent).contains(&idx) {
                    return Err(OutlineError::DuplicateChild(line.number));
                }
                tree.link(parent, idx)
                    .map_err(|_| OutlineError::Cycle(line.number))?;
                (idx, false)
            }
            None => {
                if tree.len() >= MAX_ATTACK_TREE_NODES {
                    return Err(OutlineError::TooManyNodes {
                        max: MAX_ATTACK_TREE_NODES,
                    });
                }
                let idx = tree
                    .add_child(
                        parent,
                        id_for(line.label, line.kind),
                        node_of(line.label, line.kind),
                    )
                    .map_err(|_| OutlineError::TooManyNodes {
                        max: MAX_ATTACK_TREE_NODES,
                    })?;
                by_label.insert(line.label, (idx, line.kind, true));
                (idx, true)
            }
        };
        // Children may follow only the first line of a node.
        stack.push((line.indent, idx, line.kind, fresh));
    }
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "OR Root\n  SEQ Cellular\n    - Rogue station\n    - Downgrade\n  AND Backend\n    - Phish\n    Bypass MFA\n  - Debug port\n";

    fn labels(tree: &AttackTree) -> Vec<String> {
        tree.iter().map(|(_, _, n)| n.label().to_owned()).collect()
    }

    #[test]
    fn an_outline_round_trips() {
        let tree = parse_outline(SAMPLE, None).expect("parses");
        assert_eq!(tree.len(), 8);
        let text = to_outline(&tree);
        assert!(text.contains("    - Bypass MFA\n"), "{text}");
        let again = parse_outline(&text, Some(&tree)).expect("parses");
        assert_eq!(to_outline(&again), text);
        // Unchanged nodes keep their IDs.
        for (_, id, node) in again.iter() {
            assert_eq!(
                tree.idx_of(&id).and_then(|i| tree.get(i)).map(Node::label),
                Some(node.label())
            );
        }
    }

    #[test]
    fn same_text_is_one_shared_node() {
        let text =
            "OR Root\n  AND A\n    - Shared step\n    - Only A\n  AND B\n    - Shared step\n";
        let tree = parse_outline(text, None).expect("parses");
        assert_eq!(tree.len(), 5, "{:?}", labels(&tree));
        let (shared, ..) = tree
            .iter()
            .find(|(_, _, n)| n.label() == "Shared step")
            .expect("shared");
        assert_eq!(tree.parents(shared).count(), 2);
        // Written twice, children once.
        assert_eq!(to_outline(&tree).matches("Shared step").count(), 2);
    }

    #[test]
    fn renamed_nodes_get_new_ids() {
        let tree = parse_outline(SAMPLE, None).expect("parses");
        let edited =
            parse_outline(&SAMPLE.replace("Phish", "Phish an admin"), Some(&tree)).expect("parses");
        let old_ids: Vec<NodeId> = tree.iter().map(|(_, id, _)| id).collect();
        let new_ids: Vec<NodeId> = edited.iter().map(|(_, id, _)| id).collect();
        assert_eq!(new_ids.iter().filter(|id| old_ids.contains(id)).count(), 7);
    }

    #[test]
    fn errors_name_the_line() {
        let cases = [
            ("", OutlineError::Empty),
            ("  OR Root", OutlineError::RootIndented(1)),
            ("OR Root\nOther", OutlineError::SecondRoot(2)),
            (
                "OR Root\n  - Step\n    - Under step",
                OutlineError::UnderStep(3),
            ),
            ("OR Root\n  AND\n", OutlineError::NoText(2)),
            ("OR Root\n  - X\n  AND X\n", OutlineError::KindConflict(3)),
            (
                "OR Root\n  AND A\n    AND G\n      - a\n  AND B\n    AND G\n      - b\n",
                OutlineError::SharedRedefined(7),
            ),
            ("OR Root\n  AND G\n    OR Root\n", OutlineError::Cycle(3)),
            ("OR Root\n  - a\n  - a\n", OutlineError::DuplicateChild(3)),
        ];
        for (text, expected) in cases {
            assert_eq!(parse_outline(text, None).err(), Some(expected), "{text:?}");
        }
    }

    #[test]
    fn the_node_limit_holds() {
        let mut text = String::from("OR Root\n");
        for i in 0..MAX_ATTACK_TREE_NODES {
            text.push_str(&format!("  - step {i}\n"));
        }
        assert_eq!(
            parse_outline(&text, None).err(),
            Some(OutlineError::TooManyNodes {
                max: MAX_ATTACK_TREE_NODES
            })
        );
        assert_eq!(
            parse_outline(&"x".repeat(MAX_OUTLINE_BYTES + 1), None).err(),
            Some(OutlineError::TooLong)
        );
    }

    #[test]
    fn tabs_and_bare_steps_work() {
        let tree = parse_outline("AND Root\n\tFirst\n\t- Second\n", None).expect("parses");
        assert_eq!(labels(&tree), ["Root", "First", "Second"]);
    }
}
