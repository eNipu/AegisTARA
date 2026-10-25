//! Server-level identifiers.

use std::fmt;
use std::str::FromStr;

use uuid::Uuid;

/// Identifies one project on the server: a `UUIDv7`, written as hyphenated
/// lowercase text everywhere (API, database, logs).
///
/// The model crate has no project entity (a `Project` is the whole
/// document), so the server owns this ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectId(Uuid);

/// Text that is not a hyphenated lowercase UUID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("not a project id")]
pub struct BadProjectId;

impl ProjectId {
    /// A new time-ordered ID.
    #[must_use]
    pub fn new_v7() -> Self {
        ProjectId(Uuid::now_v7())
    }

    /// Wraps an existing UUID (for tests and database rows).
    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        ProjectId(uuid)
    }

    /// The underlying UUID.
    #[must_use]
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

impl FromStr for ProjectId {
    type Err = BadProjectId;

    /// Accepts exactly the form `Display` writes (36 chars, lowercase).
    /// Untrusted text, so the length is checked before parsing (R3).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 36 || s.bytes().any(|b| b.is_ascii_uppercase()) {
            return Err(BadProjectId);
        }
        Uuid::parse_str(s).map(ProjectId).map_err(|_| BadProjectId)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_only_the_canonical_form() {
        let id = ProjectId::new_v7();
        let text = id.to_string();
        assert_eq!(text.parse::<ProjectId>(), Ok(id));
        assert_eq!(text.to_uppercase().parse::<ProjectId>(), Err(BadProjectId));
        assert_eq!(
            text.replace('-', "").parse::<ProjectId>(),
            Err(BadProjectId)
        );
        assert_eq!(
            format!("{{{text}}}").parse::<ProjectId>(),
            Err(BadProjectId)
        );
        assert_eq!("".parse::<ProjectId>(), Err(BadProjectId));
    }
}
