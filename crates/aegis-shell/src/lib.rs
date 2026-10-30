//! The AegisTARA desktop shell: a Slint window over the client core.
//!
//! The shell holds no application logic. `aegis-client` decides what is
//! selected, what an edit means and what the numbers are; this crate turns
//! its view model into Slint properties and turns the user's clicks back into
//! commands. Replacing the toolkit means rewriting this crate and nothing
//! else (decisions D7 and D9).
//!
//! Slint is confined to this crate by a `cargo-deny` ban rule. It is used
//! under the Royalty-free License 2.0, which requires the "About Slint"
//! notice the window shows (`docs/release-checklist.md`).

// Our own code has no `unsafe`. The code Slint generates from the `.slint`
// files does (its component vtables), so it lives in its own module.
#![deny(unsafe_code)]

#[allow(unsafe_code, missing_docs, clippy::all, clippy::pedantic)]
mod ui {
    slint::include_modules!();
}
pub use ui::*;

pub mod adapter;
pub mod controller;
pub mod headless;
pub mod launcher;
pub mod provider;
pub mod shortcuts;
pub mod standalone;
