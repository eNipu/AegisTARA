//! Short sentences that name an edit and its element, for the Undo/Redo
//! tooltip, the Edit menu and the toasts (UI-10.2, UI-10.5, UI-10.9,
//! UI-10.10). Built here, in the client core, from the ops and the project
//! as it was before them. Element names appear only in these user-facing
//! strings, never in logs.

use aegis_model::{ElementRef, Op, Project, StepTarget};

use crate::view::Selection;

/// Longest element name shown in a sentence, in chars.
const MAX_NAME_CHARS: usize = 40;

/// What an edit did, to what, and how many other things it touched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    verb: &'static str,
    noun: &'static str,
    name: Option<String>,
    /// Other ops of the same edit (unlinks, cleared text, ...).
    others: usize,
    /// How many threat scenarios the edit unlinked a damage scenario from.
    unlinked: usize,
    /// How many steps a removal took the "through" item from (CFG-01).
    lost_via: usize,
    /// The element the edit is about, to show after an undo or redo.
    target: Option<Selection>,
    /// For a bulk edit (UI-04), how many elements it changed; 0 otherwise.
    count: usize,
    /// How many components a system removal took out of the system.
    no_system: usize,
}

impl Label {
    /// The label of a bulk edit (UI-04): "remove 3 assets", "3 assets removed".
    #[must_use]
    pub fn bulk(verb: &'static str, noun: &'static str, count: usize) -> Label {
        Label {
            verb,
            noun,
            name: None,
            others: 0,
            unlinked: 0,
            lost_via: 0,
            target: None,
            count,
            no_system: 0,
        }
    }

    /// Whether this was one change to several elements (UI-04).
    #[must_use]
    pub fn is_bulk(&self) -> bool {
        self.count > 0
    }

    /// The element to open and select after undoing or redoing this edit
    /// (UI-10.5), if it exists in `project` now: undoing an addition leaves
    /// nothing to show.
    #[must_use]
    pub fn selection(&self, project: &Project) -> Option<Selection> {
        let sel = self.target?;
        let exists = match sel {
            Selection::Asset(id) => project.asset_idx(&id).is_some(),
            Selection::Damage(id) => project.damage_scenario_idx(&id).is_some(),
            Selection::Threat(id) => project.threat_scenario_idx(&id).is_some(),
            Selection::Tree(id) | Selection::TreeNode(id, _) => {
                project.attack_tree_idx(&id).is_some()
            }
            Selection::SharedStep(id) => project.shared_step_idx(&id).is_some(),
            Selection::Control(id) => project.security_control_idx(&id).is_some(),
            Selection::Goal(id) => project.cybersecurity_goal_idx(&id).is_some(),
            Selection::Claim(id) => project.cybersecurity_claim_idx(&id).is_some(),
            Selection::Party(id) => project.party_idx(&id).is_some(),
            Selection::Condition(id) => project.condition_idx(&id).is_some(),
            Selection::Assumption(id) => project.assumption_idx(&id).is_some(),
            Selection::Component(id) => project.component_idx(&id).is_some(),
            Selection::Interface(id) => project.interface_idx(&id).is_some(),
            Selection::DataFlow(id) => project.data_flow_idx(&id).is_some(),
            Selection::TrustBoundary(id) => project.trust_boundary_idx(&id).is_some(),
        };
        exists.then_some(sel)
    }

    /// Whether the edit took something away: a removal or an unlink. These
    /// are the edits that get an undo toast instead of a confirmation.
    #[must_use]
    pub fn is_removal(&self) -> bool {
        matches!(self.verb, "remove" | "unlink" | "unapply")
    }

    /// This edit and `steps` more edits done with it, as one: "add asset
    /// 'Brake ECU' and 2 other changes".
    #[must_use]
    pub fn with_more(mut self, steps: usize) -> Label {
        self.others += steps;
        self.unlinked = 0;
        self.lost_via = 0;
        self
    }

    /// "remove asset 'Brake ECU'", for "Undo …" / "Redo …".
    #[must_use]
    pub fn imperative(&self) -> String {
        format!("{}{}", self.what(), self.extra())
    }

    /// "Asset 'Brake ECU' removed", for the toast after a removal.
    #[must_use]
    pub fn past(&self) -> String {
        let past = match self.verb {
            "add" => "added",
            "apply" => "applied",
            "remove" => "removed",
            "rename" => "renamed",
            "move" => "moved",
            "unapply" => "unapplied",
            "unlink" => "unlinked",
            "rate" => "rated",
            "reset" => "reset",
            _ => "changed",
        };
        if self.count > 0 {
            return format!("{} {} {past}", self.count, self.noun);
        }
        let noun = match self.noun.split_at_checked(1) {
            Some((first, rest)) => format!("{}{rest}", first.to_uppercase()),
            None => self.noun.to_owned(),
        };
        match &self.name {
            Some(n) => format!("{noun} '{}' {past}{}", shorten(n), self.extra()),
            None => format!("{noun} {past}{}", self.extra()),
        }
    }

    fn what(&self) -> String {
        if self.count > 0 {
            return format!("{} {} {}", self.verb, self.count, self.noun);
        }
        match &self.name {
            Some(n) => format!("{} {} '{}'", self.verb, self.noun, shorten(n)),
            None => format!("{} {}", self.verb, self.noun),
        }
    }

    /// UI-10.10: one line saying what else the edit changed.
    fn extra(&self) -> String {
        if self.no_system > 0 {
            let n = self.no_system;
            return format!(" and {n} component{} now in no system", plural(n));
        }
        if self.lost_via > 0 {
            let n = self.lost_via;
            return format!(" and {n} step{} lost their 'through'", plural(n));
        }
        match (self.unlinked, self.others) {
            (0, 0) => String::new(),
            (0, n) => format!(" and {n} other change{}", plural(n)),
            (n, _) => format!(" and unlinked from {n} threat scenario{}", plural(n)),
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn shorten(name: &str) -> String {
    if name.chars().count() <= MAX_NAME_CHARS {
        name.to_owned()
    } else {
        let head: String = name.chars().take(MAX_NAME_CHARS).collect();
        format!("{head}\u{2026}")
    }
}

/// Describes `ops`, one user edit, against `project` as it is before them.
#[must_use]
pub fn describe(ops: &[Op], project: &Project) -> Label {
    if ops.len() >= 2
        && ops
            .iter()
            .all(|op| matches!(op, Op::SetComponentSystem { .. }))
    {
        return Label::bulk("move", "components", ops.len());
    }
    let parts: Vec<_> = ops.iter().map(|op| parts(op, project)).collect();
    // The edit is named after its removal, else its first addition, else its
    // first op: removing a damage scenario is "remove damage scenario", not
    // the unlinks that come first.
    let main = parts
        .iter()
        .rposition(|p| p.0 == "remove")
        .or_else(|| parts.iter().position(|p| p.0 == "add"))
        .unwrap_or(0);
    let (verb, noun, name) = parts
        .get(main)
        .cloned()
        .unwrap_or(("change", "project", None));
    let unlinked = if verb == "remove" {
        ops.iter()
            .filter(|op| matches!(op, Op::UnlinkDamage { .. }))
            .count()
    } else {
        0
    };
    Label {
        verb,
        noun,
        name,
        others: ops.len().saturating_sub(1),
        unlinked,
        lost_via: if verb == "remove" {
            ops.iter()
                .filter(|op| matches!(op, Op::SetStepVia { via: None, .. }))
                .count()
        } else {
            0
        },
        target: ops.get(main).and_then(target_of),
        count: 0,
        no_system: if (verb, noun) == ("remove", "system") {
            ops.iter()
                .filter(|op| matches!(op, Op::SetComponentSystem { system: None, .. }))
                .count()
        } else {
            0
        },
    }
}

/// The element an op is about.
fn target_of(op: &Op) -> Option<Selection> {
    Some(match op {
        Op::AddAsset { id, .. }
        | Op::RenameAsset { id, .. }
        | Op::SetAssetProperties { id, .. }
        | Op::RemoveAsset { id } => Selection::Asset(*id),
        Op::SetAssetComponent { asset, .. } => Selection::Asset(*asset),
        Op::AddDamageScenario { id, .. }
        | Op::RenameDamageScenario { id, .. }
        | Op::SetImpact { id, .. }
        | Op::RemoveDamageScenario { id } => Selection::Damage(*id),
        Op::AddThreatScenario { id, .. }
        | Op::RenameThreatScenario { id, .. }
        | Op::SetThreatCategory { id, .. }
        | Op::RemoveThreatScenario { id } => Selection::Threat(*id),
        Op::AddCatalogRef { threat, .. }
        | Op::RemoveCatalogRef { threat, .. }
        | Op::LinkDamage { threat, .. }
        | Op::UnlinkDamage { threat, .. }
        | Op::SetThreatFeasibility { threat, .. }
        | Op::SetThreatAttackTree { threat, .. }
        | Op::SetRiskTreatment { threat, .. }
        | Op::SetThreatGoal { threat, .. }
        | Op::LinkThreatGoal { threat, .. }
        | Op::UnlinkThreatGoal { threat, .. }
        | Op::SetThreatClaim { threat, .. }
        | Op::SetThreatScaling { threat, .. }
        | Op::SetFeasibilityRationale { threat, .. }
        | Op::SetDamageLinkRationale { threat, .. } => Selection::Threat(*threat),
        Op::SetImpactRationale { damage, .. } => Selection::Damage(*damage),
        Op::AddAttackTree { id, .. } | Op::RemoveAttackTree { id } => Selection::Tree(*id),
        Op::RemoveTreeNode { tree, .. } => Selection::Tree(*tree),
        Op::AddTreeNode { tree, id: node, .. }
        | Op::RenameTreeNode { tree, id: node, .. }
        | Op::SetTreeNodeGate { tree, id: node, .. }
        | Op::SetStepMitigation { tree, node, .. }
        | Op::SetReviewMark { tree, node, .. }
        | Op::UnuseSharedStep { tree, node, .. }
        | Op::LinkTreeNode {
            tree, child: node, ..
        }
        | Op::UnlinkTreeNode {
            tree, child: node, ..
        } => Selection::TreeNode(*tree, *node),
        Op::SetStepFeasibility { target, .. }
        | Op::SetStepFactor { target, .. }
        | Op::SetStepVia { target, .. }
        | Op::SetStepAttackType { target, .. } => {
            return match target {
                StepTarget::Step(tree, node) => Some(Selection::TreeNode(*tree, *node)),
                StepTarget::Shared(id) => Some(Selection::SharedStep(*id)),
            };
        }
        Op::AddSharedStep { id, .. }
        | Op::RenameSharedStep { id, .. }
        | Op::RemoveSharedStep { id } => Selection::SharedStep(*id),
        Op::UseSharedStep { step, .. } => Selection::SharedStep(*step),
        Op::AddSecurityControl { id, .. }
        | Op::RenameSecurityControl { id, .. }
        | Op::RemoveSecurityControl { id } => Selection::Control(*id),
        Op::SetControlRule { control, .. }
        | Op::SetControlKind { control, .. }
        | Op::SetControlStatus { control, .. } => Selection::Control(*control),
        Op::ApplyControl { target, .. } | Op::UnapplyControl { target, .. } => match target {
            aegis_model::ItemRef::Component(id) => Selection::Component(*id),
            aegis_model::ItemRef::Interface(id) => Selection::Interface(*id),
        },
        Op::AddCybersecurityGoal { id, .. }
        | Op::RenameCybersecurityGoal { id, .. }
        | Op::RemoveCybersecurityGoal { id } => Selection::Goal(*id),
        Op::LinkGoalControl { goal, .. } | Op::UnlinkGoalControl { goal, .. } => {
            Selection::Goal(*goal)
        }
        Op::LinkGoalParty { goal, .. } | Op::UnlinkGoalParty { goal, .. } => Selection::Goal(*goal),
        Op::AddCybersecurityClaim { id, .. }
        | Op::RenameCybersecurityClaim { id, .. }
        | Op::RemoveCybersecurityClaim { id } => Selection::Claim(*id),
        Op::LinkClaimParty { claim, .. } | Op::UnlinkClaimParty { claim, .. } => {
            Selection::Claim(*claim)
        }
        Op::LinkClaimAssumption { claim, .. } | Op::UnlinkClaimAssumption { claim, .. } => {
            Selection::Claim(*claim)
        }
        Op::AddAssumption { id, .. }
        | Op::RenameAssumption { id, .. }
        | Op::SetAssumptionHolds { id, .. }
        | Op::SetAssumptionExcludes { id, .. }
        | Op::RemoveAssumption { id } => Selection::Assumption(*id),
        Op::AddComponent { id, .. }
        | Op::RenameComponent { id, .. }
        | Op::SetComponentParent { id, .. }
        | Op::RemoveComponent { id } => Selection::Component(*id),
        Op::AddInterface { id, .. }
        | Op::RenameInterface { id, .. }
        | Op::SetInterfaceKind { id, .. }
        | Op::RemoveInterface { id } => Selection::Interface(*id),
        Op::AddDataFlow { id, .. }
        | Op::RenameDataFlow { id, .. }
        | Op::SetDataFlowInterface { id, .. }
        | Op::RemoveDataFlow { id } => Selection::DataFlow(*id),
        Op::AddTrustBoundary { id, .. }
        | Op::RenameTrustBoundary { id, .. }
        | Op::RemoveTrustBoundary { id } => Selection::TrustBoundary(*id),
        Op::AddBoundaryComponent { boundary, .. }
        | Op::RemoveBoundaryComponent { boundary, .. } => Selection::TrustBoundary(*boundary),
        Op::AddCondition { id, .. }
        | Op::RenameCondition { id, .. }
        | Op::SetConditionPresent { id, .. }
        | Op::RemoveCondition { id } => Selection::Condition(*id),
        Op::NeedStepCondition { tree, node, .. } | Op::UnneedStepCondition { tree, node, .. } => {
            Selection::TreeNode(*tree, *node)
        }
        Op::SetDescription { target, .. } | Op::SetDescriptionTranslation { target, .. } => {
            match target {
                ElementRef::Asset(id) => Selection::Asset(*id),
                ElementRef::Damage(id) => Selection::Damage(*id),
                ElementRef::Threat(id) => Selection::Threat(*id),
                ElementRef::Tree(id) => Selection::Tree(*id),
                ElementRef::Control(id) => Selection::Control(*id),
                ElementRef::Goal(id) => Selection::Goal(*id),
                ElementRef::Claim(id) => Selection::Claim(*id),
                ElementRef::Component(id) => Selection::Component(*id),
                ElementRef::Interface(id) => Selection::Interface(*id),
                ElementRef::DataFlow(id) => Selection::DataFlow(*id),
                ElementRef::TrustBoundary(id) => Selection::TrustBoundary(*id),
                ElementRef::Assumption(id) => Selection::Assumption(*id),
                ElementRef::SharedStep(id) => Selection::SharedStep(*id),
                _ => return None,
            }
        }
        _ => return None,
    })
}

type Parts = (&'static str, &'static str, Option<String>);

/// (verb, noun, element name) of one op.
fn parts(op: &Op, p: &Project) -> Parts {
    let asset = |id: &_| name_of(p, &ElementRef::Asset(*id));
    let damage = |id: &_| name_of(p, &ElementRef::Damage(*id));
    let threat = |id: &_| name_of(p, &ElementRef::Threat(*id));
    let control = |id: &_| name_of(p, &ElementRef::Control(*id));
    let goal = |id: &_| name_of(p, &ElementRef::Goal(*id));
    let claim = |id: &_| name_of(p, &ElementRef::Claim(*id));
    let component = |id: &_| name_of(p, &ElementRef::Component(*id));
    let interface = |id: &_| name_of(p, &ElementRef::Interface(*id));
    let flow = |id: &_| name_of(p, &ElementRef::DataFlow(*id));
    let boundary = |id: &_| name_of(p, &ElementRef::TrustBoundary(*id));
    let assumption = |id: &_| name_of(p, &ElementRef::Assumption(*id));
    let shared = |id: &_| name_of(p, &ElementRef::SharedStep(*id));
    let some = |n: &str| Some(n.to_owned());
    let condition = |id: &aegis_model::ConditionId| {
        p.condition_idx(id)
            .and_then(|i| p.condition(i))
            .map(|c| c.name().to_owned())
    };
    match op {
        Op::AddElement {
            kind: aegis_model::ElementKind::System,
            name,
            ..
        } => ("add", "system", some(name)),
        Op::RenameElement { id, .. } => match system_name(p, *id) {
            Some(n) => ("rename", "system", Some(n)),
            None => ("change", "project", None),
        },
        Op::RemoveElement { id } => match system_name(p, *id) {
            Some(n) => ("remove", "system", Some(n)),
            None => ("change", "project", None),
        },
        Op::SetComponentSystem { id, .. } => ("move", "component", component(id)),
        Op::AddAsset { name, .. } => ("add", "asset", some(name)),
        Op::RenameAsset { id, .. } => ("rename", "asset", asset(id)),
        Op::SetAssetProperties { id, .. } => ("change", "asset properties of", asset(id)),
        Op::SetAssetComponent { asset: a, .. } => ("change", "component of asset", asset(a)),
        Op::RemoveAsset { id } => ("remove", "asset", asset(id)),

        Op::AddDamageScenario { name, .. } => ("add", "damage scenario", some(name)),
        Op::RenameDamageScenario { id, .. } => ("rename", "damage scenario", damage(id)),
        Op::SetImpact { id, .. } => ("rate", "damage scenario", damage(id)),
        Op::RemoveDamageScenario { id } => ("remove", "damage scenario", damage(id)),

        Op::AddThreatScenario { name, .. } => ("add", "threat scenario", some(name)),
        Op::RenameThreatScenario { id, .. } => ("rename", "threat scenario", threat(id)),
        Op::SetThreatCategory { id, .. } => ("change", "category of threat scenario", threat(id)),
        Op::AddCatalogRef { threat: t, .. } => {
            ("change", "catalog references of threat scenario", threat(t))
        }
        Op::RemoveCatalogRef { threat: t, .. } => {
            ("change", "catalog references of threat scenario", threat(t))
        }
        Op::LinkDamage { threat: t, .. } => {
            ("change", "damage links of threat scenario", threat(t))
        }
        Op::UnlinkDamage { threat: t, .. } => {
            ("unlink", "damage link on threat scenario", threat(t))
        }
        Op::SetThreatFeasibility { threat: t, .. } => ("rate", "threat scenario", threat(t)),
        Op::SetThreatAttackTree { threat: t, .. } => {
            ("change", "attack tree of threat scenario", threat(t))
        }
        Op::SetRiskTreatment { threat: t, .. } => {
            ("change", "risk treatment of threat scenario", threat(t))
        }
        Op::SetThreatGoal { threat: t, .. } => ("change", "goal of threat scenario", threat(t)),
        Op::LinkThreatGoal { threat: t, .. } => ("link", "goal to threat scenario", threat(t)),
        Op::UnlinkThreatGoal { threat: t, .. } => {
            ("unlink", "goal from threat scenario", threat(t))
        }
        Op::SetThreatClaim { threat: t, .. } => ("change", "claim of threat scenario", threat(t)),
        Op::SetThreatScaling { threat: t, .. } => {
            ("change", "scaling of threat scenario", threat(t))
        }
        Op::SetFeasibilityRationale { threat: t, .. } => (
            "change",
            "feasibility rationale of threat scenario",
            threat(t),
        ),
        Op::SetDamageLinkRationale { threat: t, .. } => {
            ("change", "pair rationale of threat scenario", threat(t))
        }
        Op::SetImpactRationale { damage: d, .. } => {
            ("change", "impact rationale of damage scenario", damage(d))
        }
        Op::RemoveThreatScenario { id } => ("remove", "threat scenario", threat(id)),

        Op::AddAttackTree { .. } => ("add", "attack tree", None),
        Op::RemoveAttackTree { .. } => ("remove", "attack tree", None),
        Op::AddTreeNode { node, .. } => ("add", "attack tree node", some(node.label())),
        Op::RemoveTreeNode { .. } => ("remove", "attack tree node", None),
        Op::UnlinkTreeNode { .. } => ("unlink", "attack tree node", None),
        Op::RenameTreeNode { .. } => ("rename", "attack tree node", None),
        Op::SetStepFeasibility {
            target: StepTarget::Shared(id),
            ..
        } => ("rate", "shared step", shared(id)),
        Op::SetStepFactor {
            target: StepTarget::Shared(id),
            ..
        } => ("change", "factors of shared step", shared(id)),
        Op::SetStepVia {
            target: StepTarget::Shared(id),
            ..
        } => ("change", "what a shared step goes through", shared(id)),
        Op::SetStepAttackType {
            target: StepTarget::Shared(id),
            ..
        } => ("change", "attack type of shared step", shared(id)),
        Op::SetStepVia { .. } => ("change", "what a step goes through in attack tree", None),
        Op::SetStepAttackType { .. } => ("change", "attack type of a step in attack tree", None),
        Op::LinkTreeNode { .. }
        | Op::SetTreeNodeGate { .. }
        | Op::SetReviewMark { .. }
        | Op::SetStepFeasibility { .. }
        | Op::SetStepFactor { .. }
        | Op::SetStepMitigation { .. } => ("change", "attack tree", None),

        Op::AddSharedStep { name, .. } => ("add", "shared step", some(name)),
        Op::RenameSharedStep { id, .. } => ("rename", "shared step", shared(id)),
        Op::RemoveSharedStep { id } => ("remove", "shared step", shared(id)),
        Op::UseSharedStep { step, .. } => ("use", "shared step", shared(step)),
        Op::UnuseSharedStep { .. } => ("unuse", "shared step", None),

        Op::AddSecurityControl { name, .. } => ("add", "security control", some(name)),
        Op::RenameSecurityControl { id, .. } => ("rename", "security control", control(id)),
        Op::ApplyControl { control: c, .. } => ("apply", "security control", control(c)),
        Op::UnapplyControl { control: c, .. } => ("unapply", "security control", control(c)),
        Op::SetControlRule { control: c, .. } => ("change", "rule of security control", control(c)),
        Op::SetControlKind { control: c, .. } => {
            ("change", "preset of security control", control(c))
        }
        Op::SetControlStatus { control: c, .. } => {
            ("change", "status of security control", control(c))
        }
        Op::RemoveSecurityControl { id } => ("remove", "security control", control(id)),
        Op::AddCybersecurityGoal { name, .. } => ("add", "cybersecurity goal", some(name)),
        Op::RenameCybersecurityGoal { id, .. } => ("rename", "cybersecurity goal", goal(id)),
        Op::LinkGoalControl { goal: g, .. } => {
            ("change", "controls of cybersecurity goal", goal(g))
        }
        Op::UnlinkGoalControl { goal: g, .. } => {
            ("unlink", "control from cybersecurity goal", goal(g))
        }
        Op::LinkGoalParty { goal: g, .. } => (
            "change",
            "responsible parties of cybersecurity goal",
            goal(g),
        ),
        Op::UnlinkGoalParty { goal: g, .. } => (
            "unlink",
            "responsible party from cybersecurity goal",
            goal(g),
        ),
        Op::RemoveCybersecurityGoal { id } => ("remove", "cybersecurity goal", goal(id)),
        Op::AddCybersecurityClaim { name, .. } => ("add", "cybersecurity claim", some(name)),
        Op::RenameCybersecurityClaim { id, .. } => ("rename", "cybersecurity claim", claim(id)),
        Op::LinkClaimParty { claim: c, .. } => (
            "change",
            "responsible parties of cybersecurity claim",
            claim(c),
        ),
        Op::UnlinkClaimParty { claim: c, .. } => (
            "unlink",
            "responsible party from cybersecurity claim",
            claim(c),
        ),
        Op::LinkClaimAssumption { claim: c, .. } => {
            ("change", "assumptions of cybersecurity claim", claim(c))
        }
        Op::UnlinkClaimAssumption { claim: c, .. } => {
            ("unlink", "assumption from cybersecurity claim", claim(c))
        }
        Op::RemoveCybersecurityClaim { id } => ("remove", "cybersecurity claim", claim(id)),

        Op::AddComponent { name, .. } => ("add", "component", some(name)),
        Op::RenameComponent { id, .. } => ("rename", "component", component(id)),
        Op::SetComponentParent { id, .. } => ("change", "parent of component", component(id)),
        Op::RemoveComponent { id } => ("remove", "component", component(id)),
        Op::AddInterface { name, .. } => ("add", "interface", some(name)),
        Op::RenameInterface { id, .. } => ("rename", "interface", interface(id)),
        Op::SetInterfaceKind { id, .. } => ("change", "kind of interface", interface(id)),
        Op::RemoveInterface { id } => ("remove", "interface", interface(id)),
        Op::AddDataFlow { name, .. } => ("add", "data flow", some(name)),
        Op::RenameDataFlow { id, .. } => ("rename", "data flow", flow(id)),
        Op::SetDataFlowInterface { id, .. } => ("change", "interface of data flow", flow(id)),
        Op::RemoveDataFlow { id } => ("remove", "data flow", flow(id)),
        Op::AddTrustBoundary { name, .. } => ("add", "trust boundary", some(name)),
        Op::RenameTrustBoundary { id, .. } => ("rename", "trust boundary", boundary(id)),
        Op::AddBoundaryComponent { boundary: b, .. }
        | Op::RemoveBoundaryComponent { boundary: b, .. } => {
            ("change", "components of trust boundary", boundary(b))
        }
        Op::RemoveTrustBoundary { id } => ("remove", "trust boundary", boundary(id)),

        Op::AddCondition { name, .. } => ("add", "condition", some(name)),
        Op::RenameCondition { id, .. } => ("rename", "condition", condition(id)),
        Op::SetConditionPresent { id, present, .. } => (
            if *present { "switch on" } else { "switch off" },
            "condition",
            condition(id),
        ),
        Op::RemoveCondition { id } => ("remove", "condition", condition(id)),
        Op::AddAssumption { name, .. } => ("add", "assumption", some(name)),
        Op::RenameAssumption { id, .. } => ("rename", "assumption", assumption(id)),
        Op::SetAssumptionHolds { id, holds, .. } => (
            if *holds { "switch on" } else { "switch off" },
            "assumption",
            assumption(id),
        ),
        Op::SetAssumptionExcludes { id, .. } => {
            ("change", "exclusions of assumption", assumption(id))
        }
        Op::RemoveAssumption { id } => ("remove", "assumption", assumption(id)),
        Op::NeedCondition { element, .. } => ("change", "conditions of", name_of(p, element)),
        Op::UnneedCondition { element, .. } => ("clear", "condition of", name_of(p, element)),
        Op::NeedStepCondition { condition: c, .. } => ("need", "condition on step", condition(c)),
        Op::UnneedStepCondition { condition: c, .. } => {
            ("clear", "condition on step", condition(c))
        }
        Op::SetDescription { target, .. } => ("change", "description of", name_of(p, target)),
        Op::SetDescriptionTranslation { target, .. } => {
            ("change", "description translation of", name_of(p, target))
        }
        Op::SetAttackFilter { filter, .. } => (
            if filter.is_default() {
                "reset"
            } else {
                "change"
            },
            "attacker profile",
            None,
        ),
        _ => ("change", "project", None),
    }
}

/// The name of an element, if it exists.
fn name_of(p: &Project, e: &ElementRef) -> Option<String> {
    let name = match e {
        ElementRef::Asset(id) => p.asset(p.asset_idx(id)?)?.name(),
        ElementRef::Damage(id) => p.damage_scenario(p.damage_scenario_idx(id)?)?.name(),
        ElementRef::Threat(id) => p.threat_scenario(p.threat_scenario_idx(id)?)?.name(),
        ElementRef::Control(id) => p.security_control(p.security_control_idx(id)?)?.name(),
        ElementRef::Goal(id) => p.cybersecurity_goal(p.cybersecurity_goal_idx(id)?)?.name(),
        ElementRef::Claim(id) => p
            .cybersecurity_claim(p.cybersecurity_claim_idx(id)?)?
            .name(),
        ElementRef::Component(id) => p.component(p.component_idx(id)?)?.name(),
        ElementRef::Interface(id) => p.interface(p.interface_idx(id)?)?.name(),
        ElementRef::DataFlow(id) => p.data_flow(p.data_flow_idx(id)?)?.name(),
        ElementRef::TrustBoundary(id) => p.trust_boundary(p.trust_boundary_idx(id)?)?.name(),
        ElementRef::Assumption(id) => p.assumption(p.assumption_idx(id)?)?.name(),
        ElementRef::SharedStep(id) => p.shared_step(p.shared_step_idx(id)?)?.name(),
        _ => return None,
    };
    Some(name.to_owned())
}

/// The name of the system `id`, if it is one.
fn system_name(p: &Project, id: aegis_model::ElementId) -> Option<String> {
    p.element(id)
        .filter(|e| e.kind() == aegis_model::ElementKind::System)
        .map(|e| e.name().to_owned())
}
