//! A window with no screen, for tests and screenshots.
//!
//! [`Headless`] installs a Slint platform whose window renders with the
//! software renderer into memory. Tests create the real [`AppWindow`](crate::AppWindow)
//! on it, drive it with pointer and key events like a user would, and look
//! at the pixels. The same CPU renderer is what a remote desktop user gets,
//! so what the tests see is what the slowest supported setup shows.
//!
//! The platform is per thread (a Slint rule), so every test thread installs
//! its own. A test may install more than once (two users, two windows): each
//! `install` makes the window that the next `AppWindow::new` receives.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PointerEventButton, WindowAdapter, WindowEvent};
use slint::{
    LogicalPosition, PhysicalSize, PlatformError, Rgb8Pixel, SharedPixelBuffer, SharedString,
};

thread_local! {
    /// The window the next `create_window_adapter` call hands out.
    static NEXT_WINDOW: RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { RefCell::new(None) };
    static INSTALLED: RefCell<bool> = const { RefCell::new(false) };
}

struct HeadlessPlatform {
    started: Instant,
}

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = NEXT_WINDOW
            .with(|next| next.borrow_mut().take())
            .ok_or_else(|| PlatformError::Other("no headless window was prepared".into()))?;
        Ok(window)
    }

    fn duration_since_start(&self) -> Duration {
        self.started.elapsed()
    }
}

/// A rendered frame: 8-bit RGB, row after row.
pub struct Frame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 3` bytes.
    pub rgb: Vec<u8>,
}

impl Frame {
    /// The color at a pixel.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.width + x) * 3) as usize;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }

    /// How many pixels of the exact color are in the rectangle.
    #[must_use]
    pub fn count_color(&self, x: u32, y: u32, w: u32, h: u32, color: [u8; 3]) -> usize {
        (y..(y + h).min(self.height))
            .flat_map(|yy| (x..(x + w).min(self.width)).map(move |xx| (xx, yy)))
            .filter(|(xx, yy)| self.pixel(*xx, *yy) == color)
            .count()
    }
}

/// A software-rendered window.
pub struct Headless {
    window: Rc<MinimalSoftwareWindow>,
    size: (u32, u32),
}

impl Headless {
    /// Installs the platform on this thread (once) and prepares a window of
    /// this size for the next `AppWindow::new`.
    ///
    /// # Panics
    ///
    /// If a different platform was already installed on this thread.
    #[must_use]
    pub fn install(width: u32, height: u32) -> Headless {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        window.set_size(PhysicalSize::new(width, height));
        NEXT_WINDOW.with(|next| *next.borrow_mut() = Some(Rc::clone(&window)));
        if !INSTALLED.with(|done| done.replace(true)) {
            slint::platform::set_platform(Box::new(HeadlessPlatform {
                started: Instant::now(),
            }))
            .expect("no other Slint platform on this thread");
        }
        Headless {
            window,
            size: (width, height),
        }
    }

    /// Lets timers and animations advance.
    pub fn tick(&self) {
        slint::platform::update_timers_and_animations();
    }

    /// Renders the window now.
    ///
    /// Slint runs `changed` handlers (a combo box updating its text when its
    /// index changes, for example) after a property change, in the next turn
    /// of the event loop. A real window has an event loop; here the first
    /// pass lets them run and the second draws the result.
    #[must_use]
    pub fn render(&self) -> Frame {
        self.tick();
        let _ = self.draw();
        self.tick();
        self.draw()
    }

    fn draw(&self) -> Frame {
        let (w, h) = self.size;
        let mut buffer = SharedPixelBuffer::<Rgb8Pixel>::new(w, h);
        self.window.request_redraw();
        self.window.draw_if_needed(|renderer| {
            renderer.render(buffer.make_mut_slice(), w as usize);
        });
        Frame {
            width: w,
            height: h,
            rgb: buffer
                .as_slice()
                .iter()
                .flat_map(|p| [p.r, p.g, p.b])
                .collect(),
        }
    }

    fn event(&self, event: WindowEvent) {
        self.window.dispatch_event(event);
    }

    /// Presses and releases the left button at a point.
    pub fn click(&self, x: f32, y: f32) {
        let position = LogicalPosition::new(x, y);
        self.event(WindowEvent::PointerMoved { position });
        self.event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        self.event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    }

    /// Drags with the left button from one point to another.
    pub fn drag(&self, from: (f32, f32), to: (f32, f32)) {
        let start = LogicalPosition::new(from.0, from.1);
        self.event(WindowEvent::PointerMoved { position: start });
        self.event(WindowEvent::PointerPressed {
            position: start,
            button: PointerEventButton::Left,
        });
        // A few intermediate moves, like a real hand.
        for step in 1..=5 {
            let t = step as f32 / 5.0;
            self.event(WindowEvent::PointerMoved {
                position: LogicalPosition::new(
                    from.0 + (to.0 - from.0) * t,
                    from.1 + (to.1 - from.1) * t,
                ),
            });
        }
        self.event(WindowEvent::PointerReleased {
            position: LogicalPosition::new(to.0, to.1),
            button: PointerEventButton::Left,
        });
    }

    /// Turns the mouse wheel at a point (positive: away from the user).
    pub fn scroll(&self, x: f32, y: f32, delta: f32) {
        let position = LogicalPosition::new(x, y);
        self.event(WindowEvent::PointerMoved { position });
        self.event(WindowEvent::PointerScrolled {
            position,
            delta_x: 0.0,
            delta_y: delta,
        });
    }

    /// Types a key (a character, or a `slint::platform::Key` as a char).
    pub fn key(&self, text: impl Into<SharedString>) {
        let text = text.into();
        self.event(WindowEvent::KeyPressed { text: text.clone() });
        self.event(WindowEvent::KeyReleased { text });
    }
}
