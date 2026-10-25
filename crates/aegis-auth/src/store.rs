//! Where the device key and the refresh token live between runs.
//!
//! [`SecretStore`] is a minimal name-to-bytes vault. The OS keychain backend
//! ([`OsKeychain`], feature `os-keychain`) is what ships; [`MemorySecretStore`]
//! backs tests and the "do not remember me" mode.
//!
//! [`Vault`] adds the two secrets login needs, namespaced by account so
//! that two servers or two users on one machine never share a key:
//!
//! - the **device key** (PKCS#8), created on first use. It is what binds
//!   tokens to this machine, so it is never exported or sent anywhere;
//! - the **refresh token**.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::dpop::DeviceKey;
use crate::error::AuthError;

/// A vault of named secrets.
pub trait SecretStore: Send + Sync {
    /// Reads a secret.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`] if the store fails (a missing secret is `Ok(None)`).
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, AuthError>;

    /// Writes or replaces a secret.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`] if the store fails.
    fn set(&self, name: &str, value: &[u8]) -> Result<(), AuthError>;

    /// Deletes a secret; deleting a missing one succeeds.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`] if the store fails.
    fn delete(&self, name: &str) -> Result<(), AuthError>;
}

/// A shared vault is a vault: two sessions can hold the same store, as two
/// runs of the application share one OS keychain.
impl<T: SecretStore + ?Sized> SecretStore for std::sync::Arc<T> {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, AuthError> {
        (**self).get(name)
    }

    fn set(&self, name: &str, value: &[u8]) -> Result<(), AuthError> {
        (**self).set(name, value)
    }

    fn delete(&self, name: &str) -> Result<(), AuthError> {
        (**self).delete(name)
    }
}

/// A vault in process memory.
#[derive(Default)]
pub struct MemorySecretStore {
    secrets: Mutex<HashMap<String, Vec<u8>>>,
}

impl MemorySecretStore {
    /// An empty vault.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn with<R>(&self, f: impl FnOnce(&mut HashMap<String, Vec<u8>>) -> R) -> R {
        f(&mut self
            .secrets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }
}

impl SecretStore for MemorySecretStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, AuthError> {
        Ok(self.with(|m| m.get(name).cloned()))
    }

    fn set(&self, name: &str, value: &[u8]) -> Result<(), AuthError> {
        self.with(|m| m.insert(name.to_owned(), value.to_vec()));
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<(), AuthError> {
        self.with(|m| m.remove(name));
        Ok(())
    }
}

/// The operating system's secret store: Windows Credential Manager, macOS
/// Keychain, or the Linux Secret Service.
#[cfg(feature = "os-keychain")]
pub struct OsKeychain {
    service: String,
}

#[cfg(feature = "os-keychain")]
impl OsKeychain {
    /// Selects the platform store and returns a keychain scope. `service` is
    /// the application name shown in the OS credential viewer.
    ///
    /// Call once at startup; the store becomes the process-wide default of
    /// the keyring library.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`] if the platform has no usable store (for example
    /// a Linux session without a Secret Service such as GNOME Keyring or
    /// KWallet). The caller can then continue without remembering the login.
    pub fn open(service: &str) -> Result<Self, AuthError> {
        #[cfg(target_os = "windows")]
        let store = windows_native_keyring_store::Store::new();
        #[cfg(target_os = "macos")]
        let store = apple_native_keyring_store::keychain::Store::new();
        #[cfg(all(unix, not(target_os = "macos")))]
        let store = dbus_secret_service_keyring_store::Store::new();
        let store = store.map_err(|e| AuthError::Store(e.to_string()))?;
        keyring_core::set_default_store(store);
        Ok(OsKeychain {
            service: service.to_owned(),
        })
    }

    fn entry(&self, name: &str) -> Result<keyring_core::Entry, AuthError> {
        keyring_core::Entry::new(&self.service, name).map_err(|e| AuthError::Store(e.to_string()))
    }
}

#[cfg(feature = "os-keychain")]
impl SecretStore for OsKeychain {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, AuthError> {
        match self.entry(name)?.get_secret() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(AuthError::Store(e.to_string())),
        }
    }

    fn set(&self, name: &str, value: &[u8]) -> Result<(), AuthError> {
        self.entry(name)?
            .set_secret(value)
            .map_err(|e| AuthError::Store(e.to_string()))
    }

    fn delete(&self, name: &str) -> Result<(), AuthError> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(AuthError::Store(e.to_string())),
        }
    }
}

/// The login secrets of one account.
pub struct Vault<S> {
    store: S,
    account: String,
}

impl<S: SecretStore> Vault<S> {
    /// A vault for `account` (for example `user@server-host`).
    #[must_use]
    pub fn new(store: S, account: &str) -> Self {
        Vault {
            store,
            account: account.to_owned(),
        }
    }

    fn name(&self, what: &str) -> String {
        format!("{}/{what}", self.account)
    }

    /// The device key, created and saved on first use. A stored key that no
    /// longer parses is replaced (the old tokens were bound to it and are
    /// useless anyway).
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`] or [`AuthError::BadKey`].
    pub fn device_key(&self) -> Result<DeviceKey, AuthError> {
        let name = self.name("device-key");
        if let Some(bytes) = self.store.get(&name)?
            && let Ok(key) = DeviceKey::from_pkcs8(&bytes)
        {
            return Ok(key);
        }
        let key = DeviceKey::generate()?;
        self.store.set(&name, key.to_pkcs8())?;
        Ok(key)
    }

    /// Saves the refresh token.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`].
    pub fn save_refresh_token(&self, token: &str) -> Result<(), AuthError> {
        self.store
            .set(&self.name("refresh-token"), token.as_bytes())
    }

    /// Loads the refresh token, if any.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`].
    pub fn load_refresh_token(&self) -> Result<Option<String>, AuthError> {
        Ok(self
            .store
            .get(&self.name("refresh-token"))?
            .and_then(|b| String::from_utf8(b).ok()))
    }

    /// Forgets the refresh token (logout). The device key stays: it is
    /// this machine's identity, not a session.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`].
    pub fn clear_session(&self) -> Result<(), AuthError> {
        self.store.delete(&self.name("refresh-token"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_device_key_is_created_once_and_then_reused() {
        let vault = Vault::new(MemorySecretStore::new(), "alice@server");
        let first = vault.device_key().expect("key");
        let again = vault.device_key().expect("key");
        assert_eq!(first.thumbprint(), again.thumbprint());
    }

    #[test]
    fn accounts_do_not_share_secrets() {
        let store = std::sync::Arc::new(MemorySecretStore::new());
        let a = Vault::new(store.clone(), "alice@one");
        let b = Vault::new(store, "alice@two");
        assert_ne!(
            a.device_key().expect("key").thumbprint(),
            b.device_key().expect("key").thumbprint()
        );
        a.save_refresh_token("r1").expect("save");
        assert_eq!(a.load_refresh_token().expect("load").as_deref(), Some("r1"));
        assert_eq!(b.load_refresh_token().expect("load"), None);
    }

    #[test]
    fn logout_forgets_the_session_but_keeps_the_device_key() {
        let vault = Vault::new(MemorySecretStore::new(), "a");
        let key = vault.device_key().expect("key");
        vault.save_refresh_token("r").expect("save");
        vault.clear_session().expect("clear");
        assert_eq!(vault.load_refresh_token().expect("load"), None);
        assert_eq!(
            vault.device_key().expect("key").thumbprint(),
            key.thumbprint()
        );
        vault.clear_session().expect("clearing twice is fine");
    }

    #[test]
    fn a_corrupt_stored_key_is_replaced() {
        let store = MemorySecretStore::new();
        store.set("a/device-key", b"garbage").expect("set");
        let vault = Vault::new(store, "a");
        let key = vault.device_key().expect("replaced");
        assert_eq!(
            vault.device_key().expect("key").thumbprint(),
            key.thumbprint()
        );
    }
}
