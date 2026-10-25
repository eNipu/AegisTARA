//! Wires the window to the client core.
//!
//! Slint callbacks (user actions) become [`Command`]s; each new
//! [`ViewModel`] becomes Slint properties. Pan and zoom are the one thing the
//! shell owns, because they are about *this window* (its size and the user's
//! hand), not about the project: the [`Viewport`] lives here, and every change
//! of it re-renders the display list.
//!
//! All of this runs on the UI thread. The session runs elsewhere, so the
//! window never waits for the network.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use aegis_canvas::{Direction, Point, RenderOptions, Size, Viewport, render};
use aegis_client::commands::{Command, Edit};
use aegis_client::session::Session;
use aegis_client::state::{ConnectionView, Mode, Severity, SyncView, ViewModel};
use aegis_model::TreeId;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::adapter::{bucket, draw_items};
use crate::{AppWindow, FactorItem, NoticeItem, ProjectItem, ThreatItem};

/// Pixels the pointer must move before a press becomes a drag.
const DRAG_THRESHOLD: f32 = 4.0;

struct Press {
    last: Point,
    travelled: f32,
}

struct State {
    view: Option<Arc<ViewModel>>,
    viewport: Viewport,
    /// The tree on the canvas, to refit when another one is shown.
    shown_tree: Option<TreeId>,
    press: Option<Press>,
}

/// Owns the window-side state and the session handle.
pub struct Controller {
    window: slint::Weak<AppWindow>,
    session: Session,
    state: RefCell<State>,
}

fn s(text: &str) -> SharedString {
    text.into()
}

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items))
}

impl Controller {
    /// Connects `window` to `session`. `log_in` is called when the user
    /// presses the login button.
    pub fn attach(
        window: &AppWindow,
        session: Session,
        log_in: impl Fn() + 'static,
    ) -> Rc<Controller> {
        let controller = Rc::new(Controller {
            window: window.as_weak(),
            session,
            state: RefCell::new(State {
                view: None,
                viewport: Viewport::new(Size::new(800.0, 600.0)),
                shown_tree: None,
                press: None,
            }),
        });
        controller.wire(window, log_in);
        controller
    }

    /// The current viewport, for tests that need to turn world positions
    /// into window positions.
    #[doc(hidden)]
    #[must_use]
    pub fn viewport_for_tests(&self) -> Viewport {
        self.state.borrow().viewport
    }

    fn send(&self, command: Command) {
        // A full queue means the session is far behind; dropping a click is
        // better than freezing the window.
        let _ = self.session.send(command);
    }

    #[allow(clippy::too_many_lines)]
    fn wire(self: &Rc<Self>, w: &AppWindow, log_in: impl Fn() + 'static) {
        let c = Rc::clone(self);
        w.on_retry(move || c.send(Command::Reconnect));
        w.on_log_in(log_in);
        let c = Rc::clone(self);
        w.on_open_project(move |id| c.send(Command::OpenProject { id: id.to_string() }));
        let c = Rc::clone(self);
        w.on_create_project(move |name| {
            c.send(Command::CreateProject {
                name: name.to_string(),
                methodology_key: "iso21434-r155".to_owned(),
            });
        });
        let c = Rc::clone(self);
        w.on_refresh_projects(move || c.send(Command::Refresh));
        let c = Rc::clone(self);
        w.on_close_project(move || c.send(Command::CloseProject));
        let c = Rc::clone(self);
        w.on_select_threat(move |i| {
            c.state.borrow_mut().shown_tree = None;
            c.send(Command::SelectThreat(usize::try_from(i).unwrap_or(0)));
        });
        let c = Rc::clone(self);
        w.on_begin_edit(move || c.send(Command::BeginEdit));
        let c = Rc::clone(self);
        w.on_end_edit(move || c.send(Command::EndEdit));
        let c = Rc::clone(self);
        w.on_undo(move || c.send(Command::Undo));
        let c = Rc::clone(self);
        w.on_redo(move || c.send(Command::Redo));
        let c = Rc::clone(self);
        w.on_create_sample(move || c.send(Command::CreateSample));
        let c = Rc::clone(self);
        w.on_set_residual(move |on| c.send(Command::ShowResidual(on)));
        let c = Rc::clone(self);
        w.on_dismiss_notice(move |id| {
            c.send(Command::DismissNotice(u64::try_from(id).unwrap_or(0)))
        });
        let c = Rc::clone(self);
        w.on_set_rating(move |index| c.set_rating(index));
        let c = Rc::clone(self);
        w.on_set_factor(move |factor, index| c.set_factor(factor, index));

        let c = Rc::clone(self);
        w.on_canvas_resized(move |width, height| c.resize(width, height));
        let c = Rc::clone(self);
        w.on_canvas_down(move |x, y| {
            c.state.borrow_mut().press = Some(Press {
                last: Point::new(x, y),
                travelled: 0.0,
            });
        });
        let c = Rc::clone(self);
        w.on_canvas_move(move |x, y| c.drag(Point::new(x, y)));
        let c = Rc::clone(self);
        w.on_canvas_up(move |x, y| c.release(Point::new(x, y)));
        let c = Rc::clone(self);
        w.on_canvas_scroll(move |x, y, delta| c.zoom(Point::new(x, y), delta));
        let c = Rc::clone(self);
        w.on_canvas_key(move |text, shift| c.key(&text, shift));
        let c = Rc::clone(self);
        w.on_fit(move || c.fit());
        let c = Rc::clone(self);
        w.on_zoom_reset(move || c.reset_zoom());
    }

    // ---- edits from the inspector ---------------------------------------

    fn set_rating(&self, index: i32) {
        let Some(view) = self.state.borrow().view.clone() else {
            return;
        };
        let Some(inspector) = view.project.as_ref().and_then(|p| p.inspector.as_ref()) else {
            return;
        };
        // Index 0 is "Not rated"; the levels follow.
        let level = usize::try_from(index).ok().and_then(|i| i.checked_sub(1));
        self.send(Command::Edit(Edit::SetStepFeasibility {
            tree: inspector.tree,
            node: inspector.node,
            level,
        }));
    }

    fn set_factor(&self, factor: i32, index: i32) {
        let Some(view) = self.state.borrow().view.clone() else {
            return;
        };
        let Some(inspector) = view.project.as_ref().and_then(|p| p.inspector.as_ref()) else {
            return;
        };
        let (Ok(factor), Ok(index)) = (usize::try_from(factor), usize::try_from(index)) else {
            return;
        };
        self.send(Command::Edit(Edit::SetStepFactor {
            tree: inspector.tree,
            node: inspector.node,
            factor,
            level: index.checked_sub(1),
        }));
    }

    // ---- canvas interaction ---------------------------------------------

    fn resize(&self, width: f32, height: f32) {
        if width < 1.0 || height < 1.0 {
            return;
        }
        {
            let mut st = self.state.borrow_mut();
            if st.viewport.size == Size::new(width, height) {
                return;
            }
            st.viewport.size = Size::new(width, height);
        }
        self.redraw();
    }

    fn drag(&self, at: Point) {
        {
            let mut st = self.state.borrow_mut();
            let Some(press) = st.press.as_mut() else {
                return;
            };
            let (dx, dy) = (at.x - press.last.x, at.y - press.last.y);
            press.last = at;
            press.travelled += dx.abs() + dy.abs();
            if press.travelled <= DRAG_THRESHOLD {
                return;
            }
            st.viewport.pan_by_screen(dx, dy);
        }
        self.redraw();
    }

    fn release(&self, at: Point) {
        let click = {
            let mut st = self.state.borrow_mut();
            st.press
                .take()
                .is_some_and(|p| p.travelled <= DRAG_THRESHOLD)
        };
        if !click {
            return;
        }
        let (scene, world) = {
            let st = self.state.borrow();
            let scene = st
                .view
                .as_ref()
                .and_then(|v| v.project.as_ref())
                .and_then(|p| p.scene().cloned());
            (scene, st.viewport.to_world(at))
        };
        if let Some(scene) = scene {
            let hit = scene.hit_test(world).map(aegis_model::NodeIdx::index);
            self.send(Command::SelectNode(hit));
        }
    }

    fn zoom(&self, at: Point, delta: f32) {
        // One wheel notch (about 120) is a 20% change.
        let factor = 1.0015f32.powf(delta).clamp(0.5, 2.0);
        self.state.borrow_mut().viewport.zoom_at(at, factor);
        self.redraw();
    }

    fn fit(&self) {
        let scene = self.current_scene();
        if let Some(scene) = scene {
            self.state.borrow_mut().viewport.fit(scene.bounds(), 32.0);
        }
        self.redraw();
    }

    fn reset_zoom(&self) {
        let mut st = self.state.borrow_mut();
        let centre = Point::new(st.viewport.size.w / 2.0, st.viewport.size.h / 2.0);
        let factor = 1.0 / st.viewport.zoom;
        st.viewport.zoom_at(centre, factor);
        drop(st);
        self.redraw();
    }

    fn current_scene(&self) -> Option<Arc<aegis_canvas::Scene>> {
        self.state
            .borrow()
            .view
            .as_ref()
            .and_then(|v| v.project.as_ref())
            .and_then(|p| p.scene().cloned())
    }

    /// Handles a key on the canvas. Returns whether it was used.
    fn key(&self, text: &str, shift: bool) -> bool {
        use slint::platform::Key;
        let key = |k: Key| text == char::from(k).to_string();
        let direction = if key(Key::LeftArrow) {
            Some(Direction::Left)
        } else if key(Key::RightArrow) {
            Some(Direction::Right)
        } else if key(Key::UpArrow) {
            Some(Direction::Up)
        } else if key(Key::DownArrow) {
            Some(Direction::Down)
        } else {
            None
        };
        if let Some(direction) = direction {
            return self.navigate(direction);
        }
        if key(Key::Escape) {
            self.send(Command::SelectNode(None));
            return true;
        }
        match text {
            "+" | "=" => {
                self.zoom_centre(1.2);
                true
            }
            "-" | "_" => {
                self.zoom_centre(1.0 / 1.2);
                true
            }
            "0" => {
                self.reset_zoom();
                true
            }
            "f" | "F" if !shift || text == "F" => {
                self.fit();
                true
            }
            _ => false,
        }
    }

    fn zoom_centre(&self, factor: f32) {
        {
            let mut st = self.state.borrow_mut();
            let centre = Point::new(st.viewport.size.w / 2.0, st.viewport.size.h / 2.0);
            st.viewport.zoom_at(centre, factor);
        }
        self.redraw();
    }

    fn navigate(&self, direction: Direction) -> bool {
        let Some(view) = self.state.borrow().view.clone() else {
            return false;
        };
        let Some(project) = view.project.as_ref() else {
            return false;
        };
        let Some(scene) = project.scene() else {
            return false;
        };
        // With nothing selected, an arrow key selects the root.
        let target = match project.selected_node {
            Some(from) => scene.navigate(from, direction),
            None => scene.root(),
        };
        let Some(target) = target else { return true };
        if let Some(rect) = scene.node_rect(target) {
            self.state.borrow_mut().viewport.reveal(rect, 40.0);
        }
        self.send(Command::SelectNode(Some(target.index())));
        self.redraw();
        true
    }

    // ---- view model -> window -------------------------------------------

    /// Applies the session's latest view model to the window. Call it on the
    /// UI thread whenever the view model changes.
    pub fn apply_latest(&self) {
        let view = self.session.view();
        self.apply(&view);
    }

    #[allow(clippy::too_many_lines)]
    fn apply(&self, view: &Arc<ViewModel>) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        self.state.borrow_mut().view = Some(Arc::clone(view));

        let (code, text) = match &view.connection {
            ConnectionView::Connecting => (0, "Connecting to the server…".to_owned()),
            ConnectionView::Online { .. } => (1, String::new()),
            ConnectionView::Offline { reason } => {
                (2, format!("The server cannot be reached. {reason}"))
            }
            ConnectionView::LoginRequired => {
                (3, "Please log in with your company account.".to_owned())
            }
        };
        w.set_connection(code);
        w.set_connection_text(s(&text));
        w.set_user(s(&view.user));
        w.set_engine_text(s(&match &view.connection {
            ConnectionView::Online {
                server_version,
                engine_ok: true,
            } => format!("Server {server_version}"),
            ConnectionView::Online {
                server_version,
                engine_ok: false,
            } => {
                format!("Server {server_version}. Engine differs: update AegisTARA")
            }
            _ => String::new(),
        }));
        w.set_projects(model(
            view.projects
                .iter()
                .map(|p| ProjectItem {
                    id: s(&p.id),
                    name: s(&p.name),
                    methodology: s(&p.methodology),
                })
                .collect(),
        ));
        w.set_notices(model(
            view.notices
                .iter()
                .map(|n| NoticeItem {
                    id: i32::try_from(n.id).unwrap_or(i32::MAX),
                    severity: match n.severity {
                        Severity::Info => 0,
                        Severity::Warning => 1,
                        Severity::Error => 2,
                    },
                    text: s(&n.text),
                })
                .collect(),
        ));

        let Some(project) = view.project.as_ref() else {
            w.set_has_project(false);
            w.set_draw(model(Vec::new()));
            self.state.borrow_mut().shown_tree = None;
            return;
        };
        w.set_has_project(true);
        w.set_project_name(s(&project.entry.name));
        w.set_editing(project.mode == Mode::Editing);
        let (kind, band) = match (&project.mode, project.lock_holder.as_deref(), project.sync) {
            (_, _, SyncView::Resyncing) => (1, "Reloading the project from the server…".to_owned()),
            (Mode::Editing, _, _) => (
                2,
                "You're editing. Others on the project see your changes live.".to_owned(),
            ),
            (Mode::ReadOnly, Some(holder), _) if holder != view.user => (
                1,
                format!("Read-only. {holder} is editing; their changes appear here live."),
            ),
            (Mode::ReadOnly, _, _) => {
                (0, "Read-only. Press Edit to change the project.".to_owned())
            }
        };
        w.set_band_kind(kind);
        w.set_band_text(s(&band));
        w.set_sync_text(s(&match project.sync {
            SyncView::Synced => "Saved".to_owned(),
            SyncView::Syncing { pending: 1 } => "Saving 1 change…".to_owned(),
            SyncView::Syncing { pending } => format!("Saving {pending} changes…"),
            SyncView::Resyncing => "Reloading…".to_owned(),
        }));
        w.set_can_undo(project.can_undo);
        w.set_can_redo(project.can_redo);
        w.set_show_residual(project.show_residual);
        w.set_is_empty(project.is_empty);
        w.set_threats(model(
            project
                .threats
                .iter()
                .enumerate()
                .map(|(i, t)| ThreatItem {
                    name: s(&t.name),
                    asset: s(&t.asset),
                    inherent: s(t.inherent.as_ref().map_or("", |r| r.label.as_str())),
                    residual: s(t.residual.as_ref().map_or("", |r| r.label.as_str())),
                    inherent_tone: t.inherent.as_ref().map_or(0, |r| bucket(r.tone)),
                    residual_tone: t.residual.as_ref().map_or(0, |r| bucket(r.tone)),
                    selected: project.selected_threat == Some(i),
                })
                .collect(),
        ));

        // Tree title and canvas.
        let title = project
            .selected_threat
            .and_then(|i| project.threats.get(i))
            .map(|t| {
                let how = if project.show_residual {
                    "after controls"
                } else {
                    "before controls"
                };
                format!("{} · {how}", t.name)
            })
            .unwrap_or_default();
        w.set_tree_title(s(&title));
        w.set_has_tree(project.tree.is_some());
        w.set_canvas_summary(s(&match (&project.tree, &project.inspector) {
            (Some(t), Some(i)) => format!(
                "Attack tree with {} nodes. Selected: {}.",
                t.scene.len(),
                i.title
            ),
            (Some(t), None) => format!("Attack tree with {} nodes.", t.scene.len()),
            _ => "No attack tree".to_owned(),
        }));

        // Inspector.
        match project.inspector.as_ref() {
            Some(i) => {
                w.set_has_inspector(true);
                w.set_inspector_title(s(&i.title));
                w.set_inspector_kind(s(&i.kind));
                w.set_inspector_is_step(i.is_step);
                let mut levels = vec![s("Not rated")];
                levels.extend(i.levels.iter().map(|c| s(&c.label)));
                w.set_level_options(model(levels));
                w.set_rating_index(i.rating.map_or(0, |r| i32::try_from(r + 1).unwrap_or(0)));
                w.set_effective_text(s(i.effective.as_deref().unwrap_or("")));
                w.set_potential_text(s(i.potential.as_deref().unwrap_or("")));
                w.set_evaluated_text(s(i.evaluated.as_deref().unwrap_or("")));
                w.set_evaluated_tone(evaluated_tone(project, i.evaluated.is_some()));
                w.set_why_lines(model(i.why.iter().map(|l| s(l)).collect()));
                w.set_factors(model(
                    i.factors
                        .iter()
                        .map(|f| {
                            let mut options = vec![s("Not rated")];
                            options.extend(f.options.iter().map(|c| s(&c.label)));
                            FactorItem {
                                label: s(&f.label),
                                options: model(options),
                                selected: f
                                    .selected
                                    .map_or(0, |x| i32::try_from(x + 1).unwrap_or(0)),
                            }
                        })
                        .collect(),
                ));
            }
            None => w.set_has_inspector(false),
        }

        // A tree that was not on the canvas before is fitted to the window.
        let tree_id = project.tree.as_ref().map(|t| t.tree);
        {
            let mut st = self.state.borrow_mut();
            if tree_id != st.shown_tree {
                st.shown_tree = tree_id;
                if let Some(t) = &project.tree {
                    st.viewport.fit(t.scene.bounds(), 32.0);
                }
            }
        }
        self.redraw();
    }

    /// Re-renders the display list for the current view model and viewport.
    fn redraw(&self) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let st = self.state.borrow();
        let Some(project) = st.view.as_ref().and_then(|v| v.project.as_ref()) else {
            return;
        };
        let items = match &project.tree {
            Some(tree) => {
                let options = RenderOptions {
                    selected: project.selected_node,
                };
                draw_items(&render(&tree.scene, &st.viewport, &options))
            }
            None => Vec::new(),
        };
        w.set_draw(model(items));
        w.set_zoom_text(s(&format!("{:.0}%", st.viewport.zoom * 100.0)));
    }
}

/// The bucket of the selected node's evaluated rating, for the swatch.
fn evaluated_tone(project: &aegis_client::state::ProjectView, has_value: bool) -> i32 {
    if !has_value {
        return 0;
    }
    let (Some(tree), Some(selected)) = (project.tree.as_ref(), project.selected_node) else {
        return 0;
    };
    tree.scene
        .style(u32::try_from(selected.index()).unwrap_or(0))
        .map_or(0, |style| bucket(style.tone))
}
