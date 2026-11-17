//! Client-level multi-link picker (UI-15, job 150), headless through typed
//! `Edit`s. A keyboard pick and unpick are each one undo step for a threat's
//! damage scenarios and a goal's controls; chip removal is the same unlink
//! edit; Escape keeps the committed filter; an IME preedit does not filter
//! until committed; and a 200-item option list stays bounded and navigable.

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::picker::MultiLinkPicker;
use aegis_client::relations::{self, RelationKey};
use aegis_client::view::Selection;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{AssetId, ControlId, DamageId, GoalId, Methodology, ThreatId};

fn run(ws: &mut Workspace, edit: Edit) {
    let ops = edit.to_ops(ws.project(), "alice", 1).expect("ops");
    ws.apply(ops, Origin::Edit).expect("applies");
}

struct Fx {
    ws: Workspace,
    threat: ThreatId,
    damages: Vec<DamageId>,
    goal: GoalId,
    controls: Vec<ControlId>,
}

/// One asset, one threat on it, three damage scenarios, one goal and three
/// controls, with no links yet. The asset carries all six properties so the
/// spoofing threat category (index 0) fits. Rebase onto a clean `Workspace`
/// at the end so every test's undo assertions start from an empty undo stack.
fn fixture() -> Fx {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(m);
    let (asset, threat, goal) = (AssetId::new_v7(), ThreatId::new_v7(), GoalId::new_v7());
    run(
        &mut ws,
        Edit::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: vec![0, 1, 2, 3, 4, 5],
        },
    );
    run(
        &mut ws,
        Edit::AddThreat {
            id: threat,
            name: "Threat".into(),
            asset,
            category: 0,
        },
    );
    let damages: Vec<DamageId> = (0..3).map(|_| DamageId::new_v7()).collect();
    for (i, damage) in damages.iter().enumerate() {
        run(
            &mut ws,
            Edit::AddDamage {
                id: *damage,
                name: format!("Damage {i}"),
            },
        );
    }
    run(
        &mut ws,
        Edit::AddGoal {
            id: goal,
            name: "Goal".into(),
        },
    );
    let controls: Vec<ControlId> = (0..3).map(|_| ControlId::new_v7()).collect();
    for (i, control) in controls.iter().enumerate() {
        run(
            &mut ws,
            Edit::AddControl {
                id: *control,
                name: format!("Control {i}"),
            },
        );
    }
    Fx {
        ws: Workspace::new(ws.project().clone(), 0),
        threat,
        damages,
        goal,
        controls,
    }
}

/// 200 damage scenarios, alternating Japanese and ASCII names so a CJK
/// needle (`データ`, `漏えい`) and an ASCII needle (`data`) each match a strict
/// subset. Rebased onto a clean `Workspace` like [`fixture`].
fn fixture_200() -> (Workspace, ThreatId, Vec<DamageId>) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(m);
    let (asset, threat) = (AssetId::new_v7(), ThreatId::new_v7());
    run(
        &mut ws,
        Edit::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: vec![0, 1, 2, 3, 4, 5],
        },
    );
    run(
        &mut ws,
        Edit::AddThreat {
            id: threat,
            name: "Threat".into(),
            asset,
            category: 0,
        },
    );
    let damages: Vec<DamageId> = (0..200).map(|_| DamageId::new_v7()).collect();
    for (i, damage) in damages.iter().enumerate() {
        let name = if i % 2 == 0 {
            format!("データ漏えい-{i:02}")
        } else {
            format!("Data breach-{i:02}")
        };
        run(&mut ws, Edit::AddDamage { id: *damage, name });
    }
    (Workspace::new(ws.project().clone(), 0), threat, damages)
}

#[test]
fn threat_damages_pick_and_unpick_are_one_undo_step_each() {
    let mut fx = fixture();
    let from = Selection::Threat(fx.threat);
    let damage = fx.damages[0];
    let before_pick = fx.ws.project().clone();

    // Pick: Space on an unlinked option.
    let mut picker = MultiLinkPicker::open(fx.ws.project(), RelationKey::THREAT_DAMAGES, from);
    assert!(picker.is_open());
    assert_eq!(picker.options().len(), 3);
    assert!(picker.chips().is_empty(), "fixture has no links yet");
    picker.move_cursor(0);
    assert_eq!(
        picker.active().map(|o| o.selection),
        Some(Selection::Damage(damage))
    );
    let edit = picker.toggle_active();
    assert_eq!(
        edit,
        Some(Edit::LinkDamage {
            threat: fx.threat,
            damage,
            linked: true,
        })
    );
    let ops = edit
        .expect("pick edit")
        .to_ops(fx.ws.project(), "alice", 1)
        .expect("ops");
    assert_eq!(ops.len(), 1, "a pick is exactly one op");
    fx.ws.apply(ops, Origin::Edit).expect("applies");
    assert!(
        relations::related(fx.ws.project(), RelationKey::THREAT_DAMAGES, from)
            .contains(&Selection::Damage(damage))
    );
    let linked = MultiLinkPicker::open(fx.ws.project(), RelationKey::THREAT_DAMAGES, from);
    assert_eq!(linked.chips().len(), 1);
    assert_eq!(linked.chips()[0].selection, Selection::Damage(damage));

    // Undo pick: one step restores the pre-pick project.
    fx.ws.undo().expect("undo pick");
    assert!(
        fx.ws.project() == &before_pick,
        "undo pick restores the project"
    );

    // Set the link up again so unpick has something to remove.
    run(
        &mut fx.ws,
        Edit::LinkDamage {
            threat: fx.threat,
            damage,
            linked: true,
        },
    );
    let before_unpick = fx.ws.project().clone();

    // Unpick: Space on the now-linked option.
    let mut picker = MultiLinkPicker::open(fx.ws.project(), RelationKey::THREAT_DAMAGES, from);
    assert_eq!(picker.chips().len(), 1);
    picker.move_cursor(0);
    let edit = picker.toggle_active();
    assert_eq!(
        edit,
        Some(Edit::LinkDamage {
            threat: fx.threat,
            damage,
            linked: false,
        })
    );
    let ops = edit
        .expect("unpick edit")
        .to_ops(fx.ws.project(), "alice", 1)
        .expect("ops");
    assert_eq!(ops.len(), 1, "an unpick is exactly one op");
    fx.ws.apply(ops, Origin::Edit).expect("applies");
    assert!(
        !relations::related(fx.ws.project(), RelationKey::THREAT_DAMAGES, from)
            .contains(&Selection::Damage(damage))
    );
    let cleared = MultiLinkPicker::open(fx.ws.project(), RelationKey::THREAT_DAMAGES, from);
    assert!(cleared.chips().is_empty());

    // Undo unpick, then undo the setup edit: empty stack, so each toggle was
    // exactly one step.
    fx.ws.undo().expect("undo unpick");
    assert!(
        fx.ws.project() == &before_unpick,
        "undo unpick restores the link"
    );
    fx.ws.undo().expect("undo setup");
    assert!(!fx.ws.can_undo(), "each toggle was exactly one undo step");
}

#[test]
fn goal_controls_pick_and_unpick_are_one_undo_step_each() {
    let mut fx = fixture();
    let from = Selection::Goal(fx.goal);
    let control = fx.controls[0];
    let before_pick = fx.ws.project().clone();

    // Pick.
    let mut picker = MultiLinkPicker::open(fx.ws.project(), RelationKey::GOAL_CONTROLS, from);
    assert!(picker.is_open());
    assert_eq!(picker.options().len(), 3);
    assert!(picker.chips().is_empty());
    picker.move_cursor(0);
    assert_eq!(
        picker.active().map(|o| o.selection),
        Some(Selection::Control(control))
    );
    let edit = picker.toggle_active();
    assert_eq!(
        edit,
        Some(Edit::LinkGoalControl {
            goal: fx.goal,
            control,
            linked: true,
        })
    );
    let ops = edit
        .expect("pick edit")
        .to_ops(fx.ws.project(), "alice", 1)
        .expect("ops");
    assert_eq!(ops.len(), 1);
    fx.ws.apply(ops, Origin::Edit).expect("applies");
    assert!(
        relations::related(fx.ws.project(), RelationKey::GOAL_CONTROLS, from)
            .contains(&Selection::Control(control))
    );
    let linked = MultiLinkPicker::open(fx.ws.project(), RelationKey::GOAL_CONTROLS, from);
    assert_eq!(linked.chips().len(), 1);
    assert_eq!(linked.chips()[0].selection, Selection::Control(control));

    fx.ws.undo().expect("undo pick");
    assert!(fx.ws.project() == &before_pick);

    // Unpick.
    run(
        &mut fx.ws,
        Edit::LinkGoalControl {
            goal: fx.goal,
            control,
            linked: true,
        },
    );
    let before_unpick = fx.ws.project().clone();
    let mut picker = MultiLinkPicker::open(fx.ws.project(), RelationKey::GOAL_CONTROLS, from);
    assert_eq!(picker.chips().len(), 1);
    picker.move_cursor(0);
    let edit = picker.toggle_active();
    assert_eq!(
        edit,
        Some(Edit::LinkGoalControl {
            goal: fx.goal,
            control,
            linked: false,
        })
    );
    let ops = edit
        .expect("unpick edit")
        .to_ops(fx.ws.project(), "alice", 1)
        .expect("ops");
    assert_eq!(ops.len(), 1);
    fx.ws.apply(ops, Origin::Edit).expect("applies");
    assert!(
        !relations::related(fx.ws.project(), RelationKey::GOAL_CONTROLS, from)
            .contains(&Selection::Control(control))
    );
    let cleared = MultiLinkPicker::open(fx.ws.project(), RelationKey::GOAL_CONTROLS, from);
    assert!(cleared.chips().is_empty());

    fx.ws.undo().expect("undo unpick");
    assert!(fx.ws.project() == &before_unpick);
    fx.ws.undo().expect("undo setup");
    assert!(!fx.ws.can_undo());
}

#[test]
fn chip_removal_unlinks_with_one_step() {
    let mut fx = fixture();
    let from = Selection::Threat(fx.threat);
    let damage = fx.damages[0];
    run(
        &mut fx.ws,
        Edit::LinkDamage {
            threat: fx.threat,
            damage,
            linked: true,
        },
    );
    let before = fx.ws.project().clone();

    let picker = MultiLinkPicker::open(fx.ws.project(), RelationKey::THREAT_DAMAGES, from);
    assert_eq!(picker.chips().len(), 1);
    let edit = picker.remove_chip(0);
    assert_eq!(
        edit,
        Some(Edit::LinkDamage {
            threat: fx.threat,
            damage,
            linked: false,
        })
    );
    assert_eq!(picker.remove_chip(1), None, "out-of-range chip has no edit");
    let ops = edit
        .expect("unlink edit")
        .to_ops(fx.ws.project(), "alice", 1)
        .expect("ops");
    assert_eq!(ops.len(), 1);
    fx.ws.apply(ops, Origin::Edit).expect("applies");
    assert!(
        !relations::related(fx.ws.project(), RelationKey::THREAT_DAMAGES, from)
            .contains(&Selection::Damage(damage))
    );
    fx.ws.undo().expect("undo unlink");
    assert!(fx.ws.project() == &before);
}

#[test]
fn escape_close_keeps_committed_filter() {
    let fx = fixture();
    let from = Selection::Threat(fx.threat);
    let mut picker = MultiLinkPicker::open(fx.ws.project(), RelationKey::THREAT_DAMAGES, from);
    picker.set_composition("temp");
    picker.set_filter("some");
    picker.close();
    assert!(!picker.is_open());
    assert!(picker.filter().composition().is_empty());
    assert_eq!(picker.filter().committed(), "some");
}

#[test]
fn two_hundred_items_filter_and_window() {
    let (ws, threat, damages) = fixture_200();
    let from = Selection::Threat(threat);
    let mut picker = MultiLinkPicker::open(ws.project(), RelationKey::THREAT_DAMAGES, from);
    assert_eq!(picker.options().len(), 200);
    assert_eq!(picker.visible().len(), 200);

    let win = picker.window(12);
    assert!(win.len() <= 12);
    assert!(win.contains(&picker.cursor()));
    for pair in win.windows(2) {
        assert_eq!(pair[1], pair[0] + 1, "window indices are contiguous");
    }

    picker.move_cursor(isize::MAX);
    assert_eq!(picker.cursor(), 199, "cursor clamps to last visible index");
    assert_eq!(
        picker.active().map(|o| o.selection),
        Some(Selection::Damage(damages[199]))
    );

    let edit = picker.toggle_visible(199);
    assert!(matches!(edit, Some(Edit::LinkDamage { linked: true, .. })));
    let ops = edit
        .expect("toggle edit")
        .to_ops(ws.project(), "alice", 1)
        .expect("ops");
    assert_eq!(ops.len(), 1, "toggling the 200th item is one op");

    picker.set_filter("データ");
    assert!(!picker.visible().is_empty());
    assert!(picker.visible().len() < 200);
    for &idx in picker.visible() {
        assert!(picker.options()[idx].label.contains("データ"));
    }
    picker.set_filter("");
    assert_eq!(picker.visible().len(), 200);
}

#[test]
fn ime_preedit_does_not_filter_until_committed() {
    let (ws, threat, _damages) = fixture_200();
    let from = Selection::Threat(threat);
    let mut picker = MultiLinkPicker::open(ws.project(), RelationKey::THREAT_DAMAGES, from);

    // Preedit alone does not filter.
    picker.set_composition("デ");
    assert_eq!(picker.visible().len(), 200);
    assert_eq!(picker.filter().composition(), "デ");

    // Committing the preedit narrows to labels containing デ.
    picker.commit_filter();
    assert!(!picker.visible().is_empty());
    assert!(picker.visible().len() < 200);
    for &idx in picker.visible() {
        assert!(picker.options()[idx].label.contains("デ"));
    }
    assert!(picker.filter().composition().is_empty());

    // A committed filter plus a fresh preedit keeps the previous visible list.
    picker.set_filter("漏");
    let before = picker.visible().to_vec();
    picker.set_composition("えい");
    assert_eq!(
        picker.visible(),
        before.as_slice(),
        "preedit must not refilter"
    );
    picker.commit_filter();
    for &idx in picker.visible() {
        assert!(picker.options()[idx].label.contains("漏えい"));
    }
}

#[test]
fn ascii_filter_is_case_insensitive() {
    let (ws, threat, _damages) = fixture_200();
    let from = Selection::Threat(threat);
    let mut picker = MultiLinkPicker::open(ws.project(), RelationKey::THREAT_DAMAGES, from);
    picker.set_filter("data");
    assert!(!picker.visible().is_empty());
    assert!(picker.visible().len() < 200);
    for &idx in picker.visible() {
        assert!(picker.options()[idx].label.to_lowercase().contains("data"));
    }
}
