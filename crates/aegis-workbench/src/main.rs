//! The `aegis-workbench` binary: opens the workbench window.

#![forbid(unsafe_code)]

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn main() {
    aegis_workbench::app::run(std::env::args_os().nth(1).map(std::path::PathBuf::from));
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn main() {
    eprintln!("aegis-workbench runs on macOS and Windows");
    std::process::exit(1);
}
