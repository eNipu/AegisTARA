//! Client core. No UI toolkit.
#![forbid(unsafe_code)]
pub mod audit;
pub mod commands;
pub mod connection;
pub mod error;
pub mod excel;
pub mod find;
pub mod i18n;
pub mod library;
pub mod outline;
pub mod pages;
pub mod report;
pub mod sample;
pub mod session;
pub mod settings;
pub mod state;
pub mod suggest;
pub mod time;
pub mod transport;
mod tree_diff;
pub mod undo_label;
pub mod updates;
pub mod view;
pub mod workspace;
