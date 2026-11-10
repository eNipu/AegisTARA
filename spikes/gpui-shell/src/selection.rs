//! What the user has selected, shared between the project tree and the inspector.

/// The current selection. The shell holds it as `Entity<Selection>`; panels
/// update it and call `cx.notify()` so observers redraw.
pub struct Selection {
    pub kind: &'static str,
    pub label: String,
    pub detail: String,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            kind: "none",
            label: String::new(),
            detail: String::new(),
        }
    }
}
