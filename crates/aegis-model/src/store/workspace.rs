//! `Project::to_workspace_files`: a project as a list of small JSON files,
//! one per kind of element and one per attack tree, so a folder of them
//! diffs well in git. Nothing here touches the disk.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use super::{Limit, MAX_PROJECT_FILE_BYTES, MAX_WORKSPACE_FILE_BYTES, MAX_WORKSPACE_FILES, wire};
use crate::error::{ModelError, StoreError, WorkspaceProblem};
use crate::id::VisualPrefix;
use crate::methodology::Methodology;
use crate::project::Project;

/// One file of a project workspace: a path relative to the workspace
/// folder (always `/`-separated) and the bytes to write there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFile {
    /// Path relative to the workspace folder, for example `shared/damage.json`.
    pub path: String,
    /// The file content: pretty-printed JSON ending in one newline.
    pub bytes: Vec<u8>,
}

/// Fixed files: path, `kind`, and the snapshot keys each one holds.
const LAYOUT: &[(&str, &str, &[&str])] = &[
    ("aegis.json", "workspace", &["methodology", "languages"]),
    ("descriptions.json", "descriptions", &["descriptions"]),
    ("review-marks.json", "review-marks", &["review_marks"]),
    ("shared/damage.json", "damage", &["damage_scenarios"]),
    ("shared/controls.json", "controls", &["security_controls"]),
    ("shared/goals.json", "goals", &["cybersecurity_goals"]),
    ("shared/claims.json", "claims", &["cybersecurity_claims"]),
    (
        "systems/main/item.json",
        "item",
        &["components", "interfaces", "data_flows", "trust_boundaries"],
    ),
    ("systems/main/assets.json", "assets", &["assets"]),
    (
        "systems/main/threats.json",
        "threats",
        &["threat_scenarios"],
    ),
];

const TREES_KEY: &str = "attack_trees";

/// The `version` of the workspace envelope. It is the workspace format's own
/// number, not the snapshot's [`wire::FORMAT_VERSION`]: the two change
/// independently.
const WORKSPACE_VERSION: u32 = 1;

/// The envelope followed by the held entries, in order.
struct FileView<'a> {
    kind: &'a str,
    entries: Vec<(&'a str, &'a Value)>,
}

impl Serialize for FileView<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_map(
            [
                ("format", &Value::from("aegis-workspace")),
                ("version", &Value::from(WORKSPACE_VERSION)),
                ("kind", &Value::from(self.kind)),
            ]
            .into_iter()
            .chain(self.entries.iter().map(|&(k, v)| (k, v))),
        )
    }
}

fn render(kind: &str, entries: Vec<(&str, &Value)>) -> Result<Vec<u8>, StoreError> {
    let mut bytes = serde_json::to_vec_pretty(&FileView { kind, entries })
        .map_err(super::wire::encode_error)?;
    bytes.push(b'\n');
    Ok(bytes)
}

impl Project {
    /// Returns the project as workspace files, sorted by path.
    ///
    /// `aegis.json` is always present; every other file is present only
    /// when the project has something for it (an empty list counts as
    /// nothing). Each file is a JSON object
    /// starting with `format`, `version` and `kind`, then the same values
    /// the snapshot holds under the same keys. The output depends only on
    /// the project, so equal projects give equal bytes.
    ///
    /// # Errors
    ///
    /// [`StoreError::UnencodableOp`] if the snapshot has a key this layout
    /// does not place, or a tree has no visual ID.
    ///
    /// O(n) in project size.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let files = project.to_workspace_files()?;
    /// assert_eq!(files.len(), 1);
    /// assert_eq!(files[0].path, "aegis.json");
    /// # Ok(())
    /// # }
    /// ```
    pub fn to_workspace_files(&self) -> Result<Vec<WorkspaceFile>, StoreError> {
        let (snapshot, tree_ids) = self.snapshot_value()?;
        let Value::Object(map) = &snapshot else {
            return Err(StoreError::UnencodableOp);
        };
        let placed = |k: &str| {
            k == "format"
                || k == "version"
                || k == TREES_KEY
                || LAYOUT.iter().any(|(_, _, keys)| keys.contains(&k))
        };
        if !map.keys().all(|k| placed(k)) {
            return Err(StoreError::UnencodableOp);
        }

        let mut files = Vec::new();
        for &(path, kind, keys) in LAYOUT {
            let entries: Vec<(&str, &Value)> = keys
                .iter()
                .filter_map(|&k| map.get(k).map(|v| (k, v)))
                .filter(|(_, v)| !matches!(v, Value::Array(a) if a.is_empty()))
                .collect();
            if path == "aegis.json" || !entries.is_empty() {
                files.push(WorkspaceFile {
                    path: path.to_owned(),
                    bytes: render(kind, entries)?,
                });
            }
        }
        if let Some(trees) = map.get(TREES_KEY) {
            let Value::Array(trees) = trees else {
                return Err(StoreError::UnencodableOp);
            };
            if trees.len() != tree_ids.len() {
                return Err(StoreError::UnencodableOp);
            }
            for (tree, visual) in trees.iter().zip(&tree_ids) {
                let one = Value::Array(vec![tree.clone()]);
                files.push(WorkspaceFile {
                    path: format!("systems/main/trees/{}.json", visual.to_lowercase()),
                    bytes: render("tree", vec![(TREES_KEY, &one)])?,
                });
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }
}

/// Longest path put into an error.
const MAX_PATH_IN_ERROR: usize = 256;

/// `text` cut to `max` bytes at a character boundary, and short enough that
/// its `Debug` form stays within `max` bytes too (escapes such as
/// `\u{1}` are longer than the character).
fn cut(text: &str, max: usize) -> String {
    let (mut bytes, mut shown) = (0, 0);
    let mut out = String::new();
    for c in text.chars() {
        bytes += c.len_utf8();
        shown += c.escape_debug().count();
        if bytes > max || shown > max {
            break;
        }
        out.push(c);
    }
    out
}

fn short(path: &str) -> String {
    cut(path, MAX_PATH_IN_ERROR)
}

/// Whether `path` obeys the path rules: not empty, relative, `/`-separated,
/// no `.` or `..` segment, and only lower-case ASCII letters, digits, `-`,
/// `.` and `/`.
fn path_ok(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'/'))
        && path.split('/').all(|seg| seg != "." && seg != "..")
}

/// The kind and snapshot keys a path holds, or `None` if the path is not
/// in the layout.
fn lookup(path: &str) -> Option<(&'static str, &'static [&'static str])> {
    if let Some(&(_, kind, keys)) = LAYOUT.iter().find(|(p, _, _)| *p == path) {
        return Some((kind, keys));
    }
    let name = path
        .strip_prefix("systems/main/trees/")?
        .strip_suffix(".json")?;
    (!name.is_empty() && !name.contains('/')).then_some(("tree", &[TREES_KEY][..]))
}

/// The file name of a tree file without directory and extension.
fn tree_stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".json").unwrap_or(name)
}

/// The fixed file that holds `key`, or `aegis.json`.
fn file_of_key(key: &str) -> &'static str {
    LAYOUT
        .iter()
        .find(|(_, _, keys)| keys.contains(&key))
        .map_or("aegis.json", |&(path, _, _)| path)
}

/// The second tree file (in load order) whose text holds `id`, else the
/// first, else `aegis.json`. `Project::load` meets the repeat in the second.
fn tree_file_with(id: &str, trees: &[(&str, &[u8])]) -> String {
    let needle = id.as_bytes();
    let mut holders = trees
        .iter()
        .filter(|(_, bytes)| bytes.windows(needle.len()).any(|w| w == needle))
        .map(|&(path, _)| path);
    let first = holders.next();
    holders
        .next()
        .or(first)
        .map_or_else(|| "aegis.json".to_owned(), short)
}

/// The file `Project::load` refused, as well as the error tells.
fn blame(err: &StoreError, trees: &[(&str, &[u8])]) -> String {
    let StoreError::Model(model) = err else {
        return "aegis.json".to_owned();
    };
    let key = match model {
        ModelError::DuplicateAttackTree(id) => return tree_file_with(&id.to_string(), trees),
        ModelError::DuplicateId(id) => return tree_file_with(&id.to_string(), trees),
        ModelError::UnknownAsset(_)
        | ModelError::UnknownDamageScenario(_)
        | ModelError::DuplicateThreatScenario(_) => "threat_scenarios",
        ModelError::DuplicateAsset(_) => "assets",
        ModelError::DuplicateDamageScenario(_) => "damage_scenarios",
        ModelError::DuplicateSecurityControl(_) => "security_controls",
        ModelError::DuplicateCybersecurityGoal(_) => "cybersecurity_goals",
        ModelError::DuplicateCybersecurityClaim(_) => "cybersecurity_claims",
        ModelError::DuplicateComponent(_) => "components",
        ModelError::DuplicateInterface(_) => "interfaces",
        ModelError::DuplicateDataFlow(_) => "data_flows",
        ModelError::DuplicateTrustBoundary(_) => "trust_boundaries",
        _ => return "aegis.json".to_owned(),
    };
    file_of_key(key).to_owned()
}

/// A file that passed the cheap checks: path, bytes, kind, held keys.
type Taken<'a> = (&'a str, &'a [u8], &'static str, &'static [&'static str]);

/// Pass 1: the caps and the path rules, before any JSON is parsed. At most
/// `MAX_WORKSPACE_FILES` files are kept, and `aegis.json` must be among them.
fn take_files<'a, I>(files: I) -> Result<Vec<Taken<'a>>, StoreError>
where
    I: IntoIterator<Item = (&'a str, &'a [u8])>,
{
    let mut taken: Vec<Taken<'a>> = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut total: u64 = 0;
    for (path, bytes) in files {
        if taken.len() >= MAX_WORKSPACE_FILES {
            return Err(StoreError::LimitExceeded(Limit::WORKSPACE_FILES));
        }
        let len = wire::to_u64(bytes.len());
        if len > MAX_WORKSPACE_FILE_BYTES {
            return Err(StoreError::LimitExceeded(Limit::WORKSPACE_FILE_BYTES));
        }
        total = total.saturating_add(len);
        if total > MAX_PROJECT_FILE_BYTES {
            return Err(StoreError::LimitExceeded(Limit::PROJECT_FILE));
        }
        let short_enough = path.len() <= MAX_PATH_IN_ERROR;
        let looked = (short_enough && path_ok(path))
            .then(|| lookup(path))
            .flatten();
        let Some((kind, keys)) = looked else {
            return Err(StoreError::WorkspacePath { path: short(path) });
        };
        if !seen.insert(path) {
            return Err(StoreError::WorkspaceDuplicatePath { path: short(path) });
        }
        taken.push((path, bytes, kind, keys));
    }
    if !seen.contains("aegis.json") {
        return Err(StoreError::WorkspaceMissing {
            path: "aegis.json".to_owned(),
        });
    }
    Ok(taken)
}

impl Project {
    /// Reads workspace files written by [`Project::to_workspace_files`]
    /// back into a project. The files may arrive in any order.
    ///
    /// The count and sizes of the files are capped
    /// ([`MAX_WORKSPACE_FILES`], [`MAX_WORKSPACE_FILE_BYTES`], and
    /// [`MAX_PROJECT_FILE_BYTES`] for the sum) and every path is checked
    /// before any JSON is parsed. Then each file's envelope is checked, the
    /// held keys are joined into one snapshot, and that goes through
    /// [`Project::load`], so every check the snapshot reader makes still
    /// runs. A refusal returns no project.
    ///
    /// # Errors
    ///
    /// [`StoreError::LimitExceeded`] over a cap. [`StoreError::WorkspacePath`]
    /// for a path outside the layout or breaking a path rule,
    /// [`StoreError::WorkspaceDuplicatePath`] for a repeated path,
    /// [`StoreError::WorkspaceEnvelope`] for a bad envelope,
    /// [`StoreError::WorkspaceUnknownField`] for a key the file's kind does
    /// not hold, [`StoreError::WorkspaceTreeName`] for a tree file that is
    /// not one tree named by its visual ID,
    /// [`StoreError::WorkspaceMissing`] if `aegis.json` is absent,
    /// [`StoreError::WorkspaceMethodology`] for another methodology key.
    /// Any other error from [`Project::load`] comes back as
    /// [`StoreError::InWorkspace`], naming the file when it can be told.
    ///
    /// O(n) in the total size of the files.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let project = Project::new(Arc::clone(&m));
    /// let files = project.to_workspace_files()?;
    /// let back = Project::from_workspace_files(
    ///     files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
    ///     m,
    /// )?;
    /// assert_eq!(back, project);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_workspace_files<'a, I>(
        files: I,
        methodology: Arc<Methodology>,
    ) -> Result<Project, StoreError>
    where
        I: IntoIterator<Item = (&'a str, &'a [u8])>,
    {
        let taken = take_files(files)?;

        // Pass 2: parse each file.
        let mut snapshot = Map::new();
        let mut trees: Vec<(u32, &str, &[u8], Vec<Value>)> = Vec::new();
        for (path, bytes, kind, keys) in taken {
            let envelope = |problem| StoreError::WorkspaceEnvelope {
                path: short(path),
                problem,
            };
            let Ok(Value::Object(mut obj)) = serde_json::from_slice::<Value>(bytes) else {
                return Err(envelope(WorkspaceProblem::Json));
            };
            if obj.remove("format").as_ref().and_then(Value::as_str) != Some("aegis-workspace") {
                return Err(envelope(WorkspaceProblem::Format));
            }
            if obj.remove("version").as_ref().and_then(Value::as_u64)
                != Some(u64::from(WORKSPACE_VERSION))
            {
                return Err(envelope(WorkspaceProblem::Version));
            }
            if obj.remove("kind").as_ref().and_then(Value::as_str) != Some(kind) {
                return Err(envelope(WorkspaceProblem::Kind));
            }
            if let Some(field) = obj.keys().find(|k| !keys.contains(&k.as_str())) {
                return Err(StoreError::WorkspaceUnknownField {
                    path: short(path),
                    field: cut(field, super::MAX_KEY_BYTES),
                });
            }
            if kind == "tree" {
                let tree_name = || StoreError::WorkspaceTreeName { path: short(path) };
                let Some(Value::Array(mut list)) = obj.remove(TREES_KEY) else {
                    return Err(envelope(WorkspaceProblem::Json));
                };
                // The writer leaves out a tree's visual ID when it equals the
                // tree's position, so the file name is the ID to fall back on.
                let stem = tree_stem(path);
                let Some(num) = VisualPrefix::Tree.parse_num(&stem.to_uppercase()) else {
                    return Err(tree_name());
                };
                let [Value::Object(tree)] = list.as_mut_slice() else {
                    return Err(tree_name());
                };
                match tree.get("visual_id") {
                    None => {
                        tree.insert("visual_id".to_owned(), Value::from(stem.to_uppercase()));
                    }
                    Some(Value::String(v)) if v.to_lowercase() == stem => {}
                    Some(_) => return Err(tree_name()),
                }
                trees.push((num, path, bytes, list));
            } else {
                snapshot.append(&mut obj);
            }
        }
        // By the number in the file name: `at100` comes after `at99`.
        trees.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
        let tree_files: Vec<(&str, &[u8])> = trees.iter().map(|t| (t.1, t.2)).collect();
        if !trees.is_empty() {
            let joined = trees.into_iter().flat_map(|t| t.3).collect();
            snapshot.insert(TREES_KEY.to_owned(), Value::Array(joined));
        }
        // The writer leaves out empty lists; the snapshot requires these three.
        for key in ["assets", "damage_scenarios", "threat_scenarios"] {
            snapshot
                .entry(key)
                .or_insert_with(|| Value::Array(Vec::new()));
        }
        snapshot.insert("format".to_owned(), Value::from("aegis-project"));
        snapshot.insert("version".to_owned(), Value::from(wire::FORMAT_VERSION));
        let bytes = serde_json::to_vec(&Value::Object(snapshot)).map_err(wire::encode_error)?;
        Project::load(bytes.as_slice(), methodology).map_err(|err| match err {
            StoreError::MethodologyMismatch { .. } => StoreError::WorkspaceMethodology {
                path: "aegis.json".to_owned(),
            },
            other => StoreError::InWorkspace {
                path: blame(&other, &tree_files),
                source: Box::new(other),
            },
        })
    }
}
