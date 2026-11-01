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

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::i18n::Language;
use crate::report::ReportProfile;

static TMP_SEQ: AtomicU64 = AtomicU64::new(1);

/// Writes `bytes` to `path` via a new owner-only (`0600` on Unix) temporary
/// file in the same directory and renames it into place (SEC-09). Refuses to
/// overwrite a symbolic link.
///
/// # Errors
///
/// The I/O error.
pub fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    if !dir.as_os_str().is_empty() {
        std::fs::create_dir_all(dir)?;
    }
    if let Ok(meta) = std::fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "refusing to overwrite a symbolic link",
        ));
    }
    let stem = path.file_name().and_then(|n| n.to_str()).unwrap_or("aegis");
    let pid = std::process::id();
    for _ in 0..64 {
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let tmp = dir.join(format!(".{stem}.{pid}.{seq}.tmp"));
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            opts.mode(0o600);
        }
        match opts.open(&tmp) {
            Ok(mut file) => {
                let res = (|| {
                    file.write_all(bytes)?;
                    file.sync_all()?;
                    drop(file);
                    std::fs::rename(&tmp, path)
                })();
                if res.is_err() {
                    let _ = std::fs::remove_file(&tmp);
                }
                return res;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary file",
    ))
}

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
    /// Where the signed update manifest is (`https://...`); empty for none.
    /// `AEGIS_UPDATE_URL` overrides it. An air gapped site leaves it empty.
    pub update_url: String,
    /// Last standalone project folder; empty until one is chosen.
    pub standalone_folder: String,
    /// Signed license used for standalone mode; never a password or token.
    pub standalone_license: String,
    /// Saved report profiles (RPT-03), one per project or company.
    pub report_profiles: Vec<ReportProfile>,
    /// The name of the active report profile; empty means the default.
    pub report_profile_name: String,
    /// The UI language: `en`, `ja` or `pseudo`; empty follows the OS
    /// (L10N-02).
    pub language: String,
    /// The appearance: `system`, `light` or `dark`; empty follows the OS
    /// (UI-07.5).
    pub appearance: String,
    /// Width of the left panel (the TARA steps) in logical pixels; `0` is
    /// the default (UI-02).
    pub left_panel: u32,
    /// Width of the right panel (the inspector) in logical pixels; `0`
    /// follows the window (UI-02).
    pub right_panel: u32,
    /// Height of the bottom panel (the history) in logical pixels; `0` is
    /// the default (UI-02).
    pub bottom_panel: u32,
}

/// How the window picks light or dark (UI-07.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    /// Follow the operating system, live. The default.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

impl Appearance {
    /// The value stored in the settings file.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// Reads a stored value; anything unknown follows the OS.
    #[must_use]
    pub fn from_code(code: &str) -> Self {
        match code.trim() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }

    /// The position used by the window (`0` System, `1` Light, `2` Dark).
    #[must_use]
    pub const fn index(self) -> i32 {
        match self {
            Self::System => 0,
            Self::Light => 1,
            Self::Dark => 2,
        }
    }

    /// The inverse of [`Appearance::index`]; out of range follows the OS.
    #[must_use]
    pub const fn from_index(index: i32) -> Self {
        match index {
            1 => Self::Light,
            2 => Self::Dark,
            _ => Self::System,
        }
    }
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
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        write_private_file(path, text.as_bytes())
    }

    /// The active report profile, or the default when none is chosen.
    #[must_use]
    pub fn report_profile(&self) -> ReportProfile {
        self.report_profiles
            .iter()
            .find(|p| p.name == self.report_profile_name)
            .cloned()
            .unwrap_or_default()
    }

    /// The UI language, defaulting to the OS locale when none is chosen.
    #[must_use]
    pub fn language(&self) -> Language {
        if self.language.trim().is_empty() {
            Language::from_os()
        } else {
            Language::from_code(self.language.trim())
        }
    }

    /// The chosen appearance, defaulting to the OS.
    #[must_use]
    pub fn appearance(&self) -> Appearance {
        Appearance::from_code(&self.appearance)
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
            update_url: "https://updates.example.com/m".into(),
            export_dir: String::new(),
            standalone_folder: "/work/brake.aegis".into(),
            standalone_license: "/work/vendor.aegis".into(),
            report_profiles: vec![ReportProfile {
                name: "Submission".into(),
                ..ReportProfile::default()
            }],
            report_profile_name: "Submission".into(),
            language: "ja".into(),
            appearance: "dark".into(),
            left_panel: 240,
            right_panel: 420,
            bottom_panel: 0,
        };
        s.save(&path).expect("save");
        assert_eq!(Settings::load(&path), s);
        assert_eq!(s.appearance(), Appearance::Dark);
        assert_eq!(Settings::default().appearance(), Appearance::System);
        for a in [Appearance::System, Appearance::Light, Appearance::Dark] {
            assert_eq!(Appearance::from_code(a.code()), a);
            assert_eq!(Appearance::from_index(a.index()), a);
        }
        assert_eq!(Appearance::from_code("sepia"), Appearance::System);
        std::fs::write(&path, "{ not json").expect("write");
        assert_eq!(Settings::load(&path), Settings::default());
        // Unknown fields from a newer app are ignored, missing ones default.
        std::fs::write(&path, r#"{"server_url":"x","future":1}"#).expect("write");
        assert_eq!(Settings::load(&path).server_url, "x");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            s.save(&path).expect("save again");
            let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "settings file must be owner-only (0600)");
            let link = dir.join("symlink.json");
            std::os::unix::fs::symlink(&path, &link).expect("symlink");
            assert!(
                write_private_file(&link, b"nope").is_err(),
                "must refuse to overwrite a symbolic link"
            );
        }
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
