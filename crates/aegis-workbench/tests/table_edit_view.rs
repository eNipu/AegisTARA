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
use gpui::{AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext};

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
