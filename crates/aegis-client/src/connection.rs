//! The gRPC calls the client makes, with typed results.
//!
//! [`Connection`] is a thin, cloneable wrapper over the generated client. It
//! adds what every call needs: signing (the credential provider), error
//! mapping to [`ClientError`], and the size caps of rule R3 on what comes
//! back (a snapshot is at most `MAX_PROJECT_FILE_BYTES`).

use std::sync::Arc;
use std::time::Duration;

use aegis_auth::layer::{CredentialProvider, SignRequests, Signed};
use aegis_model::{MAX_PROJECT_FILE_BYTES, Methodology};
use aegis_proto::aegis_client::AegisClient;
use aegis_proto::{self as pb};
use bytes::Bytes;
use tonic::transport::{Channel, Endpoint};
use tower::Layer;

use crate::error::ClientError;

/// Longest wait to establish a connection (R6).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest a unary call may take (R6). Streams are not limited by it.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// What the server said about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInfo {
    /// Server version.
    pub server_version: String,
    /// The engine version compiled into the server.
    pub engine_version: String,
    /// Hosted methodologies.
    pub methodologies: Vec<pb::MethodologyInfo>,
    /// Lease length.
    pub lease: Duration,
}

/// A project as downloaded.
#[derive(Debug)]
pub struct Downloaded {
    /// Facts about the project.
    pub summary: pb::ProjectSummary,
    /// The sequence number the snapshot is current as of.
    pub last_seq: u64,
    /// Who holds the lease now, if anyone.
    pub lock_holder: String,
    /// `Project::save` bytes.
    pub snapshot: Vec<u8>,
}

/// A connection to one server, signing every request.
pub struct Connection<P> {
    client: AegisClient<Signed<P, Channel>>,
}

impl<P> Clone for Connection<P> {
    fn clone(&self) -> Self {
        Connection {
            client: self.client.clone(),
        }
    }
}

impl<P: CredentialProvider> Connection<P> {
    /// Connects to `url` (for example `https://aegis.example.com`). `url` is
    /// also what DPoP proofs are bound to, so it must be the address the
    /// server was configured with as its public URL.
    ///
    /// # Errors
    ///
    /// [`ClientError::Transport`] if the address is bad or unreachable.
    pub async fn connect(url: &str, provider: Arc<P>) -> Result<Self, ClientError> {
        let channel = Endpoint::from_shared(url.to_owned())
            .map_err(|e| ClientError::Transport(e.to_string()))?
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(CALL_TIMEOUT)
            .tcp_nodelay(true)
            .http2_keep_alive_interval(Duration::from_secs(30))
            .keep_alive_while_idle(true)
            .connect()
            .await?;
        let signed = SignRequests::new(provider, url).layer(channel);
        Ok(Connection {
            client: AegisClient::new(signed).max_decoding_message_size(32 * 1024 * 1024),
        })
    }

    /// Asks who the server is.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn info(&self) -> Result<ServerInfo, ClientError> {
        let r = self
            .client
            .clone()
            .get_server_info(pb::GetServerInfoRequest {})
            .await?
            .into_inner();
        Ok(ServerInfo {
            server_version: r.server_version,
            engine_version: r.engine_version,
            methodologies: r.methodologies,
            lease: Duration::from_millis(r.lease_ms),
        })
    }

    /// The methodology the server hosts under `key`, read from its pack.
    ///
    /// # Errors
    ///
    /// [`ClientError::Protocol`] if the pack is invalid.
    pub async fn methodology(&self, key: &str) -> Result<Arc<Methodology>, ClientError> {
        let r = self
            .client
            .clone()
            .get_methodology(pb::GetMethodologyRequest {
                key: key.to_owned(),
            })
            .await?
            .into_inner();
        Methodology::read_pack(r.pack.as_ref())
            .map(Arc::new)
            .map_err(|e| ClientError::Protocol(format!("methodology pack: {e}")))
    }

    /// Lists projects.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn list_projects(&self) -> Result<Vec<pb::ProjectSummary>, ClientError> {
        Ok(self
            .client
            .clone()
            .list_projects(pb::ListProjectsRequest {})
            .await?
            .into_inner()
            .projects)
    }

    /// Creates an empty project.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn create_project(
        &self,
        name: &str,
        methodology_key: &str,
    ) -> Result<pb::ProjectSummary, ClientError> {
        self.client
            .clone()
            .create_project(pb::CreateProjectRequest {
                name: name.to_owned(),
                methodology_key: methodology_key.to_owned(),
            })
            .await?
            .into_inner()
            .project
            .ok_or_else(|| ClientError::Protocol("no project in the answer".into()))
    }

    /// Downloads a project snapshot, reassembling the chunks. Stops at
    /// [`MAX_PROJECT_FILE_BYTES`] (R3).
    ///
    /// # Errors
    ///
    /// [`ClientError::Protocol`] for a stream that breaks the contract.
    pub async fn open_project(&self, id: &str) -> Result<Downloaded, ClientError> {
        let mut stream = self
            .client
            .clone()
            .open_project(pb::OpenProjectRequest {
                project_id: id.to_owned(),
            })
            .await?
            .into_inner();
        let mut header: Option<pb::OpenProjectHeader> = None;
        let mut snapshot = Vec::new();
        while let Some(chunk) = stream.message().await? {
            if let Some(h) = chunk.header {
                if header.is_some() {
                    return Err(ClientError::Protocol("second header".into()));
                }
                // Reserve what the server promised, within the cap.
                let promised =
                    usize::try_from(h.snapshot_bytes.min(MAX_PROJECT_FILE_BYTES)).unwrap_or(0);
                snapshot.reserve(promised);
                header = Some(h);
            } else if header.is_none() {
                return Err(ClientError::Protocol("data before the header".into()));
            }
            if (snapshot.len() + chunk.data.len()) as u64 > MAX_PROJECT_FILE_BYTES {
                return Err(ClientError::Protocol(
                    "snapshot is larger than the allowed maximum".into(),
                ));
            }
            snapshot.extend_from_slice(&chunk.data);
        }
        let header = header.ok_or_else(|| ClientError::Protocol("empty stream".into()))?;
        if snapshot.len() as u64 != header.snapshot_bytes {
            return Err(ClientError::Protocol(
                "snapshot is shorter than announced".into(),
            ));
        }
        Ok(Downloaded {
            summary: header
                .project
                .ok_or_else(|| ClientError::Protocol("no project in the header".into()))?,
            last_seq: header.last_seq,
            lock_holder: header.lock_holder,
            snapshot,
        })
    }

    /// Takes the edit lease.
    ///
    /// # Errors
    ///
    /// [`ServerCode::LockHeld`](crate::error::ServerCode::LockHeld) if someone else has it.
    pub async fn acquire(&self, project: &str) -> Result<pb::Lease, ClientError> {
        Ok(self
            .client
            .clone()
            .acquire_lock(pb::LockRequest {
                project_id: project.to_owned(),
            })
            .await?
            .into_inner())
    }

    /// Renews the lease.
    ///
    /// # Errors
    ///
    /// [`ServerCode::LockInvalid`](crate::error::ServerCode::LockInvalid) if it is gone.
    pub async fn renew(&self, project: &str, token: &str) -> Result<pb::Lease, ClientError> {
        Ok(self
            .client
            .clone()
            .renew_lock(pb::RenewLockRequest {
                project_id: project.to_owned(),
                token: token.to_owned(),
            })
            .await?
            .into_inner())
    }

    /// Gives the lease up.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`]; callers usually ignore a failure here, because
    /// the lease expires on its own.
    pub async fn release(&self, project: &str, token: &str) -> Result<(), ClientError> {
        self.client
            .clone()
            .release_lock(pb::ReleaseLockRequest {
                project_id: project.to_owned(),
                token: token.to_owned(),
            })
            .await?;
        Ok(())
    }

    /// Submits a batch of encoded ops.
    ///
    /// # Errors
    ///
    /// The server's refusal, with its code.
    pub async fn submit(
        &self,
        project: &str,
        token: &str,
        base_seq: u64,
        ops: Vec<Bytes>,
    ) -> Result<u64, ClientError> {
        Ok(self
            .client
            .clone()
            .submit_ops(pb::SubmitOpsRequest {
                project_id: project.to_owned(),
                lock_token: token.to_owned(),
                base_seq,
                ops,
            })
            .await?
            .into_inner()
            .last_seq)
    }

    /// Starts watching a project from `after_seq`.
    ///
    /// # Errors
    ///
    /// [`ServerCode::OutOfRange`](crate::error::ServerCode::OutOfRange) if the
    /// server no longer holds those ops.
    pub async fn watch(
        &self,
        project: &str,
        after_seq: u64,
    ) -> Result<tonic::Streaming<pb::WatchEvent>, ClientError> {
        Ok(self
            .client
            .clone()
            .watch(pb::WatchRequest {
                project_id: project.to_owned(),
                after_seq,
            })
            .await?
            .into_inner())
    }

    /// The server's official risks (used when the engines differ).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn risks(&self, project: &str) -> Result<pb::GetRisksResponse, ClientError> {
        Ok(self
            .client
            .clone()
            .get_risks(pb::GetRisksRequest {
                project_id: project.to_owned(),
            })
            .await?
            .into_inner())
    }
}

/// The development login: sends a user name header that a development
/// server (`AEGIS_DEV_AUTH=1`) trusts. Never use it against a real server.
pub struct DevLogin {
    user: String,
    admin: bool,
}

impl DevLogin {
    /// A development identity.
    #[must_use]
    pub fn new(user: &str, admin: bool) -> Self {
        DevLogin {
            user: user.to_owned(),
            admin,
        }
    }
}

impl CredentialProvider for DevLogin {
    async fn credentials(
        &self,
        _method: &str,
        _url: &str,
    ) -> Result<aegis_auth::Credentials, aegis_auth::AuthError> {
        let mut extra = vec![("x-aegis-dev-user", self.user.clone())];
        if self.admin {
            extra.push(("x-aegis-dev-admin", "1".to_owned()));
        }
        Ok(aegis_auth::Credentials {
            authorization: String::new(),
            dpop: None,
            extra,
        })
    }
}
