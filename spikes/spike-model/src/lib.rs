//! UI-free model for the UI toolkit spike.
//!
//! Every spike (egui, iced, GPUI, Slint) uses this crate, so the math and the
//! data are identical and only the UI differs. Not production code.
//!
//! Spike aggregation rule (not final):
//! - Step: attack potential (AP) = sum of its factor values.
//! - AND: per-factor maximum over the children, then sum.
//! - OR: the child with the lowest AP (the easiest path).

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Pack {
    pub name: String,
    pub factors: Vec<Factor>,
    pub ratings: Vec<Rating>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Factor {
    pub id: String,
    pub label: String,
    pub levels: Vec<Level>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Level {
    pub id: String,
    pub label: String,
    pub value: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rating {
    pub label: String,
    pub min: u32,
    pub max: u32,
    pub color: String,
}

impl Rating {
    /// Color as RGB bytes, parsed from "#RRGGBB".
    pub fn rgb(&self) -> [u8; 3] {
        let hex = self.color.trim_start_matches('#');
        let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("80"), 16).unwrap_or(128);
        [byte(0), byte(2), byte(4)]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Or,
    And,
    Step,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Or => "OR",
            Kind::And => "AND",
            Kind::Step => "STEP",
        }
    }
    pub fn is_gate(self) -> bool {
        self != Kind::Step
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    /// factor id -> level id (steps only)
    #[serde(default)]
    pub factors: BTreeMap<String, String>,
    #[serde(default)]
    pub children: Vec<Node>,
    #[serde(default)]
    pub rationale: String,
}

#[derive(Debug, Clone)]
pub struct NodeResult {
    pub ap: u32,
    /// Index into `Pack::ratings`.
    pub rating: usize,
    /// One value per factor, in pack order.
    pub factor_values: Vec<u32>,
    /// For OR nodes: the child that gives the lowest AP.
    pub chosen_child: Option<String>,
    /// Human-readable derivation, for tooltips and "why".
    pub explain: String,
}

/// A node placed in world coordinates (pixels at zoom 1.0, origin top-left).
#[derive(Debug, Clone)]
pub struct Placed {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub depth: usize,
    pub parent: Option<String>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Placed {
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

pub const NODE_W: f32 = 260.0;
pub const NODE_H: f32 = 76.0;
pub const COL_GAP: f32 = 330.0;
pub const ROW_GAP: f32 = 92.0;
pub const MARGIN: f32 = 40.0;
/// Recommended window size for every spike, so test coordinates match.
pub const WINDOW_W: f32 = 1400.0;
pub const WINDOW_H: f32 = 900.0;

pub struct Model {
    pack: Pack,
    root: Node,
    results: HashMap<String, NodeResult>,
    undo_stack: Vec<Node>,
    next_id: u64,
    generation: u64,
    pub read_only: bool,
}

impl Model {
    /// Load pack.json and tree.json from `spikes/shared` (or `AEGIS_SPIKE_DATA`),
    /// then apply the `NODES` and `READONLY` environment variables.
    pub fn load_default() -> Result<Model, String> {
        let dir = std::env::var_os("AEGIS_SPIKE_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../shared"));
        let read = |name: &str| {
            std::fs::read_to_string(dir.join(name)).map_err(|e| format!("read {name}: {e}"))
        };
        let pack: Pack = serde_json::from_str(&read("pack.json")?).map_err(|e| format!("pack.json: {e}"))?;
        let root: Node = serde_json::from_str(&read("tree.json")?).map_err(|e| format!("tree.json: {e}"))?;
        let mut model = Model::from_parts(pack, root);
        if let Some(n) = std::env::var("NODES").ok().and_then(|v| v.parse::<usize>().ok()) {
            model.add_filler(n);
        }
        model.read_only = std::env::var("READONLY").map(|v| v == "1").unwrap_or(false);
        model.recompute();
        Ok(model)
    }

    pub fn from_parts(pack: Pack, root: Node) -> Model {
        let mut model = Model {
            pack,
            root,
            results: HashMap::new(),
            undo_stack: Vec::new(),
            next_id: 1,
            generation: 0,
            read_only: false,
        };
        model.recompute();
        model
    }

    pub fn pack(&self) -> &Pack {
        &self.pack
    }
    pub fn root(&self) -> &Node {
        &self.root
    }
    /// Increases on every accepted change. Useful for cache invalidation in UIs.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn node(&self, id: &str) -> Option<&Node> {
        find(&self.root, id)
    }
    pub fn result(&self, id: &str) -> Option<&NodeResult> {
        self.results.get(id)
    }
    pub fn rating_of(&self, id: &str) -> Option<&Rating> {
        self.results.get(id).map(|r| &self.pack.ratings[r.rating])
    }
    pub fn level_of(&self, node_id: &str, factor_id: &str) -> Option<&str> {
        self.node(node_id)?.factors.get(factor_id).map(String::as_str)
    }
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Rating index for an attack potential value.
    pub fn rating_index(&self, ap: u32) -> usize {
        self.pack
            .ratings
            .iter()
            .position(|r| ap >= r.min && ap <= r.max)
            .unwrap_or(self.pack.ratings.len() - 1)
    }

    /// AP and rating for a step if the given factor levels were applied. Does not change the model.
    pub fn preview(&self, levels: &BTreeMap<String, String>) -> (u32, usize) {
        let ap = self.step_values(levels).iter().sum();
        (ap, self.rating_index(ap))
    }

    pub fn set_factor(&mut self, node_id: &str, factor_id: &str, level_id: &str) -> Result<(), String> {
        let mut levels = self.node(node_id).ok_or("no such node")?.factors.clone();
        levels.insert(factor_id.to_string(), level_id.to_string());
        let rationale = self.node(node_id).map(|n| n.rationale.clone()).unwrap_or_default();
        self.set_factors(node_id, levels, rationale)
    }

    pub fn set_factors(
        &mut self,
        node_id: &str,
        levels: BTreeMap<String, String>,
        rationale: String,
    ) -> Result<(), String> {
        self.check_writable()?;
        for (factor_id, level_id) in &levels {
            let factor = self.pack.factors.iter().find(|f| &f.id == factor_id).ok_or("unknown factor")?;
            if !factor.levels.iter().any(|l| &l.id == level_id) {
                return Err(format!("unknown level {level_id} for {factor_id}"));
            }
        }
        if self.node(node_id).ok_or("no such node")?.kind != Kind::Step {
            return Err("only steps have factors".into());
        }
        self.mutate(|root| {
            let node = find_mut(root, node_id).unwrap();
            node.factors = levels;
            node.rationale = rationale;
        });
        println!("CHANGE set_factors {node_id}");
        self.log_results();
        Ok(())
    }

    pub fn set_kind(&mut self, node_id: &str, kind: Kind) -> Result<(), String> {
        self.check_writable()?;
        if !kind.is_gate() || !self.node(node_id).ok_or("no such node")?.kind.is_gate() {
            return Err("only gates can switch between AND and OR".into());
        }
        self.mutate(|root| find_mut(root, node_id).unwrap().kind = kind);
        println!("CHANGE set_kind {node_id} {}", kind.label());
        self.log_results();
        Ok(())
    }

    /// Adds a new step under a gate, with the easiest level of each factor. Returns its id.
    pub fn add_child_step(&mut self, parent_id: &str) -> Result<String, String> {
        self.check_writable()?;
        if !self.node(parent_id).ok_or("no such node")?.kind.is_gate() {
            return Err("children can only be added to gates".into());
        }
        let id = format!("n{}", self.next_id);
        self.next_id += 1;
        let factors = self
            .pack
            .factors
            .iter()
            .map(|f| (f.id.clone(), f.levels[0].id.clone()))
            .collect();
        let child = Node {
            id: id.clone(),
            kind: Kind::Step,
            name: "New attack step".into(),
            factors,
            children: Vec::new(),
            rationale: String::new(),
        };
        self.mutate(|root| find_mut(root, parent_id).unwrap().children.push(child));
        println!("CHANGE add_child_step {parent_id} {id}");
        self.log_results();
        Ok(id)
    }

    pub fn delete(&mut self, node_id: &str) -> Result<(), String> {
        self.check_writable()?;
        if node_id == self.root.id {
            return Err("cannot delete the root".into());
        }
        self.node(node_id).ok_or("no such node")?;
        self.mutate(|root| remove(root, node_id));
        println!("CHANGE delete {node_id}");
        self.log_results();
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        match self.undo_stack.pop() {
            Some(previous) => {
                self.root = previous;
                self.generation += 1;
                self.recompute();
                println!("CHANGE undo");
                self.log_results();
                true
            }
            None => false,
        }
    }

    /// Horizontal tidy layout: root on the left, leaves stacked top to bottom.
    pub fn layout(&self) -> Vec<Placed> {
        let mut out = Vec::new();
        let mut slot = 0usize;
        place(&self.root, None, 0, &mut slot, &mut out);
        out
    }

    /// Prints `NODE <id> <x> <y>` with the screen center of each base node,
    /// assuming the canvas starts at the window's top-left with zoom 1 and no pan.
    pub fn log_layout(&self, offset_x: f32, offset_y: f32) {
        for p in self.layout().iter().filter(|p| is_base(&p.id)) {
            let (cx, cy) = p.center();
            println!("NODE {} {} {}", p.id, (cx + offset_x).round(), (cy + offset_y).round());
        }
    }

    /// Prints `RESULT <id> <ap> <rating>` for every base node.
    pub fn log_results(&self) {
        println!("RESULTS generation={}", self.generation);
        for p in self.layout().iter().filter(|p| is_base(&p.id)) {
            if let Some(r) = self.results.get(&p.id) {
                println!("RESULT {} {} {}", p.id, r.ap, self.pack.ratings[r.rating].label);
            }
        }
    }

    fn check_writable(&self) -> Result<(), String> {
        if self.read_only { Err("read-only".into()) } else { Ok(()) }
    }

    fn mutate(&mut self, change: impl FnOnce(&mut Node)) {
        self.undo_stack.push(self.root.clone());
        change(&mut self.root);
        self.generation += 1;
        self.recompute();
    }

    fn add_filler(&mut self, n: usize) {
        let factor_cycle = |i: usize, f: &Factor| {
            // Hard levels only (upper half), so the filler never changes the root result.
            let hard = &f.levels[f.levels.len() / 2..];
            hard[i % hard.len()].id.clone()
        };
        let mut groups = Vec::new();
        for g in 0..n.div_ceil(20) {
            let children = (g * 20..(g * 20 + 20).min(n))
                .map(|i| Node {
                    id: format!("f{i}"),
                    kind: Kind::Step,
                    name: format!("Generated step {i}"),
                    factors: self.pack.factors.iter().map(|f| (f.id.clone(), factor_cycle(i, f))).collect(),
                    children: Vec::new(),
                    rationale: String::new(),
                })
                .collect();
            groups.push(Node {
                id: format!("fg{g}"),
                kind: Kind::Or,
                name: format!("Generated group {g}"),
                factors: BTreeMap::new(),
                children,
                rationale: String::new(),
            });
        }
        self.root.children.push(Node {
            id: "fgroot".into(),
            kind: Kind::Or,
            name: format!("Filler subtree ({n} generated steps)"),
            factors: BTreeMap::new(),
            children: groups,
            rationale: String::new(),
        });
    }

    fn step_values(&self, levels: &BTreeMap<String, String>) -> Vec<u32> {
        self.pack
            .factors
            .iter()
            .map(|f| {
                levels
                    .get(&f.id)
                    .and_then(|lid| f.levels.iter().find(|l| &l.id == lid))
                    .map(|l| l.value)
                    .unwrap_or(0)
            })
            .collect()
    }

    fn recompute(&mut self) {
        let mut results = HashMap::new();
        self.eval(&self.root, &mut results);
        self.results = results;
    }

    fn eval(&self, node: &Node, out: &mut HashMap<String, NodeResult>) -> Vec<u32> {
        let (values, chosen, explain) = match node.kind {
            Kind::Step => {
                let values = self.step_values(&node.factors);
                let terms: Vec<String> = values.iter().map(u32::to_string).collect();
                (values, None, format!("Step: {}", terms.join(" + ")))
            }
            Kind::And => {
                let child_values: Vec<Vec<u32>> = node.children.iter().map(|c| self.eval(c, out)).collect();
                let values: Vec<u32> = (0..self.pack.factors.len())
                    .map(|i| child_values.iter().map(|v| v[i]).max().unwrap_or(0))
                    .collect();
                let terms: Vec<String> = values.iter().map(u32::to_string).collect();
                let text = format!("AND: per-factor max of {} children: {}", node.children.len(), terms.join(" + "));
                (values, None, text)
            }
            Kind::Or => {
                let mut best: Option<(u32, Vec<u32>, &Node)> = None;
                for child in &node.children {
                    let values = self.eval(child, out);
                    let ap: u32 = values.iter().sum();
                    if best.as_ref().is_none_or(|(b, _, _)| ap < *b) {
                        best = Some((ap, values, child));
                    }
                }
                match best {
                    Some((ap, values, child)) => {
                        let text = format!("OR: easiest child \"{}\" ({ap})", child.name);
                        (values, Some(child.id.clone()), text)
                    }
                    None => (vec![0; self.pack.factors.len()], None, "OR: no children".to_string()),
                }
            }
        };
        let ap: u32 = values.iter().sum();
        let rating = self.rating_index(ap);
        let explain = format!("{explain} = {ap} -> {}", self.pack.ratings[rating].label);
        out.insert(
            node.id.clone(),
            NodeResult { ap, rating, factor_values: values.clone(), chosen_child: chosen, explain },
        );
        values
    }
}

fn is_base(id: &str) -> bool {
    !id.starts_with('f')
}

fn find<'a>(node: &'a Node, id: &str) -> Option<&'a Node> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter().find_map(|c| find(c, id))
}

fn find_mut<'a>(node: &'a mut Node, id: &str) -> Option<&'a mut Node> {
    if node.id == id {
        return Some(node);
    }
    node.children.iter_mut().find_map(|c| find_mut(c, id))
}

fn remove(node: &mut Node, id: &str) {
    node.children.retain(|c| c.id != id);
    for child in &mut node.children {
        remove(child, id);
    }
}

/// Returns the node's center y.
fn place(node: &Node, parent: Option<&str>, depth: usize, slot: &mut usize, out: &mut Vec<Placed>) -> f32 {
    let index = out.len();
    out.push(Placed {
        id: node.id.clone(),
        kind: node.kind,
        name: node.name.clone(),
        depth,
        parent: parent.map(str::to_string),
        x: MARGIN + depth as f32 * COL_GAP,
        y: 0.0,
        w: NODE_W,
        h: NODE_H,
    });
    let center_y = if node.children.is_empty() {
        let y = MARGIN + *slot as f32 * ROW_GAP + NODE_H / 2.0;
        *slot += 1;
        y
    } else {
        let centers: Vec<f32> = node.children.iter().map(|c| place(c, Some(&node.id), depth + 1, slot, out)).collect();
        (centers[0] + centers[centers.len() - 1]) / 2.0
    };
    out[index].y = center_y - NODE_H / 2.0;
    center_y
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../shared");
        let pack = serde_json::from_str(&std::fs::read_to_string(dir.join("pack.json")).unwrap()).unwrap();
        let root = serde_json::from_str(&std::fs::read_to_string(dir.join("tree.json")).unwrap()).unwrap();
        Model::from_parts(pack, root)
    }

    fn ap_rating(m: &Model, id: &str) -> (u32, String) {
        let r = m.result(id).unwrap();
        (r.ap, m.pack().ratings[r.rating].label.clone())
    }

    #[test]
    fn initial_values_match_the_spec() {
        let m = model();
        let expect = [
            ("s1", 17, "Medium"), ("s2", 12, "High"), ("s3", 25, "Very low"), ("s4", 7, "High"),
            ("s5", 4, "High"), ("s6", 15, "Medium"), ("s7", 7, "High"), ("s8", 55, "Very low"),
            ("s9", 38, "Very low"), ("g1", 25, "Very low"), ("g2", 15, "Medium"), ("g3", 38, "Very low"),
            ("g0", 15, "Medium"),
        ];
        for (id, ap, rating) in expect {
            assert_eq!(ap_rating(&m, id), (ap, rating.to_string()), "node {id}");
        }
        assert_eq!(m.result("g0").unwrap().chosen_child.as_deref(), Some("g2"));
    }

    #[test]
    fn changing_one_factor_propagates_to_the_root() {
        let mut m = model();
        m.set_factor("s6", "equipment", "standard").unwrap();
        assert_eq!(ap_rating(&m, "s6"), (11, "High".into()));
        assert_eq!(ap_rating(&m, "g2"), (11, "High".into()));
        assert_eq!(ap_rating(&m, "g0"), (11, "High".into()));
        assert!(m.undo());
        assert_eq!(ap_rating(&m, "g0"), (15, "Medium".into()));
    }

    #[test]
    fn gate_switch_add_delete_and_read_only() {
        let mut m = model();
        m.set_kind("g3", Kind::And).unwrap();
        assert_eq!(ap_rating(&m, "g3"), (55, "Very low".into()));
        let id = m.add_child_step("g1").unwrap();
        assert_eq!(ap_rating(&m, &id), (0, "High".into()));
        m.delete(&id).unwrap();
        assert!(m.node(&id).is_none());
        m.read_only = true;
        assert!(m.set_factor("s1", "expertise", "layman").is_err());
    }

    #[test]
    fn filler_does_not_change_the_root_and_layout_is_stable() {
        let mut m = model();
        let before: Vec<(String, f32, f32)> =
            m.layout().iter().filter(|p| p.id.starts_with('s')).map(|p| (p.id.clone(), p.x, p.y)).collect();
        m.add_filler(2000);
        m.recompute();
        assert_eq!(ap_rating(&m, "g0"), (15, "Medium".into()));
        let after: Vec<(String, f32, f32)> =
            m.layout().iter().filter(|p| p.id.starts_with('s')).map(|p| (p.id.clone(), p.x, p.y)).collect();
        assert_eq!(before, after);
        assert!(m.layout().len() > 2000);
    }

    #[test]
    fn base_layout_fits_the_window() {
        let m = model();
        for p in m.layout() {
            assert!(p.x + p.w <= WINDOW_W && p.y + p.h <= WINDOW_H, "{} outside window", p.id);
        }
    }
}
