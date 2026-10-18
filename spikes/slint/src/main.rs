//! AegisTARA UI toolkit spike with Slint: right-click an attack node and change its
//! attack feasibility values. The UI is declared in `ui/app.slint`; this file feeds it
//! from `spike-model` and handles the callbacks. Every factor, level, label, value and
//! rating color comes from the pack at runtime.

slint::include_modules!();

use slint::{Color, ComponentHandle, Model as _, ModelRc, SharedString, Timer, VecModel};
use spike_model::{Kind, Model, Placed};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Gate kinds offered in the "Gate type" submenu; labels come from `Kind::label()`.
const GATE_KINDS: [Kind; 2] = [Kind::And, Kind::Or];
const EDITOR_W: f32 = 380.0;
const EDITOR_H: f32 = 420.0;

struct Editor {
    node: String,
    levels: BTreeMap<String, String>,
}

struct App {
    model: Model,
    layout: Vec<Placed>,
    index: HashMap<String, usize>,
    layout_generation: u64,
    selected: Option<String>,
    zoom: f32,
    pan: (f32, f32),
    visible: Vec<String>,
    nodes: Rc<VecModel<NodeView>>,
    editor: Option<Editor>,
    renderer: &'static str,
}

impl App {
    fn relayout(&mut self) {
        if self.layout_generation != self.model.generation() || self.layout.is_empty() {
            self.layout = self.model.layout();
            self.index = self.layout.iter().enumerate().map(|(i, p)| (p.id.clone(), i)).collect();
            self.layout_generation = self.model.generation();
        }
    }

    fn screen_rect(&self, p: &Placed) -> (f32, f32, f32, f32) {
        let z = self.zoom;
        (p.x * z + self.pan.0, p.y * z + self.pan.1, p.w * z, p.h * z)
    }

    fn node_view(&self, p: &Placed) -> NodeView {
        let pack = self.model.pack();
        let result = self.model.result(&p.id);
        let rating = self.model.rating_of(&p.id);
        let [r, g, b] = rating.map(|r| r.rgb()).unwrap_or([128, 128, 128]);
        let levels: Vec<i32> = if p.kind == Kind::Step {
            pack.factors
                .iter()
                .map(|f| {
                    let current = self.model.level_of(&p.id, &f.id);
                    f.levels.iter().position(|l| Some(l.id.as_str()) == current).map_or(-1, |i| i as i32)
                })
                .collect()
        } else {
            Vec::new()
        };
        NodeView {
            id: p.id.as_str().into(),
            name: p.name.as_str().into(),
            chip: if p.kind.is_gate() { p.kind.label().into() } else { SharedString::new() },
            is_root: p.parent.is_none(),
            kind_index: GATE_KINDS.iter().position(|k| *k == p.kind).map_or(-1, |i| i as i32),
            x: p.x,
            y: p.y,
            w: p.w,
            h: p.h,
            badge: match (result, rating) {
                (Some(res), Some(rat)) => format!("{} · {}", res.ap, rat.label).into(),
                _ => "?".into(),
            },
            badge_color: Color::from_rgb_u8(r, g, b),
            levels: ModelRc::new(VecModel::from(levels)),
            selected: self.selected.as_deref() == Some(p.id.as_str()),
        }
    }

    /// Culls the layout to the window, updates the node model and returns the edge path.
    /// `data_changed` forces a row update even if the same nodes stay visible.
    fn sync(&mut self, data_changed: bool) -> String {
        self.relayout();
        let (vw, vh) = (spike_model::WINDOW_W, spike_model::WINDOW_H);
        let in_view = |r: (f32, f32, f32, f32)| r.0 + r.2 >= -20.0 && r.0 <= vw + 20.0 && r.1 + r.3 >= -20.0 && r.1 <= vh + 20.0;
        let mut edges = String::new();
        let mut visible = Vec::new();
        for p in &self.layout {
            let me = self.screen_rect(p);
            if in_view(me) {
                visible.push(p.id.clone());
            }
            let Some(parent) = p.parent.as_ref().and_then(|id| self.index.get(id)).map(|&i| &self.layout[i]) else {
                continue;
            };
            let pr = self.screen_rect(parent);
            let (x0, y0, x1, y1) = (pr.0 + pr.2, pr.1 + pr.3 / 2.0, me.0, me.1 + me.3 / 2.0);
            // Edge bounding box against the view.
            let bbox = (x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs());
            if in_view(bbox) {
                let mx = (x0 + x1) / 2.0;
                edges += &format!("M {x0:.1} {y0:.1} C {mx:.1} {y0:.1} {mx:.1} {y1:.1} {x1:.1} {y1:.1} ");
            }
        }
        if visible != self.visible {
            let rows: Vec<NodeView> =
                visible.iter().map(|id| self.node_view(&self.layout[self.index[id]])).collect();
            self.nodes.set_vec(rows);
            self.visible = visible;
        } else if data_changed {
            for (row, id) in self.visible.iter().enumerate() {
                self.nodes.set_row_data(row, self.node_view(&self.layout[self.index[id]]));
            }
        }
        if edges.is_empty() {
            edges.push_str("M 0 0");
        }
        edges
    }

    fn status(&self) -> String {
        let mut s = format!(
            "{} · zoom {:.0}% · {} nodes, {} drawn",
            self.renderer,
            self.zoom * 100.0,
            self.layout.len(),
            self.visible.len()
        );
        if let Some(sel) = &self.selected {
            s += &format!(" · selected {sel}");
        }
        if self.model.can_undo() {
            s += " · Ctrl+Z undo";
        }
        if self.model.read_only {
            s += " · READ-ONLY";
        }
        s
    }
}

fn refresh(app: &RefCell<App>, ui: &AppWindow, data_changed: bool) {
    let mut a = app.borrow_mut();
    let edges = a.sync(data_changed);
    let bridge = ui.global::<Bridge>();
    bridge.set_zoom(a.zoom);
    bridge.set_pan_x(a.pan.0);
    bridge.set_pan_y(a.pan.1);
    ui.set_edges(edges.into());
    ui.set_status(a.status().into());
}

/// Runs a model change after the current Slint callback returns, because a structural change
/// (add/delete) destroys the node item whose menu is calling us.
fn defer(app: &Rc<RefCell<App>>, ui: &AppWindow, change: impl FnOnce(&mut App) + 'static) {
    let (app, ui) = (app.clone(), ui.as_weak());
    Timer::single_shot(Duration::ZERO, move || {
        let Some(ui) = ui.upgrade() else { return };
        change(&mut app.borrow_mut());
        refresh(&app, &ui, true);
        ui.invoke_focus_canvas();
    });
}

fn report(r: Result<impl std::fmt::Debug, String>) {
    if let Err(e) = r {
        println!("ERROR {e}");
    }
}

fn update_preview(app: &App, ui: &AppWindow) {
    let Some(ed) = &app.editor else { return };
    let (ap, rating) = app.model.preview(&ed.levels);
    let rating = &app.model.pack().ratings[rating];
    let [r, g, b] = rating.rgb();
    let text = format!("{ap} · {}", rating.label);
    println!("PREVIEW {} {ap} {}", ed.node, rating.label);
    let bridge = ui.global::<Bridge>();
    bridge.set_editor_preview(text.into());
    bridge.set_editor_preview_color(Color::from_rgb_u8(r, g, b));
}

/// Slint's winit backend falls back to another renderer only if creating the renderer object
/// fails. If OpenGL is missing, femtovg fails later, at window creation, and the window stays
/// black. So on Linux probe for the GL libraries first and pick the software renderer without
/// them. `SLINT_BACKEND` (for example `winit-software`) always wins.
fn pick_renderer() {
    if std::env::var_os("SLINT_BACKEND").is_some() {
        return;
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: loading system GL libraries runs only their initializers.
        let gl = ["libEGL.so.1", "libGL.so.1"].iter().any(|l| unsafe { libloading::Library::new(l) }.is_ok());
        if !gl {
            println!("RENDERER no OpenGL library found, selecting the software renderer");
            let sel = slint::BackendSelector::new().backend_name("winit".into()).renderer_name("software".into());
            report(sel.select().map_err(|e| e.to_string()));
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    pick_renderer();
    let model = Model::load_default()?;
    model.log_layout(0.0, 0.0);
    model.log_results();

    let ui = AppWindow::new()?;
    let bridge = ui.global::<Bridge>();
    let pack = model.pack().clone();
    let factors: Vec<FactorView> = pack
        .factors
        .iter()
        .map(|f| FactorView {
            label: f.label.as_str().into(),
            titles: ModelRc::new(VecModel::from(
                f.levels.iter().map(|l| SharedString::from(format!("{}  ({})", l.label, l.value))).collect::<Vec<_>>(),
            )),
        })
        .collect();
    bridge.set_factors(ModelRc::new(VecModel::from(factors)));
    bridge.set_gate_kinds(ModelRc::new(VecModel::from(
        GATE_KINDS.iter().map(|k| SharedString::from(k.label())).collect::<Vec<_>>(),
    )));
    bridge.set_read_only(model.read_only);

    let nodes = Rc::new(VecModel::<NodeView>::default());
    ui.set_nodes(nodes.clone().into());
    let app = Rc::new(RefCell::new(App {
        model,
        layout: Vec::new(),
        index: HashMap::new(),
        layout_generation: 0,
        selected: None,
        zoom: 1.0,
        pan: (0.0, 0.0),
        visible: Vec::new(),
        nodes,
        editor: None,
        renderer: "",
    }));
    refresh(&app, &ui, true);

    // --- selection, menus and model changes ---
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_select(move |id| {
        let ui = w.unwrap();
        let mut app = a.borrow_mut();
        let new = (!id.is_empty()).then(|| id.to_string());
        if app.selected != new {
            println!("SELECT {}", new.as_deref().unwrap_or("-"));
            app.selected = new;
            drop(app);
            refresh(&a, &ui, true);
        }
    });
    bridge.on_menu_opened(|id| println!("MENU_OPEN {id}"));
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_keyboard_menu(move || {
        let ui = w.unwrap();
        let app = a.borrow();
        match &app.selected {
            Some(id) if app.visible.contains(id) => {
                println!("KBMENU_OPEN {id}");
                let bridge = ui.global::<Bridge>();
                bridge.set_menu_request(bridge.get_menu_request() + 1);
            }
            _ => println!("KBMENU_OPEN none (no visible selection)"),
        }
    });
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_set_factor(move |id, fi, li| {
        let ui = w.unwrap();
        defer(&a, &ui, move |app| {
            let pack = app.model.pack();
            let f = &pack.factors[fi as usize];
            let (fid, lid) = (f.id.clone(), f.levels[li as usize].id.clone());
            report(app.model.set_factor(&id, &fid, &lid));
        });
    });
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_set_kind(move |id, ki| {
        defer(&a, &w.unwrap(), move |app| report(app.model.set_kind(&id, GATE_KINDS[ki as usize])));
    });
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_add_child(move |id| defer(&a, &w.unwrap(), move |app| report(app.model.add_child_step(&id))));
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_delete_node(move |id| {
        defer(&a, &w.unwrap(), move |app| {
            if app.selected.as_deref() == Some(id.as_str()) {
                app.selected = None;
            }
            report(app.model.delete(&id));
        })
    });
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_undo(move || {
        let ui = w.unwrap();
        if !a.borrow_mut().model.undo() {
            println!("UNDO nothing");
        }
        refresh(&a, &ui, true);
    });

    // --- edit popover ---
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_edit(move |id| {
        let (a, w) = (a.clone(), w.clone());
        // Open after the menu has closed.
        Timer::single_shot(Duration::ZERO, move || {
            let ui = w.unwrap();
            let mut app = a.borrow_mut();
            let Some(node) = app.model.node(&id) else { return };
            let levels = node.factors.clone();
            let rationale = node.rationale.clone();
            let title = node.name.clone();
            let p = &app.layout[app.index[id.as_str()]];
            let (x, y, w, _h) = app.screen_rect(p);
            // Right of the node if it fits, else left; clamped to the window.
            let mut ex = x + w + 12.0;
            if ex + EDITOR_W > spike_model::WINDOW_W - 8.0 {
                ex = x - EDITOR_W - 12.0;
            }
            let ex = ex.clamp(8.0, spike_model::WINDOW_W - EDITOR_W - 8.0);
            let ey = y.clamp(8.0, spike_model::WINDOW_H - EDITOR_H - 32.0);
            let pack = app.model.pack();
            let index: Vec<i32> = pack
                .factors
                .iter()
                .map(|f| f.levels.iter().position(|l| levels.get(&f.id) == Some(&l.id)).map_or(-1, |i| i as i32))
                .collect();
            let bridge = ui.global::<Bridge>();
            bridge.set_editor_title(title.into());
            bridge.set_editor_levels(ModelRc::new(VecModel::from(index)));
            bridge.set_editor_rationale(rationale.into());
            bridge.set_editor_x(ex);
            bridge.set_editor_y(ey);
            app.editor = Some(Editor { node: id.to_string(), levels });
            println!("EDITOR_OPEN {id} at {ex:.0},{ey:.0}");
            update_preview(&app, &ui);
            drop(app);
            ui.invoke_open_editor();
        });
    });
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_editor_level(move |fi, li| {
        let ui = w.unwrap();
        let mut app = a.borrow_mut();
        let pack = app.model.pack();
        let f = &pack.factors[fi as usize];
        let (fid, lid) = (f.id.clone(), f.levels[li as usize].id.clone());
        if let Some(ed) = app.editor.as_mut() {
            ed.levels.insert(fid, lid);
        }
        update_preview(&app, &ui);
    });
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_editor_ok(move |rationale| {
        let ui = w.unwrap();
        ui.invoke_close_editor();
        let Some(ed) = a.borrow_mut().editor.take() else { return };
        println!("EDITOR_OK {} rationale={:?}", ed.node, rationale.as_str());
        defer(&a, &ui, move |app| report(app.model.set_factors(&ed.node, ed.levels, rationale.to_string())));
    });
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_editor_cancel(move || {
        a.borrow_mut().editor = None;
        println!("EDITOR_CANCEL");
        w.unwrap().invoke_close_editor();
    });

    // --- pan and zoom ---
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_pan_to(move |x, y| {
        println!("PAN {x:.0} {y:.0}");
        a.borrow_mut().pan = (x, y);
        refresh(&a, &w.unwrap(), false);
    });
    let (a, w) = (app.clone(), ui.as_weak());
    bridge.on_zoom_at(move |x, y, delta| {
        let mut app = a.borrow_mut();
        let old = app.zoom;
        let new = (old * 1.1f32.powf(delta.signum())).clamp(0.05, 4.0);
        app.pan = (x - (x - app.pan.0) * new / old, y - (y - app.pan.1) * new / old);
        app.zoom = new;
        println!("ZOOM {new:.3} delta {delta} pan {:.0} {:.0}", app.pan.0, app.pan.1);
        drop(app);
        refresh(&a, &w.unwrap(), false);
    });

    // First frame timing, the graphics API the renderer reports, and (PERF=1) render times.
    let mut first = true;
    let perf = std::env::var("PERF").is_ok_and(|v| v == "1");
    let mut frame_start = Instant::now();
    let notifier = ui.window().set_rendering_notifier(move |state, api| match state {
        slint::RenderingState::RenderingSetup => println!("GRAPHICS_API {}", api_name(api)),
        slint::RenderingState::BeforeRendering => frame_start = Instant::now(),
        slint::RenderingState::AfterRendering => {
            if first {
                first = false;
                println!("FIRST_FRAME {:.3}s", started.elapsed().as_secs_f32());
            } else if perf {
                println!("FRAME {:.1}ms", frame_start.elapsed().as_secs_f32() * 1000.0);
            }
        }
        _ => {}
    });
    // Only the software renderer lacks rendering notifiers, so this tells which one runs.
    app.borrow_mut().renderer = if notifier.is_ok() { "OpenGL renderer" } else { "software renderer" };
    if let Err(e) = notifier {
        println!("GRAPHICS_API unknown (rendering notifier: {e})");
        Timer::single_shot(Duration::ZERO, move || println!("FIRST_FRAME~ {:.3}s", started.elapsed().as_secs_f32()));
    }
    refresh(&app, &ui, false);

    ui.run()?;
    Ok(())
}

fn api_name(api: &slint::GraphicsAPI) -> &'static str {
    match api {
        slint::GraphicsAPI::NativeOpenGL { .. } => "NativeOpenGL",
        slint::GraphicsAPI::WebGL { .. } => "WebGL",
        _ => "other",
    }
}
