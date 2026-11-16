//! The systems table adds up on random projects (120d).

use std::sync::Arc;

use aegis_client::combined::systems_overview;
use aegis_client::commands::Edit;
use aegis_client::pages::{overview_in, risk_rows, vocabulary};
use aegis_client::state::PageData;
use aegis_client::systems::{SystemView, filter_page};
use aegis_client::view::Page;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{AssetId, ComponentId, DamageId, ElementId, Methodology, ThreatId};
use proptest::prelude::*;

/// One generated threat: an optional feasibility position.
type ThreatSpec = Option<usize>;
/// One generated asset with its threats.
type AssetSpec = Vec<ThreatSpec>;
/// One generated top-level component: system rank (`None` for no system),
/// whether it has a child, and its assets.
type ComponentSpec = (Option<usize>, bool, Vec<AssetSpec>);

/// A generated project: systems, components, and a view mask.
#[derive(Clone, Debug)]
pub struct Spec {
    /// Number of systems (1 to 4).
    pub systems: usize,
    /// 1 to 8 top-level components.
    pub components: Vec<ComponentSpec>,
    /// Bit mask over the systems; 0 means all.
    pub mask: u8,
}

/// The generator, for reuse by other tests.
pub fn spec() -> impl Strategy<Value = Spec> {
    let threat = prop::option::of(0usize..16);
    let asset = prop::collection::vec(threat, 0..=2);
    let component = (
        prop::option::of(0usize..4),
        any::<bool>(),
        prop::collection::vec(asset, 0..=2),
    );
    (
        1usize..=4,
        prop::collection::vec(component, 1..=8),
        any::<u8>(),
    )
        .prop_map(|(systems, components, mask)| Spec {
            systems,
            components,
            mask,
        })
}

fn run(ws: &mut Workspace, edit: &Edit) {
    let ops = edit.to_ops(ws.project(), "mira", 1).expect("builds");
    ws.apply(ops, Origin::Edit).expect("applies");
}

/// Builds the project of a spec and the view it asks for.
pub fn build(spec: &Spec) -> (Workspace, SystemView) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let vocab = vocabulary(&m);
    let mut ws = Workspace::empty(Arc::clone(&m));
    let damage = DamageId::new_v7();
    run(
        &mut ws,
        &Edit::AddDamage {
            id: damage,
            name: "Harm".into(),
        },
    );
    run(
        &mut ws,
        &Edit::SetImpact {
            damage,
            category: 0,
            level: Some(vocab.impact_categories[0].levels.len() - 1),
        },
    );
    let mut systems: Vec<ElementId> = Vec::new();
    for i in 0..spec.systems {
        let id = ElementId::new_v7();
        run(
            &mut ws,
            &Edit::AddSystem {
                id,
                name: format!("S{i}"),
            },
        );
        systems.push(id);
    }
    for (n, (rank, child, assets)) in spec.components.iter().enumerate() {
        let id = ComponentId::new_v7();
        run(
            &mut ws,
            &Edit::AddComponent {
                id,
                name: format!("C{n}"),
                parent: None,
            },
        );
        if let Some(system) = rank.and_then(|r| systems.get(r % spec.systems)) {
            run(
                &mut ws,
                &Edit::MoveToSystem {
                    components: vec![id],
                    system: Some(*system),
                },
            );
        }
        if *child {
            run(
                &mut ws,
                &Edit::AddComponent {
                    id: ComponentId::new_v7(),
                    name: format!("C{n}.1"),
                    parent: Some(id),
                },
            );
        }
        for threats in assets {
            let asset = AssetId::new_v7();
            run(
                &mut ws,
                &Edit::AddAssetBound {
                    id: asset,
                    name: "Data".into(),
                    properties: (0..6).collect(),
                    component: id,
                },
            );
            for level in threats {
                let threat = ThreatId::new_v7();
                run(
                    &mut ws,
                    &Edit::AddThreat {
                        id: threat,
                        name: "Attack".into(),
                        asset,
                        category: 0,
                    },
                );
                run(
                    &mut ws,
                    &Edit::LinkDamage {
                        threat,
                        damage,
                        linked: true,
                    },
                );
                let level = level.map(|l| l % vocab.feasibility.len());
                run(&mut ws, &Edit::SetThreatFeasibility { threat, level });
            }
        }
    }
    let view = SystemView::of(
        systems
            .iter()
            .enumerate()
            .filter(|(i, _)| spec.mask & (1 << i) != 0)
            .map(|(_, s)| *s),
    );
    (ws, view)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn rows_add_up(spec in spec()) {
        let (ws, view) = build(&spec);
        let p = ws.project();
        let m = p.methodology();
        let o = systems_overview(&ws, &view).expect("has systems");
        prop_assert_eq!(o.rows.iter().map(|r| r.components).sum::<usize>(), o.total.components);
        prop_assert_eq!(o.rows.iter().map(|r| r.assets).sum::<usize>(), o.total.assets);
        prop_assert_eq!(o.rows.iter().map(|r| r.threats).sum::<usize>(), o.total.threats);
        if view.is_all() {
            prop_assert_eq!(o.total.components, p.component_count());
            prop_assert_eq!(o.total.assets, p.asset_count());
            prop_assert_eq!(o.total.threats, p.threat_scenario_count());
        }
        let uses = p.system_use();
        for r in &o.rows {
            let mut best: Option<usize> = None;
            for (idx, id, _) in p.threat_scenarios() {
                if uses.ids(uses.threat(idx)).next() != r.system {
                    continue;
                }
                let level = p.evaluate_residual_risk(id).ok().and_then(|r| r.max_risk());
                let pos = level.and_then(|l| m.risk_levels().position(|(x, _, _)| x == l));
                best = best.max(pos);
            }
            let want = best.and_then(|b| m.risk_levels().nth(b)).map(|(_, _, l)| l.to_owned());
            let got = r.highest_residual.as_ref().map(|v| v.label.clone());
            prop_assert_eq!(got, want);
        }
    }

    #[test]
    fn a_risk_number_is_the_same_in_every_view(spec in spec()) {
        let (ws, view) = build(&spec);
        let p = ws.project();
        let whole = risk_rows(&ws);
        let mut d = PageData { risks: whole.clone(), ..Default::default() };
        filter_page(p, &view, Page::Risks, &mut d);
        for row in &d.risks {
            let same = whole.iter().find(|r| r.id == row.id);
            prop_assert_eq!(Some(row), same);
        }
        for row in &overview_in(&ws, &view).top {
            let same = whole.iter().find(|r| r.id == row.id);
            prop_assert_eq!(Some(row), same);
        }
        let normalized = view.normalized(p);
        let uses = p.system_use();
        let v = uses.set_of(normalized.systems());
        for (idx, id, _) in p.threat_scenarios() {
            if normalized.is_all() || uses.threat(idx).shows_in(v) {
                prop_assert!(d.risks.iter().any(|r| r.id == id));
            }
        }
    }
}
