//! What the user has selected, shared between the project tree and the inspector.

/// The current selection, held as `Entity<Selection>`; writers call `cx.notify()`.
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
