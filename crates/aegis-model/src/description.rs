//! Description text on elements and notes on the project (job 064, FND-01).
//!
//! A description is plain text kept next to an element, set by one op
//! ([`Op::SetDescription`](crate::Op::SetDescription)) and undone by its
//! inverse like any other change. The project's own notes are the
//! description of [`ElementRef::Project`].
//!
//! Descriptions live in one ordered map on the [`Project`](crate::Project),
//! not inside each entity. An empty map costs nothing, a project without
//! descriptions is saved exactly as before, and the order of the map is the
//! order of the IDs, so output is deterministic (P4).
//!
//! An element with a description cannot be removed: clear the description in
//! the same batch first. That keeps the inverse of a removal (an `Add*` op)
//! unchanged, and one undo step restores the element and then its text.

use std::sync::Arc;

use crate::error::ModelError;
use crate::id::{AssetId, ClaimId, ControlId, DamageId, GoalId, ThreatId, TreeId};
use crate::provenance::Provenance;

/// Longest description of an element, in chars.
///
/// ```
/// assert_eq!(aegis_model::MAX_DESCRIPTION_CHARS, 8_000);
/// ```
pub const MAX_DESCRIPTION_CHARS: usize = 8_000;

/// Longest project notes text, in chars. At four bytes a char this stays
/// below the op log record cap.
///
/// ```
/// assert!(4 * aegis_model::MAX_NOTES_CHARS < aegis_model::MAX_OP_RECORD_BYTES);
/// ```
pub const MAX_NOTES_CHARS: usize = 15_000;

/// Most description bytes in one project, all elements and the notes
/// together (R3): a quarter of the largest snapshot.
///
/// ```
/// assert_eq!(aegis_model::MAX_DESCRIPTION_TOTAL_BYTES, 16 * 1024 * 1024);
/// ```
pub const MAX_DESCRIPTION_TOTAL_BYTES: usize = 16 * 1024 * 1024;

/// What a description belongs to.
///
/// ```
/// use aegis_model::{AssetId, ElementRef};
///
/// assert!(ElementRef::Project < ElementRef::Asset(AssetId::new_v7()));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ElementRef {
    /// The project itself: its notes.
    Project,
    /// An asset.
    Asset(AssetId),
    /// A damage scenario.
    Damage(DamageId),
    /// A threat scenario.
    Threat(ThreatId),
    /// An attack tree.
    Tree(TreeId),
    /// A security control.
    Control(ControlId),
    /// A cybersecurity goal.
    Goal(GoalId),
    /// A cybersecurity claim.
    Claim(ClaimId),
}

impl ElementRef {
    /// The longest text this target may carry, in chars.
    ///
    /// ```
    /// use aegis_model::{ElementRef, MAX_DESCRIPTION_CHARS, MAX_NOTES_CHARS};
    ///
    /// assert_eq!(ElementRef::Project.max_chars(), MAX_NOTES_CHARS);
    /// assert!(MAX_NOTES_CHARS > MAX_DESCRIPTION_CHARS);
    /// ```
    #[must_use]
    pub const fn max_chars(self) -> usize {
        match self {
            Self::Project => MAX_NOTES_CHARS,
            _ => MAX_DESCRIPTION_CHARS,
        }
    }
}

/// A stored description with the provenance of the change that wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Described {
    pub(crate) text: Arc<str>,
    pub(crate) provenance: Provenance,
}

/// Checks description text: plain text, at most `max` chars. Tabs and line
/// breaks are text; NUL and other control characters (including `\r`) are
/// not. Time O(text length).
pub(crate) fn check_text(text: &str, max: usize) -> Result<(), ModelError> {
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(ModelError::InvalidText);
    }
    if text.chars().count() > max {
        return Err(ModelError::DescriptionTooLong { max });
    }
    Ok(())
}
