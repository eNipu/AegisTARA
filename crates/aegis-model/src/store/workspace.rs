//! `Project::to_workspace_files`: a project as a list of small JSON files,
//! one per kind of element and one per attack tree, so a folder of them
//! diffs well in git. Nothing here touches the disk.

use std::sync::Arc;

use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use crate::error::{StoreError, WorkspaceProblem};
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
                ("version", &Value::from(1)),
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

/// The path cut to [`MAX_PATH_IN_ERROR`] bytes at a character boundary.
fn short(path: &str) -> String {
    let mut end = path.len().min(MAX_PATH_IN_ERROR);
    while !path.is_char_boundary(end) {
        end -= 1;
    }
    path[..end].to_owned()
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

impl Project {
    /// Reads workspace files written by [`Project::to_workspace_files`]
    /// back into a project. The files may arrive in any order.
    ///
    /// Each file's envelope is checked, the held keys are joined into one
    /// snapshot, and that goes through [`Project::load`], so every check
    /// the snapshot reader makes still runs.
    ///
    /// # Errors
    ///
    /// [`StoreError::WorkspacePath`] for a path outside the layout or a
    /// repeated path, [`StoreError::WorkspaceEnvelope`] for a bad envelope,
    /// [`StoreError::WorkspaceMissing`] if `aegis.json` is absent. Any error
    /// from [`Project::load`] is returned as it is.
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
        let mut snapshot = Map::new();
        let mut trees: Vec<(&str, Vec<Value>)> = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        for (path, bytes) in files {
            let bad_path = || StoreError::WorkspacePath { path: short(path) };
            let (kind, keys) = lookup(path).ok_or_else(bad_path)?;
            if seen.contains(&path) {
                return Err(bad_path());
            }
            seen.push(path);
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
            if obj.remove("version").as_ref().and_then(Value::as_u64) != Some(1) {
                return Err(envelope(WorkspaceProblem::Version));
            }
            if obj.remove("kind").as_ref().and_then(Value::as_str) != Some(kind) {
                return Err(envelope(WorkspaceProblem::Kind));
            }
            if obj.keys().any(|k| !keys.contains(&k.as_str())) {
                return Err(envelope(WorkspaceProblem::Json));
            }
            if kind == "tree" {
                let Some(Value::Array(list)) = obj.remove(TREES_KEY) else {
                    return Err(envelope(WorkspaceProblem::Json));
                };
                trees.push((path, list));
            } else {
                snapshot.append(&mut obj);
            }
        }
        if !seen.contains(&"aegis.json") {
            return Err(StoreError::WorkspaceMissing {
                path: "aegis.json".to_owned(),
            });
        }
        if !trees.is_empty() {
            trees.sort_by(|a, b| a.0.cmp(b.0));
            let joined = trees.into_iter().flat_map(|(_, list)| list).collect();
            snapshot.insert(TREES_KEY.to_owned(), Value::Array(joined));
        }
        // The writer leaves out empty lists; the snapshot requires these three.
        for key in ["assets", "damage_scenarios", "threat_scenarios"] {
            snapshot
                .entry(key)
                .or_insert_with(|| Value::Array(Vec::new()));
        }
        snapshot.insert("format".to_owned(), Value::from("aegis-project"));
        snapshot.insert("version".to_owned(), Value::from(1));
        let bytes =
            serde_json::to_vec(&Value::Object(snapshot)).map_err(super::wire::encode_error)?;
        Project::load(bytes.as_slice(), methodology)
    }
}
