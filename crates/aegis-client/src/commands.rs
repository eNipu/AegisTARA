//! What the user can ask for.
//!
//! [`Command`] is the whole interface between a UI and the session. Edits
//! name entities by their stable IDs (taken from the rows the view model
//! offered) and vocabulary by *positions* in the lists the view model
//! offered (a dropdown index), never by model internals; [`Edit::to_ops`]
//! resolves them against the methodology and builds the typed `Op`s.
//!
//! Removals cascade the links that would block them, in the same edit, so
//! one undo brings everything back: removing a damage scenario unlinks it
//! from its threats first, removing a control detaches it from its steps and
//! goals first, and so on. An asset that threats target is not removed:
//! the user decides what happens to those threats.

use aegis_model::{
    AssetId, AttackTree, ClaimId, ControlId, DamageId, GoalId, NodeId, Op, Project, PropertySet,
    Provenance, RiskTreatment, ThreatId, TreeId,
};

use crate::outline::{OutlineError, parse_outline};

/// The treatment decisions, in the order the view offers them.
pub const TREATMENTS: [RiskTreatment; 4] = [
    RiskTreatment::Avoid,
    RiskTreatment::Reduce,
    RiskTreatment::Share,
    RiskTreatment::Retain,
];

/// A change to the project, as the UI states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    // ---- descriptions and notes ----
    /// Sets the description of an element, or the notes of the project
    /// ([`ElementRef::Project`]). Empty text clears it.
    SetDescription {
        /// What the text belongs to.
        target: aegis_model::ElementRef,
        /// The text; line breaks are normalized to `\n`.
        text: String,
    },

    // ---- attack tree steps ----
    /// Sets or clears the direct feasibility rating of a step.
    SetStepFeasibility {
        /// Tree.
        tree: TreeId,
        /// Step node.
        node: NodeId,
        /// Position in the feasibility levels, or `None` for unrated.
        level: Option<usize>,
    },
    /// Sets or clears one attack potential factor of a step.
    SetStepFactor {
        /// Tree.
        tree: TreeId,
        /// Step node.
        node: NodeId,
        /// Position in the methodology's factors.
        factor: usize,
        /// Position in that factor's levels, or `None` for unrated.
        level: Option<usize>,
    },
    /// Attaches a control to a step with a residual feasibility cap, changes
    /// the cap, or detaches it (`None`).
    SetStepMitigation {
        /// Tree.
        tree: TreeId,
        /// Step node.
        node: NodeId,
        /// The control.
        control: ControlId,
        /// Position in the feasibility levels, or `None` to detach.
        level: Option<usize>,
    },

    // ---- assets ----
    /// Adds an asset with the properties at these positions.
    AddAsset {
        /// New ID.
        id: AssetId,
        /// Name.
        name: String,
        /// Positions in the methodology's properties.
        properties: Vec<usize>,
    },
    /// Renames an asset.
    RenameAsset {
        /// The asset.
        asset: AssetId,
        /// New name.
        name: String,
    },
    /// Turns one security property of an asset on or off.
    SetAssetProperty {
        /// The asset.
        asset: AssetId,
        /// Position in the methodology's properties.
        property: usize,
        /// On or off.
        on: bool,
    },
    /// Removes an asset that no threat targets.
    RemoveAsset {
        /// The asset.
        asset: AssetId,
    },

    // ---- damage scenarios ----
    /// Adds an unrated damage scenario.
    AddDamage {
        /// New ID.
        id: DamageId,
        /// Name.
        name: String,
    },
    /// Renames a damage scenario.
    RenameDamage {
        /// The damage scenario.
        damage: DamageId,
        /// New name.
        name: String,
    },
    /// Sets or clears one impact rating.
    SetImpact {
        /// The damage scenario.
        damage: DamageId,
        /// Position in the impact categories.
        category: usize,
        /// Position in that category's levels, or `None` for unrated.
        level: Option<usize>,
    },
    /// Removes a damage scenario, unlinking it from its threats first.
    RemoveDamage {
        /// The damage scenario.
        damage: DamageId,
    },

    // ---- threat scenarios ----
    /// Adds a threat scenario on an asset.
    AddThreat {
        /// New ID.
        id: ThreatId,
        /// Name.
        name: String,
        /// The asset it targets.
        asset: AssetId,
        /// Position in the threat categories.
        category: usize,
    },
    /// Renames a threat scenario.
    RenameThreat {
        /// The threat.
        threat: ThreatId,
        /// The new name.
        name: String,
    },
    /// Changes a threat's category.
    SetThreatCategory {
        /// The threat.
        threat: ThreatId,
        /// Position in the threat categories.
        category: usize,
    },
    /// Links or unlinks a damage scenario.
    LinkDamage {
        /// The threat.
        threat: ThreatId,
        /// The damage scenario.
        damage: DamageId,
        /// Link or unlink.
        linked: bool,
    },
    /// Adds a catalog reference (for example UN R155 Annex 5 `4.1`).
    AddCatalogRef {
        /// The threat.
        threat: ThreatId,
        /// Position in the catalog schemes.
        scheme: usize,
        /// The reference text.
        text: String,
    },
    /// Removes the catalog reference at a position in the threat's list.
    RemoveCatalogRef {
        /// The threat.
        threat: ThreatId,
        /// Position in the threat's catalog refs.
        index: usize,
    },
    /// Sets or clears the direct feasibility rating of a threat.
    SetThreatFeasibility {
        /// The threat.
        threat: ThreatId,
        /// Position in the feasibility levels, or `None`.
        level: Option<usize>,
    },
    /// Links an attack tree to a threat, or unlinks it.
    SetThreatTree {
        /// The threat.
        threat: ThreatId,
        /// The tree, or `None`.
        tree: Option<TreeId>,
    },
    /// Sets or clears the treatment decision.
    SetTreatment {
        /// The threat.
        threat: ThreatId,
        /// Position in [`TREATMENTS`], or `None`.
        treatment: Option<usize>,
    },
    /// Links a cybersecurity goal to a threat, or unlinks it.
    SetThreatGoal {
        /// The threat.
        threat: ThreatId,
        /// The goal, or `None`.
        goal: Option<GoalId>,
    },
    /// Links a cybersecurity claim to a threat, or unlinks it.
    SetThreatClaim {
        /// The threat.
        threat: ThreatId,
        /// The claim, or `None`.
        claim: Option<ClaimId>,
    },
    /// Removes a threat scenario, clearing what blocks the removal first
    /// (tree link, ratings, treatment, goal, claim).
    RemoveThreat {
        /// The threat.
        threat: ThreatId,
    },

    // ---- attack trees ----
    /// Creates a tree with one OR gate, optionally linked to a threat.
    NewTree {
        /// New ID.
        id: TreeId,
        /// Text of the root gate.
        root: String,
        /// The threat to link it to.
        threat: Option<ThreatId>,
    },
    /// Replaces a tree's structure with an outline (see [`crate::outline`]).
    /// Nodes that keep their text keep their ratings, factors and controls.
    ReplaceTree {
        /// The tree.
        tree: TreeId,
        /// The outline.
        outline: String,
    },
    /// Removes a tree, unlinking threats and clearing factors and controls
    /// first.
    RemoveTree {
        /// The tree.
        tree: TreeId,
    },

    // ---- controls, goals, claims ----
    /// Adds a security control.
    AddControl {
        /// New ID.
        id: ControlId,
        /// Name.
        name: String,
    },
    /// Renames a security control.
    RenameControl {
        /// The control.
        control: ControlId,
        /// New name.
        name: String,
    },
    /// Removes a control, detaching it from steps and goals first.
    RemoveControl {
        /// The control.
        control: ControlId,
    },
    /// Adds a cybersecurity goal.
    AddGoal {
        /// New ID.
        id: GoalId,
        /// Name.
        name: String,
    },
    /// Renames a cybersecurity goal.
    RenameGoal {
        /// The goal.
        goal: GoalId,
        /// New name.
        name: String,
    },
    /// Links or unlinks a control on a goal.
    LinkGoalControl {
        /// The goal.
        goal: GoalId,
        /// The control.
        control: ControlId,
        /// Link or unlink.
        linked: bool,
    },
    /// Removes a goal, unlinking it from threats first.
    RemoveGoal {
        /// The goal.
        goal: GoalId,
    },
    /// Adds a cybersecurity claim.
    AddClaim {
        /// New ID.
        id: ClaimId,
        /// Statement.
        name: String,
    },
    /// Renames a cybersecurity claim.
    RenameClaim {
        /// The claim.
        claim: ClaimId,
        /// New statement.
        name: String,
    },
    /// Removes a claim, unlinking it from threats first.
    RemoveClaim {
        /// The claim.
        claim: ClaimId,
    },
}

/// Why an edit could not be turned into ops. The text is for the user.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// The first line of an outline would turn the root from a gate into a
    /// step or the other way round.
    #[error("the first line must stay a gate (OR, AND, SEQ), or a step if the tree is one step")]
    RootKind,
    /// The feasibility level position does not exist.
    #[error("unknown feasibility level")]
    UnknownLevel,
    /// The factor position does not exist.
    #[error("unknown factor")]
    UnknownFactor,
    /// The factor level position does not exist.
    #[error("unknown factor level")]
    UnknownFactorLevel,
    /// A vocabulary position (property, category, scheme) does not exist.
    #[error("unknown choice")]
    UnknownChoice,
    /// The entity is gone (another editor removed it, or an undo).
    #[error("it no longer exists")]
    Missing,
    /// An empty name.
    #[error("the name is empty")]
    EmptyName,
    /// An asset needs at least one security property.
    #[error("an asset needs at least one security property")]
    NoProperties,
    /// The asset is still targeted by threats.
    #[error(
        "{0} threat scenario(s) still target this asset; remove them or change their asset first"
    )]
    AssetInUse(usize),
    /// The outline is not a tree.
    #[error("{0}")]
    Outline(#[from] OutlineError),
    /// The catalog reference does not parse.
    #[error("the reference does not match the catalog's format")]
    BadCatalogRef,
}

/// An element with a description cannot be removed, so every removal in an
/// edit is preceded by an op that clears its description. The edit is one
/// undo step, and undo brings back the element and then its text.
fn clear_descriptions_before_removal(
    project: &Project,
    ops: Vec<Op>,
    provenance: &Provenance,
) -> Vec<Op> {
    use aegis_model::ElementRef as E;
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        let target = match &op {
            Op::RemoveAsset { id } => Some(E::Asset(*id)),
            Op::RemoveDamageScenario { id } => Some(E::Damage(*id)),
            Op::RemoveThreatScenario { id } => Some(E::Threat(*id)),
            Op::RemoveAttackTree { id } => Some(E::Tree(*id)),
            Op::RemoveSecurityControl { id } => Some(E::Control(*id)),
            Op::RemoveCybersecurityGoal { id } => Some(E::Goal(*id)),
            Op::RemoveCybersecurityClaim { id } => Some(E::Claim(*id)),
            _ => None,
        };
        if let Some(target) = target.filter(|t| project.description(*t).is_some()) {
            out.push(Op::SetDescription {
                target,
                text: "".into(),
                provenance: provenance.clone(),
            });
        }
        out.push(op);
    }
    out
}

fn name(text: &str) -> Result<std::sync::Arc<str>, EditError> {
    let t = text.trim();
    if t.is_empty() {
        Err(EditError::EmptyName)
    } else {
        Ok(t.into())
    }
}

fn feasibility(
    project: &Project,
    level: Option<usize>,
) -> Result<Option<aegis_model::FeasibilityLevel>, EditError> {
    level
        .map(|i| {
            project
                .methodology()
                .feasibility_levels()
                .nth(i)
                .map(|(l, _, _)| l)
                .ok_or(EditError::UnknownLevel)
        })
        .transpose()
}

/// The node without its children, for a tree reduced to its root.
fn root_node_without_children(node: &aegis_model::Node) -> aegis_model::Node {
    match node.gate_kind() {
        Some(kind) => aegis_model::Node::gate(kind, node.label()),
        None => aegis_model::Node::step(node.label()),
    }
}

fn threats_on_tree(project: &Project, tree: TreeId) -> Vec<ThreatId> {
    let Some(t_idx) = project.attack_tree_idx(&tree) else {
        return Vec::new();
    };
    project
        .threat_scenarios()
        .filter(|(_, _, t)| t.attack_tree() == Some(t_idx))
        .map(|(_, id, _)| id)
        .collect()
}

impl Edit {
    /// Builds the ops for this edit against `project` and its methodology.
    /// The provenance records who made the change and when.
    ///
    /// # Errors
    ///
    /// [`EditError`] if a position does not exist, an entity is gone, or
    /// the input is invalid. The model checks everything else when the ops
    /// are applied.
    #[allow(clippy::too_many_lines)]
    pub fn to_ops(
        &self,
        project: &Project,
        actor: &str,
        now_ms: u64,
    ) -> Result<Vec<Op>, EditError> {
        let m = project.methodology();
        let p = Provenance::human(actor, now_ms);
        let clearing = p.clone();
        let ops = match self {
            Edit::SetDescription { target, text } => vec![Op::SetDescription {
                target: *target,
                text: text.replace("\r\n", "\n").replace('\r', "\n").into(),
                provenance: p,
            }],
            Edit::SetStepFeasibility { tree, node, level } => vec![Op::SetStepFeasibility {
                tree: *tree,
                node: *node,
                level: feasibility(project, *level)?,
                provenance: p,
            }],
            Edit::SetStepFactor {
                tree,
                node,
                factor,
                level,
            } => {
                let f = m
                    .feasibility_factors()
                    .nth(*factor)
                    .ok_or(EditError::UnknownFactor)?;
                let level = match level {
                    Some(i) => Some(
                        m.factor_levels(f)
                            .nth(*i)
                            .ok_or(EditError::UnknownFactorLevel)?,
                    ),
                    None => None,
                };
                vec![Op::SetStepFactor {
                    tree: *tree,
                    node: *node,
                    factor: f,
                    level,
                    provenance: p,
                }]
            }
            Edit::SetStepMitigation {
                tree,
                node,
                control,
                level,
            } => vec![Op::SetStepMitigation {
                tree: *tree,
                node: *node,
                control: *control,
                feasibility: feasibility(project, *level)?,
                provenance: p,
            }],

            Edit::AddAsset {
                id,
                name: n,
                properties,
            } => {
                let mut set = PropertySet::default();
                for &i in properties {
                    set = set.with(m.properties().nth(i).ok_or(EditError::UnknownChoice)?);
                }
                if set.is_empty() {
                    return Err(EditError::NoProperties);
                }
                vec![Op::AddAsset {
                    id: *id,
                    name: name(n)?,
                    properties: set,
                    provenance: p,
                }]
            }
            Edit::RenameAsset { asset, name: n } => vec![Op::RenameAsset {
                id: *asset,
                name: name(n)?,
                provenance: p,
            }],
            Edit::SetAssetProperty {
                asset,
                property,
                on,
            } => {
                let a = project
                    .asset_idx(asset)
                    .and_then(|i| project.asset(i))
                    .ok_or(EditError::Missing)?;
                let prop = m
                    .properties()
                    .nth(*property)
                    .ok_or(EditError::UnknownChoice)?;
                let set = if *on {
                    a.properties().with(prop)
                } else {
                    a.properties()
                        .iter()
                        .filter(|q| *q != prop)
                        .fold(PropertySet::default(), PropertySet::with)
                };
                if set.is_empty() {
                    return Err(EditError::NoProperties);
                }
                vec![Op::SetAssetProperties {
                    id: *asset,
                    properties: set,
                    provenance: p,
                }]
            }
            Edit::RemoveAsset { asset } => {
                let idx = project.asset_idx(asset).ok_or(EditError::Missing)?;
                let threats = project
                    .threat_scenarios()
                    .filter(|(_, _, t)| t.asset() == idx)
                    .count();
                if threats > 0 {
                    return Err(EditError::AssetInUse(threats));
                }
                vec![Op::RemoveAsset { id: *asset }]
            }

            Edit::AddDamage { id, name: n } => vec![Op::AddDamageScenario {
                id: *id,
                name: name(n)?,
                impacts: Vec::new(),
                provenance: p,
            }],
            Edit::RenameDamage { damage, name: n } => vec![Op::RenameDamageScenario {
                id: *damage,
                name: name(n)?,
                provenance: p,
            }],
            Edit::SetImpact {
                damage,
                category,
                level,
            } => {
                let cat = m
                    .impact_categories()
                    .nth(*category)
                    .ok_or(EditError::UnknownChoice)?;
                let level = level
                    .map(|i| m.levels(cat).nth(i).ok_or(EditError::UnknownLevel))
                    .transpose()?;
                vec![Op::SetImpact {
                    id: *damage,
                    category: cat,
                    level,
                    provenance: p,
                }]
            }
            Edit::RemoveDamage { damage } => {
                let d_idx = project
                    .damage_scenario_idx(damage)
                    .ok_or(EditError::Missing)?;
                let mut ops: Vec<Op> = project
                    .threat_scenarios()
                    .filter(|(_, _, t)| t.damages().contains(&d_idx))
                    .map(|(_, threat, _)| Op::UnlinkDamage {
                        threat,
                        damage: *damage,
                        provenance: p.clone(),
                    })
                    .collect();
                ops.push(Op::RemoveDamageScenario { id: *damage });
                ops
            }

            Edit::AddThreat {
                id,
                name: n,
                asset,
                category,
            } => vec![Op::AddThreatScenario {
                id: *id,
                name: name(n)?,
                asset: *asset,
                category: m
                    .threat_categories()
                    .nth(*category)
                    .ok_or(EditError::UnknownChoice)?,
                catalog_refs: Vec::new(),
                damages: Vec::new(),
                provenance: p,
            }],
            Edit::RenameThreat { threat, name: n } => vec![Op::RenameThreatScenario {
                id: *threat,
                name: name(n)?,
                provenance: p,
            }],
            Edit::SetThreatCategory { threat, category } => vec![Op::SetThreatCategory {
                id: *threat,
                category: m
                    .threat_categories()
                    .nth(*category)
                    .ok_or(EditError::UnknownChoice)?,
                provenance: p,
            }],
            Edit::LinkDamage {
                threat,
                damage,
                linked,
            } => vec![if *linked {
                Op::LinkDamage {
                    threat: *threat,
                    damage: *damage,
                    provenance: p,
                }
            } else {
                Op::UnlinkDamage {
                    threat: *threat,
                    damage: *damage,
                    provenance: p,
                }
            }],
            Edit::AddCatalogRef {
                threat,
                scheme,
                text,
            } => {
                let s = m
                    .catalog_schemes()
                    .nth(*scheme)
                    .ok_or(EditError::UnknownChoice)?;
                let catalog_ref = m
                    .parse_catalog_ref(s, text.trim())
                    .map_err(|_| EditError::BadCatalogRef)?;
                vec![Op::AddCatalogRef {
                    threat: *threat,
                    catalog_ref,
                    provenance: p,
                }]
            }
            Edit::RemoveCatalogRef { threat, index } => {
                let t = project
                    .threat_scenario_idx(threat)
                    .and_then(|i| project.threat_scenario(i))
                    .ok_or(EditError::Missing)?;
                let catalog_ref = *t.catalog_refs().get(*index).ok_or(EditError::Missing)?;
                vec![Op::RemoveCatalogRef {
                    threat: *threat,
                    catalog_ref,
                    provenance: p,
                }]
            }
            Edit::SetThreatFeasibility { threat, level } => vec![Op::SetThreatFeasibility {
                threat: *threat,
                level: feasibility(project, *level)?,
                provenance: p,
            }],
            Edit::SetThreatTree { threat, tree } => vec![Op::SetThreatAttackTree {
                threat: *threat,
                tree: *tree,
                provenance: p,
            }],
            Edit::SetTreatment { threat, treatment } => vec![Op::SetRiskTreatment {
                threat: *threat,
                treatment: treatment
                    .map(|i| TREATMENTS.get(i).copied().ok_or(EditError::UnknownChoice))
                    .transpose()?,
                provenance: p,
            }],
            Edit::SetThreatGoal { threat, goal } => vec![Op::SetThreatGoal {
                threat: *threat,
                goal: *goal,
                provenance: p,
            }],
            Edit::SetThreatClaim { threat, claim } => vec![Op::SetThreatClaim {
                threat: *threat,
                claim: *claim,
                provenance: p,
            }],
            Edit::RemoveThreat { threat } => {
                let t = project
                    .threat_scenario_idx(threat)
                    .and_then(|i| project.threat_scenario(i))
                    .ok_or(EditError::Missing)?;
                let mut ops = Vec::new();
                if t.attack_tree().is_some() {
                    ops.push(Op::SetThreatAttackTree {
                        threat: *threat,
                        tree: None,
                        provenance: p.clone(),
                    });
                }
                if t.direct_feasibility().is_some() {
                    ops.push(Op::SetThreatFeasibility {
                        threat: *threat,
                        level: None,
                        provenance: p.clone(),
                    });
                }
                if t.treatment().is_some() {
                    ops.push(Op::SetRiskTreatment {
                        threat: *threat,
                        treatment: None,
                        provenance: p.clone(),
                    });
                }
                if t.goal().is_some() {
                    ops.push(Op::SetThreatGoal {
                        threat: *threat,
                        goal: None,
                        provenance: p.clone(),
                    });
                }
                if t.claim().is_some() {
                    ops.push(Op::SetThreatClaim {
                        threat: *threat,
                        claim: None,
                        provenance: p.clone(),
                    });
                }
                ops.push(Op::RemoveThreatScenario { id: *threat });
                ops
            }

            Edit::NewTree { id, root, threat } => {
                let tree = AttackTree::new(
                    NodeId::new_v7(),
                    aegis_model::Node::gate(aegis_model::GateKind::Or, &*name(root)?),
                )
                .map_err(|_| EditError::EmptyName)?;
                let mut ops = vec![Op::AddAttackTree {
                    id: *id,
                    tree,
                    step_feasibility: Vec::new(),
                    provenance: p.clone(),
                }];
                if let Some(threat) = threat {
                    ops.push(Op::SetThreatAttackTree {
                        threat: *threat,
                        tree: Some(*id),
                        provenance: p,
                    });
                }
                ops
            }
            Edit::ReplaceTree { tree, outline } => {
                let old = project
                    .attack_tree_idx(tree)
                    .and_then(|i| project.attack_tree(i))
                    .ok_or(EditError::Missing)?;
                let new = parse_outline(outline, Some(old))?;
                crate::tree_diff::replace_tree_ops(project, *tree, &new, &p)?
            }
            Edit::RemoveTree { tree } => {
                if project.attack_tree_idx(tree).is_none() {
                    return Err(EditError::Missing);
                }
                let mut ops: Vec<Op> = threats_on_tree(project, *tree)
                    .into_iter()
                    .map(|threat| Op::SetThreatAttackTree {
                        threat,
                        tree: None,
                        provenance: p.clone(),
                    })
                    .collect();
                // Take the tree apart node by node: an op carries a whole tree
                // only up to MAX_TREE_OP_NODES nodes.
                let old = project
                    .attack_tree_idx(tree)
                    .and_then(|i| project.attack_tree(i))
                    .ok_or(EditError::Missing)?;
                let root = old.root();
                let root_id = old.id_of(root).ok_or(EditError::Missing)?;
                let root_node = old.get(root).ok_or(EditError::Missing)?;
                let bare = AttackTree::new(root_id, root_node_without_children(root_node))
                    .map_err(|_| EditError::Missing)?;
                ops.extend(crate::tree_diff::replace_tree_ops(
                    project, *tree, &bare, &p,
                )?);
                if root_node.gate_kind().is_none() {
                    ops.extend(crate::tree_diff::clear_step_ops(
                        project, *tree, root_id, &p,
                    ));
                }
                ops.push(Op::RemoveAttackTree { id: *tree });
                ops
            }

            Edit::AddControl { id, name: n } => vec![Op::AddSecurityControl {
                id: *id,
                name: name(n)?,
                provenance: p,
            }],
            Edit::RenameControl { control, name: n } => vec![Op::RenameSecurityControl {
                id: *control,
                name: name(n)?,
                provenance: p,
            }],
            Edit::RemoveControl { control } => {
                let c_idx = project
                    .security_control_idx(control)
                    .ok_or(EditError::Missing)?;
                let mut ops = Vec::new();
                for (t_idx, tree_id, tree) in project.attack_trees() {
                    for (n_idx, node_id, _) in tree.iter() {
                        if project
                            .step_mitigations(t_idx, n_idx)
                            .iter()
                            .any(|s| s.control() == c_idx)
                        {
                            ops.push(Op::SetStepMitigation {
                                tree: tree_id,
                                node: node_id,
                                control: *control,
                                feasibility: None,
                                provenance: p.clone(),
                            });
                        }
                    }
                }
                for (_, goal_id, goal) in project.cybersecurity_goals() {
                    if goal.controls().contains(&c_idx) {
                        ops.push(Op::UnlinkGoalControl {
                            goal: goal_id,
                            control: *control,
                            provenance: p.clone(),
                        });
                    }
                }
                ops.push(Op::RemoveSecurityControl { id: *control });
                ops
            }
            Edit::AddGoal { id, name: n } => vec![Op::AddCybersecurityGoal {
                id: *id,
                name: name(n)?,
                controls: Vec::new(),
                provenance: p,
            }],
            Edit::RenameGoal { goal, name: n } => vec![Op::RenameCybersecurityGoal {
                id: *goal,
                name: name(n)?,
                provenance: p,
            }],
            Edit::LinkGoalControl {
                goal,
                control,
                linked,
            } => vec![if *linked {
                Op::LinkGoalControl {
                    goal: *goal,
                    control: *control,
                    provenance: p,
                }
            } else {
                Op::UnlinkGoalControl {
                    goal: *goal,
                    control: *control,
                    provenance: p,
                }
            }],
            Edit::RemoveGoal { goal } => {
                let g_idx = project
                    .cybersecurity_goal_idx(goal)
                    .ok_or(EditError::Missing)?;
                let mut ops: Vec<Op> = project
                    .threat_scenarios()
                    .filter(|(_, _, t)| t.goal() == Some(g_idx))
                    .map(|(_, threat, _)| Op::SetThreatGoal {
                        threat,
                        goal: None,
                        provenance: p.clone(),
                    })
                    .collect();
                ops.push(Op::RemoveCybersecurityGoal { id: *goal });
                ops
            }
            Edit::AddClaim { id, name: n } => vec![Op::AddCybersecurityClaim {
                id: *id,
                name: name(n)?,
                provenance: p,
            }],
            Edit::RenameClaim { claim, name: n } => vec![Op::RenameCybersecurityClaim {
                id: *claim,
                name: name(n)?,
                provenance: p,
            }],
            Edit::RemoveClaim { claim } => {
                let c_idx = project
                    .cybersecurity_claim_idx(claim)
                    .ok_or(EditError::Missing)?;
                let mut ops: Vec<Op> = project
                    .threat_scenarios()
                    .filter(|(_, _, t)| t.claim() == Some(c_idx))
                    .map(|(_, threat, _)| Op::SetThreatClaim {
                        threat,
                        claim: None,
                        provenance: p.clone(),
                    })
                    .collect();
                ops.push(Op::RemoveCybersecurityClaim { id: *claim });
                ops
            }
        };
        Ok(clear_descriptions_before_removal(project, ops, &clearing))
    }

    /// The entity this edit creates, so the session can select it.
    #[must_use]
    pub fn created(&self) -> Option<crate::view::Selection> {
        use crate::view::Selection;
        match self {
            Edit::AddAsset { id, .. } => Some(Selection::Asset(*id)),
            Edit::AddDamage { id, .. } => Some(Selection::Damage(*id)),
            Edit::AddThreat { id, .. } => Some(Selection::Threat(*id)),
            Edit::NewTree { id, .. } => Some(Selection::Tree(*id)),
            Edit::AddControl { id, .. } => Some(Selection::Control(*id)),
            Edit::AddGoal { id, .. } => Some(Selection::Goal(*id)),
            Edit::AddClaim { id, .. } => Some(Selection::Claim(*id)),
            _ => None,
        }
    }
}

/// Everything a UI can tell the session to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Connect again (after a login, or after the server was unreachable).
    Reconnect,
    /// Reload the project list.
    Refresh,
    /// Create an empty project and open it.
    CreateProject {
        /// Display name.
        name: String,
        /// Methodology key.
        methodology_key: String,
    },
    /// Open a project (read-only until [`Command::BeginEdit`]).
    OpenProject {
        /// The project ID as listed.
        id: String,
    },
    /// Close the open project.
    CloseProject,
    /// Show a page of the open project.
    ShowPage(crate::view::Page),
    /// Select an entity (and show the page it belongs to when `navigate`).
    Select {
        /// What to select.
        selection: crate::view::Selection,
        /// Also switch to the entity's page.
        navigate: bool,
    },
    /// Show the tree of the threat at this position in the threat list.
    SelectThreat(usize),
    /// Select the node at this position in the scene, or none.
    SelectNode(Option<usize>),
    /// Show the residual (after controls) evaluation instead of the inherent one.
    ShowResidual(bool),
    /// Reload the open project from the server (a read-only view whose
    /// project has a newer version).
    Reload,
    /// Take the edit lock.
    BeginEdit,
    /// Finish pending edits and give the edit lock up.
    EndEdit,
    /// Change the project.
    Edit(Edit),
    /// Undo the last edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
    /// Fill an empty project with a small example TARA.
    CreateSample,
    /// Rename the open project (needs the edit lock).
    RenameProject(String),
    /// Delete a project (admin).
    DeleteProject(String),
    /// Load the open project's baselines.
    LoadBaselines,
    /// Freeze the open project's saved state as a baseline.
    CreateBaseline(String),
    /// Compare the current state with a baseline (by ID), or stop comparing.
    CompareWith(Option<String>),
    /// Load the open project's recent changes.
    LoadHistory,
    /// Write a report or table of the open project to a file.
    Export {
        /// What to write.
        kind: crate::report::ExportKind,
        /// Where.
        path: std::path::PathBuf,
    },
    /// Admin: load the users.
    LoadUsers,
    /// Admin: load the server's event log (logins, exports, admin actions).
    LoadEvents,
    /// Admin: load the open sessions (password login).
    LoadSessions,
    /// Admin: create, change or delete a user.
    Admin(crate::view::AdminAction),
    /// Admin: break the open project's edit lock.
    BreakLock,
    /// Admin: check the open project's audit chain.
    VerifyAudit,
    /// Change the user's own password.
    ChangePassword {
        /// The current password.
        current: String,
        /// The new password.
        new: String,
    },
    /// End the session on the server; the UI then shows the login.
    LogOut,
    /// Remove a notice from the list.
    DismissNotice(u64),
    /// Fetch the signed update manifest and say whether a newer release
    /// exists (the answer is a notice).
    CheckForUpdates,
    /// Download the installer of the release the last check offered, check
    /// its size and SHA-256 against the signed manifest, and say where it is
    /// (the answer is a notice). Nothing is run.
    DownloadUpdate,
    /// Stop the session.
    Shutdown,
}
