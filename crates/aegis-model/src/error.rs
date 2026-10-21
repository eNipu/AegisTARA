//! Typed errors for the model crate.

use std::sync::Arc;

use crate::id::{AssetId, DamageId, NodeId, NodeIdx, ThreatId};
use crate::methodology::{CatalogScheme, ImpactCategory, Level, Property, ThreatCategory};
use crate::store::{Limit, VocabKind};

/// Errors returned by the model.
///
/// Every operation that returns a `ModelError` leaves the model unchanged.
///
/// ```
/// use aegis_model::{ModelError, NodeId};
///
/// assert_eq!("not a uuid".parse::<NodeId>(), Err(ModelError::InvalidUuid));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ModelError {
    /// The UUID is well-formed but is not an RFC 9562 version 7 UUID.
    #[error("UUID is not version 7")]
    NotVersion7,
    /// The text is not a valid UUID.
    #[error("text is not a valid UUID")]
    InvalidUuid,
    /// A node with this public ID already exists in the tree.
    #[error("node ID {0} already exists in the tree")]
    DuplicateId(NodeId),
    /// The index does not refer to a node in this tree.
    #[error("node index {} is not in the tree", .0.index())]
    UnknownNode(NodeIdx),
    /// The node is a step, so it cannot have children.
    #[error("node index {} is a step and cannot have children", .0.index())]
    NotAGate(NodeIdx),
    /// A new node was passed with children; new nodes must start empty.
    #[error("new node must not have children")]
    ChildrenNotEmpty,
    /// The tree cannot hold more nodes than a `u32` index can address.
    #[error("tree node capacity exceeded")]
    CapacityExceeded,
    /// The gate already has this node as a child; a gate holds each child at
    /// most once.
    ///
    /// Carries stable public IDs, which stay valid across reloads and in
    /// logs. Errors never contain labels (customer data).
    #[error("node {child} is already a child of node {parent}")]
    DuplicateEdge {
        /// The gate that already has the edge.
        parent: NodeId,
        /// The child that is already linked.
        child: NodeId,
    },
    /// [`AttackTree::link`](crate::AttackTree::link) refused an edge because
    /// `parent` is reachable from `child`, so the new edge would close a
    /// cycle.
    ///
    /// `path` is the existing chain from `child` down to `parent` in edge
    /// order (`path[0] == child`, last element `== parent`); the new edge
    /// `parent -> child` would close it. A self-link gives `path == [child]`.
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let root_id = NodeId::new_v7();
    /// let g_id = NodeId::new_v7();
    /// let mut tree = AttackTree::new(root_id, Node::gate(GateKind::Or, "root"))?;
    /// let g = tree.add_child(tree.root(), g_id, Node::gate(GateKind::And, "g"))?;
    /// let err = tree.link(g, tree.root()).err();
    /// assert_eq!(
    ///     err,
    ///     Some(ModelError::WouldCreateCycle {
    ///         parent: g_id,
    ///         child: root_id,
    ///         path: vec![root_id, g_id],
    ///     }),
    /// );
    /// let text = err.map(|e| e.to_string()).unwrap_or_default();
    /// assert_eq!(
    ///     text,
    ///     format!(
    ///         "linking node {root_id} under node {g_id} would create a cycle: \
    ///          {g_id} -> {root_id} -> {g_id}"
    ///     ),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[error(
        "linking node {child} under node {parent} would create a cycle: {parent} -> {}",
        path_text(path, false)
    )]
    WouldCreateCycle {
        /// The gate that was to receive the new child.
        parent: NodeId,
        /// The node that was to be linked.
        child: NodeId,
        /// Existing path from `child` down to `parent`, both included.
        path: Vec<NodeId>,
    },
    /// The graph contains a cycle, so it has no topological order.
    ///
    /// [`AttackTree::link`](crate::AttackTree::link) rejects cycles, so this
    /// is a defense-in-depth check before evaluation and is not expected
    /// from a tree built through the public API.
    ///
    /// The path lists the public IDs of one cycle in edge order: `path[i]`
    /// has `path[i + 1]` as a child, and the last node has `path[0]` as a
    /// child. It is non-empty, has no repeats, and starts with the node of
    /// smallest internal index, so the same tree always reports the same
    /// path. Display repeats the first ID at the end.
    ///
    /// ```
    /// use aegis_model::{ModelError, NodeId};
    ///
    /// let (a, b) = (NodeId::new_v7(), NodeId::new_v7());
    /// assert_eq!(
    ///     ModelError::Cycle(vec![a, b]).to_string(),
    ///     format!("cycle in attack tree: {a} -> {b} -> {a}"),
    /// );
    /// ```
    #[error("cycle in attack tree: {}", path_text(.0, true))]
    Cycle(Vec<NodeId>),
    /// [`evaluate`](crate::evaluate) found a gate with no children. Such a
    /// gate has no value: the domains have no neutral element, because
    /// treating an empty AND as "trivially feasible" would be the dangerous
    /// answer in a security tool. Carries the gate's public ID, never its
    /// label.
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MinTime, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let id = NodeId::new_v7();
    /// let tree = AttackTree::new(id, Node::gate(GateKind::And, "empty"))?;
    /// let err = evaluate(&tree, &MinTime, |_| Some(1)).err();
    /// assert_eq!(err, Some(ModelError::EmptyGate(id)));
    /// assert_eq!(
    ///     err.map(|e| e.to_string()),
    ///     Some(format!("gate node {id} has no children, so it has no value")),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[error("gate node {0} has no children, so it has no value")]
    EmptyGate(NodeId),
    /// The leaf-value callback of [`evaluate`](crate::evaluate) gave no
    /// value for this step. Evaluation never substitutes a default. Carries
    /// the step's public ID, never its label.
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MinTime, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let id = NodeId::new_v7();
    /// let tree = AttackTree::new(id, Node::step("unrated"))?;
    /// let err = evaluate(&tree, &MinTime, |_| None).err();
    /// assert_eq!(err, Some(ModelError::MissingLeafValue(id)));
    /// assert_eq!(
    ///     err.map(|e| e.to_string()),
    ///     Some(format!("no value given for step node {id}")),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[error("no value given for step node {0}")]
    MissingLeafValue(NodeId),

    // -- Project ops ----------------------------------------------------------
    //
    // These carry public IDs, vocabulary indices, and counts, never entity
    // names (customer data).
    /// An asset with this ID already exists in the project.
    #[error("asset {0} already exists in the project")]
    DuplicateAsset(AssetId),
    /// A damage scenario with this ID already exists in the project.
    #[error("damage scenario {0} already exists in the project")]
    DuplicateDamageScenario(DamageId),
    /// A threat scenario with this ID already exists in the project.
    #[error("threat scenario {0} already exists in the project")]
    DuplicateThreatScenario(ThreatId),
    /// No asset with this ID is in the project.
    #[error("asset {0} is not in the project")]
    UnknownAsset(AssetId),
    /// No damage scenario with this ID is in the project.
    #[error("damage scenario {0} is not in the project")]
    UnknownDamageScenario(DamageId),
    /// No threat scenario with this ID is in the project.
    #[error("threat scenario {0} is not in the project")]
    UnknownThreatScenario(ThreatId),
    /// A name is empty after trimming whitespace.
    #[error("name is empty")]
    EmptyName,
    /// A name is longer than `max` chars.
    #[error("name is longer than {max} chars")]
    NameTooLong {
        /// The limit, in chars.
        max: usize,
    },
    /// The impact category index is not in the project's methodology.
    #[error("impact category index {} is not in the methodology", .0.index())]
    UnknownImpactCategory(ImpactCategory),
    /// The level index is not in that impact category's scale.
    #[error(
        "level index {} is not in the scale of impact category index {}",
        .level.index(),
        .category.index()
    )]
    UnknownLevel {
        /// The impact category.
        category: ImpactCategory,
        /// The out-of-range level.
        level: Level,
    },
    /// The property index is not in the project's methodology.
    #[error("property index {} is not in the methodology", .0.index())]
    UnknownProperty(Property),
    /// The threat category index is not in the project's methodology.
    #[error("threat category index {} is not in the methodology", .0.index())]
    UnknownThreatCategory(ThreatCategory),
    /// The catalog scheme index is not in the project's methodology.
    #[error("catalog scheme index {} is not in the methodology", .0.index())]
    UnknownCatalogScheme(CatalogScheme),
    /// The catalog ID does not match its scheme's syntax.
    #[error("catalog ID does not match the syntax of scheme index {}", .scheme.index())]
    InvalidCatalogId {
        /// The scheme whose syntax was not met.
        scheme: CatalogScheme,
    },
    /// An asset must have at least one security property.
    #[error("asset {0} has no security properties")]
    NoSecurityProperties(AssetId),
    /// The threat category violates a property the asset does not carry.
    #[error("asset {asset} does not carry property index {}", .property.index())]
    PropertyNotOnAsset {
        /// The targeted asset.
        asset: AssetId,
        /// The missing property.
        property: Property,
    },
    /// The property cannot be dropped: `threats` threats target it on this
    /// asset.
    #[error(
        "property index {} of asset {asset} is targeted by {threats} threat(s)",
        .property.index()
    )]
    PropertyInUse {
        /// The asset.
        asset: AssetId,
        /// The property still in use.
        property: Property,
        /// How many threats target it.
        threats: u32,
    },
    /// The asset cannot be removed: `threats` threats target it.
    #[error("asset {asset} is targeted by {threats} threat(s)")]
    AssetInUse {
        /// The asset.
        asset: AssetId,
        /// How many threats target it.
        threats: u32,
    },
    /// The damage scenario cannot be removed: `threats` threats link to it.
    #[error("damage scenario {damage} is linked by {threats} threat(s)")]
    DamageScenarioInUse {
        /// The damage scenario.
        damage: DamageId,
        /// How many threats link to it.
        threats: u32,
    },
    /// An impacts list names the same category twice.
    #[error("impact category index {} is listed twice", .0.index())]
    DuplicateImpactCategory(ImpactCategory),
    /// The threat already links to this damage scenario, or an op lists it
    /// twice.
    #[error("threat scenario {threat} already links damage scenario {damage}")]
    DuplicateDamageLink {
        /// The threat scenario.
        threat: ThreatId,
        /// The damage scenario.
        damage: DamageId,
    },
    /// The threat does not link to this damage scenario.
    #[error("threat scenario {threat} does not link damage scenario {damage}")]
    DamageNotLinked {
        /// The threat scenario.
        threat: ThreatId,
        /// The damage scenario.
        damage: DamageId,
    },
    /// The threat does not have this catalog ref.
    #[error("threat scenario {threat} does not have this catalog ref")]
    CatalogRefNotFound {
        /// The threat scenario.
        threat: ThreatId,
    },
    /// The threat already has this catalog ref, or an op lists it twice.
    #[error("threat scenario {threat} already has this catalog ref")]
    DuplicateCatalogRef {
        /// The threat scenario.
        threat: ThreatId,
    },
    /// A threat may have at most `max` catalog refs.
    #[error("threat scenario {threat} would have more than {max} catalog refs")]
    TooManyCatalogRefs {
        /// The threat scenario.
        threat: ThreatId,
        /// The limit.
        max: usize,
    },
    /// A threat may link to at most `max` damage scenarios.
    ///
    /// ```
    /// use aegis_model::{ModelError, ThreatId, MAX_DAMAGE_LINKS};
    ///
    /// let threat = ThreatId::new_v7();
    /// let err = ModelError::TooManyDamageLinks { threat, max: MAX_DAMAGE_LINKS };
    /// assert_eq!(
    ///     err.to_string(),
    ///     format!("threat scenario {threat} would link more than 256 damage scenarios"),
    /// );
    /// ```
    #[error("threat scenario {threat} would link more than {max} damage scenarios")]
    TooManyDamageLinks {
        /// The threat scenario.
        threat: ThreatId,
        /// The limit.
        max: usize,
    },
    /// The project cannot hold more entities of this kind (the arena is at
    /// its slot limit, at most `u32::MAX`).
    #[error("project capacity exceeded")]
    ProjectCapacityExceeded,
}

/// Errors from [`Methodology::new`](crate::Methodology::new): one variant
/// per rule a [`MethodologyDef`](crate::MethodologyDef) must meet.
///
/// Variants carry vocabulary keys (methodology data, not project data).
///
/// ```
/// use aegis_model::{Methodology, MethodologyDef, MethodologyError};
///
/// let def = MethodologyDef {
///     key: "empty".into(),
///     version: "1".into(),
///     label: "Empty".into(),
///     impact_categories: vec![],
///     properties: vec![],
///     threat_categories: vec![],
///     catalog_schemes: vec![],
/// };
/// assert_eq!(Methodology::new(def), Err(MethodologyError::NoImpactCategory));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MethodologyError {
    /// A key does not match `[a-z][a-z0-9_-]{0,63}`.
    #[error("key {0:?} does not match [a-z][a-z0-9_-]{{0,63}}")]
    InvalidKey(Arc<str>),
    /// The version is blank or longer than 64 chars.
    #[error("version must be 1 to 64 chars and not blank")]
    InvalidVersion,
    /// The label of the item with this key is blank.
    #[error("label of {key:?} is blank")]
    BlankLabel {
        /// Key of the item.
        key: Arc<str>,
    },
    /// The label of the item with this key is longer than `max` chars.
    #[error("label of {key:?} is longer than {max} chars")]
    LabelTooLong {
        /// Key of the item.
        key: Arc<str>,
        /// The limit, in chars.
        max: usize,
    },
    /// No impact category is declared.
    #[error("at least one impact category is required")]
    NoImpactCategory,
    /// More than `max` impact categories are declared.
    #[error("more than {max} impact categories")]
    TooManyImpactCategories {
        /// The limit.
        max: usize,
    },
    /// A rating scale has fewer than `min` or more than `max` levels.
    #[error("impact category {category:?} must have {min} to {max} levels")]
    LevelCount {
        /// Key of the impact category.
        category: Arc<str>,
        /// Fewest levels.
        min: usize,
        /// Most levels.
        max: usize,
    },
    /// Two impact categories share a key.
    #[error("impact category key {0:?} is declared twice")]
    DuplicateImpactCategory(Arc<str>),
    /// Two levels of one category share a key.
    #[error("level key {level:?} is declared twice in impact category {category:?}")]
    DuplicateLevel {
        /// Key of the impact category.
        category: Arc<str>,
        /// The repeated level key.
        level: Arc<str>,
    },
    /// No security property is declared.
    #[error("at least one security property is required")]
    NoProperty,
    /// More than `max` security properties are declared.
    #[error("more than {max} security properties")]
    TooManyProperties {
        /// The limit.
        max: usize,
    },
    /// Two properties share a key.
    #[error("property key {0:?} is declared twice")]
    DuplicateProperty(Arc<str>),
    /// No threat category is declared.
    #[error("at least one threat category is required")]
    NoThreatCategory,
    /// More than `max` threat categories are declared.
    #[error("more than {max} threat categories")]
    TooManyThreatCategories {
        /// The limit.
        max: usize,
    },
    /// Two threat categories share a key.
    #[error("threat category key {0:?} is declared twice")]
    DuplicateThreatCategory(Arc<str>),
    /// A threat category violates no property.
    #[error("threat category {category:?} violates no property")]
    EmptyViolates {
        /// Key of the threat category.
        category: Arc<str>,
    },
    /// A threat category names a property that is not declared.
    #[error("threat category {category:?} names unknown property {property:?}")]
    UnknownViolatedProperty {
        /// Key of the threat category.
        category: Arc<str>,
        /// The unknown property key.
        property: Arc<str>,
    },
    /// A threat category names the same property twice.
    #[error("threat category {category:?} names property {property:?} twice")]
    RepeatedViolatedProperty {
        /// Key of the threat category.
        category: Arc<str>,
        /// The repeated property key.
        property: Arc<str>,
    },
    /// More than `max` catalog schemes are declared.
    #[error("more than {max} catalog schemes")]
    TooManyCatalogSchemes {
        /// The limit.
        max: usize,
    },
    /// Two catalog schemes share a key.
    #[error("catalog scheme key {0:?} is declared twice")]
    DuplicateCatalogScheme(Arc<str>),
    /// A catalog syntax has fewer than 1 or more than 4 segments.
    #[error("catalog scheme {scheme:?} must have 1 to 4 segments")]
    CatalogSegmentCount {
        /// Key of the scheme.
        scheme: Arc<str>,
    },
    /// `required` is 0 or above the segment count.
    #[error("catalog scheme {scheme:?} must require 1 to all of its segments")]
    CatalogRequired {
        /// Key of the scheme.
        scheme: Arc<str>,
    },
    /// A segment has `min > max`.
    #[error("segment {segment} of catalog scheme {scheme:?} has min above max")]
    CatalogSegmentRange {
        /// Key of the scheme.
        scheme: Arc<str>,
        /// Position of the segment.
        segment: usize,
    },
    /// A fixed digit count is outside 1 to 5 or too small for `max`.
    #[error("segment {segment} of catalog scheme {scheme:?} has an invalid digit count")]
    CatalogSegmentDigits {
        /// Key of the scheme.
        scheme: Arc<str>,
        /// Position of the segment.
        segment: usize,
    },
    /// The prefix is longer than 16 bytes or has a char other than an ASCII
    /// letter, `-`, or `_`.
    #[error("catalog scheme {scheme:?} has an invalid prefix")]
    CatalogPrefix {
        /// Key of the scheme.
        scheme: Arc<str>,
    },
}

/// Errors from saving and loading: [`Project::save`](crate::Project::save),
/// [`Project::load`](crate::Project::load), [`OpLogWriter`](crate::OpLogWriter)
/// and [`OpLogReader`](crate::OpLogReader).
///
/// No variant carries decoded text except vocabulary keys, which are capped
/// at 64 bytes and printed with `{:?}` (escaped). [`Decode`](Self::Decode)
/// keeps only the position of a JSON error and drops the parser's message,
/// because that message can quote input such as an entity name.
///
/// Where it failed: [`Decode`](Self::Decode) and
/// [`TruncatedRecord`](Self::TruncatedRecord) carry their line. For any
/// other error from an op log record, [`OpLogReader::line`](crate::OpLogReader::line)
/// gives the line of the record that failed (the iterator stops there).
/// Snapshot errors other than `Decode` carry no position.
///
/// Not `PartialEq` (it can hold an [`std::io::Error`]); match it with
/// `matches!`.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{Methodology, Project, StoreError};
///
/// # fn main() -> Result<(), aegis_model::MethodologyError> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let err = Project::load(&b"{\"format\":\"spreadsheet\"}"[..], Arc::clone(&m)).err();
/// assert!(matches!(err, Some(StoreError::UnsupportedFormat { version: None })));
/// let newer = br#"{"format":"aegis-project","version":2}"#;
/// let err = Project::load(&newer[..], m).err();
/// assert!(matches!(err, Some(StoreError::UnsupportedFormat { version: Some(2) })));
/// # Ok(())
/// # }
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// Reading or writing failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The bytes are not valid JSON, or the JSON has the wrong shape (a
    /// missing, unknown, repeated, or mistyped field; trailing data; a
    /// blank line or a `\r` in an op log). Lines and columns are 1-based;
    /// a column of 0 means the end of the line.
    #[error("malformed input at line {line}, column {column}")]
    Decode {
        /// Line of the error.
        line: u64,
        /// Column of the error.
        column: u64,
    },
    /// A decode cap was exceeded, before any model validation ran.
    #[error("{0}")]
    LimitExceeded(Limit),
    /// The `format` string or the format `version` is not one this build
    /// reads. `version` is the file's format version when that is a
    /// non-negative integer other than 1 (most likely a file from a newer
    /// application: the user should update), and `None` when the file is
    /// not of the expected kind at all (the user picked the wrong file).
    #[error("{}", unsupported_format_text(*.version))]
    UnsupportedFormat {
        /// The file's format version, if it is a number other than 1.
        version: Option<u64>,
    },
    /// The file was written under another methodology (its key differs).
    #[error("file was written for methodology {found:?}, expected {expected:?}")]
    MethodologyMismatch {
        /// Key of the supplied methodology.
        expected: Arc<str>,
        /// Key in the file.
        found: Arc<str>,
    },
    /// A vocabulary key in the file is not in the supplied methodology.
    ///
    /// Likely causes: the methodology pack no longer defines the key (the
    /// file needs a pack version that does), or the file is corrupt. The
    /// message names only the vocabulary and the key; an application should
    /// add that hint for its users.
    #[error("unknown {kind} key {key:?}")]
    UnknownKey {
        /// Which vocabulary the key was looked up in.
        kind: VocabKind,
        /// The key, at most 64 bytes.
        key: Arc<str>,
    },
    /// An ID is not a hyphenated lowercase RFC 9562 version 7 UUID.
    #[error("invalid ID: not a hyphenated lowercase UUIDv7")]
    InvalidId,
    /// A catalog ref's text does not match its scheme's syntax.
    #[error("catalog ref does not match the syntax of scheme {scheme:?}")]
    InvalidCatalogRef {
        /// Key of the scheme.
        scheme: Arc<str>,
    },
    /// A property set or an impact map names the same key twice.
    #[error("a key is repeated in a set or map")]
    DuplicateEntry,
    /// An op log record has the wrong sequence number: a gap, a repeat, or
    /// a step back.
    #[error("expected sequence number {expected}, found {found}")]
    Sequence {
        /// The next number in the run.
        expected: u64,
        /// The number in the record.
        found: u64,
    },
    /// The last line of an op log does not end in `\n`, for example
    /// because a write was cut short.
    #[error("record at line {line} does not end in a newline")]
    TruncatedRecord {
        /// Line of the incomplete record.
        line: u64,
    },
    /// The op log is empty: it has no header line.
    #[error("op log has no header line")]
    MissingHeader,
    /// The op cannot be written so that it reads back the same: an index
    /// is not in the writer's methodology, an impact category is repeated,
    /// a list or name is over its decode cap, the record would exceed the
    /// record cap, or the sequence number would overflow. Nothing was
    /// written.
    #[error("op cannot be encoded under this methodology")]
    UnencodableOp,
    /// The decoded data broke a model rule (see [`ModelError`]).
    #[error("model rejected the data: {0}")]
    Model(#[from] ModelError),
}

/// The message for [`StoreError::UnsupportedFormat`]. Time O(1).
fn unsupported_format_text(version: Option<u64>) -> String {
    match version {
        Some(v) => format!(
            "file format version {v} is not supported: this build reads version 1, \
             a newer application may read it"
        ),
        None => "not a supported AegisTARA file: wrong format name or format version".into(),
    }
}

/// Formats IDs as `a -> b -> c`; with `close`, the first ID is repeated at
/// the end. Time O(path length).
fn path_text(path: &[NodeId], close: bool) -> String {
    let closing = if close { path.first() } else { None };
    let mut out = String::new();
    for n in path.iter().chain(closing) {
        if !out.is_empty() {
            out.push_str(" -> ");
        }
        out.push_str(&n.to_string());
    }
    out
}
