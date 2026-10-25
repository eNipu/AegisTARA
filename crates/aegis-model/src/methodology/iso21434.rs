//! The built-in ISO/SAE 21434 + UN R155 methodology, shipped as a pack.
//!
//! The vocabulary is data in `packs/iso21434-r155.json`, embedded in the
//! binary and decoded through the same [`Methodology::read_pack`] as any
//! other pack, so it obeys every cap and rule a pack does, needs no install
//! path, and cannot change on disk. This file holds no vocabulary; it is the
//! only `src` file that names the standard. Labels in the pack are short
//! names only, with no text or tables from the standards.

use super::Methodology;
use crate::error::PackError;

/// The built-in pack: canonical `write_pack` output (checked by
/// `tests/builtin_pack.rs`).
const BUILTIN: &str = include_str!("../../packs/iso21434-r155.json");

impl Methodology {
    /// The built-in ISO/SAE 21434 + UN R155 methodology, version `0.1.0`,
    /// decoded from the pack embedded in the binary.
    ///
    /// - Impact categories `safety`, `financial`, `operational`, `privacy`,
    ///   each rated `negligible` < `moderate` < `major` < `severe`.
    /// - Properties `confidentiality`, `integrity`, `availability`,
    ///   `authenticity`, `authorization`, `non-repudiation`.
    /// - STRIDE threat categories, each violating one property.
    /// - One catalog scheme, `r155-annex5`, with IDs `N` or `N.M`
    ///   (N in 1..=32, M in 1..=99). No catalog contents ship.
    ///
    /// Decodes on every call and keeps no cache: the same cost as
    /// [`Methodology::read_pack`] on the 3 KB pack, time and space O(m)
    /// with m = 40 vocabulary entries (about 20 µs, see
    /// `docs/adr/0011-methodology-pack-files.md`). Share the result with an
    /// `Arc` instead of calling it in a loop.
    ///
    /// # Errors
    ///
    /// Never in practice: the pack is fixed at build time and covered by
    /// tests. The `Result` is kept because it goes through the pack reader
    /// and [`Methodology::new`], which report errors honestly.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.key(), "iso21434-r155");
    /// assert_eq!(m.version(), "0.1.0");
    /// # Ok(())
    /// # }
    /// ```
    pub fn iso21434_r155() -> Result<Methodology, PackError> {
        Methodology::read_pack(BUILTIN.as_bytes())
    }

    /// The pack file the built-in methodology is decoded from (format
    /// `aegis-methodology-pack` v1, canonical JSON). A server hands these
    /// bytes to clients that need the vocabulary.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let pack = Methodology::iso21434_r155_pack();
    /// assert_eq!(Methodology::read_pack(pack.as_bytes())?, Methodology::iso21434_r155()?);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn iso21434_r155_pack() -> &'static str {
        BUILTIN
    }
}
