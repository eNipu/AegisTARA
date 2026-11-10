//! `Project::to_workspace_files`: a project as a list of small JSON files,
//! one per kind of element and one per attack tree, so a folder of them
//! diffs well in git. Nothing here touches the disk.

use serde::{Serialize, Serializer};
use serde_json::Value;

use crate::error::StoreError;
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
