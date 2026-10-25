//! Signing. Vendor tooling only (feature `issuer`).

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};

use crate::envelope;
use crate::error::LicenseError;
use crate::license::{DOMAIN as LICENSE_DOMAIN, FILE_PREFIX, License};
use crate::manifest::{DOMAIN as MANIFEST_DOMAIN, MANIFEST_PREFIX, Manifest, ManifestError};

/// Holds a signing key and produces license files.
pub struct Issuer {
    pair: Ed25519KeyPair,
}

impl Issuer {
    /// A new key pair: the PKCS#8 private key bytes and the public key.
    ///
    /// # Errors
    ///
    /// [`LicenseError::Signing`] if the system random source fails.
    pub fn generate() -> Result<(Vec<u8>, [u8; 32]), LicenseError> {
        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| LicenseError::Signing)?;
        let issuer = Self::from_pkcs8(pkcs8.as_ref())?;
        Ok((pkcs8.as_ref().to_vec(), issuer.public_key()))
    }

    /// Loads a key from PKCS#8 bytes.
    ///
    /// # Errors
    ///
    /// [`LicenseError::Signing`] if the bytes are not an Ed25519 key.
    pub fn from_pkcs8(pkcs8: &[u8]) -> Result<Self, LicenseError> {
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8).map_err(|_| LicenseError::Signing)?;
        Ok(Issuer { pair })
    }

    /// The public key to put in `keys/vendor.keys`.
    #[must_use]
    pub fn public_key(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(self.pair.public_key().as_ref());
        out
    }

    /// Signs a license. The record is checked first, so a license that the
    /// verifier would refuse is never produced.
    ///
    /// # Errors
    ///
    /// The record's own error, or [`LicenseError::Signing`].
    pub fn sign(&self, license: &License) -> Result<String, LicenseError> {
        license.check()?;
        let payload = license.payload()?;
        Ok(envelope::seal(
            &self.pair,
            FILE_PREFIX,
            LICENSE_DOMAIN,
            &payload,
        ))
    }

    /// Signs an update manifest, after checking it.
    ///
    /// # Errors
    ///
    /// The manifest's own error, or [`ManifestError::Signing`].
    pub fn sign_manifest(&self, manifest: &Manifest) -> Result<String, ManifestError> {
        manifest.check()?;
        let payload = manifest.payload()?;
        Ok(envelope::seal(
            &self.pair,
            MANIFEST_PREFIX,
            MANIFEST_DOMAIN,
            &payload,
        ))
    }
}
