//! What a screen reader is told about the nodes of an attack tree (job 051,
//! ADR 0045, gap G14).
//!
//! The canvas is drawn from a display list, which a screen reader cannot
//! read. This module describes the nodes in words instead: [`describe`] says
//! what one node is and where it sits, [`sentence`] turns that into one
//! sentence, and [`visible`] lists the nodes on screen with their screen
//! boxes, so the window can lay one accessible element over each box.
//!
//! Costs: `describe` is O(depth); `visible` is O(log n + k x depth) for k
//! visible nodes, so a tree with a million nodes costs only what is on
//! screen (rule A9).

use crate::geometry::Rect;
use crate::scene::{NodeKind, Scene};
use crate::viewport::Viewport;
use aegis_model::GateKind;

/// Facts about one node, ready to be worded.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeAccess {
    /// The node's position in the scene.
    pub node: u32,
    /// Its label.
    pub label: String,
    /// Step or gate.
    pub kind: NodeKind,
    /// Levels below the root (the root is 0).
    pub depth: u32,
    /// Position among its owner's children, from 1 (1 for the root).
    pub position: u32,
    /// How many children its owner has (1 for the root).
    pub siblings: u32,
    /// How many children it has, shared ones included.
    pub children: u32,
    /// Its owner, if it is not the root.
    pub parent: Option<u32>,
    /// The owner's label.
    pub parent_label: Option<String>,
    /// The annotation shown on the node, for example `13 · High`.
    pub score: Option<String>,
    /// What the step goes through, if set.
    pub via: Option<String>,
    /// What caps the step's feasibility, if capped (CFG-04).
    pub capped: Option<String>,
    /// Human-readable exclusion reasons when the node is excluded by applicability.
    pub excluded: Option<String>,
    /// On the deciding path.
    pub critical: bool,
    /// How many other gates reach it through a shared edge.
    pub shared_from: u32,
}

/// A node on screen: what to say, and where its box is.
#[derive(Clone, Debug, PartialEq)]
pub struct VisibleNode {
    /// The facts.
    pub access: NodeAccess,
    /// Left edge on screen, in pixels.
    pub x: f32,
    /// Top edge on screen.
    pub y: f32,
    /// Width on screen.
    pub w: f32,
    /// Height on screen.
    pub h: f32,
}

/// The facts about node `v`, or `None` if there is no such node.
/// Time O(depth).
#[must_use]
pub fn describe(scene: &Scene, v: u32) -> Option<NodeAccess> {
    if v as usize >= scene.len() {
        return None;
    }
    let parent = scene.owner_pos(v);
    let (position, siblings) = parent.map_or((1, 1), |p| {
        let kids = scene.owned_children(p);
        let at = kids.iter().position(|k| *k == v).unwrap_or(0);
        (to_u32(at + 1), to_u32(kids.len()))
    });
    let mut depth = 0;
    let mut up = parent;
    while let Some(p) = up {
        depth += 1;
        up = scene.owner_pos(p);
    }
    let style = scene.style(v);
    Some(NodeAccess {
        node: v,
        label: scene.label(v).to_owned(),
        kind: scene.kind(v),
        depth,
        position,
        siblings,
        children: to_u32(scene.edges_from(v).len()),
        parent,
        parent_label: parent.map(|p| scene.label(p).to_owned()),
        score: style.and_then(|s| s.score.clone()),
        via: style.and_then(|s| s.via.clone()),
        capped: style.and_then(|s| s.capped.clone()),
        excluded: style.and_then(|s| match (&s.excluded, &s.excluded_label) {
            (Some(reason), Some(label)) if reason != label => Some(format!("{label}: {reason}")),
            _ => s.excluded.clone(),
        }),
        critical: style.is_some_and(|s| s.critical),
        shared_from: scene.shared_in(v),
    })
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn plural(n: u32, one: &str, many: &str) -> String {
    match n {
        0 => format!("no {many}"),
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    }
}

/// One sentence about a node, in plain words. `selected` adds "Selected."
#[must_use]
pub fn sentence(a: &NodeAccess, selected: bool) -> String {
    let mut out = format!("{}.", a.label.trim_end_matches('.'));
    match a.kind {
        NodeKind::Step => out.push_str(" Step."),
        NodeKind::Gate(g) => {
            let word = match g {
                GateKind::Or => "OR gate",
                GateKind::And => "AND gate",
                GateKind::SeqAnd => "Sequential AND gate",
            };
            out.push_str(&format!(
                " {word} with {}.",
                plural(a.children, "child", "children")
            ));
        }
    }
    match &a.parent_label {
        Some(parent) => out.push_str(&format!(
            " Child {} of {} under {}.",
            a.position,
            a.siblings,
            parent.trim_end_matches('.')
        )),
        None => out.push_str(" Root of the tree."),
    }
    if let Some(score) = &a.score {
        out.push_str(&format!(" Rating {score}."));
    }
    if let Some(via) = &a.via {
        out.push_str(&format!(" Goes through {via}."));
    }
    if let Some(capped) = &a.capped {
        out.push_str(&format!(" Capped by {capped}."));
    }
    if let Some(excluded) = &a.excluded {
        out.push_str(&format!(" Excluded: {}.", excluded.trim_end_matches('.')));
    }
    if a.critical {
        out.push_str(" On the deciding path.");
    }
    if a.shared_from > 0 {
        out.push_str(&format!(
            " Also reached from {}.",
            plural(a.shared_from, "other gate", "other gates")
        ));
    }
    if selected {
        out.push_str(" Selected.");
    }
    out
}

/// The nodes that are on screen, in node order, with their screen boxes.
/// Time O(log n + k x depth) for k visible nodes.
#[must_use]
pub fn visible(scene: &Scene, view: &Viewport) -> Vec<VisibleNode> {
    let area: Rect = view.world_rect();
    scene
        .nodes_in(&area)
        .into_iter()
        .filter_map(|v| {
            let rect = scene.node_idx(v).and_then(|i| scene.node_rect(i))?;
            let at = view.to_screen(crate::geometry::Point::new(rect.origin.x, rect.origin.y));
            Some(VisibleNode {
                access: describe(scene, v)?,
                x: at.x,
                y: at.y,
                w: rect.size.w * view.zoom,
                h: rect.size.h * view.zoom,
            })
        })
        .collect()
}
