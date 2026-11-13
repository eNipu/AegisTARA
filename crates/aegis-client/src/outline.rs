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
//!   The root is the exception: it is always the tree's root, and a new text
//!   for the first line renames it.
//! - A step may say what it goes through and its attack type after its name:
//!   `- Read the flash | through: C00 | type: debug-access`. Both are
//!   optional, in either order; `-` means none. The values are text here.
//! - Only `through:` and `type:` are read after ` | `. Other text after
//!   ` | ` stays in the name, unless it looks like `key: value` with a
//!   lower-case key; such a name is written with ` | through: - | type: -`
//!   at the end, which keeps it whole.
//! - A gate has no such values. A shared step's values are written on its
//!   first line; later lines repeat the same values or none.
//!
//! Parsing is O(lines x depth); a tree has at most [`MAX_ATTACK_TREE_NODES`]
//! nodes, so the text is at most [`MAX_OUTLINE_BYTES`].

use std::collections::HashMap;
use std::fmt::Write as _;

use aegis_model::{
    AttackTree, AttackType, GateKind, ItemRef, MAX_ATTACK_TREE_NODES, Node, NodeId, NodeIdx,
    Project, TreeIdx,
};

/// Longest node text, in characters.
pub const MAX_LABEL_CHARS: usize = 256;
/// Longest outline accepted, in bytes (R3): enough for the largest tree
/// (4,096 nodes of 256 characters, with indentation).
pub const MAX_OUTLINE_BYTES: usize = 2 * 1024 * 1024;

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
    /// Text after ` | ` that looks like a value but is not `through:` or `type:`.
    #[error(
        "line {0}: after \" | \" only \"through: <ID>\" and \"type: <key>\" are understood; to keep such text in the name, end the line with \" | through: - | type: -\""
    )]
    UnknownNote(usize),
    /// `through:` or `type:` twice on one line.
    #[error("line {0}: \"through\" or \"type\" is written twice")]
    DuplicateNote(usize),
    /// `through:` or `type:` on a gate.
    #[error("line {0}: only a step can go through something or have an attack type, not a gate")]
    NoteOnGate(usize),
    /// A shared step written again with other values.
    #[error(
        "line {0}: this step is written before with another \"through\" or \"type\"; write them on its first line"
    )]
    NoteConflict(usize),
    /// A `through:` value that is no component or interface ID.
    #[error("line {0}: no component or interface has this ID (write it like C00 or IF00)")]
    UnknownItem(usize),
    /// A `type:` value that is no attack type key of the methodology.
    #[error("line {0}: this methodology has no attack type with this key")]
    UnknownAttackType(usize),
}

/// What one step line says after its name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepNote {
    /// Line of the step's first occurrence, from 1.
    pub line: usize,
    /// The `through:` value as written; `None` when absent or `-`.
    pub through: Option<String>,
    /// The `type:` value as written; `None` when absent or `-`.
    pub attack_type: Option<String>,
}

/// A parsed outline: the tree and what its steps say after their names.
#[derive(Clone, Debug)]
pub struct ParsedOutline {
    /// The tree.
    pub tree: AttackTree,
    /// One entry per step, in the order of first lines. Gates have none.
    pub notes: Vec<(NodeId, StepNote)>,
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
    to_outline_with(tree, |_| (None, None))
}

/// Writes a tree as an outline, with what `note` returns for each step
/// (through, attack type) after its name. A value that is empty, `-`, or has
/// a character other than an ASCII letter, digit, `_` or `-` is not written.
/// Time O(V + E).
#[must_use]
pub fn to_outline_with<F>(tree: &AttackTree, mut note: F) -> String
where
    F: FnMut(NodeIdx) -> (Option<String>, Option<String>),
{
    let mut out = String::new();
    // The tail of each node, built on its first line and reused after.
    let mut tails: Vec<Option<String>> = vec![None; tree.len()];
    write_node(tree, tree.root(), 0, &mut tails, &mut out, &mut note);
    out
}

/// Whether `value` reads back as written after `through:` or `type:`:
/// not empty, not `-`, and only ASCII letters, digits, `_` and `-`.
/// Time O(length).
fn is_note_value(value: &str) -> bool {
    value != "-"
        && !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The text written after a node's name.
fn tail_of(label: &str, through: Option<&str>, attack_type: Option<&str>) -> String {
    let through = through.filter(|v| is_note_value(v));
    let attack_type = attack_type.filter(|v| is_note_value(v));
    let plain = matches!(read_tail(0, label), Ok(t) if t.name == label && !t.has_tail);
    if !plain {
        return format!(
            " | through: {} | type: {}",
            through.unwrap_or("-"),
            attack_type.unwrap_or("-")
        );
    }
    let mut tail = String::new();
    if let Some(v) = through {
        let _ = write!(tail, " | through: {v}");
    }
    if let Some(v) = attack_type {
        let _ = write!(tail, " | type: {v}");
    }
    tail
}

/// The outline of a tree of `project` with what each step goes through
/// (the visual ID, `C00` or `IF00`) and its attack type (the pack key)
/// after its name. `None` if the tree is gone. Time O(V + E).
#[must_use]
pub fn to_outline_annotated(project: &Project, tree: TreeIdx) -> Option<String> {
    let t = project.attack_tree(tree)?;
    let methodology = project.methodology();
    Some(to_outline_with(t, |node| {
        let through = project.step_via(tree, node).and_then(|via| match via {
            ItemRef::Component(id) => project
                .component_idx(&id)
                .and_then(|i| project.component_visual_id(i)),
            ItemRef::Interface(id) => project
                .interface_idx(&id)
                .and_then(|i| project.interface_visual_id(i)),
        });
        let kind = project
            .step_attack_type(tree, node)
            .and_then(|k| methodology.attack_type_key(k))
            .map(str::to_owned);
        (through, kind)
    }))
}

/// One step's values, resolved against a project.
pub(crate) type ResolvedNote = (NodeId, Option<ItemRef>, Option<AttackType>);

/// Resolves what the outline says about its steps. Time O(components +
/// interfaces + notes).
///
/// # Errors
///
/// [`OutlineError::UnknownItem`] or [`OutlineError::UnknownAttackType`]
/// with the step's line.
pub(crate) fn resolve_notes(
    project: &Project,
    notes: &[(NodeId, StepNote)],
) -> Result<Vec<ResolvedNote>, OutlineError> {
    let mut items: HashMap<String, ItemRef> = HashMap::new();
    for (idx, id, _) in project.components() {
        if let Some(v) = project.component_visual_id(idx) {
            items.insert(v, ItemRef::Component(id));
        }
    }
    for (idx, id, _) in project.interfaces() {
        if let Some(v) = project.interface_visual_id(idx) {
            items.insert(v, ItemRef::Interface(id));
        }
    }
    let methodology = project.methodology();
    let mut out = Vec::with_capacity(notes.len());
    for (node, note) in notes {
        let via = match &note.through {
            Some(v) => Some(
                *items
                    .get(&v.to_ascii_uppercase())
                    .ok_or(OutlineError::UnknownItem(note.line))?,
            ),
            None => None,
        };
        let kind = match &note.attack_type {
            Some(k) => Some(
                methodology
                    .attack_type(k)
                    .ok_or(OutlineError::UnknownAttackType(note.line))?,
            ),
            None => None,
        };
        out.push((*node, via, kind));
    }
    Ok(out)
}

fn write_node<F>(
    tree: &AttackTree,
    idx: NodeIdx,
    depth: usize,
    tails: &mut [Option<String>],
    out: &mut String,
    note: &mut F,
) where
    F: FnMut(NodeIdx) -> (Option<String>, Option<String>),
{
    let Some(node) = tree.get(idx) else {
        return;
    };
    let first = tails[idx.index()].is_none();
    if first {
        let (through, attack_type) = if node.gate_kind().is_none() {
            note(idx)
        } else {
            (None, None)
        };
        tails[idx.index()] = Some(tail_of(
            node.label(),
            through.as_deref(),
            attack_type.as_deref(),
        ));
    }
    let _ = writeln!(
        out,
        "{}{}{}{}",
        "  ".repeat(depth),
        prefix(node.gate_kind()),
        node.label(),
        tails[idx.index()].as_deref().unwrap_or_default()
    );
    if first {
        for &child in tree.children(idx) {
            write_node(tree, child, depth + 1, tails, out, note);
        }
    }
}

struct Line<'a> {
    number: usize,
    indent: usize,
    kind: Option<GateKind>,
    label: &'a str,
    through: Option<&'a str>,
    attack_type: Option<&'a str>,
    has_tail: bool,
}

/// A name and the values read after it.
struct Tail<'a> {
    name: &'a str,
    through: Option<&'a str>,
    attack_type: Option<&'a str>,
    has_tail: bool,
}

/// Reads ` | through: <v>` and ` | type: <v>` off the end of `rest`.
fn read_tail(number: usize, rest: &str) -> Result<Tail<'_>, OutlineError> {
    let mut name = rest;
    let mut through: Option<&str> = None;
    let mut attack_type: Option<&str> = None;
    while through.is_none() || attack_type.is_none() {
        let Some(pos) = name.rfind(" | ") else {
            break;
        };
        let seg = name[pos + 3..].trim();
        let Some((key, value)) = seg.split_once(':') else {
            break;
        };
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_lowercase()) {
            break;
        }
        let value = value.trim();
        let slot = match key {
            "through" => &mut through,
            "type" => &mut attack_type,
            _ => return Err(OutlineError::UnknownNote(number)),
        };
        if slot.is_some() {
            return Err(OutlineError::DuplicateNote(number));
        }
        if value.is_empty() {
            return Err(OutlineError::UnknownNote(number));
        }
        *slot = Some(value);
        name = name[..pos].trim_end();
    }
    Ok(Tail {
        name,
        has_tail: through.is_some() || attack_type.is_some(),
        through: through.filter(|v| *v != "-"),
        attack_type: attack_type.filter(|v| *v != "-"),
    })
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
    let tail = read_tail(number, label)?;
    let label = tail.name;
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
        through: tail.through,
        attack_type: tail.attack_type,
        has_tail: tail.has_tail,
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
    parse_outline_annotated(text, previous).map(|p| p.tree)
}

/// Reads an outline into a tree, with what each step says after its name.
/// Nodes keep their IDs as in [`parse_outline`].
///
/// # Errors
///
/// The first [`OutlineError`], with its line.
pub fn parse_outline_annotated(
    text: &str,
    previous: Option<&AttackTree>,
) -> Result<ParsedOutline, OutlineError> {
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
    // The root is the tree's root whatever its text says, so changing the
    // text of the first line renames the root; it never replaces the tree.
    let root_id = previous
        .and_then(|t| t.id_of(t.root()))
        .unwrap_or_else(NodeId::new_v7);
    let old: HashMap<(&str, Option<GateKind>), NodeId> = previous
        .map(|t| {
            t.iter()
                .filter(|(idx, ..)| *idx != t.root())
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

    let gate_note = |line: &Line<'_>| {
        if line.kind.is_some() && (line.through.is_some() || line.attack_type.is_some()) {
            return Err(OutlineError::NoteOnGate(line.number));
        }
        Ok(())
    };
    gate_note(first)?;

    let mut tree = AttackTree::new(root_id, node_of(first.label, first.kind))
        .map_err(|_| OutlineError::Empty)?;
    let mut notes = Vec::new();
    let mut note_of = |tree: &AttackTree, idx: NodeIdx, line: &Line<'_>| {
        if line.kind.is_none()
            && let Some(id) = tree.id_of(idx)
        {
            notes.push((
                id,
                StepNote {
                    line: line.number,
                    through: line.through.map(str::to_owned),
                    attack_type: line.attack_type.map(str::to_owned),
                },
            ));
        }
    };
    note_of(&tree, tree.root(), first);
    // Text to (node, kind, whether its children were written, the first
    // line's through and type).
    type Entry<'a> = (
        NodeIdx,
        Option<GateKind>,
        bool,
        Option<&'a str>,
        Option<&'a str>,
    );
    let mut by_label: HashMap<&str, Entry<'_>> = HashMap::new();
    by_label.insert(
        first.label,
        (
            tree.root(),
            first.kind,
            true,
            first.through,
            first.attack_type,
        ),
    );
    // The open ancestors: (indent, node, kind, accepts children here).
    let mut stack: Vec<(usize, NodeIdx, Option<GateKind>, bool)> =
        vec![(0, tree.root(), first.kind, true)];

    for line in &lines[1..] {
        gate_note(line)?;
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
            Some(&(idx, kind, _, through, attack_type)) => {
                if kind != line.kind {
                    return Err(OutlineError::KindConflict(line.number));
                }
                if line.has_tail && (line.through, line.attack_type) != (through, attack_type) {
                    return Err(OutlineError::NoteConflict(line.number));
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
                by_label.insert(
                    line.label,
                    (idx, line.kind, true, line.through, line.attack_type),
                );
                note_of(&tree, idx, line);
                (idx, true)
            }
        };
        // Children may follow only the first line of a node.
        stack.push((line.indent, idx, line.kind, fresh));
    }
    Ok(ParsedOutline { tree, notes })
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
