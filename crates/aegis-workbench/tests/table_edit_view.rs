//! Headless GPUI tests for `TableTab` keyboard navigation, inline `Text` &
//! `MultiLinkPickerView` editing, multi-cell commits, and read-only checks.
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::rc::Rc;

use aegis_client::command_table::command;
use aegis_client::commands::Edit;
use aegis_client::explorer::{ExplorerKey, Folder, Owner};
use aegis_client::grid::{CellInput, GridSelection};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::settings::Appearance;
use aegis_client::view::Selection;
use aegis_model::{AssetId, GoalId, Op, PartyId, PartyKind, Project, Provenance, RiskTreatment};
use aegis_workbench::app::{TableTab, Workbench, apply_appearance, bind_all};
use aegis_workbench::sample::workbench_sample;
use gpui::{
    AppContext as _, ClipboardItem, Entity, Focusable as _, TestAppContext, VisualTestContext,
};

fn saved_bytes(project: &Project) -> Vec<u8> {
    let mut out = Vec::new();
    project.save(&mut out).expect("project saves");
    out
}

fn project_bytes(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<u8> {
    cx.update(|_, cx| saved_bytes(wb.read(cx).workspace().project()))
}

fn undo_keys() -> String {
    command("undo")
        .and_then(|c| c.keys_for(cfg!(target_os = "macos")))
        .expect("undo command has keys")
}

fn open_table(
    folder: Folder,
    project: Option<Project>,
    cx: &mut TestAppContext,
) -> (Entity<Workbench>, Entity<TableTab>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        bind_all(cx);
    });
    let catalog = Rc::new(Catalog::load(Language::En));
    let (wb, vcx) = cx.add_window_view(|window, cx| {
        apply_appearance(Appearance::Light, window, cx);
        Workbench::new(catalog, Appearance::Light, None, window, cx)
    });
    if let Some(proj) = project {
        wb.update_in(vcx, |w, window, cx| {
            w.show_project(proj, "Test project", None, window, cx);
        });
    }
    let key = ExplorerKey::Folder(Owner::Project, folder);
    wb.update_in(vcx, |w, window, cx| {
        w.open(key, window, cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let tab = vcx
        .read_entity(&wb, |w, _| w.table_tab(key))
        .expect("table tab must open");
    (wb, tab, vcx)
}

fn col_index(tab: &Entity<TableTab>, key: &str, vcx: &mut VisualTestContext) -> usize {
    vcx.read_entity(tab, |t, _| {
        t.visible_columns()
            .iter()
            .position(|c| c.key == key)
            .unwrap_or_else(|| panic!("missing visible column {key}"))
    })
}

fn select_cell(tab: &Entity<TableTab>, row: usize, col: usize, vcx: &mut VisualTestContext) {
    tab.update(vcx, |t, cx| {
        t.set_selection(Some(GridSelection::single(row, col)), cx);
    });
}

#[gpui::test]
fn keyboard_navigation_from_none_selection_syncs_inspector_and_extends(cx: &mut TestAppContext) {
    let (wb, tab, vcx) = open_table(Folder::Assets, None, cx);
    wb.update(vcx, |w, cx| {
        assert!(w.apply_edit(
            &Edit::AddAsset {
                id: AssetId::new_v7(),
                name: "Diagnostic bus".into(),
                properties: vec![0],
            },
            cx,
        ));
    });
    assert!(vcx.read_entity(&tab, |t, _| t.visible_row_count() >= 2));
    assert_eq!(vcx.read_entity(&tab, |t, _| t.selection()), None);
    assert!(vcx.read_entity(&tab, |t, _| t.grid_keys_active()));

    let handled = tab.update_in(vcx, |t, window, cx| {
        t.handle_grid_key("down", false, window, cx)
    });
    assert!(handled);

    let first_target = vcx.read_entity(&tab, |t, _| {
        assert_eq!(t.selection(), Some(GridSelection::single(0, 0)));
        t.displayed_row(0).and_then(|r| r.target)
    });
    assert!(first_target.is_some());
    let inspector_sel = vcx.read_entity(&wb, |w, cx| w.inspector().read(cx).selection());
    assert_eq!(inspector_sel, first_target);

    tab.update_in(vcx, |t, window, cx| {
        assert!(t.handle_grid_key("right", false, window, cx));
        assert!(t.handle_grid_key("tab", false, window, cx));
        assert!(t.handle_grid_key("tab", true, window, cx));
        assert!(t.handle_grid_key("down", true, window, cx));
    });
    let expected_row1 = vcx.read_entity(&tab, |t, _| {
        let sel = t.selection().expect("selection present");
        assert_eq!(sel.anchor_row, 0);
        assert_eq!(sel.active_row, 1);
        assert_eq!(sel.anchor_col, 1);
        assert_eq!(sel.active_col, 1);
        assert_eq!(sel.row_range(), 0..=1);
        t.displayed_row(1).and_then(|r| r.target)
    });
    assert_eq!(
        vcx.read_entity(&wb, |w, cx| w.inspector().read(cx).selection()),
        expected_row1
    );

    tab.update_in(vcx, |t, window, cx| {
        assert!(t.handle_grid_key("up", false, window, cx));
        assert!(t.handle_grid_key("left", false, window, cx));
        assert_eq!(t.selection(), Some(GridSelection::single(0, 0)));
        t.open_filter_for("name", window, cx);
        assert!(!t.grid_keys_active());
        assert!(!t.handle_grid_key("down", false, window, cx));
        assert_eq!(t.selection(), Some(GridSelection::single(0, 0)));
    });
}

#[gpui::test]
fn inline_name_edit_preserves_editor_on_error_commits_renames_and_undoes(cx: &mut TestAppContext) {
    let (wb, tab, vcx) = open_table(Folder::Assets, None, cx);
    let before = project_bytes(&wb, vcx);
    let name_col = col_index(&tab, "name", vcx);
    let target_id = vcx.read_entity(&tab, |t, _| {
        match t.displayed_row(0).and_then(|r| r.target) {
            Some(Selection::Asset(id)) => id,
            other => panic!("expected asset target, got {other:?}"),
        }
    });
    select_cell(&tab, 0, name_col, vcx);

    assert!(tab.update_in(vcx, |t, window, cx| t.start_edit(window, cx)));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.read_entity(&tab, |t, _| {
        assert!(t.is_editing());
        assert!(!t.grid_keys_active());
    });

    // Blank name error keeps editor open, sets note, and leaves project untouched.
    let bad_ok = tab.update_in(vcx, |t, window, cx| {
        t.commit_input(CellInput::Text("   ".into()), window, cx)
    });
    assert!(!bad_ok);
    vcx.read_entity(&tab, |t, _| assert!(t.is_editing()));
    assert!(vcx.read_entity(&wb, |w, _| w.note().is_some()));
    assert_eq!(project_bytes(&wb, vcx), before);

    // Escape cancels active InputState cleanly via capture_key_down.
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    vcx.read_entity(&tab, |t, _| {
        assert!(!t.is_editing());
        assert!(t.grid_keys_active());
    });
    assert_eq!(project_bytes(&wb, vcx), before);

    // Re-open with F2, type valid suffix in the focused InputState, and commit via Enter.
    let orig_name = vcx.read_entity(&wb, |w, _| {
        let p = w.workspace().project();
        p.asset(p.asset_idx(&target_id).unwrap())
            .unwrap()
            .name()
            .to_owned()
    });
    assert!(tab.update_in(vcx, |t, window, cx| {
        t.handle_grid_key("f2", false, window, cx)
    }));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.simulate_keystrokes("v 2 space enter");
    vcx.run_until_parked();

    vcx.read_entity(&tab, |t, _| {
        assert!(!t.is_editing());
        assert!(t.grid_keys_active());
    });
    vcx.read_entity(&wb, |w, _| {
        assert!(w.note().is_none());
        let p = w.workspace().project();
        let asset = p.asset(p.asset_idx(&target_id).unwrap()).unwrap();
        assert_eq!(asset.name(), format!("v2 {orig_name}"));
    });

    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    assert_eq!(project_bytes(&wb, vcx), before);
}

#[gpui::test]
fn computed_id_and_problems_cells_stay_read_only(cx: &mut TestAppContext) {
    let (wb, tab, vcx) = open_table(Folder::Assets, None, cx);
    let before = project_bytes(&wb, vcx);
    for key in ["id", "threats"] {
        let col = col_index(&tab, key, vcx);
        select_cell(&tab, 0, col, vcx);
        assert!(!tab.update_in(vcx, |t, window, cx| t.start_edit(window, cx)));
        assert!(!tab.update_in(vcx, |t, window, cx| {
            t.commit_input(CellInput::Text("Changed".into()), window, cx)
        }));
        vcx.read_entity(&tab, |t, _| assert!(!t.is_editing()));
        assert_eq!(project_bytes(&wb, vcx), before);
    }

    let key = ExplorerKey::Folder(Owner::Project, Folder::Problems);
    wb.update_in(vcx, |w, window, cx| w.open(key, window, cx));
    let problems = vcx
        .read_entity(&wb, |w, _| w.table_tab(key))
        .expect("Problems tab opens");
    select_cell(&problems, 0, 0, vcx);
    assert!(!problems.update_in(vcx, |t, window, cx| t.start_edit(window, cx)));
    assert_eq!(project_bytes(&wb, vcx), before);
}

#[gpui::test]
fn multi_cell_commit_updates_all_selected_rows_in_single_undo(cx: &mut TestAppContext) {
    let (wb, tab, vcx) = open_table(Folder::Threats, None, cx);
    let before = project_bytes(&wb, vcx);
    let scales_col = col_index(&tab, "scales", vcx);
    let row_count = vcx.read_entity(&tab, |t, _| t.visible_row_count());
    assert!(row_count >= 2, "sample must contain at least two threats");

    select_cell(&tab, 0, scales_col, vcx);
    tab.update_in(vcx, |t, window, cx| {
        assert!(t.handle_grid_key("down", true, window, cx));
    });
    let targets: Vec<_> = vcx.read_entity(&tab, |t, _| {
        assert!(!t.is_editing());
        assert_eq!(t.selection().unwrap().row_range(), 0..=1);
        (0..=1)
            .filter_map(|r| match t.displayed_row(r).and_then(|row| row.target) {
                Some(Selection::Threat(id)) => Some(id),
                _ => None,
            })
            .collect()
    });
    assert_eq!(targets.len(), 2);

    let ok = tab.update_in(vcx, |t, window, cx| {
        t.commit_input(CellInput::SingleChoice(Some(0)), window, cx)
    });
    assert!(ok);

    vcx.read_entity(&wb, |w, _| {
        let p = w.workspace().project();
        for id in &targets {
            let threat = p
                .threat_scenario(p.threat_scenario_idx(id).unwrap())
                .unwrap();
            assert_eq!(threat.scales(), Some(true));
        }
    });

    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    assert_eq!(project_bytes(&wb, vcx), before);
}

#[gpui::test]
fn inline_link_picker_links_refreshes_live_escapes_and_undoes(cx: &mut TestAppContext) {
    let mut proj = workbench_sample().project().clone();
    let prov = Provenance::human("tester", 1);
    let goal = GoalId::new_v7();
    let party = PartyId::new_v7();
    for op in [
        Op::AddCybersecurityGoal {
            id: goal,
            name: "Verify firmware".into(),
            controls: Vec::new(),
            provenance: prov.clone(),
        },
        Op::AddParty {
            id: party,
            name: "ECU Supplier".into(),
            kind: PartyKind::Supplier,
            provenance: prov,
        },
    ] {
        proj.apply(op).expect("seed op applies");
    }

    let (wb, tab, vcx) = open_table(Folder::Goals, Some(proj), cx);
    let before = project_bytes(&wb, vcx);
    let resp_col = col_index(&tab, "responsible", vcx);
    select_cell(&tab, 0, resp_col, vcx);

    assert!(tab.update_in(vcx, |t, window, cx| t.start_edit(window, cx)));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let picker = vcx.read_entity(&tab, |t, _| {
        assert!(t.is_editing());
        assert!(!t.grid_keys_active());
        t.active_link_picker().cloned()
    });
    let picker = picker.expect("LinkPicker active");
    assert!(vcx.read_entity(&picker, |p, _| p.picker().is_open()));

    // Enter links the party via MultiLinkPickerView without double-lease panic;
    // chip appears in TableTab::displayed_row and picker stays open and refreshed (L31).
    vcx.simulate_keystrokes("enter");
    vcx.run_until_parked();
    vcx.read_entity(&picker, |p, _| {
        assert!(p.picker().is_open());
        assert!(p.picker().active().expect("party option").linked);
    });
    vcx.read_entity(&tab, |t, _| {
        assert!(t.is_editing());
        let chips = &t.displayed_row(0).expect("goal row").cells[resp_col].chips;
        assert!(chips.iter().any(|c| c.target == Selection::Party(party)));
    });

    // Escape closes the picker via on_close and restores focus to TableTab.
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    let tab_focus = vcx.read_entity(&tab, |t, cx| {
        assert!(!t.is_editing());
        assert!(t.grid_keys_active());
        t.focus_handle(cx)
    });
    assert!(vcx.update(|window, _| tab_focus.is_focused(window)));

    vcx.simulate_keystrokes(&undo_keys());
    vcx.run_until_parked();
    assert_eq!(project_bytes(&wb, vcx), before);
}

#[gpui::test]
fn read_only_refuses_start_edit_and_commit_input(cx: &mut TestAppContext) {
    let (wb, tab, vcx) = open_table(Folder::Assets, None, cx);
    let before = project_bytes(&wb, vcx);
    let name_col = col_index(&tab, "name", vcx);
    select_cell(&tab, 0, name_col, vcx);

    wb.update(vcx, |w, cx| w.set_read_only(Some("Read-only".into()), cx));
    assert!(!tab.update_in(vcx, |t, window, cx| t.start_edit(window, cx)));
    assert_eq!(
        vcx.read_entity(&wb, |w, _| w.note().map(str::to_owned)),
        Some("Read-only".into())
    );
    assert!(!tab.update_in(vcx, |t, window, cx| {
        t.commit_input(CellInput::Text("Blocked".into()), window, cx)
    }));
    vcx.read_entity(&tab, |t, _| assert!(!t.is_editing()));
    assert_eq!(project_bytes(&wb, vcx), before);
}

#[gpui::test]
fn single_choice_rating_edits_risk_treatment_and_renders_popovers(cx: &mut TestAppContext) {
    let (wb, tab, vcx) = open_table(Folder::Risks, None, cx);
    let before = project_bytes(&wb, vcx);
    let treatment_col = col_index(&tab, "treatment", vcx);

    let (row_ix, threat_id) = vcx.read_entity(&wb, |w, cx| {
        let p = w.workspace().project();
        let t = tab.read(cx);
        (0..t.visible_row_count())
            .find_map(|r| match t.displayed_row(r).and_then(|row| row.target) {
                Some(Selection::Threat(id)) => {
                    let th = p.threat_scenario(p.threat_scenario_idx(&id)?)?;
                    (th.treatment() != Some(RiskTreatment::Reduce)).then_some((r, id))
                }
                _ => None,
            })
            .expect("sample risks must have a row without Reduce treatment")
    });
    select_cell(&tab, row_ix, treatment_col, vcx);

    assert!(tab.update_in(vcx, |t, window, cx| t.start_edit(window, cx)));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    vcx.read_entity(&tab, |t, _| {
        assert!(t.is_editing());
        assert!(!t.grid_keys_active());
    });

    assert!(tab.update_in(vcx, |t, window, cx| {
        t.commit_input(CellInput::SingleChoice(Some(1)), window, cx)
    }));
    vcx.read_entity(&tab, |t, _| {
        assert!(!t.is_editing());
        assert!(t.grid_keys_active());
    });
    vcx.read_entity(&wb, |w, _| {
        let p = w.workspace().project();
        let th = p
            .threat_scenario(p.threat_scenario_idx(&threat_id).unwrap())
            .unwrap();
        assert_eq!(th.treatment(), Some(RiskTreatment::Reduce));
    });
    assert_ne!(project_bytes(&wb, vcx), before);

    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    assert_eq!(project_bytes(&wb, vcx), before);

    // Clearing treatment via SingleChoice(None) also commits and undoes cleanly.
    assert!(tab.update_in(vcx, |t, window, cx| t.start_edit(window, cx)));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(tab.update_in(vcx, |t, window, cx| {
        t.commit_input(CellInput::SingleChoice(None), window, cx)
    }));
    vcx.read_entity(&wb, |w, _| {
        let p = w.workspace().project();
        let th = p
            .threat_scenario(p.threat_scenario_idx(&threat_id).unwrap())
            .unwrap();
        assert_eq!(th.treatment(), None);
    });
    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    assert_eq!(project_bytes(&wb, vcx), before);

    // Threats.scales also opens SingleChoice popover, renders, commits, and undoes cleanly.
    let (wb, tab, vcx) = open_table(Folder::Threats, None, cx);
    let before = project_bytes(&wb, vcx);
    let scales_col = col_index(&tab, "scales", vcx);
    let threat_id = vcx.read_entity(&tab, |t, _| {
        match t.displayed_row(0).and_then(|r| r.target) {
            Some(Selection::Threat(id)) => id,
            other => panic!("expected threat target, got {other:?}"),
        }
    });
    let choice_ix = vcx.read_entity(&wb, |w, _| {
        let p = w.workspace().project();
        let th = p
            .threat_scenario(p.threat_scenario_idx(&threat_id).unwrap())
            .unwrap();
        usize::from(th.scales() == Some(true))
    });
    select_cell(&tab, 0, scales_col, vcx);
    assert!(tab.update_in(vcx, |t, window, cx| t.start_edit(window, cx)));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(tab.update_in(vcx, |t, window, cx| {
        t.commit_input(CellInput::SingleChoice(Some(choice_ix)), window, cx)
    }));
    vcx.read_entity(&wb, |w, _| {
        let p = w.workspace().project();
        let th = p
            .threat_scenario(p.threat_scenario_idx(&threat_id).unwrap())
            .unwrap();
        assert_eq!(th.scales(), Some(choice_ix == 0));
    });
    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    assert_eq!(project_bytes(&wb, vcx), before);
}

#[gpui::test]
fn multi_choice_properties_edits_and_refuses_clearing_all(cx: &mut TestAppContext) {
    let (wb, tab, vcx) = open_table(Folder::Assets, None, cx);
    let before = project_bytes(&wb, vcx);
    let props_col = col_index(&tab, "properties", vcx);

    let asset_id = vcx.read_entity(&tab, |t, _| {
        match t.displayed_row(0).and_then(|r| r.target) {
            Some(Selection::Asset(id)) => id,
            other => panic!("expected asset target, got {other:?}"),
        }
    });
    let (toggle_ix, expected_flags, checked_indices) = vcx.read_entity(&wb, |w, _| {
        let p = w.workspace().project();
        let m = p.methodology();
        let asset = p.asset(p.asset_idx(&asset_id).unwrap()).unwrap();
        let props = asset.properties();
        let checked: Vec<usize> = m
            .properties()
            .enumerate()
            .filter_map(|(ix, prop)| props.contains(prop).then_some(ix))
            .collect();
        let unchecked = m
            .properties()
            .enumerate()
            .find_map(|(ix, prop)| (!props.contains(prop)).then_some(ix));
        let toggle_ix = unchecked.unwrap_or(0);
        let expected: Vec<bool> = m
            .properties()
            .enumerate()
            .map(|(ix, prop)| {
                if ix == toggle_ix {
                    !props.contains(prop)
                } else {
                    props.contains(prop)
                }
            })
            .collect();
        assert!(expected.iter().any(|&on| on));
        (toggle_ix, expected, checked)
    });

    select_cell(&tab, 0, props_col, vcx);
    assert!(tab.update_in(vcx, |t, window, cx| t.start_edit(window, cx)));
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    tab.update(vcx, |t, cx| t.toggle_multi_choice_option(toggle_ix, cx));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(project_bytes(&wb, vcx), before);

    assert!(tab.update_in(vcx, |t, window, cx| t.apply_multi_choice(window, cx)));
    vcx.read_entity(&tab, |t, _| {
        assert!(!t.is_editing());
        assert!(t.grid_keys_active());
    });
    vcx.read_entity(&wb, |w, _| {
        let p = w.workspace().project();
        let asset = p.asset(p.asset_idx(&asset_id).unwrap()).unwrap();
        let actual: Vec<bool> = p
            .methodology()
            .properties()
            .map(|prop| asset.properties().contains(prop))
            .collect();
        assert_eq!(actual, expected_flags);
    });
    assert_ne!(project_bytes(&wb, vcx), before);

    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    assert_eq!(project_bytes(&wb, vcx), before);

    // Toggling all properties off is refused, keeps editor open, sets wb.note(), and leaves project unchanged.
    assert!(tab.update_in(vcx, |t, window, cx| t.start_edit(window, cx)));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    tab.update(vcx, |t, cx| {
        for &ix in &checked_indices {
            t.toggle_multi_choice_option(ix, cx);
        }
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(!tab.update_in(vcx, |t, window, cx| t.apply_multi_choice(window, cx)));
    vcx.read_entity(&tab, |t, _| {
        assert!(t.is_editing());
        assert!(!t.grid_keys_active());
    });
    assert!(vcx.read_entity(&wb, |w, _| w.note().is_some()));
    assert_eq!(project_bytes(&wb, vcx), before);

    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    vcx.read_entity(&tab, |t, _| {
        assert!(!t.is_editing());
        assert!(t.grid_keys_active());
    });
}

#[gpui::test]
fn atomic_impact_ratings_stages_locally_and_undoes_in_one_step(cx: &mut TestAppContext) {
    let (wb, tab, vcx) = open_table(Folder::Damage, None, cx);
    let before = project_bytes(&wb, vcx);
    let impact_col = col_index(&tab, "impact", vcx);

    let damage_id = vcx.read_entity(&tab, |t, _| {
        match t.displayed_row(0).and_then(|r| r.target) {
            Some(Selection::Damage(id)) => id,
            other => panic!("expected damage target, got {other:?}"),
        }
    });
    let (new0, new1, exp0, exp1) = vcx.read_entity(&wb, |w, _| {
        let p = w.workspace().project();
        let m = p.methodology();
        let damage = p
            .damage_scenario(p.damage_scenario_idx(&damage_id).unwrap())
            .unwrap();
        let mut cats = m.impact_categories();
        let cat0 = cats.next().expect("category 0");
        let cat1 = cats.next().expect("category 1");
        let cur0 = damage
            .impact(cat0)
            .and_then(|lvl| m.levels(cat0).position(|x| x == lvl));
        let cur1 = damage
            .impact(cat1)
            .and_then(|lvl| m.levels(cat1).position(|x| x == lvl));
        let cnt0 = m.levels(cat0).count();
        let cnt1 = m.levels(cat1).count();
        let n0 = cur0.map_or(0, |i| (i + 1) % cnt0);
        let n1 = cur1.map_or(1 % cnt1, |i| (i + 1) % cnt1);
        (
            n0,
            n1,
            m.levels(cat0).nth(n0).unwrap(),
            m.levels(cat1).nth(n1).unwrap(),
        )
    });

    select_cell(&tab, 0, impact_col, vcx);
    assert!(tab.update_in(vcx, |t, window, cx| t.start_edit(window, cx)));
    vcx.update(|window, cx| window.draw(cx).clear(cx));

    tab.update(vcx, |t, cx| {
        t.set_impact_editor_level(0, Some(new0), cx);
        t.set_impact_editor_level(1, Some(new1), cx);
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(project_bytes(&wb, vcx), before);

    assert!(tab.update_in(vcx, |t, window, cx| t.apply_impact_ratings(window, cx)));
    vcx.read_entity(&tab, |t, _| {
        assert!(!t.is_editing());
        assert!(t.grid_keys_active());
    });
    vcx.read_entity(&wb, |w, _| {
        let p = w.workspace().project();
        let m = p.methodology();
        let mut cats = m.impact_categories();
        let cat0 = cats.next().unwrap();
        let cat1 = cats.next().unwrap();
        let damage = p
            .damage_scenario(p.damage_scenario_idx(&damage_id).unwrap())
            .unwrap();
        assert_eq!(damage.impact(cat0), Some(exp0));
        assert_eq!(damage.impact(cat1), Some(exp1));
    });
    assert_ne!(project_bytes(&wb, vcx), before);

    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    assert_eq!(project_bytes(&wb, vcx), before);
}

#[gpui::test]
fn fill_down_copies_top_row_editable_values_skips_computed_and_undoes(cx: &mut TestAppContext) {
    let (wb, tab, vcx) = open_table(Folder::Assets, None, cx);
    wb.update(vcx, |w, cx| {
        assert!(w.apply_edit(
            &Edit::AddAsset {
                id: AssetId::new_v7(),
                name: "Secondary CAN Bus".into(),
                properties: vec![0],
            },
            cx,
        ));
    });
    let props_col = col_index(&tab, "properties", vcx);
    // Give row 0 multiple properties so row 0 and row 1 differ in both name and properties.
    let all_props: Vec<bool> = vcx.read_entity(&wb, |w, _| {
        let count = w.workspace().project().methodology().properties().count();
        vec![true; count]
    });
    select_cell(&tab, 0, props_col, vcx);
    assert!(tab.update_in(vcx, |t, window, cx| {
        t.commit_input(CellInput::MultiChoice(all_props.clone()), window, cx)
    }));

    let before = project_bytes(&wb, vcx);
    let threats_col = vcx.read_entity(&tab, |t, _| {
        t.table()
            .columns()
            .iter()
            .position(|c| c.key == "threats")
            .unwrap()
    });
    let (top_id, dst_id, top_name, dst_threats_text) = vcx.read_entity(&wb, |w, cx| {
        let t = tab.read(cx);
        let top_row = t.displayed_row(0).unwrap();
        let dst_row = t.displayed_row(1).unwrap();
        let Some(Selection::Asset(top_id)) = top_row.target else {
            panic!("row 0 must be asset");
        };
        let Some(Selection::Asset(dst_id)) = dst_row.target else {
            panic!("row 1 must be asset");
        };
        let p = w.workspace().project();
        let top_name = p
            .asset(p.asset_idx(&top_id).unwrap())
            .unwrap()
            .name()
            .to_owned();
        let dst_name = p
            .asset(p.asset_idx(&dst_id).unwrap())
            .unwrap()
            .name()
            .to_owned();
        assert_ne!(top_name, dst_name);
        (
            top_id,
            dst_id,
            top_name,
            dst_row.cells[threats_col].text.clone(),
        )
    });

    // Select rows 0..=1 across all visible columns (including Id and Computed).
    let last_col = vcx.read_entity(&tab, |t, _| t.visible_columns().len() - 1);
    tab.update(vcx, |t, cx| {
        let mut sel = GridSelection::single(0, 0);
        sel.extend_to(1, last_col);
        t.set_selection(Some(sel), cx);
    });

    wb.update_in(vcx, |w, window, cx| w.run("grid-fill-down", window, cx));

    vcx.read_entity(&wb, |w, cx| {
        assert!(w.note().is_none());
        let p = w.workspace().project();
        let top = p.asset(p.asset_idx(&top_id).unwrap()).unwrap();
        let dst = p.asset(p.asset_idx(&dst_id).unwrap()).unwrap();
        assert_eq!(dst.name(), top_name);
        assert_eq!(dst.properties(), top.properties());
        // Computed threats column and distinct AssetId stayed intact.
        assert_ne!(top_id, dst_id);
        let t = tab.read(cx);
        assert_eq!(
            t.displayed_row(1).unwrap().cells[threats_col].text,
            dst_threats_text
        );
    });
    assert_ne!(project_bytes(&wb, vcx), before);

    // Single undo restores exact prior bytes.
    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    assert_eq!(project_bytes(&wb, vcx), before);
}

#[gpui::test]
fn tsv_copy_paste_reversed_selection_updates_appends_and_undoes(cx: &mut TestAppContext) {
    let (wb, tab, vcx) = open_table(Folder::Assets, None, cx);
    wb.update(vcx, |w, cx| {
        assert!(w.apply_edit(
            &Edit::AddAsset {
                id: AssetId::new_v7(),
                name: "Gateway Link".into(),
                properties: vec![0],
            },
            cx,
        ));
    });
    let before = project_bytes(&wb, vcx);
    let name_col = col_index(&tab, "name", vcx);
    let props_col = col_index(&tab, "properties", vcx);

    // Bottom-to-top, right-to-left reversed selection covering rows 0..=1 and name..=properties.
    tab.update(vcx, |t, cx| {
        let mut sel = GridSelection::single(1, props_col);
        sel.extend_to(0, name_col);
        t.set_selection(Some(sel), cx);
    });

    wb.update_in(vcx, |w, window, cx| w.run("grid-copy", window, cx));
    let copied = vcx.update(|_, cx| cx.read_from_clipboard().and_then(|i| i.text()));
    let copied = copied.expect("clipboard holds copied TSV");
    assert_eq!(copied.lines().count(), 2);
    assert!(copied.contains('\t'));

    let (row0_id, prop_label, init_count) = vcx.read_entity(&wb, |w, cx| {
        let t = tab.read(cx);
        let Some(Selection::Asset(id)) = t.displayed_row(0).and_then(|r| r.target) else {
            panic!("row 0 asset");
        };
        let p = w.workspace().project();
        let a0 = p.asset(p.asset_idx(&id).unwrap()).unwrap();
        let m = p.methodology();
        let all_props = m
            .properties()
            .filter_map(|prop| m.property_label(prop))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = a0;
        (id, all_props, p.asset_count())
    });

    // Keep a reversed selection whose top-left corner is (0, name_col), and paste 4 rows
    // (updating rows 0 and 1, plus appending 2 new assets).
    let tsv = format!(
        "Renamed Asset 0\t{prop_label}\nRenamed Asset 1\t{prop_label}\nAppended Asset A\t{prop_label}\nAppended Asset B\t{prop_label}\n"
    );
    vcx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(tsv)));
    wb.update_in(vcx, |w, window, cx| w.run("grid-paste", window, cx));

    vcx.read_entity(&wb, |w, _| {
        assert!(w.note().is_none());
        let p = w.workspace().project();
        assert_eq!(p.asset_count(), init_count + 2);
        let a0 = p.asset(p.asset_idx(&row0_id).unwrap()).unwrap();
        assert_eq!(a0.name(), "Renamed Asset 0");
        let names: Vec<&str> = p.assets().map(|(_, _, a)| a.name()).collect();
        assert!(names.contains(&"Renamed Asset 1"));
        assert!(names.contains(&"Appended Asset A"));
        assert!(names.contains(&"Appended Asset B"));
    });

    // Single undo restores exact prior bytes!
    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    assert_eq!(project_bytes(&wb, vcx), before);

    // Bad TSV (unknown rating) sets wb.note() and leaves project bytes untouched.
    let bad_tsv = "Valid Name\tNoSuchProperty_999\n";
    vcx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(bad_tsv.into())));
    wb.update_in(vcx, |w, window, cx| w.run("grid-paste", window, cx));
    assert!(vcx.read_entity(&wb, |w, _| w.note().is_some()));
    assert_eq!(project_bytes(&wb, vcx), before);
}

#[gpui::test]
fn add_delete_rows_problems_safety_no_asset_guard_and_filtered_add(cx: &mut TestAppContext) {
    // 1. Add row and delete rows on Damage, with single-step undo each.
    let (wb, tab, vcx) = open_table(Folder::Damage, None, cx);
    let before = project_bytes(&wb, vcx);
    let init_rows = vcx.read_entity(&tab, |t, _| t.visible_row_count());

    wb.update_in(vcx, |w, window, cx| w.run("grid-add-row", window, cx));
    vcx.read_entity(&tab, |t, _| {
        assert_eq!(t.visible_row_count(), init_rows + 1);
        assert!(t.is_editing());
    });
    tab.update_in(vcx, |t, window, cx| t.cancel_edit(window, cx));
    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    assert_eq!(project_bytes(&wb, vcx), before);

    select_cell(&tab, 0, 0, vcx);
    wb.update_in(vcx, |w, window, cx| w.run("grid-delete-rows", window, cx));
    assert_eq!(
        vcx.read_entity(&tab, |t, _| t.visible_row_count()),
        init_rows - 1
    );
    wb.update_in(vcx, |w, window, cx| w.run("undo", window, cx));
    assert_eq!(project_bytes(&wb, vcx), before);

    // 2. DATA LOSS REGRESSION TEST (Problems): add an unlinked asset so a lint problem with
    // row.target.is_some() exists, select it in Problems, and verify delete/add are refused.
    wb.update(vcx, |w, cx| {
        assert!(w.apply_edit(
            &Edit::AddAsset {
                id: AssetId::new_v7(),
                name: "Unlinked Problem Asset".into(),
                properties: vec![0],
            },
            cx,
        ));
    });
    let prob_key = ExplorerKey::Folder(Owner::Project, Folder::Problems);
    wb.update_in(vcx, |w, window, cx| w.open(prob_key, window, cx));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let prob_tab = vcx
        .read_entity(&wb, |w, _| w.table_tab(prob_key))
        .expect("Problems tab opens");
    let target_row = vcx.read_entity(&prob_tab, |t, _| {
        (0..t.visible_row_count())
            .find(|&r| t.displayed_row(r).and_then(|row| row.target).is_some())
            .expect("at least one lint problem with target must exist")
    });
    let before_prob = project_bytes(&wb, vcx);
    let counts_before = vcx.read_entity(&wb, |w, _| {
        let p = w.workspace().project();
        (
            p.asset_count(),
            p.damage_scenario_count(),
            p.threat_scenario_count(),
        )
    });
    select_cell(&prob_tab, target_row, 0, vcx);
    assert!(!prob_tab.update(vcx, |t, cx| t.delete_selected(cx)));
    wb.update_in(vcx, |w, window, cx| w.run("grid-delete-rows", window, cx));
    wb.update_in(vcx, |w, window, cx| w.run("grid-add-row", window, cx));
    assert_eq!(project_bytes(&wb, vcx), before_prob);
    let counts_after = vcx.read_entity(&wb, |w, _| {
        let p = w.workspace().project();
        (
            p.asset_count(),
            p.damage_scenario_count(),
            p.threat_scenario_count(),
        )
    });
    assert_eq!(counts_after, counts_before);

    // 3. Filtered-out add_row test: filter hides the newly added row, so is_editing() stays false.
    let dmg_key = ExplorerKey::Folder(Owner::Project, Folder::Damage);
    wb.update_in(vcx, |w, window, cx| w.open(dmg_key, window, cx));
    let dmg_before = vcx.read_entity(&wb, |w, _| w.workspace().project().damage_scenario_count());
    tab.update(vcx, |t, cx| {
        t.set_column_filter_query("name", "zzzz_nomatch", cx)
    });
    assert_eq!(vcx.read_entity(&tab, |t, _| t.visible_row_count()), 0);
    wb.update_in(vcx, |w, window, cx| w.run("grid-add-row", window, cx));
    vcx.read_entity(&wb, |w, cx| {
        assert_eq!(
            w.workspace().project().damage_scenario_count(),
            dmg_before + 1
        );
        assert!(!tab.read(cx).is_editing());
    });

    // 4. NoAssetForThreat guard test on a fresh empty project (0 assets).
    let empty_proj = Project::new(vcx.read_entity(&wb, |w, _| {
        std::sync::Arc::clone(w.workspace().project().methodology())
    }));
    wb.update_in(vcx, |w, window, cx| {
        w.show_project(empty_proj, "Empty", None, window, cx);
        w.open(
            ExplorerKey::Folder(Owner::Project, Folder::Threats),
            window,
            cx,
        );
    });
    let empty_bytes = project_bytes(&wb, vcx);
    wb.update_in(vcx, |w, window, cx| w.run("grid-add-row", window, cx));
    let note = vcx
        .read_entity(&wb, |w, _| w.note().map(str::to_owned))
        .expect("NoAssetForThreat must set status note");
    assert!(
        note.to_lowercase().contains("asset"),
        "expected asset mention in note: {note}"
    );
    assert_eq!(project_bytes(&wb, vcx), empty_bytes);
}

#[gpui::test]
fn simulate_keystrokes_in_grid_context_and_input_isolation(cx: &mut TestAppContext) {
    let (wb, tab, vcx) = open_table(Folder::Damage, None, cx);
    let focus = vcx.read_entity(&tab, |t, cx| t.focus_handle(cx));
    vcx.update(|window, cx| {
        window.focus(&focus, cx);
        window.draw(cx).clear(cx);
    });
    let init_count = vcx.read_entity(&wb, |w, _| w.workspace().project().damage_scenario_count());
    let add_keys = command("grid-add-row")
        .unwrap()
        .keys_for(cfg!(target_os = "macos"))
        .unwrap();
    let del_keys = command("grid-delete-rows")
        .unwrap()
        .keys_for(cfg!(target_os = "macos"))
        .unwrap();

    vcx.simulate_keystrokes(&add_keys);
    vcx.run_until_parked();
    vcx.read_entity(&wb, |w, cx| {
        assert_eq!(
            w.workspace().project().damage_scenario_count(),
            init_count + 1
        );
        assert!(tab.read(cx).is_editing());
    });

    // While is_editing() == true, sending grid-add-row and grid-delete-rows shortcuts
    // must NOT add or delete any rows (Grid key context is inactive inside inline InputState).
    vcx.simulate_keystrokes(&add_keys);
    vcx.run_until_parked();
    vcx.simulate_keystrokes(&del_keys);
    vcx.run_until_parked();
    vcx.read_entity(&wb, |w, cx| {
        assert_eq!(
            w.workspace().project().damage_scenario_count(),
            init_count + 1
        );
        assert!(tab.read(cx).is_editing());
    });
}

#[gpui::test]
fn read_only_blocks_grid_mutating_commands_but_allows_copy(cx: &mut TestAppContext) {
    let (wb, tab, vcx) = open_table(Folder::Assets, None, cx);
    wb.update(vcx, |w, cx| {
        assert!(w.apply_edit(
            &Edit::AddAsset {
                id: AssetId::new_v7(),
                name: "Second Asset".into(),
                properties: vec![0],
            },
            cx,
        ));
        w.set_read_only(Some("Read-only".into()), cx);
    });
    let before = project_bytes(&wb, vcx);
    let name_col = col_index(&tab, "name", vcx);
    tab.update(vcx, |t, cx| {
        let mut sel = GridSelection::single(0, name_col);
        sel.extend_to(1, name_col);
        t.set_selection(Some(sel), cx);
    });
    vcx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("Pasted Name\n".into())));

    for cmd in [
        "grid-fill-down",
        "grid-paste",
        "grid-add-row",
        "grid-delete-rows",
    ] {
        wb.update_in(vcx, |w, window, cx| w.run(cmd, window, cx));
        assert_eq!(project_bytes(&wb, vcx), before, "{cmd} must be refused");
    }

    // grid-copy is allowed in read-only mode.
    wb.update_in(vcx, |w, window, cx| w.run("grid-copy", window, cx));
    let clip = vcx
        .update(|_, cx| cx.read_from_clipboard().and_then(|i| i.text()))
        .expect("clipboard text");
    assert!(clip.contains("Second Asset"));
    assert_eq!(project_bytes(&wb, vcx), before);
}
