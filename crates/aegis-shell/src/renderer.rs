//! Renderer selection (UI-07.1, ADR 0063).
//!
//! Skia is the default. `AEGIS_RENDERER=software` forces Slint's software
//! renderer before any window or GPU is created, so it is also the switch for
//! a remote session or a machine with no usable driver. An unknown value is
//! ignored with one warning and the default choice.
//!
//! Slint builds the winit event loop when the backend is selected but creates
//! the renderer surface only when the first window is shown. A failed GPU
//! surface therefore shows up as a `Window::run` error, after the platform is
//! installed and after winit's one-shot event loop cannot be replaced. The app
//! handles that one case by logging one line and re-executing itself with
//! `AEGIS_RENDERER=software` (see `main.rs`), which is a real retry on the
//! software renderer rather than a silent downgrade. Skia also tries its own
//! software surface before giving up; that path stays inside one renderer and
//! is not the fallback the log line describes.

use slint::BackendSelector;

/// The renderers the app can use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Renderer {
    /// The default GPU renderer (ADR 0063).
    Skia,
    /// Slint's CPU renderer: the fallback, the escape hatch, and the renderer
    /// of the headless test platform.
    Software,
}

impl Renderer {
    /// The Slint backend name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Renderer::Skia => "skia",
            Renderer::Software => "software",
        }
    }

    /// A short name for the status bar.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Renderer::Skia => "Skia",
            Renderer::Software => "Software",
        }
    }
}

/// What the environment asked for, and an unknown value to warn about.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Choice {
    pub renderer: Renderer,
    /// `Some(value)` when `AEGIS_RENDERER` was set but not understood.
    pub unknown: Option<String>,
}

/// Parses `AEGIS_RENDERER`. Empty or absent means Skia; the comparison is
/// case-insensitive so `SOFTWARE` works in a shortcut.
#[must_use]
pub fn parse(value: Option<&str>) -> Choice {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Choice {
            renderer: Renderer::Skia,
            unknown: None,
        };
    };
    match value.to_ascii_lowercase().as_str() {
        "software" | "sw" => Choice {
            renderer: Renderer::Software,
            unknown: None,
        },
        "skia" => Choice {
            renderer: Renderer::Skia,
            unknown: None,
        },
        _ => Choice {
            renderer: Renderer::Skia,
            unknown: Some(value.to_owned()),
        },
    }
}

/// Selects the renderer and installs the winit platform. Logs one warning for
/// an unknown value and one if the backend itself cannot be created with
/// Skia, in which case it retries once on software.
///
/// # Errors
///
/// The software backend could not be created either (no display, for
/// example), or the unknown-value path could not open a window.
pub fn select() -> Result<Renderer, String> {
    let choice = parse(std::env::var("AEGIS_RENDERER").ok().as_deref());
    if let Some(value) = &choice.unknown {
        tracing::warn!(
            value = %value,
            "unknown AEGIS_RENDERER value; using the default Skia renderer"
        );
    }
    match choice.renderer {
        Renderer::Software => {
            select_backend(Renderer::Software)?;
            Ok(Renderer::Software)
        }
        Renderer::Skia => match select_backend(Renderer::Skia) {
            Ok(()) => Ok(Renderer::Skia),
            Err(error) => {
                tracing::warn!(
                    renderer = "skia",
                    fallback = "software",
                    %error,
                    "the Skia renderer could not be selected; retrying with the software renderer"
                );
                select_backend(Renderer::Software)?;
                Ok(Renderer::Software)
            }
        },
    }
}

fn select_backend(renderer: Renderer) -> Result<(), String> {
    BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name(renderer.name().into())
        .select()
        .map_err(|e| {
            format!(
                "cannot open a window with the {} renderer: {e}",
                renderer.name()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_and_empty_mean_skia() {
        assert_eq!(parse(None).renderer, Renderer::Skia);
        assert_eq!(parse(Some("")).renderer, Renderer::Skia);
        assert_eq!(parse(Some("  ")).renderer, Renderer::Skia);
        assert_eq!(parse(None).unknown, None);
    }

    #[test]
    fn software_is_case_insensitive() {
        for value in ["software", "SOFTWARE", " Software "] {
            let choice = parse(Some(value));
            assert_eq!(choice.renderer, Renderer::Software, "{value}");
            assert_eq!(choice.unknown, None, "{value}");
        }
    }

    #[test]
    fn an_unknown_value_falls_back_to_skia_with_one_warning_marker() {
        let choice = parse(Some("vulkan"));
        assert_eq!(choice.renderer, Renderer::Skia);
        assert_eq!(choice.unknown.as_deref(), Some("vulkan"));
    }

    #[test]
    fn names_are_the_slint_backend_names() {
        assert_eq!(Renderer::Skia.name(), "skia");
        assert_eq!(Renderer::Software.name(), "software");
    }
}
