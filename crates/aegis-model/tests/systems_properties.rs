//! Derived systems (`Project::system_use`, 119c): every element gets the
//! systems it belongs to or is used by, from the links the project already
//! has.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use aegis_model::{
    AttackTree, ComponentId, ElementId, ElementKind, GateKind, ItemRef, Methodology, Node, Op,
    Project, Provenance, SystemSet, SystemUse,
};
use common::{
    Gen, Seed, asset_id, builtin, component_id, control_id, damage_id, element_id, node_id, replay,
    second, seeds, threat_id, tree_id,
};
use proptest::prelude::*;

fn who() -> Provenance {
    Provenance::human("alice", 1)
}

fn ok(p: &mut Project, op: Op) {
    if let Err(e) = p.apply(op.clone()) {
        panic!("{op:?} failed: {e}");
    }
}

/// Every derived set of every element, in kind and index order.
fn every_set(p: &Project, u: &SystemUse) -> Vec<SystemSet> {
    let mut out = Vec::new();
    out.extend(p.components().map(|(i, _, _)| u.component(i)));
    out.extend(p.interfaces().map(|(i, _, _)| u.interface(i)));
    out.extend(p.data_flows().map(|(i, _, _)| u.data_flow(i)));
    out.extend(p.trust_boundaries().map(|(i, _, _)| u.trust_boundary(i)));
    out.extend(p.assets().map(|(i, _, _)| u.asset(i)));
    out.extend(p.threat_scenarios().map(|(i, _, _)| u.threat(i)));
    out.extend(p.attack_trees().map(|(i, _, _)| u.tree(i)));
    out.extend(p.damage_scenarios().map(|(i, _, _)| u.damage(i)));
    out.extend(p.security_controls().map(|(i, _, _)| u.control(i)));
    out.extend(p.cybersecurity_goals().map(|(i, _, _)| u.goal(i)));
    out.extend(p.cybersecurity_claims().map(|(i, _, _)| u.claim(i)));
    out
}

/// The systems picked by the bits of `mask`, as a set.
fn subset(u: &SystemUse, mask: u64) -> SystemSet {
    u.set_of(
        u.systems()
            .iter()
            .enumerate()
            .filter(|(rank, _)| *rank < 64 && mask & (1 << rank) != 0)
            .map(|(_, id)| *id),
    )
}

/// The checks that hold for every project.
fn check(p: &Project, masks: (u64, u64)) -> Result<(), TestCaseError> {
    let u = p.system_use();
    let systems: Vec<ElementId> = p.elements(ElementKind::System).map(|(id, _)| id).collect();
    prop_assert_eq!(u.systems(), systems.as_slice());
    for (i, _, c) in p.components() {
        let set = u.component(i);
        prop_assert!(set.len() <= 1);
        prop_assert_eq!(set, u.set_of(p.component_system(i)));
        if let Some(parent) = c.parent() {
            prop_assert_eq!(set, u.component(parent));
        }
    }
    for (i, _, _) in p.interfaces() {
        prop_assert!(u.interface(i).len() <= 1);
    }
    for (i, _, _) in p.assets() {
        prop_assert!(u.asset(i).len() <= 1);
    }
    for (i, _, _) in p.threat_scenarios() {
        prop_assert!(u.threat(i).len() <= 1);
    }
    let (a, b) = (subset(&u, masks.0), subset(&u, masks.1));
    for set in every_set(p, &u) {
        prop_assert_eq!(set.shows_in(a.union(b)), set.shows_in(a) || set.shows_in(b));
        prop_assert_eq!(u.ids(set).count(), set.len());
        prop_assert_eq!(u.set_of(u.ids(set)), set);
        if set.is_empty() {
            prop_assert!(set.shows_in(SystemSet::EMPTY));
            prop_assert!(set.shows_in(a));
        }
        if systems.is_empty() {
            prop_assert!(set.is_empty());
        }
    }
    Ok(())
}

/// Applies the ops `seeds` turn into, keeping the ones that succeed, then
/// adds three systems and puts each top-level component in one of them or
/// in none, by the bits of `spread`.
fn build(m: &Arc<Methodology>, seeds: &[Seed], spread: u64) -> Project {
    let mut generator = Gen::new(Arc::clone(m));
    let mut p = Project::new(Arc::clone(m));
    for s in seeds {
        let op = generator.op(&p, s);
        let _ = p.apply(op);
    }
    // Two top-level components and four children of picked components, so
    // every project has a hierarchy.
    for n in 0..6u64 {
        let existing: Vec<ComponentId> = p.components().map(|(_, id, _)| id).collect();
        let parent = (n >= 2)
            .then(|| usize::try_from((spread >> (8 * n)) & 0xff).unwrap_or(0) % existing.len())
            .and_then(|i| existing.get(i).copied());
        // A pick at the depth limit is refused; the project stays as it was.
        let _ = p.apply(Op::AddComponent {
            id: component_id(910_000 + n),
            name: format!("Part {n}").into(),
            parent,
            provenance: who(),
        });
    }
    let systems: Vec<ElementId> = (0..3).map(|n| element_id(900_000 + n)).collect();
    for (n, id) in systems.iter().enumerate() {
        ok(
            &mut p,
            Op::AddElement {
                id: *id,
                kind: ElementKind::System,
                name: format!("System {n}").into(),
                provenance: who(),
            },
        );
    }
    let top: Vec<ComponentId> = p
        .components()
        .filter(|(_, _, c)| c.parent().is_none())
        .map(|(_, id, _)| id)
        .collect();
    for (n, id) in top.into_iter().enumerate() {
        let pick = (spread >> ((2 * n) % 64)) & 3;
        let system = usize::try_from(pick)
            .ok()
            .and_then(|k| systems.get(k))
            .copied();
        ok(
            &mut p,
            Op::SetComponentSystem {
                id,
                system,
                provenance: who(),
            },
        );
    }
    p
}

fn methodologies() -> [Arc<Methodology>; 2] {
    [builtin(), second()]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The rules hold after every op of a random op sequence and every undo.
    #[test]
    fn rules_hold_while_replaying(s in seeds(), a in any::<u64>(), b in any::<u64>()) {
        for m in methodologies() {
            replay(&m, &s, |p| check(p, (a, b)))?;
        }
    }

    /// The rules hold on random projects with systems spread over the
    /// top-level components.
    #[test]
    fn rules_hold_with_systems(
        s in seeds(), spread in any::<u64>(), a in any::<u64>(), b in any::<u64>(),
    ) {
        for m in methodologies() {
            let p = build(&m, &s, spread);
            check(&p, (a, b))?;
        }
    }

    /// Taking every component out of a system and removing it changes
    /// only those components and the system, no risk, and undoes exactly.
    #[test]
    fn removing_a_system_touches_only_its_components(
        s in seeds(), spread in any::<u64>(), pick in 0usize..3,
    ) {
        for m in methodologies() {
            let before = build(&m, &s, spread);
            let systems: Vec<ElementId> =
                before.elements(ElementKind::System).map(|(id, _)| id).collect();
            let Some(&system) = systems.get(pick % systems.len()) else {
                continue;
            };
            let members: BTreeSet<ComponentId> = before
                .components()
                .filter(|(_, _, c)| c.system() == Some(system))
                .map(|(_, id, _)| id)
                .collect();
            let mut after = before.clone();
            let mut undo = Vec::new();
            for id in &members {
                let op = Op::SetComponentSystem { id: *id, system: None, provenance: who() };
                undo.push(after.apply(op).map_err(|e| TestCaseError::fail(e.to_string()))?);
            }
            undo.push(
                after
                    .apply(Op::RemoveElement { id: system })
                    .map_err(|e| TestCaseError::fail(e.to_string()))?,
            );
            let diff = before.diff(&after).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let modified: BTreeSet<ComponentId> =
                diff.components().modified().iter().copied().collect();
            prop_assert_eq!(&modified, &members);
            prop_assert!(diff.components().added().is_empty());
            prop_assert!(diff.components().removed().is_empty());
            prop_assert_eq!(diff.elements().removed(), &[system][..]);
            prop_assert!(diff.elements().added().is_empty());
            prop_assert!(diff.elements().modified().is_empty());
            for empty in [
                diff.interfaces().is_empty(),
                diff.data_flows().is_empty(),
                diff.trust_boundaries().is_empty(),
                diff.assets().is_empty(),
                diff.damage_scenarios().is_empty(),
                diff.threat_scenarios().is_empty(),
                diff.attack_trees().is_empty(),
                diff.security_controls().is_empty(),
                diff.cybersecurity_goals().is_empty(),
                diff.cybersecurity_claims().is_empty(),
                diff.risk_deltas().is_empty(),
            ] {
                prop_assert!(empty);
            }
            check(&after, (u64::MAX, 0))?;
            while let Some(inverse) = undo.pop() {
                after.apply(inverse).map_err(|e| TestCaseError::fail(e.to_string()))?;
            }
            prop_assert_eq!(&after, &before);
        }
    }
}

const BRAKE: u64 = 1;
const GATEWAY: u64 = 2;

/// A project with the systems Brake and Gateway, a top-level component in
/// each (`component_id(10)`, `component_id(20)`), and an asset bound to
/// each (`asset_id(11)`, `asset_id(21)`).
fn two_systems() -> Project {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let props = m.all_properties();
    for (sys, name) in [(BRAKE, "Brake"), (GATEWAY, "Gateway")] {
        let comp = component_id(sys * 10);
        let asset = asset_id(sys * 10 + 1);
        for op in [
            Op::AddElement {
                id: element_id(sys),
                kind: ElementKind::System,
                name: name.into(),
                provenance: who(),
            },
            Op::AddComponent {
                id: comp,
                name: format!("{name} ECU").into(),
                parent: None,
                provenance: who(),
            },
            Op::SetComponentSystem {
                id: comp,
                system: Some(element_id(sys)),
                provenance: who(),
            },
            Op::AddAsset {
                id: asset,
                name: format!("{name} firmware").into(),
                properties: props,
                provenance: who(),
            },
            Op::SetAssetComponent {
                asset,
                component: Some(comp),
                provenance: who(),
            },
        ] {
            ok(&mut p, op);
        }
    }
    p
}

fn add_damage(p: &mut Project, n: u64) {
    ok(
        p,
        Op::AddDamageScenario {
            id: damage_id(n),
            name: "Vehicle cannot brake".into(),
            impacts: Vec::new(),
            provenance: who(),
        },
    );
}

fn add_threat(p: &mut Project, n: u64, asset: u64, damages: Vec<u64>) {
    let category = p.methodology().threat_categories().next();
    let Some(category) = category else {
        panic!("no threat category");
    };
    ok(
        p,
        Op::AddThreatScenario {
            id: threat_id(n),
            name: "Modify firmware".into(),
            asset: asset_id(asset),
            category,
            catalog_refs: Vec::new(),
            damages: damages.into_iter().map(damage_id).collect(),
            provenance: who(),
        },
    );
}

#[test]
fn threat_on_brake_asset_is_in_brake_with_its_tree_and_damage() {
    let mut p = two_systems();
    add_damage(&mut p, 100);
    add_threat(&mut p, 200, BRAKE * 10 + 1, vec![100]);
    let tree =
        AttackTree::new(node_id(300), Node::gate(GateKind::Or, "Flash firmware")).expect("tree");
    ok(
        &mut p,
        Op::AddAttackTree {
            role: Default::default(),
            id: tree_id(300),
            tree,
            step_feasibility: Vec::new(),
            provenance: who(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: threat_id(200),
            tree: Some(tree_id(300)),
            provenance: who(),
        },
    );
    let u = p.system_use();
    let brake = u.set_of([element_id(BRAKE)]);
    let threat = p.threat_scenario_idx(&threat_id(200)).expect("threat");
    let tree = p.attack_tree_idx(&tree_id(300)).expect("tree");
    let damage = p.damage_scenario_idx(&damage_id(100)).expect("damage");
    assert_eq!(u.threat(threat), brake);
    assert_eq!(u.tree(tree), brake);
    assert_eq!(u.damage(damage), brake);
    assert_eq!(u.ids(brake).collect::<Vec<_>>(), [element_id(BRAKE)]);
}

#[test]
fn damage_linked_from_two_systems_has_both() {
    let mut p = two_systems();
    add_damage(&mut p, 100);
    add_threat(&mut p, 200, BRAKE * 10 + 1, vec![100]);
    add_threat(&mut p, 201, GATEWAY * 10 + 1, vec![100]);
    let u = p.system_use();
    let damage = p.damage_scenario_idx(&damage_id(100)).expect("damage");
    assert_eq!(
        u.damage(damage),
        u.set_of([element_id(BRAKE), element_id(GATEWAY)])
    );
    assert_eq!(u.damage(damage).len(), 2);
}

#[test]
fn control_on_components_of_two_systems_has_both() {
    let mut p = two_systems();
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: control_id(400),
            name: "Secure boot".into(),
            provenance: who(),
        },
    );
    for sys in [BRAKE, GATEWAY] {
        ok(
            &mut p,
            Op::ApplyControl {
                control: control_id(400),
                target: ItemRef::Component(component_id(sys * 10)),
                provenance: who(),
            },
        );
    }
    let u = p.system_use();
    let control = p.security_control_idx(&control_id(400)).expect("control");
    assert_eq!(
        u.control(control),
        u.set_of([element_id(BRAKE), element_id(GATEWAY)])
    );
    assert_eq!(u.control(control).len(), 2);
}
