//! Platform facts the window cannot learn from Slint: the widget style the
//! build selected and the native font families (UI-07.4, UI-07.7).
//!
//! Fonts are referenced by family name and never bundled, so the app adds no
//! font licence (D25). Slint's text shaping adds the script fallback from the
//! OS, which is how Japanese text reaches Hiragino Sans on macOS and Yu Gothic
//! UI on Windows. The fallback list is deliberately not hard-coded here: the
//! platform font database knows which families cover the text.

use crate::AppWindow;

/// The Slint widget style selected by `build.rs` for the target OS.
#[must_use]
pub const fn widget_style() -> &'static str {
    env!("AEGIS_SLINT_STYLE")
}

/// The system UI font family: SF Pro through its CoreText family name on
/// macOS, Segoe UI Variable on Windows, and the fontconfig default on Linux.
#[cfg(target_os = "macos")]
pub const UI_FONT_FAMILY: &str = ".SF NS";
#[cfg(target_os = "windows")]
pub const UI_FONT_FAMILY: &str = "Segoe UI Variable";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const UI_FONT_FAMILY: &str = "";

/// The system monospace family for identifiers, JSON, and report previews.
#[cfg(target_os = "macos")]
pub const MONO_FONT_FAMILY: &str = ".SF NS Mono";
#[cfg(target_os = "windows")]
pub const MONO_FONT_FAMILY: &str = "Cascadia Mono";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const MONO_FONT_FAMILY: &str = "monospace";

/// The Japanese UI fallback families, for documentation and tests. Slint asks
/// the OS font database for them per glyph, so they are not passed as a
/// comma-separated list (Slint takes one family name and adds its own
/// generic fallbacks).
#[cfg(target_os = "macos")]
pub const JAPANESE_FALLBACK: &[&str] = &["Hiragino Sans"];
#[cfg(target_os = "windows")]
pub const JAPANESE_FALLBACK: &[&str] = &["Yu Gothic UI"];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const JAPANESE_FALLBACK: &[&str] = &["Noto Sans CJK JP"];

/// Applies the native UI and monospace families to `window`. Called once,
/// before the window is shown.
pub fn apply_fonts(window: &AppWindow) {
    window.set_ui_font_family(UI_FONT_FAMILY.into());
    window.set_mono_font_family(MONO_FONT_FAMILY.into());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_style_follows_the_target_os() {
        let expected = if cfg!(target_os = "macos") {
            "cupertino"
        } else {
            "fluent"
        };
        assert_eq!(widget_style(), expected);
    }

    #[test]
    fn every_target_has_a_ui_and_a_monospace_family() {
        // Linux uses the fontconfig default for UI text but still needs an
        // explicit monospace family; the others name a system family.
        if !cfg!(any(target_os = "macos", target_os = "windows")) {
            assert!(UI_FONT_FAMILY.is_empty());
        }
        assert!(!MONO_FONT_FAMILY.is_empty());
        assert!(!JAPANESE_FALLBACK.is_empty());
    }
}
