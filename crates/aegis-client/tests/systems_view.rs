//! The system view: toggling, the menu and chip text, the palette entries
//! and the remembered view in the settings (119g).

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::settings::{MAX_REMEMBERED_VIEWS, Settings};
use aegis_client::systems::{SystemView, reveal, view_commands, view_menu};
use aegis_client::view::Selection;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    ComponentId, ControlId, ElementId, ElementKind, ItemRef, Methodology, Op, Provenance,
};

fn workspace() -> Workspace {
    Workspace::empty(Arc::new(Methodology::iso21434_r155().expect("builtin")))
}

fn run(ws: &mut Workspace, edit: &Edit) {
    let ops = edit.to_ops(ws.project(), "mira", 1).expect("builds");
    ws.apply(ops, Origin::Edit).expect("applies");
}

fn add_system(ws: &mut Workspace, name: &str) -> ElementId {
    let id = ElementId::new_v7();
    run(
        ws,
        &Edit::AddSystem {
            id,
            name: name.into(),
        },
    );
    id
}

fn add_component(
    ws: &mut Workspace,
    name: &str,
    parent: Option<ComponentId>,
    system: Option<ElementId>,
) -> ComponentId {
    let id = ComponentId::new_v7();
    run(
        ws,
        &Edit::AddComponent {
            id,
            name: name.into(),
            parent,
        },
    );
    if system.is_some() {
        run(
            ws,
            &Edit::MoveToSystem {
                components: vec![id],
                system,
            },
        );
    }
    id
}

/// Brake (3 components, one a child), ADAS (2), Gateway (0), one in none.
fn fixture() -> (Workspace, [ElementId; 3]) {
    let mut ws = workspace();
    let brake = add_system(&mut ws, "Brake");
    let adas = add_system(&mut ws, "ADAS");
    let gateway = add_system(&mut ws, "Gateway");
    let top = add_component(&mut ws, "Caliper", None, Some(brake));
    add_component(&mut ws, "Pad", Some(top), None);
    add_component(&mut ws, "Pump", None, Some(brake));
    add_component(&mut ws, "Camera", None, Some(adas));
    add_component(&mut ws, "Radar", None, Some(adas));
    add_component(&mut ws, "Loose", None, None);
    (ws, [brake, adas, gateway])
}

#[test]
fn toggled_and_normalized() {
    let (mut ws, [brake, adas, _]) = fixture();
    let v = SystemView::all();
    assert!(v.is_all());
    let v = v.toggled(brake).toggled(adas);
    assert!(v.contains(brake) && v.contains(adas) && !v.is_all());
    let v = v.toggled(brake);
    assert_eq!(v, SystemView::only(adas));
    assert!(v.toggled(adas).is_all());
    assert_eq!(SystemView::of([adas, brake]).systems().count(), 2);

    let misuse = ElementId::new_v7();
    ws.apply(
        vec![Op::AddElement {
            id: misuse,
            kind: ElementKind::MisuseCase,
            name: "Spoof".into(),
            provenance: Provenance::human("mira", 1),
        }],
        Origin::Edit,
    )
    .expect("misuse case");
    let stale = ElementId::new_v7();
    let v = SystemView::of([brake, adas, misuse, stale]);
    assert_eq!(
        v.normalized(ws.project()),
        SystemView::of([brake, adas]),
        "a misuse case and an unknown ID are dropped"
    );
    run(&mut ws, &Edit::RemoveSystem { system: adas });
    assert_eq!(v.normalized(ws.project()), SystemView::only(brake));
}

#[test]
fn menu_counts_and_chip_text() {
    assert!(view_menu(workspace().project(), &SystemView::all()).is_none());
    let (ws, [brake, adas, gateway]) = fixture();
    let p = ws.project();
    let m = view_menu(p, &SystemView::all()).expect("menu");
    assert_eq!(m.total, 6);
    assert_eq!(m.unassigned, 1);
    assert!(m.all);
    assert_eq!(m.chip, "All systems");
    assert_eq!(m.chip_short, "All systems");
    let counts: Vec<(ElementId, usize)> = m.choices.iter().map(|c| (c.id, c.components)).collect();
    let mut want = vec![(brake, 3), (adas, 2), (gateway, 0)];
    want.sort();
    assert_eq!(counts, want);
    assert_eq!(
        m.choices.iter().map(|c| c.components).sum::<usize>() + m.unassigned,
        m.total
    );
    assert!(m.choices.iter().all(|c| !c.checked));

    // Names in ID order: Brake, ADAS, Gateway were created in that order.
    let one = view_menu(p, &SystemView::only(brake)).expect("menu");
    assert_eq!(
        (one.chip.as_str(), one.chip_short.as_str()),
        ("Brake", "Brake")
    );
    assert!(!one.all);
    assert!(one.choices.iter().any(|c| c.id == brake && c.checked));
    let two = view_menu(p, &SystemView::of([brake, adas])).expect("menu");
    assert_eq!(two.chip, "Brake + ADAS");
    assert_eq!(two.chip_short, "Brake +1");
    let three = view_menu(p, &SystemView::of([brake, adas, gateway])).expect("menu");
    assert_eq!(three.chip, "Brake +2");
    assert_eq!(three.chip_short, "Brake +2");
}

#[test]
fn palette_entries() {
    assert!(view_commands(workspace().project(), &SystemView::all()).is_empty());
    let (ws, [brake, adas, gateway]) = fixture();
    let p = ws.project();
    let cmds = view_commands(p, &SystemView::all());
    let labels: Vec<&str> = cmds.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "View: All systems",
            "View: Brake",
            "View: ADAS",
            "View: Gateway"
        ]
    );
    assert!(cmds[0].view.is_all());
    assert_eq!(cmds[1].view, SystemView::only(brake));

    let cmds = view_commands(p, &SystemView::only(adas));
    let labels: Vec<&str> = cmds.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "View: All systems",
            "View: Brake",
            "View: ADAS",
            "View: Gateway",
            "View: add Brake",
            "View: add Gateway"
        ]
    );
    assert_eq!(cmds[4].view, SystemView::of([adas, brake]));
    assert_eq!(cmds[5].view, SystemView::of([adas, gateway]));
}

#[test]
fn remembered_view() {
    let a = ElementId::new_v7();
    let b = ElementId::new_v7();
    let key = Settings::view_key("mira", "https://x/p1");
    let mut s = Settings::default();
    assert!(s.system_view(&key).is_all());
    s.set_system_view(&key, &SystemView::of([a, b]));
    assert_eq!(s.system_view(&key), SystemView::of([a, b]));

    let dir = std::env::temp_dir().join(format!("aegis-view-{}", std::process::id()));
    let path = Settings::path_in(&dir);
    s.save(&path).expect("save");
    assert_eq!(
        Settings::load(&path).system_view(&key),
        SystemView::of([a, b])
    );

    // Two users of one project keep their own views.
    let other = Settings::view_key("ken", "https://x/p1");
    s.set_system_view(&other, &SystemView::only(a));
    assert_eq!(s.system_view(&key), SystemView::of([a, b]));
    assert_eq!(s.system_view(&other), SystemView::only(a));

    // All removes the entry.
    s.set_system_view(&key, &SystemView::all());
    assert!(s.system_view(&key).is_all());
    assert_eq!(s.system_views.len(), 1);

    // The oldest project is dropped first.
    let mut s = Settings::default();
    for i in 0..=MAX_REMEMBERED_VIEWS {
        s.set_system_view(
            &Settings::view_key("mira", &format!("p{i}")),
            &SystemView::only(a),
        );
    }
    assert_eq!(s.system_views.len(), MAX_REMEMBERED_VIEWS);
    assert!(s.system_view(&Settings::view_key("mira", "p0")).is_all());
    assert_eq!(
        s.system_view(&Settings::view_key("mira", "p1")),
        SystemView::only(a)
    );

    // A bad ID is skipped; a file from before this slice still loads.
    let json = format!(r#"{{"system_views":[{{"key":"k","systems":["nope","{a}"]}}]}}"#);
    std::fs::write(&path, json).expect("write");
    assert_eq!(Settings::load(&path).system_view("k"), SystemView::only(a));
    std::fs::write(&path, r#"{"server_url":"x"}"#).expect("write");
    let old = Settings::load(&path);
    assert_eq!(old.server_url, "x");
    assert!(old.system_views.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- filtering rows (119h) ----

mod filtering {
    use std::collections::BTreeSet;

    use aegis_client::pages;
    use aegis_client::state::PageData;
    use aegis_client::systems::{SystemView, filter_page, filter_threats};
    use aegis_client::view;
    use aegis_client::view::Page;
    use aegis_model::{
        AssetId, ClaimId, ControlId, DamageId, GoalId, InterfaceId, ItemRef, Project, ThreatId,
        TreeId,
    };

    use super::*;

    struct Fx {
        ws: Workspace,
        brake: ElementId,
        adas: ElementId,
    }

    fn fixture() -> Fx {
        let mut ws = workspace();
        let brake = add_system(&mut ws, "Brake");
        let adas = add_system(&mut ws, "ADAS");
        let damage = DamageId::new_v7();
        run(
            &mut ws,
            &Edit::AddDamage {
                id: damage,
                name: "Harm".into(),
            },
        );
        for (name, system) in [
            ("Caliper", Some(brake)),
            ("Camera", Some(adas)),
            ("Loose", None),
        ] {
            let comp = add_component(&mut ws, name, None, system);
            run(
                &mut ws,
                &Edit::AddInterface {
                    id: InterfaceId::new_v7(),
                    name: format!("{name} port"),
                    component: comp,
                    kind: 0,
                },
            );
            let asset = AssetId::new_v7();
            run(
                &mut ws,
                &Edit::AddAssetBound {
                    id: asset,
                    name: format!("{name} data"),
                    properties: (0..6).collect(),
                    component: comp,
                },
            );
            let threat = ThreatId::new_v7();
            run(
                &mut ws,
                &Edit::AddThreat {
                    id: threat,
                    name: format!("{name} attack"),
                    asset,
                    category: 0,
                },
            );
            let (control, goal, claim) = (ControlId::new_v7(), GoalId::new_v7(), ClaimId::new_v7());
            run(
                &mut ws,
                &Edit::AddControl {
                    id: control,
                    name: format!("{name} control"),
                },
            );
            run(
                &mut ws,
                &Edit::ApplyControl {
                    control,
                    target: ItemRef::Component(comp),
                },
            );
            run(
                &mut ws,
                &Edit::AddGoal {
                    id: goal,
                    name: format!("{name} goal"),
                },
            );
            run(
                &mut ws,
                &Edit::SetThreatGoal {
                    threat,
                    goal: Some(goal),
                },
            );
            run(
                &mut ws,
                &Edit::AddClaim {
                    id: claim,
                    name: format!("{name} claim"),
                },
            );
            run(
                &mut ws,
                &Edit::SetThreatClaim {
                    threat,
                    claim: Some(claim),
                },
            );
            if system.is_some() {
                run(
                    &mut ws,
                    &Edit::LinkDamage {
                        threat,
                        damage,
                        linked: true,
                    },
                );
            }
            if system == Some(brake) {
                run(
                    &mut ws,
                    &Edit::NewTree {
                        id: TreeId::new_v7(),
                        root: "Goal".into(),
                        threat: Some(threat),
                    },
                );
            }
        }
        Fx { ws, brake, adas }
    }

    /// Every table row of the pages the view filters, as `page:id` text.
    fn shown(ws: &Workspace, view: &SystemView) -> BTreeSet<String> {
        let project: &Project = ws.project();
        let mut out = BTreeSet::new();
        for page in [
            Page::Item,
            Page::Assets,
            Page::Damage,
            Page::Risks,
            Page::Trees,
            Page::Controls,
            Page::Goals,
            Page::Problems,
        ] {
            let mut d = PageData::default();
            match page {
                Page::Item => {
                    d.components = pages::component_rows(ws);
                    d.interfaces = pages::interface_rows(ws);
                }
                Page::Assets => d.assets = pages::asset_rows(ws),
                Page::Damage => d.damages = pages::damage_rows(ws),
                Page::Risks => d.risks = pages::risk_rows(ws),
                Page::Trees => d.trees = pages::tree_rows(ws),
                Page::Controls => d.controls = pages::control_rows(ws),
                Page::Goals => {
                    d.goals = pages::goal_rows(ws);
                    d.claims = pages::claim_rows(ws);
                }
                _ => d.problems = pages::problems(ws),
            }
            filter_page(project, view, page, &mut d);
            let mut put = |kind: &str, id: String| {
                out.insert(format!("{kind}:{id}"));
            };
            d.components
                .iter()
                .for_each(|r| put("c", format!("{:?}", r.id)));
            d.interfaces
                .iter()
                .for_each(|r| put("i", format!("{:?}", r.id)));
            d.assets
                .iter()
                .for_each(|r| put("a", format!("{:?}", r.id)));
            d.damages
                .iter()
                .for_each(|r| put("d", format!("{:?}", r.id)));
            d.risks.iter().for_each(|r| put("r", format!("{:?}", r.id)));
            d.trees.iter().for_each(|r| put("t", format!("{:?}", r.id)));
            d.controls
                .iter()
                .for_each(|r| put("k", format!("{:?}", r.id)));
            d.goals.iter().for_each(|r| put("g", format!("{:?}", r.id)));
            d.claims
                .iter()
                .for_each(|r| put("m", format!("{:?}", r.id)));
            d.problems
                .iter()
                .for_each(|r| put("p", format!("{}|{}", r.visual_id, r.text)));
        }
        let put_threat = |out: &mut BTreeSet<String>, id: ThreatId| {
            out.insert(format!("th:{id:?}"));
        };
        let mut rows = view::threat_rows(ws);
        filter_threats(project, view, &mut rows);
        rows.iter().for_each(|r| put_threat(&mut out, r.id));
        out
    }

    #[test]
    fn a_view_keeps_its_rows_and_the_rows_of_no_system() {
        let fx = fixture();
        let everything = shown(&fx.ws, &SystemView::all());
        let count =
            |set: &BTreeSet<String>, kind: &str| set.iter().filter(|r| r.starts_with(kind)).count();
        // Per kind: components, interfaces, assets, damages, risks, trees,
        // controls, goals, claims and threats (the Threats list).
        let kinds = ["c:", "i:", "a:", "d:", "r:", "t:", "k:", "g:", "m:", "th:"];
        let all_counts = [3, 3, 3, 1, 3, 1, 3, 3, 3, 3];
        // Brake: its own rows and those of "Loose"; the shared damage; its tree.
        let brake_counts = [2, 2, 2, 1, 2, 1, 2, 2, 2, 2];
        let brake = shown(&fx.ws, &SystemView::only(fx.brake));
        let adas = shown(&fx.ws, &SystemView::only(fx.adas));
        for (i, kind) in kinds.iter().enumerate() {
            assert_eq!(count(&everything, kind), all_counts[i], "all {kind}");
            assert_eq!(count(&brake, kind), brake_counts[i], "brake {kind}");
        }
        assert!(count(&everything, "p:") > count(&brake, "p:"));
        assert!(count(&adas, "t:") == 0);
        assert_ne!(brake, adas);
        assert!(brake.is_subset(&everything) && adas.is_subset(&everything));
        let both = shown(&fx.ws, &SystemView::of([fx.brake, fx.adas]));
        assert_eq!(both, brake.union(&adas).cloned().collect());
        assert_eq!(both, everything);
    }

    #[test]
    fn a_removed_system_behaves_as_all() {
        let fx = fixture();
        let gone = ElementId::new_v7();
        assert_eq!(
            shown(&fx.ws, &SystemView::only(gone)),
            shown(&fx.ws, &SystemView::all())
        );
    }

    #[test]
    fn filtering_changes_nothing_in_the_workspace() {
        let fx = fixture();
        let before = fx.ws.project().clone();
        let (pending, undo) = (fx.ws.pending_batches(), fx.ws.can_undo());
        let _ = shown(&fx.ws, &SystemView::only(fx.brake));
        assert_eq!(*fx.ws.project(), before);
        assert_eq!(fx.ws.pending_batches(), pending);
        assert_eq!(fx.ws.can_undo(), undo);
    }
}

fn component_named(ws: &Workspace, name: &str) -> ComponentId {
    ws.project()
        .components()
        .find(|(_, _, c)| c.name() == name)
        .map(|(_, id, _)| id)
        .expect("component")
}

#[test]
fn reveal_adds_the_first_system_of_a_hidden_element() {
    let (mut ws, [brake, adas, gateway]) = fixture();
    let p = ws.project();
    let camera = Selection::Component(component_named(&ws, "Camera"));
    let (view, name) = reveal(p, &SystemView::only(brake), camera).expect("grows");
    assert!(view.contains(brake) && view.contains(adas) && !view.contains(gateway));
    assert_eq!(name, "ADAS");

    let caliper = Selection::Component(component_named(&ws, "Caliper"));
    let loose = Selection::Component(component_named(&ws, "Loose"));
    assert!(reveal(p, &SystemView::only(brake), caliper).is_none());
    assert!(reveal(p, &SystemView::only(brake), loose).is_none());
    assert!(reveal(p, &SystemView::all(), camera).is_none());
    let gone = Selection::Component(ComponentId::new_v7());
    assert!(reveal(p, &SystemView::only(brake), gone).is_none());

    // A control applied to Camera (ADAS) and a Gateway component.
    let gate = add_component(&mut ws, "Gate", None, Some(gateway));
    let control = ControlId::new_v7();
    run(
        &mut ws,
        &Edit::AddControl {
            id: control,
            name: "Signing".into(),
        },
    );
    for target in [component_named(&ws, "Camera"), gate] {
        run(
            &mut ws,
            &Edit::ApplyControl {
                control,
                target: ItemRef::Component(target),
            },
        );
    }
    let first = adas.min(gateway);
    let (view, name) = reveal(
        ws.project(),
        &SystemView::only(brake),
        Selection::Control(control),
    )
    .expect("grows");
    assert!(view.contains(first) && view.contains(brake));
    assert_eq!(view.systems().count(), 2, "only one system is added");
    let want = if first == adas { "ADAS" } else { "Gateway" };
    assert_eq!(name, want);
}
