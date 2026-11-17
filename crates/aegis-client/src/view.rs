//! What the user sees, as plain data.
//!
//! The view model is computed from a [`Workspace`] and the user's selection.
//! It contains no toolkit types and no model indices the UI could misuse:
//! the UI shows labels, sends back *positions in the lists it was given*
//! (an option index, a threat position), and the session maps them to typed
//! model values. Replacing the UI toolkit means rewriting only the code that
//! turns this structure into widgets.
//!
//! Everything here is derived data (D10): drop it and rebuild it from the
//! workspace at any time. Building it for a project of 2,000 threats reads the
//! incremental cache, so it costs O(threats), not a re-evaluation.

use std::collections::HashMap;
use std::sync::Arc;

use aegis_canvas::{LayoutConfig, NodeKind, NodeStyle, Scene, Tone};
use aegis_model::{
    AssetId, AttackTree, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, ElementRef,
    FeasibilityLevel, GateKind, GoalId, InterfaceId, ItemRef, Methodology, NodeId, NodeIdx,
    Project, SuggestionId, ThreatId, TreeId, TreeIdx, TrustBoundaryId,
};

use aegis_proto as pb;

use crate::i18n::Catalog;
use crate::workspace::Workspace;

/// One pending AI or catalog suggestion item shown in the UI (job 089).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestionRow {
    /// Stable suggestion ID.
    pub id: SuggestionId,
    /// Proposed step or entity label.
    pub label: String,
    /// Grounded citation and predicted rating summary.
    pub detail: String,
    /// Kind badge (`Step`, `OR gate`, ...).
    pub kind: String,
}

/// A rating shown as text with its color.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskView {
    /// Label from the methodology, for example `Risk 4`.
    pub label: String,
    /// Color bucket.
    pub tone: Tone,
}

/// One threat scenario in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatRow {
    /// Stable ID.
    pub id: ThreatId,
    /// Short visual ID (`T00`, `T01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Name of the asset it targets.
    pub asset: String,
    /// Risk before controls.
    pub inherent: Option<RiskView>,
    /// Risk after controls.
    pub residual: Option<RiskView>,
    /// Whether an attack tree is attached.
    pub has_tree: bool,
    /// Whether the attack scales to many vehicles (FND-10); `None` is not
    /// rated.
    pub scales: Option<bool>,
}

/// A page of the open project: the steps of the TARA flow (ISO/SAE 21434
/// clause 15, direction section 11) and the tools around them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Page {
    /// Counts, risk distribution, top risks, problems.
    #[default]
    Overview,
    /// Assets and their security properties.
    Assets,
    /// Damage scenarios and impact ratings.
    Damage,
    /// Threat scenarios.
    Threats,
    /// Attack trees on the canvas.
    Trees,
    /// The risk register and treatment decisions.
    Risks,
    /// Security controls.
    Controls,
    /// Cybersecurity goals and claims.
    Goals,
    /// Completeness and consistency findings.
    Problems,
    /// Baselines and the comparison with one.
    Versions,
    /// Recent accepted changes.
    History,
    /// Exports.
    Reports,
    /// Users, project tools, license (admins).
    Admin,
    /// Item definition: components, interfaces, data flows and trust boundaries (job 074).
    Item,
    /// The item definition's description and scope texts (ARC-05).
    Scope,
}

/// The group of pages a page sits in on the sidebar: the phases of
/// ISO/SAE 21434 clause 15 in order, then outputs and the project (UI-13,
/// D30). The overview has no group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageGroup {
    /// 1. Item definition.
    ItemDefinition,
    /// 2. Impact rating of damage scenarios.
    Impact,
    /// 3. Assets and threat scenarios.
    AssetsThreats,
    /// 4. Attack feasibility.
    Feasibility,
    /// 5. Risk determination and treatment.
    Risk,
    /// What the analysis produces.
    Outputs,
    /// Findings, versions, history and administration.
    Project,
}

impl PageGroup {
    /// Every group, in sidebar order.
    pub const ALL: [PageGroup; 7] = [
        PageGroup::ItemDefinition,
        PageGroup::Impact,
        PageGroup::AssetsThreats,
        PageGroup::Feasibility,
        PageGroup::Risk,
        PageGroup::Outputs,
        PageGroup::Project,
    ];

    /// Position in [`PageGroup::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        PageGroup::ALL.iter().position(|g| *g == self).unwrap_or(0)
    }

    /// A stable name, for the settings file.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            PageGroup::ItemDefinition => "item-definition",
            PageGroup::Impact => "impact",
            PageGroup::AssetsThreats => "assets-threats",
            PageGroup::Feasibility => "feasibility",
            PageGroup::Risk => "risk",
            PageGroup::Outputs => "outputs",
            PageGroup::Project => "project",
        }
    }

    /// The group with this key, if any.
    #[must_use]
    pub fn from_key(key: &str) -> Option<PageGroup> {
        PageGroup::ALL.into_iter().find(|g| g.key() == key)
    }

    /// Whether it is a phase of the analysis, whose collapsed header shows
    /// the number of elements on its pages.
    #[must_use]
    pub fn is_phase(self) -> bool {
        !matches!(self, PageGroup::Outputs | PageGroup::Project)
    }

    /// A human-readable name, for the process map (UI-18).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            PageGroup::ItemDefinition => "Item definition",
            PageGroup::Impact => "Impact",
            PageGroup::AssetsThreats => "Assets and threats",
            PageGroup::Feasibility => "Attack feasibility",
            PageGroup::Risk => "Risk and treatment",
            PageGroup::Outputs => "Outputs",
            PageGroup::Project => "Project",
        }
    }
}

/// A key naming a relation between two entity kinds, used by `Links`
/// columns. The relation registry owns the keys and resolves a row's link
/// targets; this re-export keeps the key in one place.
pub use crate::relations::RelationKey;

/// The kind of a table column (UI-14.1): one look and one editing rule each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColumnKind {
    /// The visual ID. Never edited.
    Id,
    /// The row's name.
    Name,
    /// Links to related elements.
    Links(RelationKey),
    /// A value the app works out from other data. Never edited in place.
    Computed,
    /// A value picked from a fixed or pack-defined list.
    Rating,
    /// Free text.
    Text,
}

impl ColumnKind {
    /// Whether a cell of this kind can be edited in place.
    #[must_use]
    pub fn editable(self) -> bool {
        !matches!(self, Self::Id | Self::Computed)
    }
}

/// An element type the methodology pack names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ElementType {
    /// Asset.
    Asset,
    /// Damage scenario.
    DamageScenario,
    /// Threat scenario.
    ThreatScenario,
    /// Security control.
    SecurityControl,
    /// Cybersecurity goal.
    CybersecurityGoal,
    /// Cybersecurity claim.
    CybersecurityClaim,
}

/// Where a column heading comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColumnLabel {
    /// A message id in the client catalog (`locales/*/main.ftl`).
    Catalog(&'static str),
    /// The singular name the methodology pack gives an element type, so a
    /// pack that renames "Damage scenario" also renames the heading.
    Element(ElementType),
}

/// One column of a table page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnSpec {
    /// Stable key, unique in its table. Settings (column chooser, job 129b)
    /// and the cell lookup of [`crate::table`] refer to a column by it.
    pub key: &'static str,
    /// The column's kind.
    pub kind: ColumnKind,
    /// The heading.
    pub label: ColumnLabel,
    /// Default width in logical pixels, before the user resizes.
    pub width: u16,
    /// Whether the column is shown by default.
    pub visible: bool,
}

impl ColumnSpec {
    /// The heading text. `names` are the pack's element names in the wanted
    /// language (`Vocabulary::element_names`). A missing catalog id shows
    /// as `[id]`, as everywhere else.
    #[must_use]
    pub fn heading(&self, names: &aegis_model::ElementNamesDef, catalog: &Catalog) -> String {
        let name = match self.label {
            ColumnLabel::Catalog(id) => return catalog.tr(id),
            ColumnLabel::Element(ElementType::Asset) => &names.asset,
            ColumnLabel::Element(ElementType::DamageScenario) => &names.damage_scenario,
            ColumnLabel::Element(ElementType::ThreatScenario) => &names.threat_scenario,
            ColumnLabel::Element(ElementType::SecurityControl) => &names.security_control,
            ColumnLabel::Element(ElementType::CybersecurityGoal) => &names.cybersecurity_goal,
            ColumnLabel::Element(ElementType::CybersecurityClaim) => &names.cybersecurity_claim,
        };
        name.singular.to_string()
    }
}

/// A shown-by-default column.
const fn col(key: &'static str, kind: ColumnKind, label: ColumnLabel, width: u16) -> ColumnSpec {
    ColumnSpec {
        key,
        kind,
        label,
        width,
        visible: true,
    }
}

/// A column the user can switch on.
const fn hidden(key: &'static str, kind: ColumnKind, label: ColumnLabel, width: u16) -> ColumnSpec {
    ColumnSpec {
        key,
        kind,
        label,
        width,
        visible: false,
    }
}

const fn msg(id: &'static str) -> ColumnLabel {
    ColumnLabel::Catalog(id)
}

const fn element(e: ElementType) -> ColumnLabel {
    ColumnLabel::Element(e)
}

const ID: ColumnSpec = col("id", ColumnKind::Id, msg("id"), 76);
const THREATS: ColumnSpec = col("threats", ColumnKind::Computed, msg("threats"), 90);
const BEFORE: ColumnSpec = col("before", ColumnKind::Computed, msg("risk-before"), 100);
const AFTER: ColumnSpec = col("after", ColumnKind::Computed, msg("risk-after"), 100);

const fn name(e: ElementType) -> ColumnSpec {
    col("name", ColumnKind::Name, element(e), 240)
}

/// One table of the app. Its columns are the single declaration every
/// renderer, the column chooser and the cell lookup read (UI-14, D30).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TableId {
    /// Assets.
    Assets,
    /// Damage scenarios.
    Damage,
    /// Threat scenarios.
    Threats,
    /// The risk register; one row per threat scenario.
    Risks,
    /// Security controls.
    Controls,
    /// Cybersecurity goals.
    Goals,
    /// Cybersecurity claims.
    Claims,
    /// Lint findings.
    Problems,
}

impl TableId {
    /// Every table.
    pub const ALL: [TableId; 8] = [
        TableId::Assets,
        TableId::Damage,
        TableId::Threats,
        TableId::Risks,
        TableId::Controls,
        TableId::Goals,
        TableId::Claims,
        TableId::Problems,
    ];

    /// The table's columns, in default order. Columns whose number depends
    /// on the pack (one per attack factor, job 152) are not expressible
    /// here yet; no table shown today has any.
    #[must_use]
    pub fn columns(self) -> &'static [ColumnSpec] {
        use ColumnKind::{Computed, Links, Rating};
        use ElementType as E;
        match self {
            TableId::Assets => {
                const C: &[ColumnSpec] = &[
                    ID,
                    name(E::Asset),
                    hidden(
                        "component",
                        Links(RelationKey::ASSET_COMPONENT),
                        msg("component"),
                        160,
                    ),
                    col("properties", Rating, msg("properties"), 200),
                    THREATS,
                    col("worst-risk", Computed, msg("worst-risk"), 110),
                ];
                C
            }
            TableId::Damage => {
                const C: &[ColumnSpec] = &[
                    ID,
                    name(E::DamageScenario),
                    col("impact", Rating, msg("impact"), 200),
                    THREATS,
                ];
                C
            }
            TableId::Threats => {
                const C: &[ColumnSpec] = &[
                    ID,
                    name(E::ThreatScenario),
                    col(
                        "asset",
                        Links(RelationKey::THREAT_ASSET),
                        element(E::Asset),
                        160,
                    ),
                    col("scales", Rating, msg("scales"), 64),
                    BEFORE,
                    AFTER,
                ];
                C
            }
            TableId::Risks => {
                const C: &[ColumnSpec] = &[
                    ID,
                    name(E::ThreatScenario),
                    hidden(
                        "asset",
                        Links(RelationKey::THREAT_ASSET),
                        element(E::Asset),
                        160,
                    ),
                    col("damage", Computed, msg("deciding-damage"), 160),
                    col("impact", Computed, msg("impact"), 110),
                    col("feasibility", Computed, msg("feasibility"), 110),
                    BEFORE,
                    AFTER,
                    col("treatment", Rating, msg("treatment"), 110),
                    hidden("backing", Computed, msg("goal-or-claim"), 160),
                    hidden("reduced-by", Computed, msg("reduced-by"), 200),
                ];
                C
            }
            TableId::Controls => {
                const C: &[ColumnSpec] = &[
                    ID,
                    name(E::SecurityControl),
                    col("steps", Computed, msg("attack-steps"), 100),
                    col("goals", Computed, msg("goals"), 90),
                    hidden("lowers", Computed, msg("risks-lowered"), 110),
                ];
                C
            }
            TableId::Goals => {
                const C: &[ColumnSpec] = &[
                    ID,
                    name(E::CybersecurityGoal),
                    col("controls", Computed, msg("controls"), 90),
                    THREATS,
                ];
                C
            }
            TableId::Claims => {
                const C: &[ColumnSpec] = &[ID, name(E::CybersecurityClaim), THREATS];
                C
            }
            TableId::Problems => {
                const C: &[ColumnSpec] = &[
                    ID,
                    col("severity", Computed, msg("severity"), 100),
                    col("problem", Computed, msg("problem"), 480),
                ];
                C
            }
        }
    }
}

/// The layout a page uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageLayout {
    /// A table page; the columns of its first table ([`Page::tables`]).
    Table(&'static [ColumnSpec]),
    /// A free-form canvas.
    Canvas,
    /// A custom layout.
    Custom,
}

impl Page {
    /// Every page, in menu and sidebar order: the analysis in the order of
    /// its phases, item definition first (UI-13).
    pub const ALL: [Page; 15] = [
        Page::Overview,
        Page::Item,
        Page::Scope,
        Page::Damage,
        Page::Assets,
        Page::Threats,
        Page::Trees,
        Page::Risks,
        Page::Controls,
        Page::Goals,
        Page::Reports,
        Page::Problems,
        Page::Versions,
        Page::History,
        Page::Admin,
    ];

    /// The sidebar group, or `None` for the overview.
    #[must_use]
    pub fn group(self) -> Option<PageGroup> {
        Some(match self {
            Page::Overview => return None,
            Page::Item | Page::Scope => PageGroup::ItemDefinition,
            Page::Damage => PageGroup::Impact,
            Page::Assets | Page::Threats => PageGroup::AssetsThreats,
            Page::Trees => PageGroup::Feasibility,
            Page::Risks | Page::Controls | Page::Goals => PageGroup::Risk,
            Page::Reports => PageGroup::Outputs,
            Page::Problems | Page::Versions | Page::History | Page::Admin => PageGroup::Project,
        })
    }

    /// Position in [`Page::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Page::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    /// The page at a position, if any.
    #[must_use]
    pub fn from_index(i: usize) -> Option<Page> {
        Page::ALL.get(i).copied()
    }

    /// The first page in [`Page::ALL`] of group `g`, or the overview when the
    /// group has none.
    #[must_use]
    pub fn first_in_group(g: PageGroup) -> Page {
        Page::ALL
            .iter()
            .copied()
            .find(|p| p.group() == Some(g))
            .unwrap_or(Page::Overview)
    }

    /// A stable key, used as the page key of the pack's `clause_refs`.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Page::Overview => "overview",
            Page::Item => "item",
            Page::Scope => "scope",
            Page::Damage => "damage",
            Page::Assets => "assets",
            Page::Threats => "threats",
            Page::Trees => "trees",
            Page::Risks => "risks",
            Page::Controls => "controls",
            Page::Goals => "goals",
            Page::Reports => "reports",
            Page::Problems => "problems",
            Page::Versions => "versions",
            Page::History => "history",
            Page::Admin => "admin",
        }
    }

    /// The tables the page shows, main one first; empty for a page that
    /// is not a table page.
    #[must_use]
    pub fn tables(self) -> &'static [TableId] {
        match self {
            Page::Assets => &[TableId::Assets],
            Page::Damage => &[TableId::Damage],
            Page::Threats => &[TableId::Threats],
            Page::Risks => &[TableId::Risks],
            Page::Controls => &[TableId::Controls],
            Page::Goals => &[TableId::Goals, TableId::Claims],
            Page::Problems => &[TableId::Problems],
            Page::Overview
            | Page::Trees
            | Page::Versions
            | Page::History
            | Page::Reports
            | Page::Admin
            | Page::Item
            | Page::Scope => &[],
        }
    }

    /// The page's layout.
    #[must_use]
    pub fn layout(self) -> PageLayout {
        match (self.tables().first(), self) {
            (Some(table), _) => PageLayout::Table(table.columns()),
            (None, Page::Item | Page::Trees) => PageLayout::Canvas,
            (None, _) => PageLayout::Custom,
        }
    }

    /// The title.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Assets => "Assets",
            Page::Damage => "Damage scenarios",
            Page::Threats => "Threat scenarios",
            Page::Trees => "Attack trees",
            Page::Risks => "Risks and treatment",
            Page::Controls => "Security controls",
            Page::Goals => "Goals and claims",
            Page::Problems => "Problems",
            Page::Versions => "Versions",
            Page::History => "History",
            Page::Reports => "Reports",
            Page::Admin => "Administration",
            Page::Item => "Item definition",
            Page::Scope => "Description and scope",
        }
    }
}

/// One selected entity, by stable ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Selection {
    /// A component in the item definition.
    Component(ComponentId),
    /// An interface on a component.
    Interface(InterfaceId),
    /// A data flow between two interfaces.
    DataFlow(DataFlowId),
    /// A trust boundary over components.
    TrustBoundary(TrustBoundaryId),
    /// An asset.
    Asset(AssetId),
    /// A damage scenario.
    Damage(DamageId),
    /// A threat scenario.
    Threat(ThreatId),
    /// An attack tree.
    Tree(TreeId),
    /// A specific node in an attack tree.
    TreeNode(TreeId, NodeId),
    /// A security control.
    Control(ControlId),
    /// A cybersecurity goal.
    Goal(GoalId),
    /// A cybersecurity claim.
    Claim(ClaimId),
}

/// A user administration request (admins, password login).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminAction {
    /// Creates a user.
    CreateUser {
        /// Login name.
        username: String,
        /// Shown name.
        display_name: String,
        /// First password.
        password: String,
        /// Administrator.
        admin: bool,
    },
    /// Grants or revokes administrator rights.
    SetAdmin {
        /// The user.
        username: String,
        /// Admin or not.
        admin: bool,
    },
    /// Disables or enables a user (disabling ends their sessions).
    SetDisabled {
        /// The user.
        username: String,
        /// Disabled or not.
        disabled: bool,
    },
    /// Sets a new password (ends the user's sessions).
    ResetPassword {
        /// The user.
        username: String,
        /// The new password.
        password: String,
    },
    /// Deletes a user.
    DeleteUser {
        /// The user.
        username: String,
    },
    /// Ends one open session (job 055).
    RevokeSession {
        /// The session's ID from the session list.
        id: String,
        /// Whose it is, for the message.
        username: String,
    },
    /// Turns password login on or off for the whole server (job 045).
    SetPasswordLogin {
        /// On or off.
        enabled: bool,
    },
}

/// What a draft will become (UI-08). The draft lives only in the client: it
/// reaches the server as one ordinary edit when the element is complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftKind {
    /// A new asset.
    Asset,
    /// A new damage scenario.
    Damage,
    /// A new security control.
    Control,
    /// A new threat scenario: needs an asset and a category.
    Threat,
    /// A new cybersecurity goal.
    Goal,
    /// A new cybersecurity claim.
    Claim,
    /// A new attack tree (its root is named).
    Tree,
    /// A new component of the item definition.
    Component,
    /// A new interface: needs a component and a kind.
    Interface,
    /// A new data flow: needs two different components.
    DataFlow,
    /// A new trust boundary.
    TrustBoundary,
    /// A new system (its name).
    System,
}

impl UiState {
    /// A required field of the draft has no value: a threat needs an asset and
    /// a category; the other drafts need only a name (UI-08.2).
    #[must_use]
    pub fn draft_incomplete(&self) -> bool {
        match self.draft {
            Some(DraftKind::Threat) => self.draft_asset.is_none() || self.draft_category.is_none(),
            Some(DraftKind::Interface) => {
                self.draft_component.is_none() || self.draft_interface_kind.is_none()
            }
            Some(DraftKind::DataFlow) => {
                self.draft_component.is_none()
                    || self.draft_to.is_none()
                    || self.draft_component == self.draft_to
            }
            _ => false,
        }
    }
}

impl DraftKind {
    /// The page whose table and inspector show the draft.
    #[must_use]
    pub fn page(self) -> Page {
        match self {
            Self::Asset => Page::Assets,
            Self::Damage => Page::Damage,
            Self::Control => Page::Controls,
            Self::Threat => Page::Threats,
            Self::Goal | Self::Claim => Page::Goals,
            Self::Tree => Page::Trees,
            Self::Component
            | Self::Interface
            | Self::DataFlow
            | Self::TrustBoundary
            | Self::System => Page::Item,
        }
    }

    /// A stable key for the window.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Damage => "damage",
            Self::Control => "control",
            Self::Threat => "threat",
            Self::Goal => "goal",
            Self::Claim => "claim",
            Self::Tree => "tree",
            Self::Component => "component",
            Self::Interface => "interface",
            Self::DataFlow => "data-flow",
            Self::TrustBoundary => "trust-boundary",
            Self::System => "system",
        }
    }

    /// The kind with this key.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        [
            Self::Asset,
            Self::Damage,
            Self::Control,
            Self::Threat,
            Self::Goal,
            Self::Claim,
            Self::Tree,
            Self::Component,
            Self::Interface,
            Self::DataFlow,
            Self::TrustBoundary,
            Self::System,
        ]
        .into_iter()
        .find(|k| k.key() == key)
    }
}

/// The user's selection and display choices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiState {
    /// The page on screen.
    pub page: Page,
    /// The draft being typed, if any (UI-08).
    pub draft: Option<DraftKind>,
    /// Changes with every new draft, so the window clears its text field.
    pub draft_seq: u64,
    /// Why the last create was refused, shown in the draft (UI-08.6).
    pub draft_error: Option<String>,
    /// The asset of a threat draft (from the context, or chosen).
    pub draft_asset: Option<AssetId>,
    /// The category of a threat draft, as a position in the vocabulary.
    pub draft_category: Option<usize>,
    /// The component of an interface draft, or the "from" end of a data flow.
    pub draft_component: Option<ComponentId>,
    /// The "to" end of a data flow draft.
    pub draft_to: Option<ComponentId>,
    /// The kind of an interface draft, as a position in the vocabulary.
    pub draft_interface_kind: Option<usize>,
    /// The damage scenario a threat draft leads to (row-action context, UI-09).
    pub draft_damage: Option<DamageId>,
    /// The threat scenario a tree draft is linked to (row-action context, UI-09).
    pub draft_threat: Option<ThreatId>,
    /// The security control a goal draft is linked to (row-action context, UI-09).
    pub draft_control: Option<ControlId>,
    /// The selected threat (threat and risk pages; its tree when no tree is
    /// selected).
    pub selected_threat: Option<ThreatId>,
    /// The tree on the canvas, when chosen on the trees page.
    pub selected_tree: Option<TreeId>,
    /// The selected node of the tree on the canvas.
    pub selected_node: Option<NodeId>,
    /// Show the residual evaluation instead of the inherent one.
    pub show_residual: bool,
    /// The systems on show (SYS-03). A display choice: never an op.
    pub system_view: crate::systems::SystemView,
    /// The selected asset.
    pub selected_asset: Option<AssetId>,
    /// The selected damage scenario.
    pub selected_damage: Option<DamageId>,
    /// The selected security control.
    pub selected_control: Option<ControlId>,
    /// The selected goal.
    pub selected_goal: Option<GoalId>,
    /// The selected claim.
    pub selected_claim: Option<ClaimId>,
    /// The selected component on the Item definition page.
    pub selected_component: Option<ComponentId>,
    /// The selected interface on the Item definition page.
    pub selected_interface: Option<InterfaceId>,
    /// The selected data flow on the Item definition page.
    pub selected_data_flow: Option<DataFlowId>,
    /// The selected trust boundary on the Item definition page.
    pub selected_trust_boundary: Option<TrustBoundaryId>,
}

/// Converts an [`ElementRef`] to the wire target string used by figure
/// attachments (`"project"` or `<kind>:<uuidv7>`).
#[must_use]
pub fn element_ref_to_target(target: ElementRef) -> String {
    match target {
        ElementRef::Project => "project".to_owned(),
        ElementRef::Component(id) => format!("component:{id}"),
        ElementRef::Interface(id) => format!("interface:{id}"),
        ElementRef::DataFlow(id) => format!("data_flow:{id}"),
        ElementRef::TrustBoundary(id) => format!("trust_boundary:{id}"),
        ElementRef::Asset(id) => format!("asset:{id}"),
        ElementRef::Damage(id) => format!("damage:{id}"),
        ElementRef::Threat(id) => format!("threat:{id}"),
        ElementRef::Tree(id) => format!("tree:{id}"),
        ElementRef::Control(id) => format!("control:{id}"),
        ElementRef::Goal(id) => format!("goal:{id}"),
        ElementRef::Claim(id) => format!("claim:{id}"),
        ElementRef::ItemDescription => "item_description".to_owned(),
        ElementRef::ItemScope => "item_scope".to_owned(),
    }
}

/// Parses a wire target string (`""`, `"project"`, or `<kind>:<uuidv7>`) back
/// into an [`ElementRef`].
#[must_use]
pub fn target_to_element_ref(target: &str) -> Option<ElementRef> {
    let trimmed = target.trim();
    match trimmed {
        "" | "project" => return Some(ElementRef::Project),
        "item_description" => return Some(ElementRef::ItemDescription),
        "item_scope" => return Some(ElementRef::ItemScope),
        _ => {}
    }
    let (kind, uuid_str) = trimmed.split_once(':')?;
    match kind {
        "component" => Some(ElementRef::Component(uuid_str.parse().ok()?)),
        "interface" => Some(ElementRef::Interface(uuid_str.parse().ok()?)),
        "data_flow" => Some(ElementRef::DataFlow(uuid_str.parse().ok()?)),
        "trust_boundary" => Some(ElementRef::TrustBoundary(uuid_str.parse().ok()?)),
        "asset" => Some(ElementRef::Asset(uuid_str.parse().ok()?)),
        "damage" | "damage_scenario" => Some(ElementRef::Damage(uuid_str.parse().ok()?)),
        "threat" | "threat_scenario" => Some(ElementRef::Threat(uuid_str.parse().ok()?)),
        "tree" | "attack_tree" => Some(ElementRef::Tree(uuid_str.parse().ok()?)),
        "control" | "security_control" => Some(ElementRef::Control(uuid_str.parse().ok()?)),
        "goal" | "cybersecurity_goal" => Some(ElementRef::Goal(uuid_str.parse().ok()?)),
        "claim" | "cybersecurity_claim" => Some(ElementRef::Claim(uuid_str.parse().ok()?)),
        _ => None,
    }
}

/// The tree the canvas shows: the chosen tree, else the selected threat's.
#[must_use]
pub fn shown_tree(ws: &Workspace, ui: &UiState) -> Option<TreeId> {
    let project = ws.project();
    if let Some(t) = ui
        .selected_tree
        .filter(|t| project.attack_tree_idx(t).is_some())
    {
        return Some(t);
    }
    let threat = project.threat_scenario(project.threat_scenario_idx(&ui.selected_threat?)?)?;
    project.attack_tree_id(threat.attack_tree()?)
}

/// One description box on a page: the text of the selected element, or the
/// project notes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescriptionView {
    /// What the text belongs to; edits go there.
    pub target: aegis_model::ElementRef,
    /// Which box this is on the page (`project`, `asset`, `damage`,
    /// `threat`, `tree`, `control`, `goal`, `claim`).
    pub kind: &'static str,
    /// The text, empty when there is none.
    pub text: String,
    /// Who wrote it and when, empty when there is no text.
    pub by: String,
    /// The translations of the text, in insertion order (FND-04).
    pub translations: Vec<TranslationView>,
}

/// One translation of a description, as the view shows it (FND-04).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslationView {
    /// The BCP 47 language tag.
    pub lang: String,
    /// The translated text.
    pub text: String,
}

/// The description boxes the page offers, for what is selected on it.
/// Pages that show no element (problems, versions, history, reports,
/// administration) offer none, and neither does a selection that no longer
/// exists. Time O(log n).
#[must_use]
pub fn page_descriptions(ws: &Workspace, ui: &UiState) -> Vec<DescriptionView> {
    use aegis_model::ElementRef as E;
    let project = ws.project();
    let mut targets: Vec<(&'static str, E)> = Vec::new();
    let mut push = |kind, live: bool, target: E| {
        if live {
            targets.push((kind, target));
        }
    };
    match ui.page {
        Page::Overview => push("project", true, E::Project),
        Page::Scope => {
            push("item-description", true, E::ItemDescription);
            push("item-scope", true, E::ItemScope);
        }
        Page::Assets => {
            if let Some(id) = ui.selected_asset {
                push("asset", project.asset_idx(&id).is_some(), E::Asset(id));
            }
        }
        Page::Damage => {
            if let Some(id) = ui.selected_damage {
                push(
                    "damage",
                    project.damage_scenario_idx(&id).is_some(),
                    E::Damage(id),
                );
            }
        }
        Page::Threats | Page::Risks => {
            if let Some(id) = ui.selected_threat {
                push(
                    "threat",
                    project.threat_scenario_idx(&id).is_some(),
                    E::Threat(id),
                );
            }
        }
        Page::Trees => {
            if let Some(id) = shown_tree(ws, ui) {
                push("tree", true, E::Tree(id));
            }
        }
        Page::Controls => {
            if let Some(id) = ui.selected_control {
                push(
                    "control",
                    project.security_control_idx(&id).is_some(),
                    E::Control(id),
                );
            }
        }
        Page::Goals => {
            if let Some(id) = ui.selected_goal {
                push(
                    "goal",
                    project.cybersecurity_goal_idx(&id).is_some(),
                    E::Goal(id),
                );
            }
            if let Some(id) = ui.selected_claim {
                push(
                    "claim",
                    project.cybersecurity_claim_idx(&id).is_some(),
                    E::Claim(id),
                );
            }
        }
        Page::Item => {
            if let Some(id) = ui.selected_component {
                push(
                    "component",
                    project.component_idx(&id).is_some(),
                    E::Component(id),
                );
            }
            if let Some(id) = ui.selected_interface {
                push(
                    "interface",
                    project.interface_idx(&id).is_some(),
                    E::Interface(id),
                );
            }
            if let Some(id) = ui.selected_data_flow {
                push(
                    "data-flow",
                    project.data_flow_idx(&id).is_some(),
                    E::DataFlow(id),
                );
            }
            if let Some(id) = ui.selected_trust_boundary {
                push(
                    "trust-boundary",
                    project.trust_boundary_idx(&id).is_some(),
                    E::TrustBoundary(id),
                );
            }
        }
        Page::Problems | Page::Versions | Page::History | Page::Reports | Page::Admin => {}
    }
    targets
        .into_iter()
        .map(|(kind, target)| DescriptionView {
            target,
            kind,
            text: project.description(target).unwrap_or_default().to_owned(),
            by: project
                .description_provenance(target)
                .map_or_else(String::new, |p| {
                    let when = crate::time::format_ms(p.timestamp_ms());
                    match (p.actor(), when.as_str()) {
                        ("", "") => String::new(),
                        ("", when) => format!("Written {when}"),
                        (who, "") => format!("Written by {who}"),
                        (who, when) => format!("Written by {who}, {when}"),
                    }
                }),
            translations: project
                .description_translations(target)
                .map(|(lang, text)| TranslationView {
                    lang: lang.to_owned(),
                    text: text.to_owned(),
                })
                .collect(),
        })
        .collect()
}

/// A control attached to the selected step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MitigationView {
    /// The control.
    pub control: ControlId,
    /// Short visual ID (`SC00`, `SC01`, ...).
    pub visual_id: String,
    /// Its name.
    pub name: String,
    /// The cap, as a position in the feasibility levels.
    pub level: usize,
}

/// The tree on the canvas.
pub struct TreeView {
    /// A threat it belongs to, if any.
    pub threat: Option<ThreatId>,
    /// The tree.
    pub tree: TreeId,
    /// Short visual ID (`AT00`, `AT01`, ...).
    pub visual_id: String,
    /// The laid-out scene, ready for `aegis_canvas::render`.
    pub scene: Arc<Scene>,
    /// Which node is selected, as a scene position.
    pub selected: Option<NodeIdx>,
    /// Whether this is the residual view.
    pub residual: bool,
}

/// One choice in a dropdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// Label from the methodology.
    pub label: String,
}

/// One attack potential factor of a step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactorView {
    /// Factor name.
    pub label: String,
    /// Levels, with their points in the label.
    pub options: Vec<Choice>,
    /// The chosen level's position in `options`, if rated.
    pub selected: Option<usize>,
}

/// One thing a step can go through: a component, or an interface of one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThroughChoice {
    /// The component or interface.
    pub item: ItemRef,
    /// Its name.
    pub label: String,
    /// "ADAS ECU" or "ADAS ECU / JTAG": what a filter matches and lists.
    pub full: String,
    /// Interfaces are shown indented under their component.
    pub indent: bool,
}

/// One step that goes through a component or an interface (CFG-01).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepThrough {
    /// The tree.
    pub tree: TreeId,
    /// Short visual ID of the tree.
    pub tree_visual_id: String,
    /// The step.
    pub node: NodeId,
    /// The step's label.
    pub step: String,
    /// What it goes through, "ADAS ECU / JTAG".
    pub through: String,
    /// Its attack type label, if any.
    pub attack_type: Option<String>,
    /// Its inherent feasibility label, if rated.
    pub inherent: Option<String>,
}

/// One attack step of a tree with what it goes through and its attack type
/// (CFG-01, CFG-02), for reports and exports (job 122c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepLink {
    /// The step's label.
    pub step: String,
    /// Its inherent feasibility label, if rated.
    pub feasibility: Option<String>,
    /// What it goes through, "ADAS ECU / JTAG", if set.
    pub through: Option<String>,
    /// Its attack type label, if set.
    pub attack_type: Option<String>,
}

/// The leaf steps of tree `tree`, in node order, with what each goes through
/// and its attack type. Time O(nodes).
#[must_use]
pub fn tree_steps(project: &Project, tree: TreeIdx) -> Vec<StepLink> {
    let m = project.methodology();
    let Some(t) = project.attack_tree(tree) else {
        return Vec::new();
    };
    t.iter()
        .filter(|(_, _, node)| node.gate_kind().is_none())
        .map(|(idx, _, node)| StepLink {
            step: node.label().to_owned(),
            feasibility: project
                .effective_step_feasibility(tree, idx)
                .map(|l| level_label(m, l)),
            through: project
                .step_via(tree, idx)
                .map(|v| through_label(project, v)),
            attack_type: project
                .step_attack_type(tree, idx)
                .and_then(|a| m.attack_type_label(a))
                .map(str::to_owned),
        })
        .collect()
}

/// "ADAS ECU" for a component, "ADAS ECU / JTAG" for an interface.
#[must_use]
pub fn through_label(project: &Project, item: ItemRef) -> String {
    match item {
        ItemRef::Component(id) => project
            .component_idx(&id)
            .and_then(|i| project.component(i))
            .map(|c| c.name().to_owned())
            .unwrap_or_default(),
        ItemRef::Interface(id) => project
            .interface_idx(&id)
            .and_then(|i| project.interface(i))
            .map(|i| {
                let owner = project.component(i.component()).map_or("", |c| c.name());
                format!("{owner} / {}", i.name())
            })
            .unwrap_or_default(),
    }
}

/// The name of the system a step leaves its tree's systems for: the step
/// goes via a component or interface of a system that is not among the
/// systems of its tree. `None` when the step has no via, the via
/// component is in no system, the tree is in no system, or the via
/// system is one of the tree's. Time O(depth + log systems).
#[must_use]
pub fn step_other_system(
    project: &Project,
    usage: &aegis_model::SystemUse,
    tree: TreeIdx,
    node: NodeIdx,
) -> Option<String> {
    let tree_systems = usage.tree(tree);
    if tree_systems.is_empty() {
        return None;
    }
    let comp = match project.step_via(tree, node)? {
        ItemRef::Component(id) => project.component_idx(&id)?,
        ItemRef::Interface(id) => project.interface(project.interface_idx(&id)?)?.component(),
    };
    let system = project.component_system(comp)?;
    if usage.set_of([system]).intersects(tree_systems) {
        return None;
    }
    crate::systems::system_name(project, system)
}

/// The steps that go through a component (including through its interfaces)
/// or through an interface, in tree then node order. Time O(total nodes).
#[must_use]
pub fn steps_through(project: &Project, item: ItemRef) -> Vec<StepThrough> {
    let m = project.methodology();
    let mut items = vec![item];
    if let ItemRef::Component(id) = item
        && let Some(c_idx) = project.component_idx(&id)
    {
        items.extend(
            project
                .interfaces()
                .filter(|(_, _, i)| i.component() == c_idx)
                .map(|(_, iid, _)| ItemRef::Interface(iid)),
        );
    }
    let mut rows: Vec<(TreeIdx, NodeIdx, ItemRef)> = items
        .iter()
        .flat_map(|it| project.steps_via(*it).map(move |(t, n)| (t, n, *it)))
        .collect();
    rows.sort_by_key(|(t, n, _)| (t.index(), n.index()));
    rows.into_iter()
        .filter_map(|(t, n, it)| {
            let tree = project.attack_tree(t)?;
            Some(StepThrough {
                tree: project.attack_tree_id(t)?,
                tree_visual_id: project.attack_tree_visual_id(t).unwrap_or_default(),
                node: tree.id_of(n)?,
                step: tree.get(n)?.label().to_owned(),
                through: through_label(project, it),
                attack_type: project
                    .step_attack_type(t, n)
                    .and_then(|a| m.attack_type_label(a))
                    .map(str::to_owned),
                inherent: project
                    .effective_step_feasibility(t, n)
                    .map(|l| level_label(m, l)),
            })
        })
        .collect()
}

/// The details panel for the selected node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inspector {
    /// Tree the node is in.
    pub tree: TreeId,
    /// Short visual ID of the tree (`AT00`, `AT01`, ...).
    pub tree_visual_id: String,
    /// The node.
    pub node: NodeId,
    /// Its label.
    pub title: String,
    /// `Step`, `AND gate`, `OR gate`, or `Sequential AND gate`.
    pub kind: String,
    /// Whether the rating can be edited (steps only).
    pub is_step: bool,
    /// Feasibility levels for the dropdown.
    pub levels: Vec<Choice>,
    /// The directly set rating's position in `levels`.
    pub rating: Option<usize>,
    /// The rating the engine uses (from attack potential if complete).
    pub effective: Option<String>,
    /// Attack potential, `13 (High)`, if every factor is rated.
    pub potential: Option<String>,
    /// Factor dropdowns (steps only).
    pub factors: Vec<FactorView>,
    /// The evaluated value at this node, for gates.
    pub evaluated: Option<String>,
    /// Explain-why lines for gates: the rule and the deciding child.
    pub why: Vec<String>,
    /// What the step can go through: components, each followed by its
    /// interfaces (steps only).
    pub through_options: Vec<ThroughChoice>,
    /// The chosen item's position in `through_options`.
    pub through: Option<usize>,
    /// The other system the chosen item belongs to, when the step leaves its
    /// tree's systems.
    pub through_system: Option<String>,
    /// The pack's attack types (steps only).
    pub attack_types: Vec<Choice>,
    /// The chosen attack type's position in `attack_types`.
    pub attack_type: Option<usize>,
    /// Controls attached to this step (steps only).
    pub mitigations: Vec<MitigationView>,
    /// Explain-why ceilings on this step, deciding one first, e.g.
    /// `JTAG disabled (from ADAS ECU): Very low` (steps only, CFG-04).
    pub ceilings: Vec<String>,
    /// Controls that can still be attached (steps only).
    pub attachable: Vec<(ControlId, String)>,
    /// The step's feasibility after controls, if they change it.
    pub residual: Option<String>,
    /// The rating came from the library and nobody has reviewed it.
    pub needs_review: bool,
    /// Whether this node is the root of the tree.
    pub is_root: bool,
    /// Position of the gate kind in [`crate::commands::GATES`] (`0` = OR,
    /// `1` = AND, `2` = SEQ), or `0` for steps.
    pub gate_index: usize,
    /// Whether this node can move up among its owner gate's children.
    pub can_move_up: bool,
    /// Whether this node can move down among its owner gate's children.
    pub can_move_down: bool,
}

fn risk_view(m: &Methodology, level: aegis_model::RiskLevel) -> Option<RiskView> {
    let label = m.risk_level_label(level)?.to_owned();
    let index = m.risk_levels().position(|(l, _, _)| l == level)?;
    Some(RiskView {
        label,
        tone: Tone::from_level(index, m.risk_level_count()),
    })
}

/// The risk keys of every threat as this computer's engine computes them,
/// by threat ID. Empty keys: not computable. Time O(threats).
#[must_use]
pub fn local_risk_keys(ws: &Workspace) -> HashMap<String, (String, String)> {
    let project = ws.project();
    let m = project.methodology();
    let key = |r: Result<&aegis_model::ThreatRisk, _>| {
        r.ok()
            .and_then(aegis_model::ThreatRisk::max_risk)
            .and_then(|l| m.risk_level_key(l))
            .unwrap_or("")
            .to_owned()
    };
    project
        .threat_scenarios()
        .map(|(_, id, _)| {
            (
                id.to_string(),
                (
                    key(ws.cache().evaluate_risk(project, id)),
                    key(ws.cache().evaluate_residual_risk(project, id)),
                ),
            )
        })
        .collect()
}

/// Whether the server's numbers differ from this engine's for the same
/// project state. Time O(threats).
#[must_use]
pub fn risks_differ(ws: &Workspace, server: &pb::GetRisksResponse) -> bool {
    let local = local_risk_keys(ws);
    local.len() != server.threats.len()
        || server.threats.iter().any(|t| {
            local
                .get(&t.threat_id)
                .is_none_or(|(i, r)| *i != t.inherent_risk || *r != t.residual_risk)
        })
}

/// Replaces the risks in `rows` with the server's. For a client whose engine
/// differs from the server's, or whose own numbers were found wrong.
pub fn use_server_risks(ws: &Workspace, rows: &mut [ThreatRow], server: &pb::GetRisksResponse) {
    let m = ws.project().methodology();
    let view = |key: &str| {
        m.risk_level(key)
            .filter(|_| !key.is_empty())
            .and_then(|l| risk_view(m, l))
    };
    let by_id: HashMap<&str, &pb::ThreatRiskEntry> = server
        .threats
        .iter()
        .map(|t| (t.threat_id.as_str(), t))
        .collect();
    for row in rows {
        if let Some(entry) = by_id.get(row.id.to_string().as_str()) {
            row.inherent = view(&entry.inherent_risk);
            row.residual = view(&entry.residual_risk);
        }
    }
}

/// The threat list with inherent and residual risk. Time O(threats), reading
/// the cache.
#[must_use]
pub fn threat_rows(ws: &Workspace) -> Vec<ThreatRow> {
    let project = ws.project();
    let m = project.methodology();
    project
        .threat_scenarios()
        .map(|(t_idx, id, threat)| {
            let risk = |r: Result<&aegis_model::ThreatRisk, _>| {
                r.ok()
                    .and_then(aegis_model::ThreatRisk::max_risk)
                    .and_then(|l| risk_view(m, l))
            };
            ThreatRow {
                id,
                visual_id: project.threat_scenario_visual_id(t_idx).unwrap_or_default(),
                name: threat.name().to_owned(),
                asset: project
                    .asset(threat.asset())
                    .map_or_else(String::new, |a| a.name().to_owned()),
                inherent: risk(ws.cache().evaluate_risk(project, id)),
                residual: risk(ws.cache().evaluate_residual_risk(project, id)),
                has_tree: threat.attack_tree().is_some(),
                scales: threat.scales(),
            }
        })
        .collect()
}

fn level_tone(m: &Methodology, level: FeasibilityLevel) -> Tone {
    let index = m
        .feasibility_levels()
        .position(|(l, _, _)| l == level)
        .unwrap_or(0);
    Tone::from_level(index, m.feasibility_levels().len())
}

fn level_label(m: &Methodology, level: FeasibilityLevel) -> String {
    m.feasibility_level_label(level).unwrap_or("?").to_owned()
}

/// A tree laid out and annotated with its evaluation (inherent or
/// residual). A tree that cannot be evaluated yet (an empty gate, a step
/// with no rating on the deciding path) is still drawn, unrated, so the user
/// can see and fix it. Time O(n log n) in the tree size.
#[must_use]
pub fn tree_scene(ws: &Workspace, tree_id: TreeId, residual: bool) -> Option<Scene> {
    let project = ws.project();
    let m = project.methodology();
    let tree_idx = project.attack_tree_idx(&tree_id)?;
    let tree = project.attack_tree(tree_idx)?;
    let evaluation = if residual {
        ws.cache().evaluate_residual_tree(project, tree_id)
    } else {
        ws.cache().evaluate_tree(project, tree_id)
    }
    .ok();
    let ghost_overlay = ws
        .suggestions()
        .and_then(|branch| branch.preview_tree(project, tree_id));
    let usage =
        (project.element_count(aegis_model::ElementKind::System) > 0).then(|| project.system_use());
    let style = |idx: NodeIdx| -> NodeStyle {
        if let Some((ref preview_tree, ref ghosts)) = ghost_overlay
            && let Some(nid) = preview_tree.id_of(idx)
            && let Some(g) = ghosts.iter().find(|g| g.node_id == nid)
        {
            return NodeStyle {
                score: Some(
                    g.feasibility
                        .map_or_else(|| "proposed".to_owned(), |l| level_label(m, l)),
                ),
                tone: g.feasibility.map_or(Tone::Unrated, |l| level_tone(m, l)),
                critical: false,
                ghost: true,
                via: None,
                capped: None,
            };
        }
        let value = evaluation.and_then(|e| e.get(idx).copied());
        let is_step = tree.get(idx).is_some_and(|n| n.gate_kind().is_none());
        let capped = if is_step
            && project
                .effective_step_feasibility(tree_idx, idx)
                .zip(project.residual_step_feasibility(tree_idx, idx))
                .is_some_and(|(inh, res)| res < inh)
        {
            project
                .step_ceilings(tree_idx, idx)
                .into_iter()
                .find_map(|c| match c.source() {
                    aegis_model::CeilingSource::ControlRule { target, .. } => {
                        let name = project.security_control(c.control())?.name();
                        Some(format!("{name} on {}", through_label(project, target)))
                    }
                    aegis_model::CeilingSource::StepMitigation => None,
                })
        } else {
            None
        };
        let score = if is_step {
            let potential = project.step_attack_potential(tree_idx, idx);
            match (potential, value) {
                (Some(p), Some(v)) => Some(format!("{} · {}", p.score(), level_label(m, v))),
                (None, Some(v)) => Some(level_label(m, v)),
                _ => Some("unrated".to_owned()),
            }
        } else {
            value.map(|v| level_label(m, v))
        };
        NodeStyle {
            score,
            tone: value.map_or(Tone::Unrated, |v| level_tone(m, v)),
            critical: false,
            ghost: false,
            via: project.step_via(tree_idx, idx).map(|item| {
                let label = through_label(project, item);
                match usage
                    .as_ref()
                    .and_then(|u| step_other_system(project, u, tree_idx, idx))
                {
                    Some(sys) => format!("{label} · {sys}"),
                    None => label,
                }
            }),
            capped,
        }
    };
    let layout_tree = ghost_overlay.as_ref().map_or(tree, |(pt, _)| pt);
    let mut scene = Scene::from_tree(layout_tree, LayoutConfig::default(), style);
    if let Some(e) = evaluation {
        let path: Vec<NodeIdx> = e.why_path(tree.root()).collect();
        scene.set_critical_path(&path);
    }
    Some(scene)
}

/// The tree of the selected threat, laid out and annotated with the
/// evaluation (inherent or residual).
///
/// Time O(n log n) in the tree size (at most 4,096 nodes in a tree).
#[must_use]
pub fn tree_view(ws: &Workspace, ui: &UiState) -> Option<TreeView> {
    tree_view_cached(ws, ui, None)
}

/// Like [`tree_view`], reusing `cached` when its `(tree_id, residual)` matches
/// the currently shown tree and mode.
#[must_use]
pub fn tree_view_cached(
    ws: &Workspace,
    ui: &UiState,
    cached: Option<&(TreeId, bool, Arc<Scene>)>,
) -> Option<TreeView> {
    let project = ws.project();
    let tree_id = shown_tree(ws, ui)?;
    let tree_idx = project.attack_tree_idx(&tree_id)?;
    let tree = project.attack_tree(tree_idx)?;
    // The threat this tree belongs to, for the title: the selected one if it
    // uses the tree, else the first that does.
    let threat_id = ui
        .selected_threat
        .filter(|t| {
            project
                .threat_scenario_idx(t)
                .and_then(|i| project.threat_scenario(i))
                .is_some_and(|t| t.attack_tree() == Some(tree_idx))
        })
        .or_else(|| {
            project
                .threat_scenarios()
                .find(|(_, _, t)| t.attack_tree() == Some(tree_idx))
                .map(|(_, id, _)| id)
        });
    let scene = match cached {
        Some((c_tree, c_res, c_scene)) if *c_tree == tree_id && *c_res == ui.show_residual => {
            Arc::clone(c_scene)
        }
        _ => Arc::new(tree_scene(ws, tree_id, ui.show_residual)?),
    };
    let selected = ui.selected_node.and_then(|n| {
        tree.idx_of(&n).or_else(|| {
            ws.suggestions()
                .and_then(|b| b.preview_tree(project, tree_id))
                .and_then(|(pt, _)| pt.idx_of(&n))
        })
    });
    Some(TreeView {
        threat: threat_id,
        tree: tree_id,
        visual_id: project.attack_tree_visual_id(tree_idx).unwrap_or_default(),
        scene,
        selected,
        residual: ui.show_residual,
    })
}

fn kind_text(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Step => "Step",
        NodeKind::Gate(GateKind::And) => "AND gate",
        NodeKind::Gate(GateKind::Or) => "OR gate",
        NodeKind::Gate(GateKind::SeqAnd) => "Sequential AND gate",
    }
}

/// The details of the selected node.
///
/// Time O(node degree + factors).
#[must_use]
pub fn inspector(ws: &Workspace, ui: &UiState) -> Option<Inspector> {
    let project = ws.project();
    let m = project.methodology();
    let tree_id = shown_tree(ws, ui)?;
    let tree_idx = project.attack_tree_idx(&tree_id)?;
    let tree: &AttackTree = project.attack_tree(tree_idx)?;
    let node_id = ui.selected_node?;
    let idx = tree.idx_of(&node_id)?;
    let node = tree.get(idx)?;
    let kind = node.gate_kind().map_or(NodeKind::Step, NodeKind::Gate);
    let is_step = kind == NodeKind::Step;
    let evaluation = if ui.show_residual {
        ws.cache().evaluate_residual_tree(project, tree_id)
    } else {
        ws.cache().evaluate_tree(project, tree_id)
    }
    .ok();
    let value = evaluation.and_then(|e| e.get(idx).copied());

    let levels: Vec<Choice> = m
        .feasibility_levels()
        .map(|(_, _, label)| Choice {
            label: label.to_owned(),
        })
        .collect();
    let direct = project.step_feasibility(tree_idx, idx);
    let rating = direct.and_then(|d| m.feasibility_levels().position(|(l, _, _)| l == d));
    let potential = project.step_attack_potential(tree_idx, idx);

    let factors = if is_step {
        m.feasibility_factors()
            .map(|f| {
                let current = project.step_factor(tree_idx, idx, f);
                FactorView {
                    label: m.feasibility_factor_label(f).unwrap_or("?").to_owned(),
                    options: m
                        .factor_levels(f)
                        .map(|l| Choice {
                            label: format!(
                                "{} ({})",
                                m.factor_level_label(f, l).unwrap_or("?"),
                                m.factor_level_points(f, l).unwrap_or(0)
                            ),
                        })
                        .collect(),
                    selected: current.and_then(|c| m.factor_levels(f).position(|l| l == c)),
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut why = Vec::new();
    if let Some(e) = evaluation
        && let Some(explanation) = e.explain(tree, idx)
        && !is_step
    {
        let rule = match explanation.rule() {
            Some(GateKind::And) => "AND: the hardest child decides",
            Some(GateKind::Or) => "OR: the easiest child decides",
            Some(GateKind::SeqAnd) => "Sequential AND: the hardest step decides",
            None => "",
        };
        if !rule.is_empty() {
            why.push(rule.to_owned());
        }
        if let Some(decisive) = explanation.decisive()
            && let Some(child) = tree.get(decisive)
        {
            why.push(format!(
                "Decided by \"{}\" ({})",
                child.label(),
                e.get(decisive)
                    .map_or_else(|| "unrated".to_owned(), |v| level_label(m, *v))
            ));
        }
    }

    let (mitigations, ceilings, attachable, residual) = if is_step {
        let attached = project.step_mitigations(tree_idx, idx);
        let mitigations: Vec<MitigationView> = attached
            .iter()
            .filter_map(|s| {
                let control = project.security_control_id(s.control())?;
                Some(MitigationView {
                    control,
                    visual_id: project
                        .security_control_visual_id(s.control())
                        .unwrap_or_default(),
                    name: project
                        .security_control(s.control())
                        .map(|c| c.name().to_owned())
                        .unwrap_or_default(),
                    level: m
                        .feasibility_levels()
                        .position(|(l, _, _)| l == s.feasibility())
                        .unwrap_or(0),
                })
            })
            .collect();
        let ceilings: Vec<String> = project
            .step_ceilings(tree_idx, idx)
            .into_iter()
            .filter_map(|c| {
                let name = project.security_control(c.control())?.name();
                let lvl = level_label(m, c.feasibility());
                Some(match c.source() {
                    aegis_model::CeilingSource::ControlRule { target, .. } => {
                        format!("{name} (from {}): {lvl}", through_label(project, target))
                    }
                    aegis_model::CeilingSource::StepMitigation => {
                        format!("{name} (on this step): {lvl}")
                    }
                })
            })
            .collect();
        let attachable = project
            .security_controls()
            .filter(|(c_idx, _, _)| !attached.iter().any(|s| s.control() == *c_idx))
            .map(|(c_idx, id, c)| {
                let vis = project
                    .security_control_visual_id(c_idx)
                    .unwrap_or_default();
                (id, format!("{vis} \u{b7} {}", c.name()))
            })
            .collect();
        let inherent = project.effective_step_feasibility(tree_idx, idx);
        let after = project.residual_step_feasibility(tree_idx, idx);
        let residual = after
            .filter(|a| Some(*a) != inherent)
            .map(|l| level_label(m, l));
        (mitigations, ceilings, attachable, residual)
    } else {
        (Vec::new(), Vec::new(), Vec::new(), None)
    };

    let mut through_options = Vec::new();
    if is_step {
        for (c_idx, c_id, c) in project.components() {
            through_options.push(ThroughChoice {
                item: ItemRef::Component(c_id),
                label: c.name().to_owned(),
                full: c.name().to_owned(),
                indent: false,
            });
            through_options.extend(
                project
                    .interfaces()
                    .filter(|(_, _, i)| i.component() == c_idx)
                    .map(|(_, i_id, i)| ThroughChoice {
                        item: ItemRef::Interface(i_id),
                        label: i.name().to_owned(),
                        full: format!("{} / {}", c.name(), i.name()),
                        indent: true,
                    }),
            );
        }
    }
    let through = project
        .step_via(tree_idx, idx)
        .and_then(|v| through_options.iter().position(|o| o.item == v));
    let through_system = if project.element_count(aegis_model::ElementKind::System) > 0 {
        step_other_system(project, &project.system_use(), tree_idx, idx)
    } else {
        None
    };
    let attack_types: Vec<Choice> = if is_step {
        m.attack_types()
            .map(|a| Choice {
                label: m.attack_type_label(a).unwrap_or("?").to_owned(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let attack_type = project
        .step_attack_type(tree_idx, idx)
        .and_then(|a| m.attack_types().position(|x| x == a));

    let is_root = idx == tree.root();
    let gate_index = match node.gate_kind() {
        Some(GateKind::Or) | None => 0,
        Some(GateKind::And) => 1,
        Some(GateKind::SeqAnd) => 2,
    };
    let (can_move_up, can_move_down) = tree
        .owner(idx)
        .map(|owner_idx| {
            let siblings = tree.children(owner_idx);
            let pos = siblings.iter().position(|c| *c == idx);
            (
                pos.is_some_and(|p| p > 0),
                pos.is_some_and(|p| p + 1 < siblings.len()),
            )
        })
        .unwrap_or((false, false));

    Some(Inspector {
        tree: tree_id,
        tree_visual_id: project.attack_tree_visual_id(tree_idx).unwrap_or_default(),
        node: node_id,
        title: node.label().to_owned(),
        kind: kind_text(kind).to_owned(),
        is_step,
        levels,
        rating,
        effective: project
            .effective_step_feasibility(tree_idx, idx)
            .filter(|_| is_step)
            .map(|l| level_label(m, l)),
        potential: potential
            .map(|p| format!("{} ({})", p.score(), level_label(m, p.feasibility()))),
        factors,
        evaluated: value.map(|v| level_label(m, v)),
        why,
        through_options,
        through,
        through_system,
        attack_types,
        attack_type,
        mitigations,
        ceilings,
        attachable,
        residual,
        needs_review: project.review_marked(tree_id, node_id),
        is_root,
        gate_index,
        can_move_up,
        can_move_down,
    })
}
