//! What the app remembers between runs: the last server, how to reach it,
//! the user name, and the export folder. Never a password or a token (those
//! go to the OS keychain, see the shell).
//!
//! The file is JSON in the user's configuration folder:
//!
//! | OS | Path |
//! |---|---|
//! | Windows | `%APPDATA%\AegisTARA\settings.json` |
//! | macOS | `~/Library/Application Support/AegisTARA/settings.json` |
//! | Linux | `$XDG_CONFIG_HOME/aegistara/settings.json`, else `~/.config/aegistara/settings.json` |
//!
//! Reading is capped at [`MAX_SETTINGS_BYTES`] (R3) and forgiving: a
//! missing, unreadable or damaged file gives the defaults, so the app always
//! starts. Writing goes to a temporary file first and is renamed into place,
//! so a crash never leaves half a file.

use std::io::Read as _;
use std::path::{Path, PathBuf};

/// Largest settings file read.
pub const MAX_SETTINGS_BYTES: u64 = 64 * 1024;

/// The remembered settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The server address, for example `https://aegis.example.com`.
    pub server_url: String,
    /// `auto`, `grpc` or `grpc-web`.
    pub transport: String,
    /// The last user name typed at the password login.
    pub username: String,
    /// Where exports are written; empty for the documents folder.
    pub export_dir: String,
}

/// The configuration folder of this app, if the OS tells us one.
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(target_os = "windows") {
        var("APPDATA").map(|p| p.join("AegisTARA"))
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|p| p.join("Library/Application Support/AegisTARA"))
    } else {
        var("XDG_CONFIG_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".config")))
            .map(|p| p.join("aegistara"))
    }
}

/// The default export folder: `Documents/AegisTARA` in the home folder, or
/// the current folder when there is no home.
#[must_use]
pub fn default_export_dir() -> PathBuf {
    let home = std::env::var_os(if cfg!(target_os = "windows") {
        "USERPROFILE"
    } else {
        "HOME"
    })
    .filter(|v| !v.is_empty())
    .map(PathBuf::from);
    home.map_or_else(
        || PathBuf::from("."),
        |h| h.join("Documents").join("AegisTARA"),
    )
}

impl Settings {
    /// The settings file in `dir`.
    #[must_use]
    pub fn path_in(dir: &Path) -> PathBuf {
        dir.join("settings.json")
    }

    /// Reads the settings from `path`, or the defaults.
    #[must_use]
    pub fn load(path: &Path) -> Settings {
        let Ok(file) = std::fs::File::open(path) else {
            return Settings::default();
        };
        let mut text = String::new();
        if file
            .take(MAX_SETTINGS_BYTES)
            .read_to_string(&mut text)
            .is_err()
        {
            return Settings::default();
        }
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Writes the settings to `path`, creating the folder.
    ///
    /// # Errors
    ///
    /// The I/O error.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    /// The export folder to use.
    #[must_use]
    pub fn export_dir(&self) -> PathBuf {
        if self.export_dir.trim().is_empty() {
            default_export_dir()
        } else {
            PathBuf::from(self.export_dir.trim())
        }
    }
}

/// A file name for an export: the project name with anything but letters,
/// digits, `-` and `_` replaced, the date, and the extension. Never a path
/// separator, so it cannot leave the export folder.
#[must_use]
pub fn export_file_name(project: &str, what: &str, now_ms: u64, extension: &str) -> String {
    let safe: String = project
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let safe = safe.trim_matches('-');
    let safe = if safe.is_empty() { "project" } else { safe };
    let short: String = safe.chars().take(60).collect();
    format!(
        "{short}-{what}-{}.{extension}",
        crate::time::date_ms(now_ms)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_survive_damage() {
        let dir = std::env::temp_dir().join(format!("aegis-settings-{}", std::process::id()));
        let path = Settings::path_in(&dir);
        assert_eq!(Settings::load(&path), Settings::default());
        let s = Settings {
            server_url: "https://aegis.example.com".into(),
            transport: "grpc-web".into(),
            username: "mira".into(),
            export_dir: String::new(),
        };
        s.save(&path).expect("save");
        assert_eq!(Settings::load(&path), s);
        std::fs::write(&path, "{ not json").expect("write");
        assert_eq!(Settings::load(&path), Settings::default());
        // Unknown fields from a newer app are ignored, missing ones default.
        std::fs::write(&path, r#"{"server_url":"x","future":1}"#).expect("write");
        assert_eq!(Settings::load(&path).server_url, "x");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_names_stay_in_the_folder() {
        assert_eq!(
            export_file_name("../Brake ECU/x", "report", 1_792_956_600_000, "html"),
            "Brake-ECU-x-report-2026-10-25.html"
        );
        assert_eq!(
            export_file_name("///", "risks", 0, "csv"),
            "project-risks-1970-01-01.csv"
        );
    }
}
