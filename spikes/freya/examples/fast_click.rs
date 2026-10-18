//! Repro for the lost fast click in Freya 0.4.3, using only built-in components.
//!
//! When button down and up arrive in the same event-loop batch (a synthetic click with no gap),
//! `on_press` does not fire. A click held for a moment works.
//!
//! ```sh
//! cargo run --example fast_click
//! # X11, in another terminal:
//! xdotool mousemove 80 40 click 1                              # no output
//! xdotool mousemove 80 40 mousedown 1 sleep 0.2 mouseup 1      # prints "PRESS 1"
//! ```
use freya::prelude::*;

fn main() {
    launch(LaunchConfig::new().with_window(WindowConfig::new(app).with_size(400.0, 200.0).with_title("fast click")));
}

fn app() -> impl IntoElement {
    let mut count = use_state(|| 0);
    rect().expanded().padding(20.0).child(
        Button::new()
            .on_press(move |_| {
                *count.write() += 1;
                println!("PRESS {}", count.peek());
            })
            .child(format!("Pressed {} times", count.read())),
    )
}
