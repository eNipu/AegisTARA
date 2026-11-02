//! Step "through" and attack type (job 122, CFG-01, CFG-02): the edits are
//! one undo step each, removing what a step goes through clears it in the
//! same step, and the "steps through" list counts a component's interfaces.

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::view::steps_through;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{ComponentId, InterfaceId, ItemRef, Methodology, NodeId, TreeId};

struct Fx {
    ws: Workspace,
    tree: TreeId,
    flash: NodeId,
    jtag_step: NodeId,
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
        Edit::ReplaceTree {
            tree,
            outline: "OR Goal\n  - Flash firmware\n  - Open JTAG\n".into(),
        },
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
    let t = ws
        .project()
        .attack_tree_idx(&tree)
        .and_then(|i| ws.project().attack_tree(i))
        .expect("t");
    let id = |l: &str| {
        t.iter()
            .find(|(_, _, n)| n.label() == l)
            .map(|(_, i, _)| i)
            .expect("node")
    };
    let (flash, jtag_step) = (id("Flash firmware"), id("Open JTAG"));
    Fx {
        ws,
        tree,
        flash,
        jtag_step,
        ecu,
        jtag,
    }
}

#[test]
fn via_and_attack_type_are_one_undo_step_each() {
    let mut fx = fixture();
    let before = fx.ws.project().clone();
    run(
        &mut fx.ws,
        Edit::SetStepVia {
            tree: fx.tree,
            node: fx.flash,
            via: Some(ItemRef::Component(fx.ecu)),
        },
    );
    run(
        &mut fx.ws,
        Edit::SetStepAttackType {
            tree: fx.tree,
            node: fx.flash,
            attack_type: Some(0),
        },
    );
    fx.ws.undo().expect("undo type");
    fx.ws.undo().expect("undo via");
    assert!(fx.ws.project() == &before);
    let bad = Edit::SetStepAttackType {
        tree: fx.tree,
        node: fx.flash,
        attack_type: Some(999),
    };
    assert!(bad.to_ops(fx.ws.project(), "alice", 1).is_err());
}

#[test]
fn removing_a_component_clears_every_via_and_undoes_as_one_step() {
    let mut fx = fixture();
    run(
        &mut fx.ws,
        Edit::SetStepVia {
            tree: fx.tree,
            node: fx.flash,
            via: Some(ItemRef::Component(fx.ecu)),
        },
    );
    run(
        &mut fx.ws,
        Edit::SetStepVia {
            tree: fx.tree,
            node: fx.jtag_step,
            via: Some(ItemRef::Interface(fx.jtag)),
        },
    );
    let before = fx.ws.project().clone();
    let ops = Edit::RemoveComponent { component: fx.ecu }
        .to_ops(fx.ws.project(), "alice", 2)
        .expect("ops");
    fx.ws
        .apply_labeled(ops, Origin::Edit, None)
        .expect("applies");
    assert!(fx.ws.project().component_idx(&fx.ecu).is_none());
    let t = fx.ws.project().attack_tree_idx(&fx.tree).expect("tree");
    assert!(
        fx.ws
            .project()
            .attack_tree(t)
            .expect("t")
            .iter()
            .all(|(n, _, _)| fx.ws.project().step_via(t, n).is_none())
    );
    let label = fx.ws.undo_label().expect("label").past();
    assert!(label.contains("2 steps lost their 'through'"), "{label}");
    fx.ws.undo().expect("one undo");
    assert!(fx.ws.project() == &before);
}

#[test]
fn a_components_list_includes_steps_through_its_interfaces() {
    let mut fx = fixture();
    run(
        &mut fx.ws,
        Edit::SetStepVia {
            tree: fx.tree,
            node: fx.flash,
            via: Some(ItemRef::Component(fx.ecu)),
        },
    );
    run(
        &mut fx.ws,
        Edit::SetStepVia {
            tree: fx.tree,
            node: fx.jtag_step,
            via: Some(ItemRef::Interface(fx.jtag)),
        },
    );
    let all = steps_through(fx.ws.project(), ItemRef::Component(fx.ecu));
    assert_eq!(all.len(), 2);
    assert_eq!(all[1].through, "ADAS ECU / JTAG");
    let only = steps_through(fx.ws.project(), ItemRef::Interface(fx.jtag));
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].step, "Open JTAG");
}

#[test]
fn an_outline_edit_that_moves_a_step_keeps_its_through_and_attack_type() {
    let mut fx = fixture();
    run(
        &mut fx.ws,
        Edit::SetStepVia {
            tree: fx.tree,
            node: fx.flash,
            via: Some(ItemRef::Component(fx.ecu)),
        },
    );
    run(
        &mut fx.ws,
        Edit::SetStepAttackType {
            tree: fx.tree,
            node: fx.flash,
            attack_type: Some(1),
        },
    );
    // Same steps, other order, one renamed neighbour.
    run(
        &mut fx.ws,
        Edit::ReplaceTree {
            tree: fx.tree,
            outline: "OR Goal\n  - Open JTAG port\n  - Flash firmware\n".into(),
        },
    );
    let p = fx.ws.project();
    let t_idx = p.attack_tree_idx(&fx.tree).expect("tree");
    let tree = p.attack_tree(t_idx).expect("t");
    let n = tree
        .iter()
        .find(|(_, _, n)| n.label() == "Flash firmware")
        .map(|(i, _, _)| i)
        .expect("step");
    assert_eq!(p.step_via(t_idx, n), Some(ItemRef::Component(fx.ecu)));
    assert!(p.step_attack_type(t_idx, n).is_some());
}
