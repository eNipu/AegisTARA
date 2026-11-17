//! The share-treatment lint (job 145c): a `Share` threat with a linked claim
//! must name a responsible party other than `Us`. Six cases, each asserting
//! the code and its warning severity, so removing the rule fails them.

use std::sync::Arc;

use aegis_model::{
    AssetId, ClaimId, DamageId, LintCode, LintFinding, LintSeverity, Methodology, Op, PartyId,
    PartyKind, Project, Provenance, RiskTreatment, ThreatId,
};

struct Fx {
    p: Project,
    threat: ThreatId,
    claim: ClaimId,
    us: PartyId,
    customer: PartyId,
    supplier: PartyId,
    other: PartyId,
}

/// A threat with a treatment and a claim ready to be linked, plus one party
/// of each kind. No responsible party is linked yet.
fn fixture(treatment: RiskTreatment, link_claim: bool) -> Fx {
    let m = Arc::new(Methodology::iso21434_r155().expect("methodology"));
    let mut p = Project::new(Arc::clone(&m));
    let (asset, damage, threat) = (AssetId::new_v7(), DamageId::new_v7(), ThreatId::new_v7());
    let category = m.impact_categories().next().expect("category");
    let level = m.levels(category).next().expect("level");
    let claim = ClaimId::new_v7();
    let (us, customer, supplier, other) = (
        PartyId::new_v7(),
        PartyId::new_v7(),
        PartyId::new_v7(),
        PartyId::new_v7(),
    );
    let ops = vec![
        Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
        Op::AddDamageScenario {
            id: damage,
            name: "Brakes fail".into(),
            impacts: vec![(category, level)],
            provenance: Provenance::default(),
        },
        Op::AddThreatScenario {
            id: threat,
            name: "Spoof CAN".into(),
            asset,
            category: m.threat_categories().next().expect("threat category"),
            catalog_refs: vec![],
            damages: vec![damage],
            provenance: Provenance::default(),
        },
        Op::SetThreatFeasibility {
            threat,
            level: m.feasibility_levels().next().map(|(l, _, _)| l),
            provenance: Provenance::default(),
        },
        Op::SetRiskTreatment {
            threat,
            treatment: Some(treatment),
            provenance: Provenance::default(),
        },
        Op::AddCybersecurityClaim {
            id: claim,
            name: "claim".into(),
            provenance: Provenance::default(),
        },
        Op::AddParty {
            id: us,
            name: "us".into(),
            kind: PartyKind::Us,
            provenance: Provenance::default(),
        },
        Op::AddParty {
            id: customer,
            name: "customer".into(),
            kind: PartyKind::Customer,
            provenance: Provenance::default(),
        },
        Op::AddParty {
            id: supplier,
            name: "supplier".into(),
            kind: PartyKind::Supplier,
            provenance: Provenance::default(),
        },
        Op::AddParty {
            id: other,
            name: "other".into(),
            kind: PartyKind::Other,
            provenance: Provenance::default(),
        },
    ];
    for op in ops {
        p.apply(op).expect("setup");
    }
    if link_claim {
        p.apply(Op::SetThreatClaim {
            threat,
            claim: Some(claim),
            provenance: Provenance::default(),
        })
        .expect("link claim");
    }
    Fx {
        p,
        threat,
        claim,
        us,
        customer,
        supplier,
        other,
    }
}

fn link(fx: &mut Fx, party: PartyId) {
    fx.p.apply(Op::LinkClaimParty {
        claim: fx.claim,
        party,
        provenance: Provenance::default(),
    })
    .expect("link party");
}

/// Only the two treatment/claim findings this rule is about.
fn treatment_findings(p: &Project) -> Vec<LintFinding> {
    p.lint()
        .into_iter()
        .filter(|f| {
            matches!(
                f.code(),
                LintCode::ShareTreatmentWithoutOtherParty { .. }
                    | LintCode::RetainOrShareTreatmentWithoutClaim { .. }
            )
        })
        .collect()
}

fn assert_share_warning(findings: &[LintFinding], threat: ThreatId) {
    assert_eq!(
        findings,
        &[LintFinding::new(
            LintCode::ShareTreatmentWithoutOtherParty { threat }
        )]
    );
    assert_eq!(
        LintCode::ShareTreatmentWithoutOtherParty { threat }.severity(),
        LintSeverity::Warning
    );
}

#[test]
fn share_with_only_us_fires() {
    let mut fx = fixture(RiskTreatment::Share, true);
    let us = fx.us;
    link(&mut fx, us);
    assert_share_warning(&treatment_findings(&fx.p), fx.threat);
}

#[test]
fn share_with_zero_parties_fires() {
    let fx = fixture(RiskTreatment::Share, true);
    assert_share_warning(&treatment_findings(&fx.p), fx.threat);
}

#[test]
fn share_with_customer_supplier_or_other_does_not_fire() {
    for party in [PartyKind::Customer, PartyKind::Supplier, PartyKind::Other] {
        let mut fx = fixture(RiskTreatment::Share, true);
        let id = match party {
            PartyKind::Customer => fx.customer,
            PartyKind::Supplier => fx.supplier,
            _ => fx.other,
        };
        link(&mut fx, id);
        assert!(
            treatment_findings(&fx.p).is_empty(),
            "{party:?} should count as another party"
        );
    }
}

#[test]
fn retain_with_no_party_does_not_fire() {
    let fx = fixture(RiskTreatment::Retain, true);
    assert!(treatment_findings(&fx.p).is_empty());
}

#[test]
fn reduce_does_not_fire() {
    let fx = fixture(RiskTreatment::Reduce, true);
    assert!(treatment_findings(&fx.p).is_empty());
}

#[test]
fn share_with_no_claim_only_fires_existing_code() {
    let fx = fixture(RiskTreatment::Share, false);
    let findings = treatment_findings(&fx.p);
    assert_eq!(
        findings,
        &[LintFinding::new(
            LintCode::RetainOrShareTreatmentWithoutClaim { threat: fx.threat }
        )]
    );
    assert_eq!(
        LintCode::RetainOrShareTreatmentWithoutClaim { threat: fx.threat }.severity(),
        LintSeverity::Warning
    );
}
