//! Through and attack type in the outline (job 122, slice 122db): the text
//! sets, changes and clears both values in the same undo step as the tree,
//! and a wrong ID or key is an error that applies nothing.

use std::sync::Arc;

use aegis_client::commands::{Edit, EditError};
use aegis_client::outline::{
    OutlineError, parse_outline, to_outline, to_outline_annotated, to_outline_with,
};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{ComponentId, InterfaceId, ItemRef, Methodology, NodeId, TreeId};

struct Fx {
    ws: Workspace,
    tree: TreeId,
    ecu: ComponentId,
    jtag: InterfaceId,
}

fn run(ws: &mut Workspace, edit: Edit) {
    let ops = edit.to_ops(ws.project(), "alice", 1).expect("ops");
    ws.apply(ops, Origin::Edit).expect("applies");
}

fn fixture() -> Fx {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(m);
    let (tree, ecu, jtag) = (
        TreeId::new_v7(),
        ComponentId::new_v7(),
        InterfaceId::new_v7(),
    );
    run(
        &mut ws,
        Edit::NewTree {
            id: tree,
            root: "Goal".into(),
            threat: None,
        },
    );
    run(
        &mut ws,
        replace(tree, "OR Goal\n  - Flash firmware\n  - Open JTAG\n"),
    );
    run(
        &mut ws,
        Edit::AddComponent {
            id: ecu,
            name: "ADAS ECU".into(),
            parent: None,
        },
    );
    run(
        &mut ws,
        Edit::AddInterface {
            id: jtag,
            name: "JTAG".into(),
            component: ecu,
            kind: 0,
        },
    );
    Fx {
        ws,
        tree,
        ecu,
        jtag,
    }
}

fn replace(tree: TreeId, text: &str) -> Edit {
    Edit::ReplaceTree {
        tree,
        outline: text.into(),
    }
}

fn node(fx: &Fx, label: &str) -> NodeId {
    let p = fx.ws.project();
    let t = p
        .attack_tree_idx(&fx.tree)
        .and_then(|i| p.attack_tree(i))
        .expect("tree");
    t.iter()
        .find(|(_, _, n)| n.label() == label)
        .map(|(_, i, _)| i)
        .expect("node")
}

/// (through, attack type key) of a step, by label.
fn values(fx: &Fx, label: &str) -> (Option<ItemRef>, Option<String>) {
    let p = fx.ws.project();
    let t_idx = p.attack_tree_idx(&fx.tree).expect("tree");
    let t = p.attack_tree(t_idx).expect("t");
    let idx = t.idx_of(&node(fx, label)).expect("idx");
    (
        p.step_via(t_idx, idx),
        p.step_attack_type(t_idx, idx)
            .and_then(|k| p.methodology().attack_type_key(k))
            .map(str::to_owned),
    )
}

/// A picture of what the tests compare before and after.
fn state(fx: &Fx) -> (Option<String>, Option<usize>) {
    let p = fx.ws.project();
    let t = p.attack_tree_idx(&fx.tree).expect("tree");
    (
        to_outline_annotated(p, t),
        p.attack_tree(t).map(|t| t.len()),
    )
}

fn annotated(fx: &Fx) -> String {
    let p = fx.ws.project();
    to_outline_annotated(p, p.attack_tree_idx(&fx.tree).expect("tree")).expect("outline")
}

fn first_type_key(fx: &Fx) -> String {
    let m = fx.ws.project().methodology();
    let kind = m.attack_types().next().expect("a type");
    m.attack_type_key(kind).expect("key").to_owned()
}

fn set_flash(fx: &mut Fx, via: Option<ItemRef>, typed: bool) {
    let tree = fx.tree;
    let flash = node(fx, "Flash firmware");
    run(
        &mut fx.ws,
        Edit::SetStepVia {
            tree,
            node: flash,
            via,
        },
    );
    if typed {
        let kind = Some(0);
        run(
            &mut fx.ws,
            Edit::SetStepAttackType {
                tree,
                node: flash,
                attack_type: kind,
            },
        );
    }
}

fn error_of(fx: &Fx, text: &str) -> EditError {
    replace(fx.tree, text)
        .to_ops(fx.ws.project(), "alice", 1)
        .expect_err("refused")
}

#[test]
fn the_annotated_outline_reads_back_to_no_ops() {
    let mut fx = fixture();
    let via = Some(ItemRef::Component(fx.ecu));
    set_flash(&mut fx, via, true);
    let key = first_type_key(&fx);
    let text = annotated(&fx);
    assert!(text.contains(&format!(
        "  - Flash firmware | through: C00 | type: {key}\n"
    )));
    assert!(text.contains("  - Open JTAG\n"));
    let ops = replace(fx.tree, &text).to_ops(fx.ws.project(), "alice", 1);
    assert_eq!(ops, Ok(vec![]));
}

#[test]
fn values_written_in_the_text_are_set_and_undone_together() {
    let mut fx = fixture();
    let before = state(&fx);
    run(
        &mut fx.ws,
        replace(
            fx.tree,
            "OR Goal\n  - Flash firmware | through: IF00\n  - Open JTAG | type: debug-access | through: c00\n",
        ),
    );
    assert_eq!(
        values(&fx, "Flash firmware").0,
        Some(ItemRef::Interface(fx.jtag))
    );
    let (via, key) = values(&fx, "Open JTAG");
    assert_eq!(via, Some(ItemRef::Component(fx.ecu)));
    assert_eq!(key.as_deref(), Some("debug-access"));
    fx.ws.undo().expect("undo");
    assert_eq!(state(&fx), before);
    assert_eq!(values(&fx, "Open JTAG"), (None, None));
}

#[test]
fn a_plain_text_clears_both_values_and_one_undo_restores_them() {
    let mut fx = fixture();
    let via = Some(ItemRef::Component(fx.ecu));
    set_flash(&mut fx, via, true);
    let before = state(&fx);
    run(
        &mut fx.ws,
        replace(fx.tree, "OR Goal\n  - Flash firmware\n  - Open JTAG\n"),
    );
    assert_eq!(values(&fx, "Flash firmware"), (None, None));
    fx.ws.undo().expect("undo");
    assert_eq!(state(&fx), before);
    assert!(values(&fx, "Flash firmware").0.is_some());
    assert!(values(&fx, "Flash firmware").1.is_some());
}

#[test]
fn a_dash_clears_one_value_and_keeps_the_other() {
    let mut fx = fixture();
    let via = Some(ItemRef::Component(fx.ecu));
    set_flash(&mut fx, via, true);
    let key = first_type_key(&fx);
    run(
        &mut fx.ws,
        replace(
            fx.tree,
            &format!("OR Goal\n  - Flash firmware | through: - | type: {key}\n  - Open JTAG\n"),
        ),
    );
    let (via, typed) = values(&fx, "Flash firmware");
    assert_eq!(via, None);
    assert_eq!(typed.as_deref(), Some(key.as_str()));
}

#[test]
fn a_new_step_gets_its_values_in_the_same_batch() {
    let mut fx = fixture();
    let before = state(&fx);
    run(
        &mut fx.ws,
        replace(
            fx.tree,
            "OR Goal\n  - Flash firmware\n  - Open JTAG\n  - Dump keys | through: IF00 | type: debug-access\n",
        ),
    );
    let (via, key) = values(&fx, "Dump keys");
    assert_eq!(via, Some(ItemRef::Interface(fx.jtag)));
    assert_eq!(key.as_deref(), Some("debug-access"));
    fx.ws.undo().expect("undo");
    assert_eq!(state(&fx), before);
}

#[test]
fn a_moved_step_takes_the_through_item_of_the_text() {
    let mut fx = fixture();
    let via = Some(ItemRef::Interface(fx.jtag));
    set_flash(&mut fx, via, false);
    run(
        &mut fx.ws,
        replace(
            fx.tree,
            "OR Goal\n  AND Inner\n    - Flash firmware | through: C00\n  - Open JTAG\n",
        ),
    );
    assert_eq!(
        values(&fx, "Flash firmware").0,
        Some(ItemRef::Component(fx.ecu))
    );
}

#[test]
fn an_unknown_item_is_an_error_naming_the_line_and_applies_nothing() {
    let fx = fixture();
    let before = state(&fx);
    for bad in ["C99", "T00", "banana"] {
        let text = format!("OR Goal\n  - Flash firmware | through: {bad}\n  - Open JTAG\n");
        assert_eq!(
            error_of(&fx, &text),
            EditError::Outline(OutlineError::UnknownItem(2)),
            "{bad}"
        );
    }
    assert_eq!(state(&fx), before);
    assert!(fx.ws.undo_label().is_some());
}

#[test]
fn an_unknown_attack_type_is_an_error_naming_the_line() {
    let fx = fixture();
    let text = "OR Goal\n  - Flash firmware\n  - Open JTAG | type: no-such-type\n";
    assert_eq!(
        error_of(&fx, text),
        EditError::Outline(OutlineError::UnknownAttackType(3))
    );
}

#[test]
fn a_removed_component_is_no_longer_an_item() {
    let mut fx = fixture();
    // The interface goes with the component; nothing goes through either.
    run(&mut fx.ws, Edit::RemoveComponent { component: fx.ecu });
    let text = "OR Goal\n  - Flash firmware | through: C00\n  - Open JTAG\n";
    assert_eq!(
        error_of(&fx, text),
        EditError::Outline(OutlineError::UnknownItem(2))
    );
}

#[test]
fn the_writer_drops_unsafe_values() {
    let tree = parse_outline("OR Goal\n  - Flash firmware\n  - Open JTAG\n", None).expect("tree");
    let plain = to_outline(&tree);
    let both =
        "OR Goal\n  - Flash firmware | type: debug-access\n  - Open JTAG | type: debug-access\n";
    for bad in [
        "",
        "-",
        "C00 | type: x",
        "C00\nOR Evil",
        "C00\r",
        " C00",
        "C 00",
        "a:b",
        "C00|",
    ] {
        assert_eq!(
            to_outline_with(&tree, |_| (Some(bad.into()), None)),
            plain,
            "through {bad:?}"
        );
        assert_eq!(
            to_outline_with(&tree, |_| (None, Some(bad.into()))),
            plain,
            "type {bad:?}"
        );
        assert_eq!(
            to_outline_with(&tree, |_| (Some(bad.into()), Some("debug-access".into()))),
            both,
            "both {bad:?}"
        );
    }
    for good in ["C00", "IF12", "debug-access", "a_b", "x"] {
        let text = to_outline_with(&tree, |_| (Some(good.into()), None));
        assert!(
            text.contains(&format!("  - Flash firmware | through: {good}\n")),
            "{good}"
        );
    }
}

#[test]
fn every_attack_type_of_the_methodology_is_written_and_read_back() {
    let mut fx = fixture();
    let (tree, flash) = (fx.tree, node(&fx, "Flash firmware"));
    let kinds: Vec<_> = fx.ws.project().methodology().attack_types().collect();
    for (pos, kind) in kinds.into_iter().enumerate() {
        run(
            &mut fx.ws,
            Edit::SetStepAttackType {
                tree,
                node: flash,
                attack_type: Some(pos),
            },
        );
        run(
            &mut fx.ws,
            Edit::SetStepVia {
                tree,
                node: flash,
                via: Some(ItemRef::Interface(fx.jtag)),
            },
        );
        let key = fx
            .ws
            .project()
            .methodology()
            .attack_type_key(kind)
            .expect("key")
            .to_owned();
        let text = annotated(&fx);
        assert!(
            text.contains(&format!(
                "  - Flash firmware | through: IF00 | type: {key}\n"
            )),
            "{key}"
        );
        let ops = replace(tree, &text).to_ops(fx.ws.project(), "alice", 1);
        assert_eq!(ops, Ok(vec![]), "{key}");
    }
}
