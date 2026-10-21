//! Shared workload for the save/load benches (`benches/store.rs` for time,
//! `benches/store_memory.rs` for heap): a project shaped like the
//! `typical` reference (300 assets, 2,000 damage scenarios, 2,000 threats
//! with 2 catalog refs and 2 damage links each) over the built-in
//! methodology, and a 10,000-op log. Deterministic: fixed IDs, no
//! randomness.

#![forbid(unsafe_code)]

use std::sync::Arc;

use aegis_model::{AssetId, DamageId, Methodology, Op, Project, ThreatId};

/// Assets in the `typical` reference project.
pub const ASSETS: u64 = 300;
/// Damage scenarios in the workload.
pub const DAMAGES: u64 = 2_000;
/// Threat scenarios in the `typical` reference project.
pub const THREATS: u64 = 2_000;
/// Ops in the log workload.
pub const LOG_OPS: usize = 10_000;

/// A `UUIDv7` from a counter (the same scheme as the test kit).
fn v7(n: u64) -> uuid::Uuid {
    let mut bytes = [0u8; 10];
    if let Some(tail) = bytes.get_mut(2..) {
        tail.copy_from_slice(&n.to_be_bytes());
    }
    uuid::Builder::from_unix_timestamp_millis(1_760_000_000_000 + n, &bytes).into_uuid()
}

fn asset(n: u64) -> AssetId {
    AssetId::from_uuid(v7(n)).unwrap_or_else(|_| AssetId::new_v7())
}

fn damage(n: u64) -> DamageId {
    DamageId::from_uuid(v7(10_000 + n)).unwrap_or_else(|_| DamageId::new_v7())
}

fn threat(n: u64) -> ThreatId {
    ThreatId::from_uuid(v7(20_000 + n)).unwrap_or_else(|_| ThreatId::new_v7())
}

/// The built-in methodology.
pub fn methodology() -> Arc<Methodology> {
    match Methodology::iso21434_r155() {
        Ok(m) => Arc::new(m),
        Err(e) => panic!("built-in methodology: {e}"),
    }
}

/// The ops that build the typical project, in dependency order, followed
/// by renames and rating changes up to [`LOG_OPS`] ops.
pub fn ops(m: &Methodology) -> Vec<Op> {
    let cats: Vec<_> = m.impact_categories().collect();
    let threat_cats: Vec<_> = m.threat_categories().collect();
    let scheme = m.catalog_schemes().next();
    let mut ops = Vec::with_capacity(LOG_OPS);
    for i in 0..ASSETS {
        ops.push(Op::AddAsset {
            id: asset(i),
            name: format!("Electronic control unit {i}").into(),
            properties: m.all_properties(),
        });
    }
    for i in 0..DAMAGES {
        let pick = |k: u64| cats[usize::try_from((i + k) % cats.len() as u64).unwrap_or(0)];
        let (a, b) = (pick(0), pick(1));
        let level = |c, k: u64| m.levels(c).nth(usize::try_from((i + k) % 4).unwrap_or(0));
        let impacts = [(a, level(a, 0)), (b, level(b, 1))]
            .into_iter()
            .filter_map(|(c, l)| l.map(|l| (c, l)))
            .collect();
        ops.push(Op::AddDamageScenario {
            id: damage(i),
            name: format!("Damage scenario {i}: loss of a vehicle function").into(),
            impacts,
        });
    }
    for i in 0..THREATS {
        let refs = scheme
            .map(|s| {
                let n = i % 32 + 1;
                [format!("{n}"), format!("{n}.{}", i % 99 + 1)]
                    .iter()
                    .filter_map(|t| m.parse_catalog_ref(s, t).ok())
                    .collect()
            })
            .unwrap_or_default();
        ops.push(Op::AddThreatScenario {
            id: threat(i),
            name: format!("Threat scenario {i}: attacker manipulates messages").into(),
            asset: asset(i % ASSETS),
            category: threat_cats[usize::try_from(i % threat_cats.len() as u64).unwrap_or(0)],
            catalog_refs: refs,
            damages: vec![damage(i % DAMAGES), damage((i + 1) % DAMAGES)],
        });
    }
    let mut i = 0u64;
    while ops.len() < LOG_OPS {
        ops.push(if i % 2 == 0 {
            Op::RenameThreatScenario {
                id: threat(i % THREATS),
                name: format!("Threat scenario {i}, revised").into(),
            }
        } else {
            let c = cats[usize::try_from(i % cats.len() as u64).unwrap_or(0)];
            Op::SetImpact {
                id: damage(i % DAMAGES),
                category: c,
                level: m.levels(c).last(),
            }
        });
        i += 1;
    }
    ops
}

/// Applies `ops` to a fresh project.
pub fn build(m: &Arc<Methodology>, ops: &[Op]) -> Project {
    let mut p = Project::new(Arc::clone(m));
    for op in ops {
        if let Err(e) = p.apply(op.clone()) {
            panic!("workload op failed: {e}");
        }
    }
    p
}

/// The typical project: only the ops that add entities.
pub fn typical(m: &Arc<Methodology>) -> Project {
    let all = ops(m);
    let n = usize::try_from(ASSETS + DAMAGES + THREATS).unwrap_or(all.len());
    build(m, all.get(..n).unwrap_or(&all))
}
