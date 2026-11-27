//! Tests for `AttackFilter`, `ScalabilityFilter`, `Op::SetAttackFilter`,
//! `ProjectDiff::attack_filter_changed`, and wire persistence (job 137a).

mod common;

use std::sync::Arc;

use aegis_model::{
    AttackFilter, AttackTree, ControlId, ElementRef, GateKind, Limit, MAX_ACTOR_BYTES,
    MAX_FEASIBILITY_FACTORS, MAX_NAME_CHARS, Methodology, ModelError, Node, Op, OpCodec,
    OpLogReader, OpLogWriter, Project, ProjectEvalCache, Provenance, Reason, ScalabilityFilter,
    StepTarget, StoreError, TreeId, VocabKind,
};
use serde_json::Value;

use common::{
    asset_id, builtin, cat, damage_id, feas, lvl, node_id, reordered, second, second_def,
    threat_id, tree_id, wide,
};

const MAX_LEVELS: usize = 16;

#[test]
fn scalability_and_attack_filter_operations() {
    assert!(!ScalabilityFilter::Any.is_active());
    assert!(ScalabilityFilter::ScaledOnly.is_active());
    assert!(ScalabilityFilter::UnscaledOnly.is_active());

    for (s, exp) in [
        (ScalabilityFilter::Any, [true, true, true]),
        (ScalabilityFilter::ScaledOnly, [true, false, true]),
        (ScalabilityFilter::UnscaledOnly, [false, true, true]),
    ] {
        assert_eq!(s.allows(Some(true)), exp[0]);
        assert_eq!(s.allows(Some(false)), exp[1]);
        assert_eq!(s.allows(None), exp[2]);
    }

    let m = builtin();
    let f0 = m.feasibility_factors().next().expect("factor 0");
    let f1 = m.feasibility_factors().nth(1).expect("factor 1");
    let l0 = m.factor_levels(f0).next().expect("l0");
    let l1 = m.factor_levels(f0).nth(1).expect("l1");
    let f1_l0 = m.factor_levels(f1).next().expect("f1_l0");

    let mut filter = AttackFilter::default();
    assert!(filter.is_default());
    assert!(!filter.has_excluded_levels());
    assert_eq!(filter.scalability(), ScalabilityFilter::Any);
    assert!(filter.allows_scalability(Some(false)));
    assert!(filter.allows(f0, l0));
    assert!(!filter.is_excluded(f0, l0));

    filter.exclude(f0, l0);
    assert!(!filter.is_default());
    assert!(filter.has_excluded_levels());
    assert!(filter.is_excluded(f0, l0));
    assert!(!filter.allows(f0, l0));
    assert!(filter.allows(f0, l1));

    filter.include(f0, l0);
    assert!(filter.is_default());

    let built = AttackFilter::default()
        .without(f0, l0)
        .without(f1, f1_l0)
        .with_toggled(f0, l1)
        .with_toggled(f0, l0)
        .with_scalability(ScalabilityFilter::UnscaledOnly);
    assert!(!built.is_excluded(f0, l0));
    assert!(built.is_excluded(f0, l1));
    assert!(built.is_excluded(f1, f1_l0));
    assert_eq!(built.scalability(), ScalabilityFilter::UnscaledOnly);
    assert!(!built.allows_scalability(Some(true)));
    assert!(built.allows_scalability(Some(false)));
    assert!(built.allows_scalability(None));

    let cleared = built.with_factor_included(f0);
    assert!(!cleared.is_excluded(f0, l1));
    assert!(cleared.is_excluded(f1, f1_l0));
    assert!(!cleared.is_default());

    // Out-of-range indices are ignored safely.
    let w = wide();
    let wf = w.feasibility_factors().nth(15).expect("wide factor 15");
    let wl = w.factor_levels(wf).nth(15).expect("wide level 15");
    let mut oob = AttackFilter::default();
    oob.exclude(wf, wl);
    assert!(oob.is_excluded(wf, wl));
    // Construct beyond MAX_FEASIBILITY_FACTORS / MAX_LEVELS isn't possible via public API,
    // so verify valid 15th index works and clearing via with_factor_included resets it.
    oob = oob.with_factor_included(wf);
    assert!(oob.is_default());
}

#[test]
fn apply_set_attack_filter_returns_exact_inverse_and_diff() {
    let m = second();
    let mut p = Project::new(Arc::clone(&m));
    let f0 = m.feasibility_factors().next().expect("f0");
    let f1 = m.feasibility_factors().nth(1).expect("f1");
    let f0_l0 = m.factor_levels(f0).next().expect("f0_l0");
    let f1_l1 = m.factor_levels(f1).nth(1).expect("f1_l1");

    let filter1 = AttackFilter::default()
        .without(f0, f0_l0)
        .with_scalability(ScalabilityFilter::ScaledOnly);
    let prov1 = Provenance::human("alice", 100);
    let inv1 = p
        .apply(Op::SetAttackFilter {
            filter: filter1,
            provenance: prov1.clone(),
        })
        .expect("apply filter1");
    assert_eq!(
        inv1,
        Op::SetAttackFilter {
            filter: AttackFilter::default(),
            provenance: Provenance::default(),
        }
    );
    assert_eq!(p.attack_filter(), &filter1);
    assert_eq!(p.attack_filter_provenance(), &prov1);

    let p_after_first = p.clone();
    // Provenance-only change leaves PartialEq true and diff empty.
    let mut p_other_prov = p.clone();
    let prov_only = Provenance::ai("assistant", 150);
    p_other_prov
        .apply(Op::SetAttackFilter {
            filter: filter1,
            provenance: prov_only.clone(),
        })
        .expect("provenance-only update");
    assert_eq!(p_other_prov.attack_filter_provenance(), &prov_only);
    assert_eq!(p_other_prov, p_after_first);
    let prov_diff = p_after_first.diff(&p_other_prov).expect("diff");
    assert!(!prov_diff.attack_filter_changed());
    assert!(prov_diff.is_empty());

    // Value change sets attack_filter_changed and !is_empty in both directions.
    let filter2 = AttackFilter::default()
        .without(f1, f1_l1)
        .with_scalability(ScalabilityFilter::UnscaledOnly);
    let prov2 = Provenance::ai("bot", 200);
    let inv2 = p
        .apply(Op::SetAttackFilter {
            filter: filter2,
            provenance: prov2.clone(),
        })
        .expect("apply filter2");
    assert_eq!(
        inv2,
        Op::SetAttackFilter {
            filter: filter1,
            provenance: prov1.clone(),
        }
    );
    assert_eq!(p.attack_filter(), &filter2);
    assert_eq!(p.attack_filter_provenance(), &prov2);

    let fwd = p_after_first.diff(&p).expect("fwd diff");
    let rev = p.diff(&p_after_first).expect("rev diff");
    assert!(fwd.attack_filter_changed());
    assert!(!fwd.is_empty());
    assert!(rev.attack_filter_changed());
    assert!(!rev.is_empty());

    // Applying inverses in reverse restores exact filter and provenance.
    p.apply(inv2).expect("undo filter2");
    assert_eq!(p, p_after_first);
    assert_eq!(p.attack_filter_provenance(), &prov1);

    p.apply(inv1).expect("undo filter1");
    assert_eq!(p, Project::new(m));
    assert_eq!(p.attack_filter_provenance(), &Provenance::default());
}

#[test]
fn validation_rejects_bad_provenance_foreign_vocab_and_handles_eu_cra() {
    let m = second();
    let mut p = Project::new(Arc::clone(&m));
    let before = p.clone();

    // L15: whitespace-only actor fails EmptyActor; over-cap actor fails ActorTooLong; default succeeds.
    let blank = Op::SetAttackFilter {
        filter: AttackFilter::default().with_scalability(ScalabilityFilter::ScaledOnly),
        provenance: Provenance::human("   ", 1),
    };
    assert_eq!(p.validate(&blank), Err(ModelError::EmptyActor));
    assert_eq!(p.apply(blank), Err(ModelError::EmptyActor));
    assert_eq!(p, before);

    let too_long = Op::SetAttackFilter {
        filter: AttackFilter::default(),
        provenance: Provenance::human("a".repeat(MAX_NAME_CHARS + 1), 1),
    };
    let expected_long = ModelError::ActorTooLong {
        max: MAX_NAME_CHARS,
    };
    assert_eq!(p.validate(&too_long), Err(expected_long.clone()));
    assert_eq!(p.apply(too_long), Err(expected_long));
    assert_eq!(p, before);

    p.apply(Op::SetAttackFilter {
        filter: AttackFilter::default(),
        provenance: Provenance::default(),
    })
    .expect("default provenance succeeds");

    // Foreign factor index and out-of-range level index fail validation.
    let w = wide();
    let foreign_factor = w
        .feasibility_factors()
        .nth(m.feasibility_factor_count())
        .expect("foreign factor");
    let foreign_level = w
        .factor_levels(foreign_factor)
        .next()
        .expect("foreign level");
    let bad_factor_op = Op::SetAttackFilter {
        filter: AttackFilter::default().without(foreign_factor, foreign_level),
        provenance: Provenance::default(),
    };
    assert_eq!(
        p.validate(&bad_factor_op),
        Err(ModelError::UnknownFeasibilityFactor(foreign_factor))
    );

    let valid_factor = m.feasibility_factors().next().expect("valid factor");
    let wf0 = w.feasibility_factors().next().expect("wide f0");
    let oob_level = w
        .factor_levels(wf0)
        .nth(m.factor_level_count(valid_factor))
        .expect("oob level");
    let bad_level_op = Op::SetAttackFilter {
        filter: AttackFilter::default().without(valid_factor, oob_level),
        provenance: Provenance::default(),
    };
    assert_eq!(
        p.validate(&bad_level_op),
        Err(ModelError::UnknownFactorLevel {
            factor: valid_factor,
            level: oob_level,
        })
    );

    // EU CRA has zero feasibility factors: factor exclusion fails, ScalabilityFilter succeeds.
    let cra = Arc::new(Methodology::eu_cra().expect("eu_cra"));
    assert_eq!(cra.feasibility_factor_count(), 0);
    let mut p_cra = Project::new(Arc::clone(&cra));
    let f0 = w.feasibility_factors().next().expect("w f0");
    let l0 = w.factor_levels(f0).next().expect("w l0");
    assert_eq!(
        p_cra.apply(Op::SetAttackFilter {
            filter: AttackFilter::default().without(f0, l0),
            provenance: Provenance::default(),
        }),
        Err(ModelError::UnknownFeasibilityFactor(f0))
    );
    p_cra
        .apply(Op::SetAttackFilter {
            filter: AttackFilter::default().with_scalability(ScalabilityFilter::ScaledOnly),
            provenance: Provenance::human("cra-user", 10),
        })
        .expect("scalability-only filter succeeds on eu_cra");
    assert_eq!(
        p_cra.attack_filter().scalability(),
        ScalabilityFilter::ScaledOnly
    );
}

#[test]
fn snapshot_oplog_codec_and_workspace_round_trips() {
    let m = second();
    let default_p = Project::new(Arc::clone(&m));

    // Default snapshot omits "attack_filter" and round-trips.
    let mut default_snap = Vec::new();
    default_p.save(&mut default_snap).expect("save default");
    let default_json: Value = serde_json::from_slice(&default_snap).expect("json");
    assert!(default_json.get("attack_filter").is_none());
    let loaded_default = Project::load(default_snap.as_slice(), Arc::clone(&m)).expect("load");
    assert_eq!(loaded_default, default_p);

    // Default workspace omits "attack_filter" from aegis.json and round-trips.
    let default_ws = default_p.to_workspace_files().expect("ws default");
    let aegis_default = default_ws
        .iter()
        .find(|f| f.path == "aegis.json")
        .expect("aegis.json");
    let aegis_default_val: Value = serde_json::from_slice(&aegis_default.bytes).expect("ws json");
    assert!(aegis_default_val.get("attack_filter").is_none());
    let loaded_ws_default = Project::from_workspace_files(
        default_ws
            .iter()
            .map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(&m),
    )
    .expect("load ws default");
    assert_eq!(loaded_ws_default, default_p);

    // Non-default filter round-trips through snapshot, reordered snapshot, op-log, OpCodec, and workspace.
    let mut p = Project::new(Arc::clone(&m));
    let exp = m.feasibility_factor("exposure").expect("exposure");
    let exp_iso = m.factor_level(exp, "isolated").expect("isolated");
    let exp_local = m.factor_level(exp, "local").expect("local");
    let res = m.feasibility_factor("resources").expect("resources");
    let res_bespoke = m.factor_level(res, "bespoke").expect("bespoke");

    let filter = AttackFilter::default()
        .without(exp, exp_local)
        .without(exp, exp_iso)
        .without(res, res_bespoke)
        .with_scalability(ScalabilityFilter::ScaledOnly);
    let prov = Provenance::human("bob", 42);
    let op = Op::SetAttackFilter {
        filter,
        provenance: prov.clone(),
    };
    p.apply(op.clone()).expect("apply");

    // Snapshot round-trip + canonical key ordering.
    let mut snap = Vec::new();
    p.save(&mut snap).expect("save");
    let snap_val: Value = serde_json::from_slice(&snap).expect("snap json");
    let af = snap_val.get("attack_filter").expect("attack_filter");
    assert_eq!(
        af.get("scalability").and_then(Value::as_str),
        Some("scaled-only")
    );
    let excluded = af
        .get("excluded")
        .and_then(Value::as_object)
        .expect("excluded map");
    let factor_keys: Vec<_> = excluded.keys().map(String::as_str).collect();
    assert_eq!(factor_keys, ["exposure", "resources"]);
    let exp_levels: Vec<_> = excluded["exposure"]
        .as_array()
        .expect("arr")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(exp_levels, ["isolated", "local"]);

    let loaded = Project::load(snap.as_slice(), Arc::clone(&m)).expect("load snap");
    assert_eq!(loaded, p);

    // Reordered methodology: pinned snapshot rejects with MethodologyMismatch,
    // unpinned snapshot loads and matches by key.
    let reord = Arc::new(Methodology::new(reordered(second_def())).expect("reordered"));
    assert!(matches!(
        Project::load(snap.as_slice(), Arc::clone(&reord)),
        Err(StoreError::MethodologyMismatch { .. })
    ));
    let needle = format!(r#","digest":"{}""#, m.pack_digest());
    let unpinned_snap = String::from_utf8(snap.clone())
        .expect("utf8")
        .replacen(&needle, "", 1)
        .into_bytes();
    let loaded_reord =
        Project::load(unpinned_snap.as_slice(), Arc::clone(&reord)).expect("load reordered");
    let r_exp = reord.feasibility_factor("exposure").expect("r exp");
    let r_res = reord.feasibility_factor("resources").expect("r res");
    assert!(
        loaded_reord
            .attack_filter()
            .is_excluded(r_exp, reord.factor_level(r_exp, "isolated").expect("iso"))
    );
    assert!(
        loaded_reord
            .attack_filter()
            .is_excluded(r_exp, reord.factor_level(r_exp, "local").expect("loc"))
    );
    assert!(
        loaded_reord
            .attack_filter()
            .is_excluded(r_res, reord.factor_level(r_res, "bespoke").expect("bes"))
    );
    assert_eq!(
        loaded_reord.attack_filter().scalability(),
        ScalabilityFilter::ScaledOnly
    );

    // Op-log round-trip (non-default and reset-to-default).
    let reset_op = Op::SetAttackFilter {
        filter: AttackFilter::default(),
        provenance: Provenance::default(),
    };
    let mut writer = OpLogWriter::create(Vec::new(), Arc::clone(&m)).expect("log header");
    writer.append(&op).expect("append op");
    writer.append(&reset_op).expect("append reset");
    let log_bytes = writer.into_inner().expect("finish log");
    let records: Vec<_> = OpLogReader::open(log_bytes.as_slice(), Arc::clone(&m))
        .expect("open log")
        .collect::<Result<Vec<_>, _>>()
        .expect("read records");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].op, op);
    assert_eq!(records[1].op, reset_op);

    // OpCodec round-trip & actor byte cap check.
    let codec = OpCodec::new(Arc::clone(&m));
    let encoded = codec.encode(&op).expect("encode");
    assert_eq!(codec.decode(&encoded).expect("decode"), op);
    let encoded_reset = codec.encode(&reset_op).expect("encode reset");
    assert_eq!(
        codec.decode(&encoded_reset).expect("decode reset"),
        reset_op
    );

    let over_actor = Op::SetAttackFilter {
        filter,
        provenance: Provenance::human("x".repeat(MAX_ACTOR_BYTES + 1), 1),
    };
    assert!(matches!(
        codec.encode(&over_actor),
        Err(StoreError::UnencodableOp)
    ));

    // Foreign factor and foreign level indices refused with UnencodableOp and no output.
    let w = wide();
    let foreign_f = w
        .feasibility_factors()
        .nth(m.feasibility_factor_count())
        .expect("foreign f");
    let foreign_l = w.factor_levels(foreign_f).next().expect("foreign l");
    let wf0 = w.feasibility_factors().next().expect("wf0");
    let oob_l = w
        .factor_levels(wf0)
        .nth(m.factor_level_count(exp))
        .expect("oob l");
    for foreign_op in [
        Op::SetAttackFilter {
            filter: AttackFilter::default().without(foreign_f, foreign_l),
            provenance: Provenance::default(),
        },
        Op::SetAttackFilter {
            filter: AttackFilter::default().without(exp, oob_l),
            provenance: Provenance::default(),
        },
    ] {
        assert!(matches!(
            codec.encode(&foreign_op),
            Err(StoreError::UnencodableOp)
        ));
        let mut log = OpLogWriter::resume(Vec::new(), Arc::clone(&m), 0);
        assert!(matches!(
            log.append(&foreign_op),
            Err(StoreError::UnencodableOp)
        ));
        assert_eq!(log.last_seq(), 0);
        assert!(log.into_inner().is_ok_and(|bytes| bytes.is_empty()));
    }

    // Workspace round-trip with non-default filter.
    let ws = p.to_workspace_files().expect("ws");
    let aegis = ws
        .iter()
        .find(|f| f.path == "aegis.json")
        .expect("aegis.json");
    let aegis_val: Value = serde_json::from_slice(&aegis.bytes).expect("ws json");
    assert!(aegis_val.get("attack_filter").is_some());
    let loaded_ws =
        Project::from_workspace_files(ws.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())), m)
            .expect("load ws");
    assert_eq!(loaded_ws, p);
}

#[test]
fn l09_single_guard_decode_limits_and_errors() {
    let w = Arc::new(wide());
    let codec = OpCodec::new(Arc::clone(&w));

    // Boundary pass: 16 factors and 16 levels per factor.
    let mut full = AttackFilter::default().with_scalability(ScalabilityFilter::UnscaledOnly);
    for f in w.feasibility_factors() {
        for l in w.factor_levels(f) {
            full.exclude(f, l);
        }
    }
    let full_op = Op::SetAttackFilter {
        filter: full,
        provenance: Provenance::default(),
    };
    let encoded_full = codec.encode(&full_op).expect("encode 16x16");
    assert_eq!(codec.decode(&encoded_full).expect("decode 16x16"), full_op);

    // 17 factors (17th is a valid key so only the factor count cap rejects it).
    let seventeen_factors: String = (0..=MAX_FEASIBILITY_FACTORS)
        .map(|i| format!("\"ff{}\":[\"fl0\"]", i % MAX_FEASIBILITY_FACTORS))
        .collect::<Vec<_>>()
        .join(",");
    let err_17_factors = codec.decode(
        format!(
            "{{\"op\":\"set_attack_filter\",\"filter\":{{\"excluded\":{{{seventeen_factors}}}}}}}"
        )
        .as_bytes(),
    );
    assert!(
        matches!(
            err_17_factors,
            Err(StoreError::LimitExceeded(Limit::FeasibilityFactors {
                max: MAX_FEASIBILITY_FACTORS,
            }))
        ),
        "got {err_17_factors:?}"
    );

    // 17 levels (17th is a valid key so only the level count cap rejects it).
    let seventeen_levels: String = (0..=MAX_LEVELS)
        .map(|i| format!("\"fl{}\"", i % MAX_LEVELS))
        .collect::<Vec<_>>()
        .join(",");
    let err_17_levels = codec.decode(
        format!(
            "{{\"op\":\"set_attack_filter\",\"filter\":{{\"excluded\":{{\"ff0\":[{seventeen_levels}]}}}}}}"
        )
        .as_bytes(),
    );
    assert!(
        matches!(
            err_17_levels,
            Err(StoreError::LimitExceeded(Limit::Levels { max: MAX_LEVELS }))
        ),
        "got {err_17_levels:?}"
    );

    // Duplicate factor key (within 16-factor cap).
    let err_dup_factor = codec.decode(
        br#"{"op":"set_attack_filter","filter":{"excluded":{"ff0":["fl0"],"ff0":["fl1"]}}}"#,
    );
    assert!(
        matches!(err_dup_factor, Err(StoreError::DuplicateEntry)),
        "got {err_dup_factor:?}"
    );

    // Duplicate level key (within 16-level cap).
    let err_dup_level =
        codec.decode(br#"{"op":"set_attack_filter","filter":{"excluded":{"ff0":["fl0","fl0"]}}}"#);
    assert!(
        matches!(err_dup_level, Err(StoreError::DuplicateEntry)),
        "got {err_dup_level:?}"
    );

    // Unknown factor key.
    let err_unk_factor =
        codec.decode(br#"{"op":"set_attack_filter","filter":{"excluded":{"no-factor":["fl0"]}}}"#);
    assert!(
        matches!(
            err_unk_factor,
            Err(StoreError::UnknownKey {
                kind: VocabKind::FeasibilityFactor,
                ref key,
            }) if &**key == "no-factor"
        ),
        "got {err_unk_factor:?}"
    );

    // Unknown level key.
    let err_unk_level =
        codec.decode(br#"{"op":"set_attack_filter","filter":{"excluded":{"ff0":["no-level"]}}}"#);
    assert!(
        matches!(
            err_unk_level,
            Err(StoreError::UnknownKey {
                kind: VocabKind::FactorLevel,
                ref key,
            }) if &**key == "no-level"
        ),
        "got {err_unk_level:?}"
    );

    // Unknown field inside attack filter.
    let err_unk_field =
        codec.decode(br#"{"op":"set_attack_filter","filter":{"unknown_field":true}}"#);
    assert!(
        matches!(err_unk_field, Err(StoreError::Decode { .. })),
        "got {err_unk_field:?}"
    );

    // Workspace aegis.json blame on invalid attack_filter.
    let p = Project::new(Arc::clone(&w));
    let mut files = p.to_workspace_files().expect("ws");
    let aegis = files
        .iter_mut()
        .find(|f| f.path == "aegis.json")
        .expect("aegis.json");
    let mut val: Value = serde_json::from_slice(&aegis.bytes).expect("ws json");
    val.as_object_mut().expect("obj").insert(
        "attack_filter".to_owned(),
        serde_json::json!({ "excluded": { "no-factor": [] } }),
    );
    aegis.bytes = serde_json::to_vec_pretty(&val).expect("ser");
    let ws_err = Project::from_workspace_files(
        files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
        w,
    );
    match ws_err {
        Err(StoreError::InWorkspace { path, source }) => {
            assert_eq!(path, "aegis.json");
            assert!(matches!(
                *source,
                StoreError::UnknownKey {
                    kind: VocabKind::FeasibilityFactor,
                    ref key,
                } if &**key == "no-factor"
            ));
        }
        other => panic!("expected InWorkspace for aegis.json, got {other:?}"),
    }
}

#[test]
fn each_factor_excludes_only_rated_steps() {
    let m = builtin();
    let factors: Vec<_> = m.feasibility_factors().collect();
    assert_eq!(factors.len(), 5);
    let safety = cat(&m, "safety");
    let severe = lvl(&m, safety, "severe");
    let tcat = m.threat_categories().next().expect("cat");
    let high = feas(&m, "high");
    let med = feas(&m, "medium");
    let low = feas(&m, "low");
    let vlow = feas(&m, "very-low");
    let prov = Provenance::human("alice", 100);

    for (fi, &factor) in factors.iter().enumerate() {
        let excluded_level = m.factor_levels(factor).last().expect("excluded level");
        let other_factor = factors[(fi + 1) % factors.len()];
        let other_level = m.factor_levels(other_factor).next().expect("other level");

        let mut p = Project::new(Arc::clone(&m));
        let (asset, dmg, ctrl) = (asset_id(1), damage_id(1), ControlId::new_v7());
        let (t1_id, t2_id) = (threat_id(1), threat_id(2));
        let (tr1, tr2) = (tree_id(1), tree_id(2));
        let (r1, g_and, s_rated, s_sibling, s_unrated) =
            (node_id(1), node_id(2), node_id(3), node_id(4), node_id(5));
        let (r2, s_only) = (node_id(6), node_id(7));

        let mut tree1 = AttackTree::new(r1, Node::gate(GateKind::Or, "root1")).expect("r1");
        let and_idx = tree1
            .add_child(tree1.root(), g_and, Node::gate(GateKind::And, "and"))
            .expect("and");
        tree1
            .add_child(and_idx, s_rated, Node::step("rated on factor"))
            .expect("s_rated");
        tree1
            .add_child(and_idx, s_sibling, Node::step("sibling on other"))
            .expect("s_sibling");
        tree1
            .add_child(tree1.root(), s_unrated, Node::step("unrated on factor"))
            .expect("s_unrated");

        let mut tree2 = AttackTree::new(r2, Node::gate(GateKind::Or, "root2")).expect("r2");
        tree2
            .add_child(tree2.root(), s_only, Node::step("only path"))
            .expect("s_only");

        for op in [
            Op::AddAsset {
                id: asset,
                name: "ECU".into(),
                properties: m.all_properties(),
                provenance: prov.clone(),
            },
            Op::AddDamageScenario {
                id: dmg,
                name: "Crash".into(),
                impacts: vec![(safety, severe)],
                provenance: prov.clone(),
            },
            Op::AddSecurityControl {
                id: ctrl,
                name: "SecOC".into(),
                provenance: prov.clone(),
            },
            Op::AddAttackTree {
                role: Default::default(),
                id: tr1,
                tree: tree1,
                step_feasibility: vec![(s_rated, high), (s_sibling, med), (s_unrated, low)],
                provenance: prov.clone(),
            },
            Op::SetStepMitigation {
                tree: tr1,
                node: s_unrated,
                control: ctrl,
                feasibility: Some(vlow),
                provenance: prov.clone(),
            },
            Op::SetStepFactor {
                target: StepTarget::Step(tr1, s_rated),
                factor,
                level: Some(excluded_level),
                provenance: prov.clone(),
            },
            Op::SetStepFactor {
                target: StepTarget::Step(tr1, s_sibling),
                factor: other_factor,
                level: Some(other_level),
                provenance: prov.clone(),
            },
            Op::SetStepFactor {
                target: StepTarget::Step(tr1, s_unrated),
                factor: other_factor,
                level: Some(other_level),
                provenance: prov.clone(),
            },
            Op::AddAttackTree {
                role: Default::default(),
                id: tr2,
                tree: tree2,
                step_feasibility: vec![(s_only, high)],
                provenance: prov.clone(),
            },
            Op::SetStepFactor {
                target: StepTarget::Step(tr2, s_only),
                factor,
                level: Some(excluded_level),
                provenance: prov.clone(),
            },
            Op::AddThreatScenario {
                id: t1_id,
                name: "T1".into(),
                asset,
                category: tcat,
                catalog_refs: vec![],
                damages: vec![dmg],
                provenance: prov.clone(),
            },
            Op::SetThreatAttackTree {
                threat: t1_id,
                tree: Some(tr1),
                provenance: prov.clone(),
            },
            Op::AddThreatScenario {
                id: t2_id,
                name: "T2".into(),
                asset,
                category: tcat,
                catalog_refs: vec![],
                damages: vec![dmg],
                provenance: prov.clone(),
            },
            Op::SetThreatAttackTree {
                threat: t2_id,
                tree: Some(tr2),
                provenance: prov.clone(),
            },
        ] {
            p.apply(op).expect("setup");
        }

        let mut cache = ProjectEvalCache::new(&p);
        p.apply_cached(
            Op::SetAttackFilter {
                filter: AttackFilter::default().without(factor, excluded_level),
                provenance: prov.clone(),
            },
            &mut cache,
        )
        .expect("set filter");

        let app = p.applicability();
        assert_eq!(cache.applicability(), &app);
        assert_eq!(cache, ProjectEvalCache::new(&p));

        assert_eq!(
            app.node_reasons(tr1, s_rated),
            &[Reason::OutsideProfile {
                factor,
                level: excluded_level
            }]
        );
        assert!(!app.is_node_excluded(tr1, s_sibling));
        assert!(!app.is_node_excluded(tr1, s_unrated));
        assert_eq!(app.node_reasons(tr1, g_and), &[Reason::AllPathsExcluded]);
        assert!(!app.is_node_excluded(tr1, r1));
        assert!(!app.is_excluded(ElementRef::Tree(tr1)));

        assert_eq!(
            app.node_reasons(tr2, s_only),
            &[Reason::OutsideProfile {
                factor,
                level: excluded_level
            }]
        );
        assert_eq!(app.node_reasons(tr2, r2), &[Reason::AllPathsExcluded]);
        assert_eq!(
            app.reasons(ElementRef::Tree(tr2)),
            &[Reason::AllPathsExcluded]
        );

        assert_eq!(
            cache.evaluate_tree(&p, tr1).expect("tr1").root_value(),
            &low
        );
        assert_eq!(
            cache
                .evaluate_residual_tree(&p, tr1)
                .expect("tr1 res")
                .root_value(),
            &vlow
        );
        assert_eq!(
            cache.evaluate_risk(&p, t1_id).cloned(),
            p.evaluate_risk(t1_id)
        );
        assert_eq!(
            cache.evaluate_residual_risk(&p, t1_id).cloned(),
            p.evaluate_residual_risk(t1_id)
        );
        assert_eq!(
            cache.evaluate_risk(&p, t2_id).cloned(),
            Err(ModelError::ThreatNotApplicable(t2_id))
        );
        assert_eq!(
            p.evaluate_risk(t2_id),
            Err(ModelError::ThreatNotApplicable(t2_id))
        );
        assert_eq!(
            p.evaluate_residual_risk(t2_id),
            Err(ModelError::ThreatNotApplicable(t2_id))
        );

        // Even with every level of `factor` excluded, the step unrated on `factor` remains included.
        let mut all_off = AttackFilter::default();
        for l in m.factor_levels(factor) {
            all_off.exclude(factor, l);
        }
        p.apply_cached(
            Op::SetAttackFilter {
                filter: all_off,
                provenance: prov.clone(),
            },
            &mut cache,
        )
        .expect("all off");
        assert!(!p.applicability().is_node_excluded(tr1, s_unrated));
        assert_eq!(cache, ProjectEvalCache::new(&p));
    }
}

#[test]
fn scalability_uses_threat_scaling() {
    let m = builtin();
    let safety = cat(&m, "safety");
    let severe = lvl(&m, safety, "severe");
    let tcat = m.threat_categories().next().expect("cat");
    let high = feas(&m, "high");
    let prov = Provenance::human("alice", 100);

    let mut p = Project::new(Arc::clone(&m));
    let (asset, dmg) = (asset_id(1), damage_id(1));
    p.apply(Op::AddAsset {
        id: asset,
        name: "ECU".into(),
        properties: m.all_properties(),
        provenance: prov.clone(),
    })
    .expect("asset");
    p.apply(Op::AddDamageScenario {
        id: dmg,
        name: "Crash".into(),
        impacts: vec![(safety, severe)],
        provenance: prov.clone(),
    })
    .expect("dmg");

    let mut make_tree = |n: u64| {
        let tid = tree_id(n);
        let step = node_id(n * 10);
        p.apply(Op::AddAttackTree {
            role: Default::default(),
            id: tid,
            tree: AttackTree::new(step, Node::step("s")).expect("tree"),
            step_feasibility: vec![(step, high)],
            provenance: prov.clone(),
        })
        .expect("add tree");
        tid
    };
    let (tr_scaled, tr_unscaled, tr_unrated, tr_shared) =
        (make_tree(1), make_tree(2), make_tree(3), make_tree(4));

    let mut make_threat = |n: u64, tree: Option<TreeId>, scales: Option<bool>| {
        let id = threat_id(n);
        p.apply(Op::AddThreatScenario {
            id,
            name: format!("T{n}").into(),
            asset,
            category: tcat,
            catalog_refs: vec![],
            damages: vec![dmg],
            provenance: prov.clone(),
        })
        .expect("add threat");
        if let Some(tr) = tree {
            p.apply(Op::SetThreatAttackTree {
                threat: id,
                tree: Some(tr),
                provenance: prov.clone(),
            })
            .expect("link tree");
        } else {
            p.apply(Op::SetThreatFeasibility {
                threat: id,
                level: Some(high),
                provenance: prov.clone(),
            })
            .expect("direct feas");
        }
        if scales.is_some() {
            p.apply(Op::SetThreatScaling {
                threat: id,
                scales,
                reason: "assessed".into(),
                provenance: prov.clone(),
            })
            .expect("set scaling");
        }
        id
    };

    let t_scaled = make_threat(1, Some(tr_scaled), Some(true));
    let t_unscaled = make_threat(2, Some(tr_unscaled), Some(false));
    let t_unrated = make_threat(3, Some(tr_unrated), None);
    let t_shared_scaled = make_threat(4, Some(tr_shared), Some(true));
    let t_shared_unscaled = make_threat(5, Some(tr_shared), Some(false));
    let t_direct_scaled = make_threat(6, None, Some(true));
    let t_direct_unscaled = make_threat(7, None, Some(false));
    let t_direct_unrated = make_threat(8, None, None);

    let mut cache = ProjectEvalCache::new(&p);
    assert!(p.applicability().is_empty());

    // ScaledOnly excludes ONLY Some(false), never None.
    p.apply_cached(
        Op::SetAttackFilter {
            filter: AttackFilter::default().with_scalability(ScalabilityFilter::ScaledOnly),
            provenance: prov.clone(),
        },
        &mut cache,
    )
    .expect("scaled only");
    assert_eq!(cache, ProjectEvalCache::new(&p));
    assert_eq!(cache.applicability(), &p.applicability());
    let app = p.applicability();
    assert!(!app.is_excluded(ElementRef::Threat(t_scaled)));
    assert!(!app.is_excluded(ElementRef::Threat(t_unrated)));
    assert!(!app.is_excluded(ElementRef::Threat(t_shared_scaled)));
    assert!(!app.is_excluded(ElementRef::Threat(t_direct_scaled)));
    assert!(!app.is_excluded(ElementRef::Threat(t_direct_unrated)));
    assert!(!app.is_excluded(ElementRef::Tree(tr_shared)));
    for excluded in [t_unscaled, t_shared_unscaled, t_direct_unscaled] {
        assert_eq!(
            app.reasons(ElementRef::Threat(excluded)),
            &[Reason::Scalability]
        );
        assert_eq!(
            cache.evaluate_risk(&p, excluded).cloned(),
            Err(ModelError::ThreatNotApplicable(excluded))
        );
        assert_eq!(
            cache.evaluate_residual_risk(&p, excluded).cloned(),
            Err(ModelError::ThreatNotApplicable(excluded))
        );
    }
    assert_eq!(
        app.reasons(ElementRef::Tree(tr_unscaled)),
        &[Reason::OwnerExcluded(ElementRef::Threat(t_unscaled))]
    );
    assert!(cache.evaluate_risk(&p, t_direct_unrated).is_ok());

    // UnscaledOnly excludes ONLY Some(true), never None.
    p.apply_cached(
        Op::SetAttackFilter {
            filter: AttackFilter::default().with_scalability(ScalabilityFilter::UnscaledOnly),
            provenance: prov.clone(),
        },
        &mut cache,
    )
    .expect("unscaled only");
    assert_eq!(cache, ProjectEvalCache::new(&p));
    assert_eq!(cache.applicability(), &p.applicability());
    let app = p.applicability();
    assert!(!app.is_excluded(ElementRef::Threat(t_unscaled)));
    assert!(!app.is_excluded(ElementRef::Threat(t_unrated)));
    assert!(!app.is_excluded(ElementRef::Threat(t_shared_unscaled)));
    assert!(!app.is_excluded(ElementRef::Threat(t_direct_unscaled)));
    assert!(!app.is_excluded(ElementRef::Threat(t_direct_unrated)));
    assert!(!app.is_excluded(ElementRef::Tree(tr_shared)));
    for excluded in [t_scaled, t_shared_scaled, t_direct_scaled] {
        assert_eq!(
            app.reasons(ElementRef::Threat(excluded)),
            &[Reason::Scalability]
        );
        assert_eq!(
            cache.evaluate_risk(&p, excluded).cloned(),
            Err(ModelError::ThreatNotApplicable(excluded))
        );
    }
    assert_eq!(
        app.reasons(ElementRef::Tree(tr_scaled)),
        &[Reason::OwnerExcluded(ElementRef::Threat(t_scaled))]
    );

    // Direct-feasibility threat updates cleanly when SetThreatScaling changes while active.
    p.apply_cached(
        Op::SetThreatScaling {
            threat: t_direct_unscaled,
            scales: Some(true),
            reason: "now scales".into(),
            provenance: prov.clone(),
        },
        &mut cache,
    )
    .expect("flip direct");
    assert_eq!(cache, ProjectEvalCache::new(&p));
    assert_eq!(
        cache.evaluate_risk(&p, t_direct_unscaled).cloned(),
        Err(ModelError::ThreatNotApplicable(t_direct_unscaled))
    );
    p.apply_cached(
        Op::SetThreatScaling {
            threat: t_direct_unscaled,
            scales: None,
            reason: "".into(),
            provenance: prov,
        },
        &mut cache,
    )
    .expect("clear direct");
    assert_eq!(cache, ProjectEvalCache::new(&p));
    assert!(cache.evaluate_risk(&p, t_direct_unscaled).is_ok());
}

#[test]
fn default_filter_skips_applicability_and_bounds_cache_recomputes() {
    let m = builtin();
    let safety = cat(&m, "safety");
    let severe = lvl(&m, safety, "severe");
    let tcat = m.threat_categories().next().expect("cat");
    let low = feas(&m, "low");
    let prov = Provenance::human("alice", 100);
    let mut p = Project::new(Arc::clone(&m));
    let (asset, dmg, tr, threat) = (asset_id(1), damage_id(1), tree_id(1), threat_id(1));
    let (r, g1, g2, s1) = (node_id(1), node_id(2), node_id(3), node_id(4));
    let depth: u64 = 4;

    let mut tree = AttackTree::new(r, Node::gate(GateKind::Or, "r")).expect("r");
    let i1 = tree
        .add_child(tree.root(), g1, Node::gate(GateKind::And, "g1"))
        .expect("g1");
    let i2 = tree
        .add_child(i1, g2, Node::gate(GateKind::Or, "g2"))
        .expect("g2");
    tree.add_child(i2, s1, Node::step("s1")).expect("s1");

    for op in [
        Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: m.all_properties(),
            provenance: prov.clone(),
        },
        Op::AddDamageScenario {
            id: dmg,
            name: "Crash".into(),
            impacts: vec![(safety, severe)],
            provenance: prov.clone(),
        },
        Op::AddAttackTree {
            role: Default::default(),
            id: tr,
            tree,
            step_feasibility: vec![(s1, low)],
            provenance: prov.clone(),
        },
        Op::AddThreatScenario {
            id: threat,
            name: "T".into(),
            asset,
            category: tcat,
            catalog_refs: vec![],
            damages: vec![dmg],
            provenance: prov.clone(),
        },
        Op::SetThreatAttackTree {
            threat,
            tree: Some(tr),
            provenance: prov.clone(),
        },
    ] {
        p.apply(op).expect("setup");
    }

    let mut cache = ProjectEvalCache::new(&p);
    cache.reset_counters();
    p.apply_cached(
        Op::SetThreatScaling {
            threat,
            scales: Some(false),
            reason: "local".into(),
            provenance: prov.clone(),
        },
        &mut cache,
    )
    .expect("scaling");
    assert_eq!(cache.recomputed_nodes(), 0);
    assert_eq!(cache.recomputed_threats(), 0);
    assert!(cache.applicability().is_empty());

    let factors: Vec<_> = m.feasibility_factors().collect();
    let f0 = factors[0];
    let l0 = m.factor_levels(f0).next().expect("l0");
    cache.reset_counters();
    p.apply_cached(
        Op::SetStepFactor {
            target: StepTarget::Step(tr, s1),
            factor: f0,
            level: Some(l0),
            provenance: prov.clone(),
        },
        &mut cache,
    )
    .expect("set single factor");
    assert!(cache.recomputed_nodes() <= depth);
    assert_eq!(cache.recomputed_threats(), 0);
    assert!(cache.applicability().is_empty());

    // Rating all remaining factors shifts feasibility along depth 4 (inherent + residual <= 2 * depth).
    for &f in &factors[1..] {
        let l = m.factor_levels(f).last().expect("level");
        cache.reset_counters();
        p.apply_cached(
            Op::SetStepFactor {
                target: StepTarget::Step(tr, s1),
                factor: f,
                level: Some(l),
                provenance: prov.clone(),
            },
            &mut cache,
        )
        .expect("set factor");
        assert!(cache.recomputed_nodes() <= depth * 2);
        assert!(cache.applicability().is_empty());
    }
    assert_eq!(cache, ProjectEvalCache::new(&p));
}
