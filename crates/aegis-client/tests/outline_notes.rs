//! `through:` and `type:` after a step's name in the outline text (122da).

use aegis_client::outline::{
    OutlineError, StepNote, parse_outline, parse_outline_annotated, to_outline, to_outline_with,
};
use aegis_model::{AttackTree, GateKind, Node, NodeId};

type Read = (String, Option<String>, Option<String>);

/// Parses `OR Goal` with `step` on line 2; gives the step's name and values.
fn read_step(step: &str) -> Result<Read, OutlineError> {
    let parsed = parse_outline_annotated(&format!("OR Goal\n  - {step}\n"), None)?;
    let tree = &parsed.tree;
    let idx = tree.children(tree.root())[0];
    let name = tree
        .get(idx)
        .map(|n| n.label().to_owned())
        .unwrap_or_default();
    let note = parsed
        .notes
        .iter()
        .find(|(id, _)| Some(*id) == tree.id_of(idx))
        .map(|(_, n)| n.clone())
        .unwrap_or_default();
    assert_eq!(note.line, 2);
    Ok((name, note.through, note.attack_type))
}

fn some(s: &str) -> Option<String> {
    Some(s.to_owned())
}

#[test]
fn the_format_examples_hold() {
    let ok: [(&str, &str, Option<String>, Option<String>); 6] = [
        (
            "Flash | through: C00 | type: debug-access",
            "Flash",
            some("C00"),
            some("debug-access"),
        ),
        (
            "Flash | type: debug-access | through: IF03",
            "Flash",
            some("IF03"),
            some("debug-access"),
        ),
        ("Flash | through: -", "Flash", None, None),
        ("Read A | B", "Read A | B", None, None),
        ("Ratio | Then: go", "Ratio | Then: go", None, None),
        (
            "A | note: x | through: - | type: -",
            "A | note: x",
            None,
            None,
        ),
    ];
    for (line, name, through, attack_type) in ok {
        assert_eq!(
            read_step(line),
            Ok((name.to_owned(), through, attack_type)),
            "{line:?}"
        );
    }
    let bad = [
        ("A | note: x", OutlineError::UnknownNote(2)),
        ("A | throgh: C00", OutlineError::UnknownNote(2)),
        ("A | through:", OutlineError::UnknownNote(2)),
        ("A | type: a | type: b", OutlineError::DuplicateNote(2)),
    ];
    for (line, err) in bad {
        assert_eq!(read_step(line), Err(err), "{line:?}");
    }
}

#[test]
fn a_dash_counts_as_a_tail() {
    // The first line is bare; `| through: -` equals it, so no conflict.
    let text = "OR Goal\n  AND A\n    - Flash\n  AND B\n    - Flash | through: -\n";
    assert!(parse_outline_annotated(text, None).is_ok());
    // A dash on the first line and a value later is a conflict.
    let text = "OR Goal\n  AND A\n    - Flash | through: -\n  AND B\n    - Flash | through: C00\n";
    assert_eq!(
        parse_outline_annotated(text, None).err(),
        Some(OutlineError::NoteConflict(5))
    );
}

#[test]
fn both_parsers_give_the_same_tree() {
    let text =
        "OR Goal\n  AND A\n    - S1 | through: C00\n    - S2 | type: t\n  - S1 | through: C00\n";
    let plain = parse_outline(text, None).expect("parses");
    let annotated = parse_outline_annotated(text, None).expect("parses");
    assert_eq!(to_outline(&plain), to_outline(&annotated.tree));
    assert_eq!(plain.len(), 4);
}

#[test]
fn notes_are_one_per_step_in_first_line_order() {
    let text = "OR Goal\n  - S1 | type: t1\n  AND G\n    - S2 | through: C00\n    - S1 | type: t1\n  - S3\n";
    let parsed = parse_outline_annotated(text, None).expect("parses");
    let tree = &parsed.tree;
    let summary: Vec<(String, StepNote)> = parsed
        .notes
        .iter()
        .map(|(id, note)| {
            let label = tree
                .idx_of(id)
                .and_then(|i| tree.get(i))
                .map(|n| n.label().to_owned())
                .unwrap_or_default();
            (label, note.clone())
        })
        .collect();
    let note = |line, through: Option<String>, attack_type: Option<String>| StepNote {
        line,
        through,
        attack_type,
    };
    assert_eq!(
        summary,
        vec![
            ("S1".to_owned(), note(2, None, some("t1"))),
            ("S2".to_owned(), note(4, some("C00"), None)),
            ("S3".to_owned(), note(6, None, None)),
        ]
    );

    // A root that is a step has a note too.
    let parsed = parse_outline_annotated("- Only | through: C01\n", None).expect("parses");
    assert_eq!(parsed.notes.len(), 1);
    assert_eq!(
        parsed.notes[0].0,
        parsed.tree.id_of(parsed.tree.root()).expect("root")
    );
    assert_eq!(parsed.notes[0].1, note(1, some("C01"), None));
}

#[test]
fn gates_cannot_have_values() {
    assert_eq!(
        parse_outline_annotated("OR Goal | type: x\n  - S\n", None).err(),
        Some(OutlineError::NoteOnGate(1))
    );
    assert_eq!(
        parse_outline_annotated("OR Goal\n  AND G | through: C00\n    - S\n", None).err(),
        Some(OutlineError::NoteOnGate(2))
    );
    let parsed =
        parse_outline_annotated("OR Goal\n  AND G | through: - | type: -\n    - S\n", None)
            .expect("parses");
    assert_eq!(parsed.notes.len(), 1);
}

#[test]
fn shared_steps_keep_their_first_values() {
    let parse = |second: &str| {
        let text = format!(
            "OR Goal\n  AND A\n    - S | through: C00 | type: t\n  AND B\n    - {second}\n"
        );
        parse_outline_annotated(&text, None).map(|p| p.notes)
    };
    assert_eq!(parse("S").map(|n| n.len()), Ok(1));
    assert_eq!(parse("S | type: t | through: C00").map(|n| n.len()), Ok(1));
    assert_eq!(
        parse("S | through: C01 | type: t").err(),
        Some(OutlineError::NoteConflict(5))
    );
    assert_eq!(
        parse("S | through: C00").err(),
        Some(OutlineError::NoteConflict(5))
    );
    let text = "OR Goal\n  AND A\n    - S\n  AND B\n    - S | type: t\n";
    assert_eq!(
        parse_outline_annotated(text, None).err(),
        Some(OutlineError::NoteConflict(5))
    );
}

#[test]
fn the_name_limit_ignores_the_tail() {
    let tail = " | through: C00 | type: debug-access";
    let name = "x".repeat(256);
    assert_eq!(
        read_step(&format!("{name}{tail}")),
        Ok((name.clone(), some("C00"), some("debug-access")))
    );
    assert_eq!(
        read_step(&format!("{name}x{tail}")),
        Err(OutlineError::LabelTooLong(2))
    );
}

const ODD_NAMES: [&str; 7] = [
    "Flash",
    "Read A | B",
    "A | note: x",
    "A | type: x",
    "A | through: C00",
    "A | through: - | type: -",
    "A | b: c | d: e | f: g",
];

fn tree_with(step: Node) -> AttackTree {
    let mut tree =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Goal")).expect("tree");
    let root = tree.root();
    tree.add_child(root, NodeId::new_v7(), step).expect("child");
    tree
}

#[test]
fn names_and_values_round_trip() {
    let values = [
        (None, None),
        (some("C00"), None),
        (None, some("debug-access")),
        (some("IF01"), some("debug-access")),
    ];
    for name in ODD_NAMES {
        let tree = tree_with(Node::step(name));
        for (through, attack_type) in values.clone() {
            let text = to_outline_with(&tree, |_| (through.clone(), attack_type.clone()));
            let parsed = parse_outline_annotated(&text, None).expect(&text);
            let idx = parsed.tree.children(parsed.tree.root())[0];
            assert_eq!(parsed.tree.get(idx).map(Node::label), Some(name), "{text}");
            assert_eq!(parsed.notes.len(), 1, "{text}");
            assert_eq!(parsed.notes[0].1.through, through, "{text}");
            assert_eq!(parsed.notes[0].1.attack_type, attack_type, "{text}");
        }
        let text = to_outline(&tree);
        let again = parse_outline(&text, None).expect(&text);
        let idx = again.children(again.root())[0];
        assert_eq!(again.get(idx).map(Node::label), Some(name), "{text}");
    }
}

#[test]
fn an_odd_gate_name_round_trips() {
    let mut tree =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Goal")).expect("tree");
    let root = tree.root();
    let gate = tree
        .add_child(
            root,
            NodeId::new_v7(),
            Node::gate(GateKind::And, "A | type: x"),
        )
        .expect("gate");
    tree.add_child(gate, NodeId::new_v7(), Node::step("S"))
        .expect("step");
    let text = to_outline(&tree);
    assert!(
        text.contains("AND A | type: x | through: - | type: -\n"),
        "{text}"
    );
    let again = parse_outline(&text, None).expect(&text);
    let idx = again.children(again.root())[0];
    assert_eq!(again.get(idx).map(Node::label), Some("A | type: x"));
}
