//! The easiest attack paths and the choke points of a view (job 120).

use aegis_canvas::Tone;
use aegis_model::{
    AttackTree, FeasibilityLevel, NodeId, NodeIdx, Project, SystemUse, TreeId, TreeIdx,
};

use crate::systems::SystemView;
use crate::view::{step_other_system, through_label};
use crate::workspace::Workspace;

/// How many trees are searched, how many paths each gives, and how many
/// rows each list keeps.
pub const TOP: usize = 5;

/// One of the easiest attack paths of the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathRow {
    /// The tree the path belongs to.
    pub tree: TreeId,
    /// Short visual ID of the tree (`AT00`, ...).
    pub tree_visual_id: String,
    /// The tree's root label: what the attacker achieves.
    pub goal: String,
    /// The steps in path order: "Inject on CAN-1 via GW / OBD · Gateway",
    /// or the bare label for a step with no via.
    pub steps: Vec<String>,
    /// The path's feasibility after controls, as a label.
    pub feasibility: String,
    /// The tone of the feasibility.
    pub tone: Tone,
    /// The step that limits the path, to select it.
    pub bottleneck: NodeId,
}

/// A step that lies on many of the easiest paths of its tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChokeRow {
    /// The tree the step belongs to.
    pub tree: TreeId,
    /// Short visual ID of the tree.
    pub tree_visual_id: String,
    /// The tree's root label.
    pub goal: String,
    /// The step, to select it.
    pub node: NodeId,
    /// The step's text, formed as in `PathRow::steps`.
    pub step: String,
    /// How many of the tree's searched paths hold the step.
    pub paths: usize,
    /// How many paths of the tree were searched (at most `TOP`).
    pub of: usize,
    /// On how many of them the step is the limiting one.
    pub bottlenecks: usize,
}

/// The easiest attack paths and the choke points of what a view shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TopPaths {
    /// The `TOP` easiest paths, easiest first.
    pub paths: Vec<PathRow>,
    /// The `TOP` steps on most of the easiest paths of their trees.
    pub choke_points: Vec<ChokeRow>,
}

fn step_text(
    project: &Project,
    usage: &SystemUse,
    tree: TreeIdx,
    t: &AttackTree,
    node: NodeIdx,
) -> String {
    let label = t
        .get(node)
        .map(|n| n.label().to_owned())
        .unwrap_or_default();
    let Some(via) = project.step_via(tree, node) else {
        return label;
    };
    let mut text = format!("{label} via {}", through_label(project, via));
    if let Some(system) = step_other_system(project, usage, tree, node) {
        text.push_str(" · ");
        text.push_str(&system);
    }
    text
}

/// The easiest attack paths and the choke points of what `view` shows.
///
/// Time O(threats + T log T) for T candidate trees to choose the trees,
/// then at most `TOP` calls of `attack_paths` and of `choke_points`, each
/// bounded by the tree size as documented in `aegis_model::Evaluation`.
#[must_use]
pub fn top_paths(ws: &Workspace, view: &SystemView) -> TopPaths {
    let project = ws.project();
    let view = view.normalized(project);
    let usage = project.system_use();
    let set = usage.set_of(view.systems());
    let all = view.is_all();
    let m = project.methodology();
    let count = m.feasibility_levels().len();
    let tone = |l: FeasibilityLevel| {
        let pos = m
            .feasibility_levels()
            .position(|(x, _, _)| x == l)
            .unwrap_or(0);
        Tone::from_level(pos, count)
    };

    // 1. Candidate trees, once each, in tree index order.
    let mut seen = std::collections::BTreeSet::new();
    for (t_idx, _, threat) in project.threat_scenarios() {
        if !all && !usage.threat(t_idx).shows_in(set) {
            continue;
        }
        if let Some(tree) = threat.attack_tree() {
            seen.insert(tree);
        }
    }
    let mut candidates = Vec::new();
    for tree in seen {
        let Some(id) = project.attack_tree_id(tree) else {
            continue;
        };
        if let Ok(eval) = ws.cache().evaluate_residual_tree(project, id) {
            candidates.push((tree, id, eval));
        }
    }
    // 2. Easiest root first; stable, so ties keep tree order.
    candidates.sort_by(|a, b| b.2.root_value().cmp(a.2.root_value()));
    candidates.truncate(TOP);

    let mut paths: Vec<(FeasibilityLevel, PathRow)> = Vec::new();
    let mut chokes: Vec<ChokeRow> = Vec::new();
    for (tree, id, eval) in candidates {
        let Some(t) = project.attack_tree(tree) else {
            continue;
        };
        let Some(iter) = eval.attack_paths(t) else {
            continue;
        };
        let visual = project.attack_tree_visual_id(tree).unwrap_or_default();
        let goal = t
            .get(t.root())
            .map(|n| n.label().to_owned())
            .unwrap_or_default();
        let mut taken = 0;
        for p in iter.take(TOP) {
            taken += 1;
            let Some(bottleneck) = t.id_of(p.bottleneck()) else {
                continue;
            };
            let level = *p.feasibility();
            paths.push((
                level,
                PathRow {
                    tree: id,
                    tree_visual_id: visual.clone(),
                    goal: goal.clone(),
                    steps: p
                        .steps()
                        .iter()
                        .map(|&s| step_text(project, &usage, tree, t, s))
                        .collect(),
                    feasibility: m.feasibility_level_label(level).unwrap_or("?").to_owned(),
                    tone: tone(level),
                    bottleneck,
                },
            ));
        }
        // 4. Choke points of this tree.
        for c in eval.choke_points(t, TOP).unwrap_or_default() {
            let Some(node) = t.id_of(c.step()) else {
                continue;
            };
            chokes.push(ChokeRow {
                tree: id,
                tree_visual_id: visual.clone(),
                goal: goal.clone(),
                node,
                step: step_text(project, &usage, tree, t, c.step()),
                paths: c.path_count(),
                of: taken,
                bottlenecks: c.bottleneck_count(),
            });
        }
    }
    paths.sort_by_key(|a| std::cmp::Reverse(a.0));
    paths.truncate(TOP);
    chokes.sort_by(|a, b| {
        b.paths
            .cmp(&a.paths)
            .then_with(|| b.bottlenecks.cmp(&a.bottlenecks))
    });
    chokes.truncate(TOP);
    TopPaths {
        paths: paths.into_iter().map(|(_, r)| r).collect(),
        choke_points: chokes,
    }
}
