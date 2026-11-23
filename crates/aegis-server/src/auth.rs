//! Who is calling.
//!
//! Authentication happens before the service: a tower layer or tonic
//! interceptor validates the credentials and puts a [`Principal`] into the
//! request extensions. The service only reads it, so the login mechanism
//! (OIDC with DPoP in job 031, the development header here) can change
//! without touching the API code.

use std::sync::Arc;

use tonic::{Request, Status};

use crate::identity::UserId;

/// How a caller proved who they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMethod {
    /// A password session of this server (`AEGIS_AUTH=password`).
    Password,
    /// An OIDC access token from the company identity provider.
    Oidc,
    /// The development header. No proof at all.
    Development,
}

impl AuthMethod {
    /// The name `WhoAmI` reports.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AuthMethod::Password => "password",
            AuthMethod::Oidc => "oidc",
            AuthMethod::Development => "development",
        }
    }
}

/// The authenticated caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Principal {
    /// Stable subject: the identity provider's `sub` claim, the password
    /// user name, or the development user name. Used as the display subject
    /// and the lease holder.
    pub subject: Arc<str>,
    /// Verified OIDC issuer URL (`iss`) for [`AuthMethod::Oidc`], or `None`
    /// for password and development callers.
    pub issuer: Option<Arc<str>>,
    /// Stable internal user ID assigned by [`crate::identity::IdentityDirectory`],
    /// when resolved.
    pub user_id: Option<UserId>,
    /// May break other users' edit leases, manage users, verify audits.
    pub admin: bool,
    /// How the caller logged in.
    pub method: AuthMethod,
    /// SSO groups from the access token (`roles` and `groups` claims). Empty
    /// for password and development logins.
    pub groups: Vec<Arc<str>>,
}

impl Principal {
    /// A non-admin principal.
    #[must_use]
    pub fn user(subject: &str) -> Self {
        Principal {
            subject: subject.into(),
            issuer: None,
            user_id: None,
            admin: false,
            method: AuthMethod::Development,
            groups: Vec::new(),
        }
    }

    /// An admin principal.
    #[must_use]
    pub fn admin(subject: &str) -> Self {
        Principal {
            subject: subject.into(),
            issuer: None,
            user_id: None,
            admin: true,
            method: AuthMethod::Development,
            groups: Vec::new(),
        }
    }

    /// Qualified membership key for access checks. Never links local password
    /// accounts and OIDC `(issuer, sub)` identities by equal bare strings.
    #[must_use]
    pub fn qualified_member_key(&self) -> String {
        if let Some(uid) = self.user_id {
            return uid.member_key();
        }
        match self.method {
            AuthMethod::Password => format!("local:{}", self.subject),
            AuthMethod::Oidc => format!(
                "oidc:{}:{}",
                self.issuer.as_deref().unwrap_or(""),
                self.subject
            ),
            AuthMethod::Development => format!("user:{}", self.subject),
        }
    }

    /// Actor string recorded on new audit events and ops. Uses the stable
    /// internal `uid:<uuid>` when resolved; historical audit bytes are never
    /// rewritten.
    #[must_use]
    pub fn audit_actor(&self) -> String {
        if let Some(uid) = self.user_id {
            uid.member_key()
        } else {
            self.subject.to_string()
        }
    }

    /// Reads the principal the authentication layer attached.
    ///
    /// # Errors
    ///
    /// `UNAUTHENTICATED` if none is attached.
    pub fn of<T>(request: &Request<T>) -> Result<&Principal, Status> {
        request
            .extensions()
            .get::<Principal>()
            .ok_or_else(|| Status::unauthenticated("no credentials"))
    }
}

/// Metadata key the development authenticator reads.
pub const DEV_USER_HEADER: &str = "x-aegis-dev-user";
/// Metadata key that makes a development user an admin when set to `1`.
pub const DEV_ADMIN_HEADER: &str = "x-aegis-dev-admin";

/// Longest development user name, in bytes.
const MAX_DEV_USER_BYTES: usize = 128;

/// A tonic interceptor that trusts the `x-aegis-dev-user` header.
///
/// **Development and tests only.** It performs no authentication at all; the
/// binary enables it only with an explicit flag and logs a warning.
///
/// # Errors
///
/// `UNAUTHENTICATED` if the header is missing, empty, or too long.
pub fn dev_interceptor(mut request: Request<()>) -> Result<Request<()>, Status> {
    let user = request
        .metadata()
        .get(DEV_USER_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty() && v.len() <= MAX_DEV_USER_BYTES)
        .map(str::to_owned)
        .ok_or_else(|| Status::unauthenticated("missing development user"))?;
    let admin = request
        .metadata()
        .get(DEV_ADMIN_HEADER)
        .is_some_and(|v| v.as_bytes() == b"1");
    request.extensions_mut().insert(Principal {
        subject: user.into(),
        issuer: None,
        user_id: None,
        admin,
        method: AuthMethod::Development,
        groups: Vec::new(),
    });
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_interceptor_attaches_a_principal() {
        let mut request = Request::new(());
        request
            .metadata_mut()
            .insert(DEV_USER_HEADER, "alice".parse().expect("ascii"));
        let request = dev_interceptor(request).expect("accepted");
        assert_eq!(
            Principal::of(&request).expect("attached"),
            &Principal::user("alice")
        );
    }

    #[test]
    fn dev_interceptor_reads_the_admin_flag() {
        let mut request = Request::new(());
        request
            .metadata_mut()
            .insert(DEV_USER_HEADER, "root".parse().expect("ascii"));
        request
            .metadata_mut()
            .insert(DEV_ADMIN_HEADER, "1".parse().expect("ascii"));
        let request = dev_interceptor(request).expect("accepted");
        assert!(Principal::of(&request).expect("attached").admin);
    }

    #[test]
    fn dev_interceptor_rejects_missing_and_oversized_users() {
        let missing = dev_interceptor(Request::new(()));
        assert_eq!(
            missing.err().map(|s| s.code()),
            Some(tonic::Code::Unauthenticated)
        );
        let mut long = Request::new(());
        long.metadata_mut().insert(
            DEV_USER_HEADER,
            "x".repeat(MAX_DEV_USER_BYTES + 1).parse().expect("ascii"),
        );
        assert!(dev_interceptor(long).is_err());
    }

    #[test]
    fn principal_of_requires_an_attached_principal() {
        let request = Request::new(());
        assert_eq!(
            Principal::of(&request).err().map(|s| s.code()),
            Some(tonic::Code::Unauthenticated)
        );
    }
}
