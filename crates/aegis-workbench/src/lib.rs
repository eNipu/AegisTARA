//! AegisTARA workbench: the GPUI window over the client core (job 182).
//!
//! `theme` and `sample` are plain Rust and build everywhere; the window
//! itself (`app`) needs GPUI, which runs on macOS and Windows only.

#![forbid(unsafe_code)]

pub mod sample;
pub mod theme;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod app;
