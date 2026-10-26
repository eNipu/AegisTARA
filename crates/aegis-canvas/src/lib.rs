//! Canvas engine for AegisTARA: attack tree layout, hit testing, and a
//! toolkit-neutral display list (`docs/direction-v1.md` section 5).
//!
//! ```text
//!  AttackTree ──► Scene ──render(Viewport)──► DisplayList ──► screen adapter
//!   (model)      layout O(n)    O(visible)      plain data ──► SVG adapter
//!                R-tree index                                  (PDF reports)
//! ```
//!
//! The engine owns layout, hit testing, level of detail, and selection. It
//! never draws: it emits a [`DisplayList`] that small adapters paint. The
//! canvas can therefore be tested without a window, and the toolkit can be
//! replaced by rewriting one adapter. See `docs/adr/0029-canvas-engine.md`.
//!
//! # Example
//!
//! ```
//! use aegis_canvas::{LayoutConfig, NodeStyle, RenderOptions, Scene, Size, SvgOptions, Viewport, render, to_svg};
//! use aegis_model::{AttackTree, GateKind, Node, NodeId};
//!
//! # fn main() -> Result<(), aegis_model::ModelError> {
//! let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Steal car"))?;
//! tree.add_child(tree.root(), NodeId::new_v7(), Node::step("Relay key fob"))?;
//! tree.add_child(tree.root(), NodeId::new_v7(), Node::step("Hot-wire"))?;
//!
//! let scene = Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle::default());
//! let view = Viewport::new(Size::new(800.0, 400.0));
//! let list = render(&scene, &view, &RenderOptions::default());
//! let svg = to_svg(&list, view.size, &SvgOptions::light());
//! assert!(svg.starts_with("<svg"));
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

pub mod access;
pub mod display;
pub mod geometry;
pub mod index;
pub mod layout;
pub mod render;
pub mod scene;
pub mod style;
pub mod svg;
pub mod text;
pub mod viewport;

pub use display::{Align, DisplayList, Item};
pub use geometry::{Point, Rect, Size};
pub use render::{RenderOptions, render};
pub use scene::{Direction, Edge, LayoutConfig, NodeKind, NodeStyle, Scene};
pub use style::{Paint, Stroke, Tone};
pub use svg::{SvgOptions, SvgTheme, scene_to_svg, to_svg};
pub use viewport::Viewport;
