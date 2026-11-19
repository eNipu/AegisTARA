//! Headless GPUI tests for the reusable keyboard-operable
//! [`MultiLinkPickerView`] (UI-15).
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::cell::Cell;
use std::rc::Rc;

use aegis_client::command_table::command;
use aegis_client::commands::Edit;
use aegis_client::explorer::{ExplorerKey, Folder, Owner};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::picker::MultiLinkPicker;
use aegis_client::relations::{ElementKind, RELATIONS, RelationKey, related};
use aegis_client::settings::Appearance;
use aegis_client::view::Selection;
use aegis_model::{
    ConditionId, DamageId, ElementRef, GoalId, Op, PartyId, PartyKind, Project, Provenance,
};
use aegis_workbench::app::{
    InspectorPanel, MultiLinkPickerView, Workbench, apply_appearance, bind_all,
};
use aegis_workbench::sample::workbench_sample;
use gpui::{
    AppContext as _, Context, Entity, Focusable as _, IntoElement, ParentElement as _, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div,
};

struct PickerHost {
    workbench: Entity<Workbench>,
    picker: Option<Entity<MultiLinkPickerView>>,
}

impl Render for PickerHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.workbench.clone())
            .children(self.picker.clone())
    }
}

fn undo_keys() -> String {
    command("undo")
        .and_then(|c| c.keys_for(cfg!(target_os = "macos")))
        .expect("undo command has keys")
}

fn saved_bytes(project: &Project) -> Vec<u8> {
    let mut out = Vec::new();
    project.save(&mut out).expect("project saves");
    out
}

fn project_bytes(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<u8> {
    cx.update(|_, cx| saved_bytes(wb.read(cx).workspace().project()))
}

fn open_host(
    cx: &mut TestAppContext,
) -> (
    Rc<Catalog>,
    Entity<PickerHost>,
    Entity<Workbench>,
    &mut VisualTestContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        bind_all(cx);
    });
    let catalog = Rc::new(Catalog::load(Language::En));
    let cat = Rc::clone(&catalog);
    let (host, vcx) = cx.add_window_view(|window, cx| {
        apply_appearance(Appearance::Light, window, cx);
        let workbench = cx.new(|cx| Workbench::new(cat, Appearance::Light, None, window, cx));
        PickerHost {
            workbench,
            picker: None,
        }
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let workbench = vcx.read_entity(&host, |h, _| h.workbench.clone());
    (catalog, host, workbench, vcx)
}

fn mount_picker(
    host: &Entity<PickerHost>,
    wb: &Entity<Workbench>,
    catalog: &Rc<Catalog>,
    key: RelationKey,
    from: Selection,
    vcx: &mut VisualTestContext,
) -> Entity<MultiLinkPickerView> {
    let picker_state = vcx.read_entity(wb, |w, _| {
        MultiLinkPicker::open(w.workspace().project(), key, from)
    });
    let weak = wb.downgrade();
    let cat = Rc::clone(catalog);
    let picker = host.update_in(vcx, |h, window, cx| {
        let view =
            cx.new(|cx| MultiLinkPickerView::from_picker(picker_state, cat, weak, window, cx));
        h.picker = Some(view.clone());
        cx.notify();
        view
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.run_until_parked();
    picker
}

#[gpui::test]
fn threat_damages_keyboard_toggle_live_refresh_and_undo(cx: &mut TestAppContext) {
    let (catalog, host, wb, vcx) = open_host(cx);

    let extra_a = DamageId::new_v7();
    let extra_b = DamageId::new_v7();
    wb.update(vcx, |w, cx| {
        assert!(w.apply_edit(
            &Edit::AddDamage {
                id: extra_a,
                name: "Brake pressure loss".into(),
            },
            cx,
        ));
        assert!(w.apply_edit(
            &Edit::AddDamage {
                id: extra_b,
                name: "Battery drain".into(),
            },
            cx,
        ));
    });
    let threat = vcx.read_entity(&wb, |w, _| {
        w.workspace()
            .project()
            .threat_scenarios()
            .next()
            .map(|(_, id, _)| id)
            .expect("sample has threat")
    });
    let before_bytes = project_bytes(&wb, vcx);

    let picker = mount_picker(
        &host,
        &wb,
        &catalog,
        RelationKey::THREAT_DAMAGES,
        Selection::Threat(threat),
        vcx,
    );
    let closed = Rc::new(Cell::new(0usize));
    let closed_cb = Rc::clone(&closed);
    picker.update(vcx, |p, _| {
        p.set_on_close(move |_, _| closed_cb.set(closed_cb.get() + 1));
    });

    // The filter input starts focused; spaces type into the filter instead of toggling.
    let filter_focused = vcx.update(|window, cx| {
        let input = picker.read(cx).filter_input().clone();
        input.read(cx).focus_handle(cx).is_focused(window)
    });
    assert!(filter_focused, "filter input should start focused");

    let initial_chips = vcx.read_entity(&picker, |p, _| p.picker().chips().len());
    vcx.simulate_keystrokes("b r a k e space p");
    vcx.run_until_parked();
    vcx.read_entity(&picker, |p, _| {
        assert_eq!(p.picker().filter().committed(), "brake p");
        assert_eq!(p.picker().visible().len(), 1);
        assert_eq!(p.picker().chips().len(), initial_chips);
    });

    // Filter to the two "b" damages, navigate down/up via capture_key_down, toggle live.
    picker.update_in(vcx, |p, window, cx| p.set_filter("b", window, cx));
    vcx.run_until_parked();
    vcx.read_entity(&picker, |p, _| {
        assert_eq!(p.picker().visible().len(), 2);
        assert_eq!(p.picker().cursor(), 0);
    });

    vcx.simulate_keystrokes("down");
    vcx.run_until_parked();
    assert_eq!(vcx.read_entity(&picker, |p, _| p.picker().cursor()), 1);

    vcx.simulate_keystrokes("up");
    vcx.run_until_parked();
    assert_eq!(vcx.read_entity(&picker, |p, _| p.picker().cursor()), 0);

    let target_sel = vcx.read_entity(&picker, |p, _| {
        let active = p.picker().active().expect("active option");
        assert!(!active.linked);
        active.selection
    });

    // Enter links the active option; picker stays open with live chips & linked flag (no reopen).
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    vcx.read_entity(&picker, |p, _| {
        assert!(p.picker().is_open(), "picker must stay open after toggle");
        assert_eq!(p.picker().filter().committed(), "b");
        assert!(
            p.picker().active().expect("active").linked,
            "active option must refresh live without reopening"
        );
        assert!(p.picker().chips().iter().any(|c| c.selection == target_sel));
        assert_eq!(p.picker().chips().len(), initial_chips + 1);
    });
    assert_ne!(project_bytes(&wb, vcx), before_bytes);

    // Enter again unlinks the active option, still open with updated live chips.
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    vcx.read_entity(&picker, |p, _| {
        assert!(p.picker().is_open());
        assert!(!p.picker().active().expect("active").linked);
        assert!(!p.picker().chips().iter().any(|c| c.selection == target_sel));
        assert_eq!(p.picker().chips().len(), initial_chips);
    });

    // Link once more, close via Escape, verify on_close fires, then undo via cmd-z.
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert_ne!(project_bytes(&wb, vcx), before_bytes);

    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    assert!(!vcx.read_entity(&picker, |p, _| p.picker().is_open()));
    assert_eq!(closed.get(), 1);

    // Focus workbench and press cmd-z (undo command) across the three toggles
    // to restore exact saved project bytes (including provenance).
    wb.update_in(vcx, |w, window, cx| w.run("focus-explorer", window, cx));
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.simulate_keystrokes(&undo_keys());
        vcx.run_until_parked();
    }
    assert_eq!(project_bytes(&wb, vcx), before_bytes);
}

#[gpui::test]
fn goal_responsible_picker_links_party_and_undoes(cx: &mut TestAppContext) {
    let (catalog, host, wb, vcx) = open_host(cx);

    let mut project = workbench_sample().project().clone();
    let prov = Provenance::human("tester", 1);
    let goal = GoalId::new_v7();
    let party_a = PartyId::new_v7();
    let party_b = PartyId::new_v7();
    for op in [
        Op::AddCybersecurityGoal {
            id: goal,
            name: "Keep commands authentic".into(),
            controls: Vec::new(),
            provenance: prov.clone(),
        },
        Op::AddParty {
            id: party_a,
            name: "Bosch".into(),
            kind: PartyKind::Supplier,
            provenance: prov.clone(),
        },
        Op::AddParty {
            id: party_b,
            name: "OEM".into(),
            kind: PartyKind::Customer,
            provenance: prov,
        },
    ] {
        project.apply(op).expect("seed op applies");
    }
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(project, "Goal parties", None, window, cx);
    });
    let before_bytes = project_bytes(&wb, vcx);

    // Exercise MultiLinkPickerView::new directly too.
    let weak = wb.downgrade();
    let cat = Rc::clone(&catalog);
    let picker = host.update_in(vcx, |h, window, cx| {
        let view = cx.new(|cx| {
            let proj = wb.read(cx).workspace().project().clone();
            MultiLinkPickerView::new(
                &proj,
                RelationKey::GOAL_RESPONSIBLE,
                Selection::Goal(goal),
                cat,
                weak,
                window,
                cx,
            )
        });
        h.picker = Some(view.clone());
        cx.notify();
        view
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.run_until_parked();

    let closed = Rc::new(Cell::new(0usize));
    let closed_cb = Rc::clone(&closed);
    picker.update(vcx, |p, _| {
        p.set_on_close(move |_, _| closed_cb.set(closed_cb.get() + 1));
    });

    picker.update_in(vcx, |p, window, cx| p.set_filter("oem", window, cx));
    vcx.run_until_parked();
    vcx.read_entity(&picker, |p, _| {
        assert_eq!(p.picker().visible().len(), 1);
        assert_eq!(
            p.picker().active().expect("oem").selection,
            Selection::Party(party_b)
        );
    });

    picker.update_in(vcx, |p, window, cx| p.set_filter("", window, cx));
    vcx.run_until_parked();
    vcx.simulate_keystrokes("down");
    vcx.run_until_parked();
    assert_eq!(vcx.read_entity(&picker, |p, _| p.picker().cursor()), 1);

    // Link party_b via enter; verify live refresh without reopening.
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    vcx.read_entity(&picker, |p, _| {
        assert!(p.picker().is_open());
        let chips = p.picker().chips();
        assert_eq!(chips.len(), 1);
        assert_eq!(chips[0].selection, Selection::Party(party_b));
        assert!(p.picker().active().expect("active").linked);
    });
    assert_ne!(project_bytes(&wb, vcx), before_bytes);

    // Move up, link party_a, then remove first chip directly.
    vcx.simulate_keystrokes("up");
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    assert_eq!(vcx.read_entity(&picker, |p, _| p.picker().chips().len()), 2);

    assert!(picker.update(vcx, |p, cx| p.remove_chip(0, cx)));
    vcx.read_entity(&picker, |p, _| {
        assert!(p.picker().is_open());
        assert_eq!(p.picker().chips().len(), 1);
    });

    // Escape closes and invokes on_close.
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    assert!(!vcx.read_entity(&picker, |p, _| p.picker().is_open()));
    assert_eq!(closed.get(), 1);

    // Undo the three edits via cmd-z and verify exact saved bytes match initial state.
    wb.update_in(vcx, |w, window, cx| w.run("focus-explorer", window, cx));
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.simulate_keystrokes(&undo_keys());
        vcx.run_until_parked();
    }
    assert_eq!(project_bytes(&wb, vcx), before_bytes);
}

#[gpui::test]
fn read_only_workbench_refuses_picker_edits(cx: &mut TestAppContext) {
    let (catalog, host, wb, vcx) = open_host(cx);

    // Add an unlinked damage so toggling would succeed if not read-only.
    let unlinked = DamageId::new_v7();
    wb.update(vcx, |w, cx| {
        assert!(w.apply_edit(
            &Edit::AddDamage {
                id: unlinked,
                name: "Sensor spoofing damage".into(),
            },
            cx,
        ));
    });
    let threat = vcx.read_entity(&wb, |w, _| {
        w.workspace()
            .project()
            .threat_scenarios()
            .next()
            .map(|(_, id, _)| id)
            .expect("sample has threat")
    });
    let before_bytes = project_bytes(&wb, vcx);

    wb.update(vcx, |w, cx| {
        w.set_read_only(Some("Read-only license expired".into()), cx);
    });

    let picker = mount_picker(
        &host,
        &wb,
        &catalog,
        RelationKey::THREAT_DAMAGES,
        Selection::Threat(threat),
        vcx,
    );
    let before_chips: Vec<Selection> = vcx.read_entity(&picker, |p, _| {
        p.picker().chips().iter().map(|c| c.selection).collect()
    });
    assert!(!before_chips.is_empty());

    // Filter to the unlinked damage and press Enter -> must be refused.
    picker.update_in(vcx, |p, window, cx| p.set_filter("Sensor", window, cx));
    vcx.run_until_parked();
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    vcx.read_entity(&picker, |p, _| {
        assert!(p.picker().is_open());
        assert!(!p.picker().active().expect("active").linked);
        let chips: Vec<Selection> = p.picker().chips().iter().map(|c| c.selection).collect();
        assert_eq!(chips, before_chips);
    });
    assert_eq!(
        vcx.read_entity(&wb, |w, _| w.note().map(str::to_owned)),
        Some("Read-only license expired".into())
    );
    assert_eq!(project_bytes(&wb, vcx), before_bytes);

    // Removing an existing chip is also refused.
    assert!(!picker.update(vcx, |p, cx| p.remove_chip(0, cx)));
    assert_eq!(project_bytes(&wb, vcx), before_bytes);
}

#[gpui::test]
fn chip_navigation_opens_target_tab_without_double_lease(cx: &mut TestAppContext) {
    let (catalog, host, wb, vcx) = open_host(cx);
    let threat = vcx.read_entity(&wb, |w, _| {
        w.workspace()
            .project()
            .threat_scenarios()
            .next()
            .map(|(_, id, _)| id)
            .expect("sample has threat")
    });
    let picker = mount_picker(
        &host,
        &wb,
        &catalog,
        RelationKey::THREAT_DAMAGES,
        Selection::Threat(threat),
        vcx,
    );
    let chip_sel = vcx.read_entity(&picker, |p, _| {
        p.picker()
            .chips()
            .first()
            .expect("linked damage chip")
            .selection
    });
    picker.update_in(vcx, |p, window, cx| p.open_chip(chip_sel, window, cx));
    vcx.run_until_parked();

    let expected_key = ExplorerKey::Element(Owner::Project, Folder::Damage, chip_sel);
    assert!(vcx.update(|_, cx| wb.read(cx).has_tab(expected_key, cx)));
}

#[gpui::test]
fn external_refresh_preserves_filter_and_space_toggles_when_unfocused(cx: &mut TestAppContext) {
    let (catalog, host, wb, vcx) = open_host(cx);
    let threat = vcx.read_entity(&wb, |w, _| {
        w.workspace()
            .project()
            .threat_scenarios()
            .next()
            .map(|(_, id, _)| id)
            .expect("sample has threat")
    });
    let picker = mount_picker(
        &host,
        &wb,
        &catalog,
        RelationKey::THREAT_DAMAGES,
        Selection::Threat(threat),
        vcx,
    );
    picker.update_in(vcx, |p, window, cx| {
        p.set_filter("New", window, cx);
        assert!(!p.handle_key("a", window, cx));
    });
    vcx.run_until_parked();
    assert_eq!(
        vcx.read_entity(&picker, |p, _| p.picker().visible().len()),
        0
    );

    // Add a matching damage externally and call refresh(); picker stays open with filter intact.
    let added = DamageId::new_v7();
    wb.update(vcx, |w, cx| {
        assert!(w.apply_edit(
            &Edit::AddDamage {
                id: added,
                name: "New thermal runaway".into(),
            },
            cx,
        ));
    });
    let proj = vcx.read_entity(&wb, |w, _| w.workspace().project().clone());
    picker.update(vcx, |p, cx| p.refresh(&proj, cx));
    vcx.read_entity(&picker, |p, _| {
        assert!(p.picker().is_open());
        assert_eq!(p.picker().filter().committed(), "New");
        assert_eq!(p.picker().visible().len(), 1);
        assert!(!p.picker().active().expect("active").linked);
    });

    // Focus the picker container (not the filter input) and press Space -> toggles the active option.
    picker.update_in(vcx, |p, window, cx| {
        window.focus(&p.focus_handle(cx), cx);
    });
    vcx.run_until_parked();
    vcx.simulate_keystrokes("space");
    vcx.run_until_parked();
    vcx.read_entity(&picker, |p, _| {
        assert!(p.picker().is_open());
        assert!(p.picker().active().expect("active").linked);
    });
}

fn inspector_of(wb: &Entity<Workbench>, vcx: &mut VisualTestContext) -> Entity<InspectorPanel> {
    vcx.read_entity(wb, |w, _| w.inspector().clone())
}

#[gpui::test]
fn inspector_relation_rows_match_registry_for_threat_and_goal(cx: &mut TestAppContext) {
    let (_catalog, _host, wb, vcx) = open_host(cx);
    let inspector = inspector_of(&wb, vcx);

    // Seed a Goal and an off Condition excluding the Threat so we also cover excluded rendering.
    let mut project = workbench_sample().project().clone();
    let prov = Provenance::human("tester", 1);
    let threat = project
        .threat_scenarios()
        .next()
        .map(|(_, id, _)| id)
        .expect("sample has threat");
    let goal = GoalId::new_v7();
    let cond = ConditionId::new_v7();
    for op in [
        Op::AddCybersecurityGoal {
            id: goal,
            name: "Authenticate brake commands".into(),
            controls: Vec::new(),
            provenance: prov.clone(),
        },
        Op::AddCondition {
            id: cond,
            name: "Hardware security module fitted".into(),
            description: "".into(),
            present: false,
            provenance: prov.clone(),
        },
        Op::NeedCondition {
            element: ElementRef::Threat(threat),
            condition: cond,
            provenance: prov,
        },
    ] {
        project.apply(op).expect("seed op applies");
    }
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(project, "Inspector test", None, window, cx);
    });

    // Selecting a Threat via ExplorerKey (Workbench::select) populates InspectorPanel.
    let threat_key = vcx.read_entity(&wb, |w, _| {
        w.index()
            .rows(&aegis_client::explorer::ExplorerState::default())
            .into_iter()
            .find_map(|r| match r.key {
                k @ ExplorerKey::Element(_, _, Selection::Threat(id)) if id == threat => Some(k),
                _ => None,
            })
            .unwrap_or(ExplorerKey::Element(
                Owner::Project,
                Folder::Threats,
                Selection::Threat(threat),
            ))
    });
    wb.update(vcx, |w, cx| w.select(threat_key, cx));
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    let expected_threat_rels: Vec<(RelationKey, bool)> = RELATIONS
        .iter()
        .filter(|r| r.from == ElementKind::Threat)
        .map(|r| (r.key, r.link.is_some()))
        .collect();
    let expected_damages = vcx.read_entity(&wb, |w, _| {
        related(
            w.workspace().project(),
            RelationKey::THREAT_DAMAGES,
            Selection::Threat(threat),
        )
    });
    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), Some(Selection::Threat(threat)));
        let actual: Vec<(RelationKey, bool)> = i
            .relation_rows()
            .iter()
            .map(|r| (r.key, r.editable))
            .collect();
        assert_eq!(actual, expected_threat_rels);
        let dmg_row = i
            .relation_rows()
            .iter()
            .find(|r| r.key == RelationKey::THREAT_DAMAGES)
            .expect("THREAT_DAMAGES row present");
        let chip_targets: Vec<Selection> = dmg_row.chips.iter().map(|c| c.target).collect();
        assert_eq!(chip_targets, expected_damages);
    });

    // Non-editable relation refuses open_picker without changing active_picker.
    inspector.update_in(vcx, |i, window, cx| {
        i.open_picker(RelationKey::THREAT_ASSET, window, cx);
    });
    assert!(vcx.read_entity(&inspector, |i, _| i.active_picker().is_none()));

    // Selecting a Goal via Workbench::select_element updates rows to match ElementKind::Goal.
    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::Goal(goal)), cx)
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    let expected_goal_rels: Vec<(RelationKey, bool)> = RELATIONS
        .iter()
        .filter(|r| r.from == ElementKind::Goal)
        .map(|r| (r.key, r.link.is_some()))
        .collect();
    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), Some(Selection::Goal(goal)));
        let actual: Vec<(RelationKey, bool)> = i
            .relation_rows()
            .iter()
            .map(|r| (r.key, r.editable))
            .collect();
        assert_eq!(actual, expected_goal_rels);
    });
}

#[gpui::test]
fn inspector_inline_picker_toggles_without_double_lease_undoes_and_closes_on_reselect(
    cx: &mut TestAppContext,
) {
    let (_catalog, _host, wb, vcx) = open_host(cx);
    let inspector = inspector_of(&wb, vcx);

    let extra_damage = DamageId::new_v7();
    wb.update(vcx, |w, cx| {
        assert!(w.apply_edit(
            &Edit::AddDamage {
                id: extra_damage,
                name: "Thermal runaway".into(),
            },
            cx,
        ));
    });
    let threat = vcx.read_entity(&wb, |w, _| {
        w.workspace()
            .project()
            .threat_scenarios()
            .next()
            .map(|(_, id, _)| id)
            .expect("sample has threat")
    });
    let asset = vcx.read_entity(&wb, |w, _| {
        w.workspace()
            .project()
            .assets()
            .next()
            .map(|(_, id, _)| id)
            .expect("sample has asset")
    });

    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::Threat(threat)), cx);
    });
    let before_bytes = project_bytes(&wb, vcx);
    let before_chips: Vec<Selection> = vcx.read_entity(&inspector, |i, _| {
        i.relation_rows()
            .iter()
            .find(|r| r.key == RelationKey::THREAT_DAMAGES)
            .expect("damages row")
            .chips
            .iter()
            .map(|c| c.target)
            .collect()
    });
    assert!(!before_chips.contains(&Selection::Damage(extra_damage)));

    // Open the inline picker on THREAT_DAMAGES and draw so it is mounted in the right dock.
    inspector.update_in(vcx, |i, window, cx| {
        i.open_picker(RelationKey::THREAT_DAMAGES, window, cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.run_until_parked();

    let picker = vcx.read_entity(&inspector, |i, _| {
        i.active_picker().cloned().expect("inline picker is open")
    });
    let picker_id = picker.entity_id();

    // Filter to the new damage and press Enter -> live link toggle with no double-lease panic.
    picker.update_in(vcx, |p, window, cx| p.set_filter("Thermal", window, cx));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.run_until_parked();

    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();

    // Inspector chips updated AND the exact same picker entity stayed open and refreshed (L31).
    vcx.read_entity(&inspector, |i, _| {
        let active = i
            .active_picker()
            .expect("picker must stay open after toggle");
        assert_eq!(active.entity_id(), picker_id, "picker must not be reopened");
        let chips: Vec<Selection> = i
            .relation_rows()
            .iter()
            .find(|r| r.key == RelationKey::THREAT_DAMAGES)
            .expect("damages row")
            .chips
            .iter()
            .map(|c| c.target)
            .collect();
        assert_eq!(chips.len(), before_chips.len() + 1);
        assert!(chips.contains(&Selection::Damage(extra_damage)));
    });
    vcx.read_entity(&picker, |p, _| {
        assert!(p.picker().is_open());
        assert!(p.picker().active().expect("active").linked);
    });
    assert_ne!(project_bytes(&wb, vcx), before_bytes);

    // Run "undo" via Workbench::run -> bytes match prior snapshot and chips & picker revert.
    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    vcx.run_until_parked();

    assert_eq!(project_bytes(&wb, vcx), before_bytes);
    vcx.read_entity(&inspector, |i, _| {
        let active = i.active_picker().expect("picker stays open across undo");
        assert_eq!(active.entity_id(), picker_id);
        let chips: Vec<Selection> = i
            .relation_rows()
            .iter()
            .find(|r| r.key == RelationKey::THREAT_DAMAGES)
            .expect("damages row")
            .chips
            .iter()
            .map(|c| c.target)
            .collect();
        assert_eq!(chips, before_chips);
    });
    vcx.read_entity(&picker, |p, _| {
        assert!(!p.picker().active().expect("active").linked);
    });

    // Clicking a chip in the inline picker navigates without double-lease and switches selection.
    let linked_damage = *before_chips.first().expect("sample threat has a damage");
    picker.update_in(vcx, |p, window, cx| p.open_chip(linked_damage, window, cx));
    vcx.run_until_parked();
    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), Some(linked_damage));
        assert!(i.active_picker().is_none());
    });

    // Re-select threat, open picker, then select a different element -> active_picker closes.
    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::Threat(threat)), cx);
    });
    inspector.update_in(vcx, |i, window, cx| {
        i.open_picker(RelationKey::THREAT_DAMAGES, window, cx);
    });
    assert!(vcx.read_entity(&inspector, |i, _| i.active_picker().is_some()));

    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::Asset(asset)), cx);
    });
    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), Some(Selection::Asset(asset)));
        assert!(i.active_picker().is_none());
    });
}

#[gpui::test]
fn inspector_clears_on_deleted_selection_and_project_replace(cx: &mut TestAppContext) {
    let (_catalog, _host, wb, vcx) = open_host(cx);
    let inspector = inspector_of(&wb, vcx);

    let threat = vcx.read_entity(&wb, |w, _| {
        w.workspace()
            .project()
            .threat_scenarios()
            .next()
            .map(|(_, id, _)| id)
            .expect("sample has threat")
    });
    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::Threat(threat)), cx);
    });
    inspector.update_in(vcx, |i, window, cx| {
        i.open_picker(RelationKey::THREAT_DAMAGES, window, cx);
    });
    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), Some(Selection::Threat(threat)));
        assert!(!i.relation_rows().is_empty());
        assert!(i.active_picker().is_some());
    });

    // Deleting the inspected element via Workbench::apply_edit clears selection, rows, and picker.
    wb.update(vcx, |w, cx| {
        assert!(w.apply_edit(&Edit::RemoveThreat { threat }, cx));
    });
    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), None);
        assert!(i.relation_rows().is_empty());
        assert!(i.active_picker().is_none());
    });

    // Select an asset with an open picker, then replace the project via show_project -> clears.
    let asset = vcx.read_entity(&wb, |w, _| {
        w.workspace()
            .project()
            .assets()
            .next()
            .map(|(_, id, _)| id)
            .expect("sample has asset")
    });
    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::Asset(asset)), cx);
    });
    inspector.update_in(vcx, |i, window, cx| {
        i.open_picker(RelationKey::ASSET_COMPONENT, window, cx);
    });
    assert!(vcx.read_entity(&inspector, |i, _| i.active_picker().is_some()));

    let fresh = workbench_sample().project().clone();
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(fresh, "Replaced project", None, window, cx);
    });
    vcx.read_entity(&inspector, |i, _| {
        assert_eq!(i.selection(), None);
        assert!(i.relation_rows().is_empty());
        assert!(i.active_picker().is_none());
    });
}

#[gpui::test]
fn inspector_read_only_guard_refuses_opening_picker(cx: &mut TestAppContext) {
    let (_catalog, _host, wb, vcx) = open_host(cx);
    let inspector = inspector_of(&wb, vcx);

    let threat = vcx.read_entity(&wb, |w, _| {
        w.workspace()
            .project()
            .threat_scenarios()
            .next()
            .map(|(_, id, _)| id)
            .expect("sample has threat")
    });
    wb.update(vcx, |w, cx| {
        w.select_element(Some(Selection::Threat(threat)), cx);
        w.set_read_only(Some("Read-only".into()), cx);
    });

    inspector.update_in(vcx, |i, window, cx| {
        i.open_picker(RelationKey::THREAT_DAMAGES, window, cx);
    });
    assert!(vcx.read_entity(&inspector, |i, _| i.active_picker().is_none()));
    assert_eq!(
        vcx.read_entity(&wb, |w, _| w.note().map(str::to_owned)),
        Some("Read-only".into())
    );

    // Clearing read-only allows the exact same open_picker call to succeed (L09).
    wb.update(vcx, |w, cx| w.set_read_only(None, cx));
    inspector.update_in(vcx, |i, window, cx| {
        i.open_picker(RelationKey::THREAT_DAMAGES, window, cx);
    });
    assert!(vcx.read_entity(&inspector, |i, _| i.active_picker().is_some()));
}
