//! Client errors.
//!
//! Server refusals keep their machine-readable code (the `x-aegis-error`
//! header), so the session can tell "you lost the lock" from "that edit is
//! invalid" without parsing messages. Messages are for people and never
//! contain secrets: they come from the server's fixed texts.

use aegis_auth::AuthError;
use aegis_auth::layer::auth_error_of;

/// The server's error codes the client acts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerCode {
    /// Another editor holds the lock.
    LockHeld,
    /// The project is not locked.
    NotLocked,
    /// The lock token is wrong or the lease expired.
    LockInvalid,
    /// The ops were built on an older state. `current` is the server's sequence.
    StaleBase {
        /// The server's current sequence number, if it said.
        current: Option<u64>,
    },
    /// An op did not decode.
    MalformedOp,
    /// The model refused an op.
    RejectedOp,
    /// The server is busy or full.
    Busy,
    /// The project does not exist.
    UnknownProject,
    /// The caller may not do this.
    NotAdmin,
    /// The server's license is over: no editing.
    LicenseExpired,
    /// Every seat of the license is taken by other users.
    LicenseSeatsFull,
    /// The server is temporarily unable (storage, unloading project).
    Unavailable,
    /// Password login: wrong user name or password.
    InvalidCredentials,
    /// Password login: too many failures; wait this many seconds.
    Throttled {
        /// Seconds to wait, if the server said.
        retry_after_s: Option<u64>,
    },
    /// A new password breaks a password rule (the message says which).
    WeakPassword,
    /// A user name with characters that are not allowed.
    InvalidUsername,
    /// No such user.
    UnknownUser,
    /// The user name is taken.
    UserExists,
    /// The last administrator cannot be removed.
    LastAdmin,
    /// The server does not use password login.
    PasswordLoginOff,
    /// Password login cannot be turned off: the server has no company login.
    NoCompanyLogin,
    /// A per-file or per-project quota (figures, bytes) was exceeded.
    QuotaExceeded,
    /// The client's engine semantics or protocol version is incompatible with
    /// the server's write contract (ADR 0075).
    IncompatibleClient,
    /// Anything else, with the raw code.
    Other(String),
}

/// What went wrong.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// Nobody is logged in, or the server did not accept the credentials.
    #[error("login required")]
    LoginRequired,
    /// The server refused or failed a call.
    #[error("{message}")]
    Server {
        /// What the client should do about it.
        code: ServerCode,
        /// A sentence for the user.
        message: String,
    },
    /// The network failed.
    #[error("network error: {0}")]
    Transport(String),
    /// The server's answer is not what the protocol says.
    #[error("unexpected answer from the server: {0}")]
    Protocol(String),
    /// The server's engine is not this client's engine.
    #[error("the server runs engine {server}, this client runs {client}")]
    EngineMismatch {
        /// The server's engine version.
        server: String,
        /// This client's engine version.
        client: String,
    },
}

impl ClientError {
    /// The server code, if this is a server refusal.
    #[must_use]
    pub fn code(&self) -> Option<&ServerCode> {
        match self {
            ClientError::Server { code, .. } => Some(code),
            _ => None,
        }
    }

    /// Whether retrying the same call later may succeed.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            ClientError::Transport(_)
                | ClientError::Server {
                    code: ServerCode::Busy | ServerCode::Unavailable,
                    ..
                }
        )
    }
}

impl From<tonic::Status> for ClientError {
    fn from(status: tonic::Status) -> Self {
        // A request that could not be signed never reached the server.
        if let Some(AuthError::LoginRequired) = auth_error_of(&status) {
            return ClientError::LoginRequired;
        }
        let header = |name: &str| {
            status
                .metadata()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let code_header = header(aegis_proto::ERROR_CODE_HEADER);
        // A wrong password is an answer to a login, not a request to log in.
        if status.code() == tonic::Code::Unauthenticated
            && code_header.as_deref() != Some("invalid_credentials")
        {
            return ClientError::LoginRequired;
        }
        let Some(code) = code_header else {
            return match status.code() {
                tonic::Code::Unavailable
                | tonic::Code::DeadlineExceeded
                | tonic::Code::Cancelled => ClientError::Transport(status.message().to_owned()),
                _ => ClientError::Server {
                    code: ServerCode::Other(status.code().to_string()),
                    message: status.message().to_owned(),
                },
            };
        };
        let current = header(aegis_proto::CURRENT_SEQ_HEADER).and_then(|v| v.parse().ok());
        let code = match code.as_str() {
            "lock_held" => ServerCode::LockHeld,
            "not_locked" => ServerCode::NotLocked,
            "lock_invalid" => ServerCode::LockInvalid,
            "stale_base" => ServerCode::StaleBase { current },
            "malformed_op" => ServerCode::MalformedOp,
            "rejected_op" => ServerCode::RejectedOp,
            "busy" => ServerCode::Busy,
            "unknown_project" => ServerCode::UnknownProject,
            "not_admin" => ServerCode::NotAdmin,
            "license_expired" => ServerCode::LicenseExpired,
            "license_seats" => ServerCode::LicenseSeatsFull,
            "storage" | "unloaded" => ServerCode::Unavailable,
            "invalid_credentials" => ServerCode::InvalidCredentials,
            "throttled" => ServerCode::Throttled {
                retry_after_s: header("x-aegis-retry-after").and_then(|v| v.parse().ok()),
            },
            "weak_password" => ServerCode::WeakPassword,
            "invalid_username" => ServerCode::InvalidUsername,
            "unknown_user" => ServerCode::UnknownUser,
            "user_exists" => ServerCode::UserExists,
            "last_admin" => ServerCode::LastAdmin,
            "password_login_off" => ServerCode::PasswordLoginOff,
            "no_company_login" => ServerCode::NoCompanyLogin,
            "quota_exceeded" => ServerCode::QuotaExceeded,
            "incompatible_client" => ServerCode::IncompatibleClient,
            other => ServerCode::Other(other.to_owned()),
        };
        ClientError::Server {
            code,
            message: status.message().to_owned(),
        }
    }
}

impl From<tonic::transport::Error> for ClientError {
    fn from(e: tonic::transport::Error) -> Self {
        ClientError::Transport(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use tonic::Status;
    use tonic::metadata::MetadataValue;

    use super::*;

    fn status(code: tonic::Code, server_code: &'static str) -> Status {
        let mut s = Status::new(code, "text");
        s.metadata_mut().insert(
            aegis_proto::ERROR_CODE_HEADER,
            MetadataValue::from_static(server_code),
        );
        s
    }

    #[test]
    fn server_codes_are_mapped() {
        let e = ClientError::from(status(tonic::Code::FailedPrecondition, "lock_invalid"));
        assert_eq!(e.code(), Some(&ServerCode::LockInvalid));
        let mut s = status(tonic::Code::FailedPrecondition, "stale_base");
        s.metadata_mut().insert(
            aegis_proto::CURRENT_SEQ_HEADER,
            MetadataValue::from_static("42"),
        );
        assert_eq!(
            ClientError::from(s).code(),
            Some(&ServerCode::StaleBase { current: Some(42) })
        );
        let e = ClientError::from(status(tonic::Code::Unavailable, "storage"));
        assert!(e.is_transient());
        let e = ClientError::from(status(tonic::Code::Internal, "something-new"));
        assert_eq!(e.code(), Some(&ServerCode::Other("something-new".into())));
    }

    #[test]
    fn a_wrong_password_is_not_a_login_request() {
        let e = ClientError::from(status(tonic::Code::Unauthenticated, "invalid_credentials"));
        assert_eq!(e.code(), Some(&ServerCode::InvalidCredentials));
        let mut s = status(tonic::Code::ResourceExhausted, "throttled");
        s.metadata_mut()
            .insert("x-aegis-retry-after", MetadataValue::from_static("8"));
        assert_eq!(
            ClientError::from(s).code(),
            Some(&ServerCode::Throttled {
                retry_after_s: Some(8)
            })
        );
    }

    #[test]
    fn unauthenticated_and_transport_failures_are_distinguished() {
        assert!(matches!(
            ClientError::from(Status::unauthenticated("no")),
            ClientError::LoginRequired
        ));
        let e = ClientError::from(Status::unavailable("connection reset"));
        assert!(matches!(e, ClientError::Transport(_)));
        assert!(e.is_transient());
    }
}
