//! The license file on this computer: is it signed by a trusted key, and does
//! it still allow editing? An expired license still opens projects, read-only.

use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis_license::{License, LicenseState, TrustedKeys};

use crate::i18n::Catalog;

/// A checked license, as the start screen and the workbench need it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalLicense {
    /// The licensed organisation, as signed.
    pub customer: String,
    /// Whether editing is allowed (paid period or grace period).
    pub editing: bool,
    /// Whether the paid period is over and the grace period runs.
    pub grace: bool,
    /// Whole days to the end of the paid period when active, of the grace
    /// period in grace; `None` when expired.
    pub days_left: Option<u64>,
}

/// The largest license file read. Real files are well under 2 KiB.
pub const MAX_LICENSE_BYTES: u64 = 64 * 1024;

/// The longest error text shown from the license library (lesson L33).
const MAX_ERROR_CHARS: usize = 200;

const DAY: u64 = 86_400;

fn cut(text: &str) -> String {
    text.chars().take(MAX_ERROR_CHARS).collect()
}

/// Checks a license file's text against `keys` at `now_secs`. Time O(n) in
/// the text length.
///
/// # Errors
///
/// A translated message: `license-invalid` for a bad or foreign signature,
/// `license-not-yet-valid` before the start. An expired license is `Ok`
/// with `editing == false`.
pub fn check_license(
    text: &str,
    keys: &TrustedKeys,
    now_secs: u64,
    catalog: &Catalog,
) -> Result<LocalLicense, String> {
    let license = License::verify(text, keys)
        .map_err(|e| catalog.tr_with("license-invalid", &[("error", &cut(&e.to_string()))]))?;
    let (editing, grace, days_left) = match license.state(now_secs) {
        LicenseState::NotYetValid => return Err(catalog.tr("license-not-yet-valid")),
        LicenseState::Active { expires_in } => (true, false, Some(expires_in / DAY)),
        LicenseState::Grace { ends_in } => (true, true, Some(ends_in / DAY)),
        LicenseState::Expired => (false, false, None),
    };
    Ok(LocalLicense {
        customer: license.customer,
        editing,
        grace,
        days_left,
    })
}

/// Reads at most [`MAX_LICENSE_BYTES`] from `path` and checks it with
/// [`check_license`].
///
/// # Errors
///
/// `license-unreadable` for an I/O error, `license-invalid` for a file that
/// is too large or not text, else whatever [`check_license`] returns.
pub fn read_license(
    path: &Path,
    keys: &TrustedKeys,
    now_secs: u64,
    catalog: &Catalog,
) -> Result<LocalLicense, String> {
    let unreadable = |e: std::io::Error| {
        catalog.tr_with(
            "license-unreadable",
            &[
                ("path", &path.display().to_string()),
                ("error", &cut(&e.to_string())),
            ],
        )
    };
    let file = std::fs::File::open(path).map_err(unreadable)?;
    let mut bytes = Vec::new();
    file.take(MAX_LICENSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    let invalid = |error: &str| catalog.tr_with("license-invalid", &[("error", error)]);
    if bytes.len() as u64 > MAX_LICENSE_BYTES {
        return Err(invalid("the file is too large"));
    }
    let text = String::from_utf8(bytes).map_err(|_| invalid("the file is not text"))?;
    check_license(&text, keys, now_secs, catalog)
}

/// Seconds since the Unix epoch on the system clock; 0 before 1970.
#[must_use]
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
