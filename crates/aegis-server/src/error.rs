//! Server errors and their mapping to gRPC status codes.
//!
//! Messages never echo user text (R6 of the model's rules carries over):
//! they name IDs, positions, and counts only. Backend failures are logged
//! with detail on the server and reach the client as a bare `UNAVAILABLE`.

use aegis_model::{ModelError, StoreError};
use tonic::metadata::MetadataValue;
use tonic::{Code, Status};

use crate::ids::ProjectId;
use crate::store::StoreFault;

pub use aegis_proto::{CURRENT_SEQ_HEADER, ERROR_CODE_HEADER};

/// Everything the server can refuse or fail at.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// No such project.
    #[error("unknown project {0}")]
    UnknownProject(ProjectId),
    /// A project ID that does not parse.
    #[error("malformed project id")]
    BadProjectId,
    /// A request field is out of range or empty.
    #[error("invalid request: {0}")]
    InvalidRequest(&'static str),
    /// A methodology key the server does not host.
    #[error("unknown methodology")]
    UnknownMethodology,
    /// The caller is not an admin.
    #[error("admin only")]
    NotAdmin,
    /// The caller has no membership that grants this access on the project.
    #[error("not authorized for this project")]
    NotAuthorized,
    /// Another editor holds the lease.
    #[error("project is locked by {holder}")]
    LockHeld {
        /// Subject of the current holder.
        holder: String,
    },
    /// The project has no lease.
    #[error("project is not locked")]
    NotLocked,
    /// The token does not match the current lease, or the lease expired.
    #[error("lock token is not valid")]
    LockInvalid,
    /// The client built its ops on an older state.
    #[error("project is at sequence {current}, not {base}")]
    StaleBase {
        /// Sequence number the client sent.
        base: u64,
        /// The project's current sequence number.
        current: u64,
    },
    /// An op did not decode.
    #[error("op {index} is malformed: {source}")]
    MalformedOp {
        /// Position in the batch.
        index: usize,
        /// Why.
        source: StoreError,
    },
    /// An op decoded but the model refused it.
    #[error("op {index} was rejected: {source}")]
    RejectedOp {
        /// Position in the batch.
        index: usize,
        /// Why.
        source: ModelError,
    },
    /// The license is over (past its grace period): reading works, editing
    /// does not.
    #[error("the license has expired")]
    LicenseExpired,
    /// Every seat of the license is held by other users.
    #[error("all licensed seats are in use")]
    LicenseSeats,
    /// The project's queue or the server's project table is full.
    #[error("server is busy")]
    Busy,
    /// A per-project figure quota (count or total bytes) was reached.
    #[error("project figure limit reached: {0}")]
    QuotaExceeded(String),
    /// Storage failed or returned data the server cannot trust.
    #[error("storage failure")]
    Store(#[from] StoreFault),
    /// The project's actor stopped while the call was in flight. Retrying
    /// loads it again.
    #[error("project was unloaded; retry")]
    Unloaded,
    /// The server is shutting down and is no longer loading projects.
    #[error("server is shutting down")]
    ShuttingDown,
    /// The client's engine semantics or protocol version is incompatible with
    /// this server's write contract (ADR 0075).
    #[error("{0}")]
    IncompatibleClient(String),
    /// A bug or corrupted state; details are in the server log.
    #[error("internal error")]
    Internal,
}

impl ServerError {
    /// The stable code clients branch on.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            ServerError::UnknownProject(_) => "unknown_project",
            ServerError::BadProjectId => "bad_project_id",
            ServerError::InvalidRequest(_) => "invalid_request",
            ServerError::UnknownMethodology => "unknown_methodology",
            ServerError::NotAdmin => "not_admin",
            ServerError::NotAuthorized => "not_authorized",
            ServerError::LockHeld { .. } => "lock_held",
            ServerError::NotLocked => "not_locked",
            ServerError::LockInvalid => "lock_invalid",
            ServerError::StaleBase { .. } => "stale_base",
            ServerError::MalformedOp { .. } => "malformed_op",
            ServerError::RejectedOp { .. } => "rejected_op",
            ServerError::LicenseExpired => "license_expired",
            ServerError::LicenseSeats => "license_seats",
            ServerError::Busy => "busy",
            ServerError::QuotaExceeded(_) => "quota_exceeded",
            ServerError::Store(_) => "storage",
            ServerError::Unloaded => "unloaded",
            ServerError::ShuttingDown => "shutting_down",
            ServerError::IncompatibleClient(_) => "incompatible_client",
            ServerError::Internal => "internal",
        }
    }

    fn grpc_code(&self) -> Code {
        match self {
            ServerError::UnknownProject(_) => Code::NotFound,
            ServerError::BadProjectId
            | ServerError::InvalidRequest(_)
            | ServerError::UnknownMethodology
            | ServerError::MalformedOp { .. }
            | ServerError::RejectedOp { .. } => Code::InvalidArgument,
            ServerError::NotAdmin | ServerError::NotAuthorized => Code::PermissionDenied,
            ServerError::LockHeld { .. }
            | ServerError::NotLocked
            | ServerError::LockInvalid
            | ServerError::StaleBase { .. }
            | ServerError::LicenseExpired
            | ServerError::IncompatibleClient(_) => Code::FailedPrecondition,
            ServerError::Busy | ServerError::LicenseSeats | ServerError::QuotaExceeded(_) => {
                Code::ResourceExhausted
            }
            ServerError::Store(_) | ServerError::Unloaded | ServerError::ShuttingDown => {
                Code::Unavailable
            }
            ServerError::Internal => Code::Internal,
        }
    }
}

impl From<ServerError> for Status {
    fn from(e: ServerError) -> Status {
        let mut status = Status::new(e.grpc_code(), e.to_string());
        if let ServerError::Store(fault) = &e {
            tracing::error!(%fault, "storage failure");
        }
        let md = status.metadata_mut();
        md.insert(ERROR_CODE_HEADER, MetadataValue::from_static(e.code()));
        if let ServerError::StaleBase { current, .. } = &e
            && let Ok(v) = current.to_string().parse()
        {
            md.insert(CURRENT_SEQ_HEADER, v);
        }
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_carries_code_and_header() {
        let status: Status = ServerError::StaleBase {
            base: 3,
            current: 5,
        }
        .into();
        assert_eq!(status.code(), Code::FailedPrecondition);
        let code = status.metadata().get(ERROR_CODE_HEADER).expect("code");
        assert_eq!(code.as_bytes(), b"stale_base");
        let seq = status.metadata().get(CURRENT_SEQ_HEADER).expect("seq");
        assert_eq!(seq.as_bytes(), b"5");
    }

    #[test]
    fn storage_detail_does_not_reach_the_client() {
        let status: Status =
            ServerError::Store(StoreFault::Backend("password=hunter2".into())).into();
        assert_eq!(status.code(), Code::Unavailable);
        assert!(!status.message().contains("hunter2"));
    }
}
