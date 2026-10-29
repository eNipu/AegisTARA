//! Built-in STRIDE methodology for services and cloud.
//!
//! The pack defines a service-impact example scale, the six STRIDE threat
//! categories, and reference syntaxes for CAPEC, MITRE ATT&CK Enterprise
//! technique IDs, and EMB3D threat IDs. It carries no catalog records.

use super::Methodology;
use crate::error::PackError;

const PACK: &[u8] = include_bytes!("../../packs/stride-services-cloud.json");

impl Methodology {
    /// Loads the built-in STRIDE methodology for services and cloud.
    ///
    /// # Errors
    ///
    /// Returns [`PackError`] if the embedded pack is invalid.
    pub fn stride_services_cloud() -> Result<Self, PackError> {
        Self::read_pack(PACK)
    }

    /// Returns the canonical `aegis-methodology-pack` JSON for the built-in
    /// services/cloud STRIDE methodology.
    #[must_use]
    pub const fn stride_services_cloud_pack() -> &'static [u8] {
        PACK
    }
}
