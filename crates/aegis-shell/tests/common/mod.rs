//! A real server on a loopback port, and a way to drive the real window.
#![allow(dead_code)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use aegis_client::commands::Command;
use aegis_client::connection::DevLogin;
use aegis_client::session::{Session, SessionConfig};
use aegis_client::state::{ConnectionView, ViewModel};
use aegis_server::auth_layer::{AuthLayer, DevAuthn};
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::{AegisService, MemoryStore, Registry, server};
use aegis_shell::AppWindow;
use aegis_shell::controller::Controller;
use aegis_shell::headless::{Frame, Headless};
use slint::ComponentHandle;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;

pub struct TestServer {
    pub url: String,
    stop: Option<oneshot::Sender<()>>,
    done: Option<tokio::task::JoinHandle<()>>,
}

impl TestServer {
    pub async fn start() -> Self {
        let registry = Arc::new(Registry::new(
            Arc::new(MemoryStore::new()),
            Arc::new(Methodologies::builtin().expect("builtin")),
            Arc::new(ServerConfig::default()),
        ));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let (stop, stopped) = oneshot::channel::<()>();
        let service = AegisService::new(Arc::clone(&registry)).into_server();
        let layer = AuthLayer::new(Arc::new(DevAuthn), &url);
        let done = tokio::spawn(async move {
            server::builder()
                .layer(layer)
                .add_service(service)
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                    let _ = stopped.await;
                    registry.shutdown().await;
                })
                .await
                .expect("serve");
        });
        TestServer {
            url,
            stop: Some(stop),
            done: Some(done),
        }
    }

    pub fn session(&self, user: &str) -> Session {
        let mut config = SessionConfig::new(user);
        config.retry_delay = Duration::from_millis(100);
        config.version_check = Duration::from_millis(100);
        Session::start(&self.url, Arc::new(DevLogin::new(user, false)), config).0
    }

    /// A session of a development administrator.
    pub fn admin_session(&self, user: &str) -> Session {
        let mut config = SessionConfig::new(user);
        config.retry_delay = Duration::from_millis(100);
        config.version_check = Duration::from_millis(100);
        Session::start(&self.url, Arc::new(DevLogin::new(user, true)), config).0
    }

    pub async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(done) = self.done.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), done).await;
        }
    }
}

/// The real window, on a headless platform, attached to a session.
pub struct App {
    pub headless: Headless,
    pub window: AppWindow,
    pub controller: std::rc::Rc<Controller>,
    pub session: Session,
}

impl App {
    pub fn new(session: Session, width: u32, height: u32) -> App {
        let headless = Headless::install(width, height);
        let window = AppWindow::new().expect("window");
        window
            .window()
            .set_size(slint::PhysicalSize::new(width, height));
        let controller = Controller::attach(&window, session.clone(), || {});
        window.show().expect("show");
        App {
            headless,
            window,
            controller,
            session,
        }
    }

    /// Applies the session's state to the window until `ready` is true.
    pub async fn until(&self, what: &str, ready: impl Fn(&ViewModel) -> bool) -> Arc<ViewModel> {
        let mut rx = self.session.subscribe();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.controller.apply_latest();
            self.headless.tick();
            let view = rx.borrow_and_update().clone();
            if ready(&view) {
                // The session may have moved on since the apply above. Keys
                // act on the window's state, so bring it up to date.
                self.controller.apply_latest();
                self.headless.tick();
                return view;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "timed out waiting for: {what}");
            let _ = tokio::time::timeout(left, rx.changed()).await;
        }
    }

    pub async fn online(&self) {
        self.until("online", |vm| {
            matches!(vm.connection, ConnectionView::Online { .. })
        })
        .await;
    }

    pub fn send(&self, command: Command) {
        self.session.send(command).expect("sent");
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.headless.resize(width, height);
        self.window
            .window()
            .set_size(slint::PhysicalSize::new(width, height));
    }

    pub fn frame(&self) -> Frame {
        self.controller.apply_latest();
        self.headless.render()
    }
}

/// The window position (logical pixels) of the node whose label starts with
/// `label`, from the display list the controller drew.
pub fn node_position(app: &App, label: &str) -> (f32, f32) {
    let view = app.session.view();
    let project = view.project.as_ref().expect("project");
    let scene = project.scene().expect("scene");
    let (position, _) = (0..scene.len())
        .map(|i| (i, scene.label(i as u32)))
        .find(|(_, l)| l.starts_with(label))
        .expect("node");
    // The window reports where the canvas is.
    let idx = scene.node_idx(position as u32).expect("idx");
    let rect = scene.node_rect(idx).expect("rect");
    let centre = rect.center();
    let st = app.controller.viewport_for_tests();
    let screen = st.to_screen(aegis_canvas::Point::new(centre.x, centre.y));
    (
        app.window.get_canvas_x() + screen.x,
        app.window.get_canvas_y() + screen.y,
    )
}

/// Saves a frame as a PNG under `target/shell-screenshots/`.
pub fn save(frame: &Frame, name: &str) {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/shell-screenshots");
    std::fs::create_dir_all(&dir).expect("dir");
    let file = std::fs::File::create(dir.join(format!("{name}.png"))).expect("file");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("header")
        .write_image_data(&frame.rgb)
        .expect("png");
}

/// Types `text` into whatever has focus, one key at a time.
pub fn type_text(app: &App, text: &str) {
    use slint::ComponentHandle;
    for c in text.chars() {
        let s: slint::SharedString = c.to_string().into();
        let w = app.window.window();
        w.dispatch_event(slint::platform::WindowEvent::KeyPressed { text: s.clone() });
        w.dispatch_event(slint::platform::WindowEvent::KeyReleased { text: s });
    }
}

/// Presses and releases one named key, with Control (Command on macOS) held
/// when `control` is set.
pub fn press_key(app: &App, key: slint::platform::Key, control: bool) {
    use slint::ComponentHandle;
    use slint::platform::{Key, WindowEvent};
    let w = app.window.window();
    let ctrl: slint::SharedString = Key::Control.into();
    let k: slint::SharedString = key.into();
    if control {
        w.dispatch_event(WindowEvent::KeyPressed { text: ctrl.clone() });
    }
    w.dispatch_event(WindowEvent::KeyPressed { text: k.clone() });
    w.dispatch_event(WindowEvent::KeyReleased { text: k });
    if control {
        w.dispatch_event(WindowEvent::KeyReleased { text: ctrl });
    }
}

/// Presses a character with Control (Command on macOS) and optionally Shift
/// held, as a keyboard shortcut.
pub fn press_chord(app: &App, c: char, shift: bool) {
    use slint::ComponentHandle;
    use slint::platform::{Key, WindowEvent};
    let w = app.window.window();
    let ctrl: slint::SharedString = Key::Control.into();
    let shift_key: slint::SharedString = Key::Shift.into();
    let text: slint::SharedString = if shift {
        c.to_ascii_uppercase().to_string().into()
    } else {
        c.to_string().into()
    };
    w.dispatch_event(WindowEvent::KeyPressed { text: ctrl.clone() });
    if shift {
        w.dispatch_event(WindowEvent::KeyPressed {
            text: shift_key.clone(),
        });
    }
    w.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    w.dispatch_event(WindowEvent::KeyReleased { text });
    if shift {
        w.dispatch_event(WindowEvent::KeyReleased { text: shift_key });
    }
    w.dispatch_event(WindowEvent::KeyReleased { text: ctrl });
}
