//! The gRPC calls the client makes, with typed results.
//!
//! [`Connection`] is a thin, cloneable wrapper over the generated client. It
//! adds what every call needs: signing (the credential provider), error
//! mapping to [`ClientError`], and the size caps of rule R3 on what comes
//! back (a snapshot is at most `MAX_PROJECT_FILE_BYTES`). It speaks gRPC or
//! gRPC-Web ([`Transport`]).
//!
//! [`Public`] calls the unauthenticated `Accounts` service: how to log in,
//! and the password login itself.

use std::sync::Arc;
use std::time::Duration;

use aegis_auth::layer::{CredentialProvider, SignRequests, Signed};
use aegis_model::{MAX_PROJECT_FILE_BYTES, Methodology};
use aegis_proto::accounts_client::AccountsClient;
use aegis_proto::aegis_client::AegisClient;
use aegis_proto::{self as pb};
use bytes::Bytes;
use tower::Layer;

use crate::error::ClientError;
use crate::transport::{Transport, TransportKind};

/// Largest answer the client reads (R3): a full project snapshot chunk or
/// list, with room to spare.
const MAX_ANSWER_BYTES: usize = 32 * 1024 * 1024;

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
    /// The server's license summary, if it has one.
    pub license: Option<pb::LicenseInfo>,
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
    client: AegisClient<Signed<P, Transport>>,
}

impl<P> Clone for Connection<P> {
    fn clone(&self) -> Self {
        Connection {
            client: self.client.clone(),
        }
    }
}

impl<P: CredentialProvider> Connection<P> {
    /// Connects to `url` (for example `https://aegis.example.com`) with the
    /// default transport for it ([`TransportKind::auto`]). `url` is also what
    /// DPoP proofs are bound to, so it must be the address the server was
    /// configured with as its public URL.
    ///
    /// # Errors
    ///
    /// [`ClientError::Transport`] if the address is bad or unreachable.
    pub async fn connect(url: &str, provider: Arc<P>) -> Result<Self, ClientError> {
        Self::connect_with(url, TransportKind::auto(url), provider).await
    }

    /// Like [`connect`](Self::connect), with a chosen transport.
    ///
    /// # Errors
    ///
    /// [`ClientError::Transport`] if the address is bad or unreachable.
    pub async fn connect_with(
        url: &str,
        kind: TransportKind,
        provider: Arc<P>,
    ) -> Result<Self, ClientError> {
        let transport = Transport::connect(url, kind).await?;
        let signed = SignRequests::new(provider, url).layer(transport);
        Ok(Connection {
            client: AegisClient::new(signed).max_decoding_message_size(MAX_ANSWER_BYTES),
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
            license: r.license,
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
        let stream = self
            .client
            .clone()
            .open_project(pb::OpenProjectRequest {
                project_id: id.to_owned(),
            })
            .await?
            .into_inner();
        let (header, snapshot) = read_snapshot(stream).await?;
        Ok(Downloaded {
            summary: header
                .project
                .ok_or_else(|| ClientError::Protocol("no project in the header".into()))?,
            last_seq: header.last_seq,
            lock_holder: header.lock_holder,
            snapshot,
        })
    }

    /// Downloads a baseline's snapshot. Returns its sequence number and the
    /// `Project::save` bytes. Stops at [`MAX_PROJECT_FILE_BYTES`] (R3).
    ///
    /// # Errors
    ///
    /// [`ClientError::Protocol`] for a stream that breaks the contract.
    pub async fn baseline(
        &self,
        project: &str,
        baseline: &str,
    ) -> Result<(u64, Vec<u8>), ClientError> {
        let stream = self
            .client
            .clone()
            .get_baseline(pb::GetBaselineRequest {
                project_id: project.to_owned(),
                baseline_id: baseline.to_owned(),
            })
            .await?
            .into_inner();
        let (header, snapshot) = read_snapshot(stream).await?;
        Ok((header.last_seq, snapshot))
    }

    /// Lists a project's baselines, oldest first.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn baselines(&self, project: &str) -> Result<Vec<pb::BaselineInfo>, ClientError> {
        Ok(self
            .client
            .clone()
            .list_baselines(pb::ListBaselinesRequest {
                project_id: project.to_owned(),
            })
            .await?
            .into_inner()
            .baselines)
    }

    /// Freezes the project's current server state as a baseline.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn create_baseline(
        &self,
        project: &str,
        name: &str,
    ) -> Result<pb::BaselineInfo, ClientError> {
        Ok(self
            .client
            .clone()
            .create_baseline(pb::CreateBaselineRequest {
                project_id: project.to_owned(),
                name: name.to_owned(),
            })
            .await?
            .into_inner())
    }

    /// Renames a project (needs the lease).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn rename_project(
        &self,
        project: &str,
        token: &str,
        name: &str,
    ) -> Result<pb::ProjectSummary, ClientError> {
        Ok(self
            .client
            .clone()
            .rename_project(pb::RenameProjectRequest {
                project_id: project.to_owned(),
                lock_token: token.to_owned(),
                name: name.to_owned(),
            })
            .await?
            .into_inner())
    }

    /// Deletes a project (admin).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn delete_project(&self, project: &str) -> Result<(), ClientError> {
        self.client
            .clone()
            .delete_project(pb::DeleteProjectRequest {
                project_id: project.to_owned(),
            })
            .await?;
        Ok(())
    }

    /// The latest accepted changes of a project, newest first.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn history(
        &self,
        project: &str,
        limit: u32,
    ) -> Result<Vec<pb::HistoryEntry>, ClientError> {
        Ok(self
            .client
            .clone()
            .list_history(pb::ListHistoryRequest {
                project_id: project.to_owned(),
                limit,
            })
            .await?
            .into_inner()
            .entries)
    }

    /// Records an export in the server's event log, with an optional
    /// machine-readable profile summary (RPT-03).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn record_export(
        &self,
        project: &str,
        kind: &str,
        detail: &str,
    ) -> Result<(), ClientError> {
        self.client
            .clone()
            .record_export(pb::RecordExportRequest {
                project_id: project.to_owned(),
                kind: kind.to_owned(),
                detail: detail.to_owned(),
            })
            .await?;
        Ok(())
    }

    /// Breaks another user's edit lease (admin).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn break_lock(&self, project: &str) -> Result<(), ClientError> {
        self.client
            .clone()
            .break_lock(pb::LockRequest {
                project_id: project.to_owned(),
            })
            .await?;
        Ok(())
    }

    /// Checks a project's audit hash chain (admin).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn verify_audit(
        &self,
        project: &str,
    ) -> Result<pb::VerifyAuditResponse, ClientError> {
        Ok(self
            .client
            .clone()
            .verify_audit(pb::VerifyAuditRequest {
                project_id: project.to_owned(),
            })
            .await?
            .into_inner())
    }

    /// Who the server thinks this user is.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn who_am_i(&self) -> Result<pb::WhoAmIResponse, ClientError> {
        Ok(self
            .client
            .clone()
            .who_am_i(pb::WhoAmIRequest {})
            .await?
            .into_inner())
    }

    /// Ends the password session on the server.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn log_out(&self) -> Result<(), ClientError> {
        self.client.clone().log_out(pb::LogOutRequest {}).await?;
        Ok(())
    }

    /// Changes the user's own password.
    ///
    /// # Errors
    ///
    /// [`ServerCode::InvalidCredentials`](crate::error::ServerCode::InvalidCredentials)
    /// for a wrong current password, [`ServerCode::WeakPassword`](crate::error::ServerCode::WeakPassword).
    pub async fn change_password(&self, current: &str, new: &str) -> Result<(), ClientError> {
        self.client
            .clone()
            .change_password(pb::ChangePasswordRequest {
                current_password: current.to_owned(),
                new_password: new.to_owned(),
            })
            .await?;
        Ok(())
    }

    /// Every user (admin).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn users(&self) -> Result<Vec<pb::UserInfo>, ClientError> {
        Ok(self
            .client
            .clone()
            .list_users(pb::ListUsersRequest {})
            .await?
            .into_inner()
            .users)
    }

    /// Creates a user (admin).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn create_user(
        &self,
        request: pb::CreateUserRequest,
    ) -> Result<pb::UserInfo, ClientError> {
        Ok(self.client.clone().create_user(request).await?.into_inner())
    }

    /// Changes a user (admin).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn update_user(
        &self,
        request: pb::UpdateUserRequest,
    ) -> Result<pb::UserInfo, ClientError> {
        Ok(self.client.clone().update_user(request).await?.into_inner())
    }

    /// The newest events of the server's event log, newest first (admin).
    ///
    /// # Errors
    ///
    /// [`ServerCode::NotAdmin`](crate::error::ServerCode::NotAdmin), or any
    /// other [`ClientError`]; a server with no event log answers
    /// `ServerCode::Other("no_event_log")`.
    pub async fn events(&self, limit: u32) -> Result<Vec<pb::EventInfo>, ClientError> {
        Ok(self
            .client
            .clone()
            .list_events(pb::ListEventsRequest { limit })
            .await?
            .into_inner()
            .events)
    }

    /// The open sessions (admin, password login).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn sessions(&self) -> Result<Vec<pb::SessionInfo>, ClientError> {
        Ok(self
            .client
            .clone()
            .list_sessions(pb::ListSessionsRequest {})
            .await?
            .into_inner()
            .sessions)
    }

    /// Ends one session by its ID (admin).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`]; `unknown_session` if it ended already.
    pub async fn revoke_session(&self, id: &str) -> Result<(), ClientError> {
        self.client
            .clone()
            .revoke_session(pb::RevokeSessionRequest { id: id.to_owned() })
            .await?;
        Ok(())
    }

    /// The login switch.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn server_settings(&self) -> Result<pb::ServerSettings, ClientError> {
        Ok(self
            .client
            .clone()
            .get_server_settings(pb::GetServerSettingsRequest {})
            .await?
            .into_inner())
    }

    /// Turns password login on or off (admin).
    ///
    /// # Errors
    ///
    /// [`ServerCode::NoCompanyLogin`](crate::error::ServerCode::NoCompanyLogin)
    /// when `enabled` is false and the server has no company login, or any
    /// other [`ClientError`].
    pub async fn set_password_login(
        &self,
        enabled: bool,
    ) -> Result<pb::ServerSettings, ClientError> {
        Ok(self
            .client
            .clone()
            .set_password_login(pb::SetPasswordLoginRequest { enabled })
            .await?
            .into_inner())
    }

    /// Deletes a user (admin).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn delete_user(&self, username: &str) -> Result<(), ClientError> {
        self.client
            .clone()
            .delete_user(pb::DeleteUserRequest {
                username: username.to_owned(),
            })
            .await?;
        Ok(())
    }

    /// Uploads a PNG, JPEG, or sanitized static SVG figure to a project
    /// (requires the edit lease; job 073, ARC-01, RPT-06).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn put_figure(
        &self,
        project: &str,
        token: &str,
        caption: &str,
        target: &str,
        data: Vec<u8>,
    ) -> Result<pb::FigureInfo, ClientError> {
        let media_type = sniff_media_type(&data).to_owned();
        let chunks: Vec<Bytes> = data.chunks(64 * 1024).map(Bytes::copy_from_slice).collect();
        Ok(self
            .client
            .clone()
            .put_figure(pb::PutFigureRequest {
                project_id: project.to_owned(),
                lock_token: token.to_owned(),
                caption: caption.to_owned(),
                target: target.to_owned(),
                media_type,
                chunks,
            })
            .await?
            .into_inner())
    }

    /// Lists metadata for all figures attached to a project.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn list_figures(&self, project: &str) -> Result<Vec<pb::FigureInfo>, ClientError> {
        Ok(self
            .client
            .clone()
            .list_figures(pb::ListFiguresRequest {
                project_id: project.to_owned(),
            })
            .await?
            .into_inner()
            .figures)
    }

    /// Downloads a figure's metadata and raw bytes by streaming chunks. Stops
    /// at 5 MiB (`MAX_FIGURE_BYTES`, R3).
    ///
    /// # Errors
    ///
    /// [`ClientError::Protocol`] if the stream breaks the contract or exceeds
    /// 5 MiB.
    pub async fn get_figure(
        &self,
        project: &str,
        figure_id: &str,
    ) -> Result<(pb::FigureInfo, Vec<u8>), ClientError> {
        const MAX_FIGURE_BYTES: u64 = 5 * 1024 * 1024;
        let mut stream = self
            .client
            .clone()
            .get_figure(pb::GetFigureRequest {
                project_id: project.to_owned(),
                figure_id: figure_id.to_owned(),
            })
            .await?
            .into_inner();
        let mut info: Option<pb::FigureInfo> = None;
        let mut data = Vec::new();
        while let Some(chunk) = stream.message().await? {
            if let Some(i) = chunk.info {
                if info.is_some() {
                    return Err(ClientError::Protocol("second figure header".into()));
                }
                let promised = usize::try_from(i.byte_len.min(MAX_FIGURE_BYTES)).unwrap_or(0);
                data.reserve(promised);
                info = Some(i);
            } else if info.is_none() {
                return Err(ClientError::Protocol("figure data before header".into()));
            }
            if (data.len() + chunk.data.len()) as u64 > MAX_FIGURE_BYTES {
                return Err(ClientError::Protocol(
                    "figure is larger than the allowed maximum".into(),
                ));
            }
            data.extend_from_slice(&chunk.data);
        }
        let info = info.ok_or_else(|| ClientError::Protocol("empty figure stream".into()))?;
        if data.len() as u64 != info.byte_len {
            return Err(ClientError::Protocol(
                "figure stream is shorter than announced".into(),
            ));
        }
        Ok((info, data))
    }

    /// Updates a figure's caption and/or target (requires the edit lease).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn update_figure(
        &self,
        project: &str,
        token: &str,
        figure_id: &str,
        caption: &str,
        target: &str,
    ) -> Result<pb::FigureInfo, ClientError> {
        Ok(self
            .client
            .clone()
            .update_figure(pb::UpdateFigureRequest {
                project_id: project.to_owned(),
                lock_token: token.to_owned(),
                figure_id: figure_id.to_owned(),
                caption: caption.to_owned(),
                target: target.to_owned(),
            })
            .await?
            .into_inner())
    }

    /// Deletes a figure from a project (requires the edit lease).
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn delete_figure(
        &self,
        project: &str,
        token: &str,
        figure_id: &str,
    ) -> Result<(), ClientError> {
        self.client
            .clone()
            .delete_figure(pb::DeleteFigureRequest {
                project_id: project.to_owned(),
                lock_token: token.to_owned(),
                figure_id: figure_id.to_owned(),
            })
            .await?;
        Ok(())
    }
}

/// Reassembles a chunked snapshot stream: the header, then the bytes.
/// Stops at [`MAX_PROJECT_FILE_BYTES`] (R3).
async fn read_snapshot(
    mut stream: tonic::Streaming<pb::OpenProjectChunk>,
) -> Result<(pb::OpenProjectHeader, Vec<u8>), ClientError> {
    {
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
        Ok((header, snapshot))
    }
}

fn sniff_media_type(data: &[u8]) -> &'static str {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if data.starts_with(b"\xff\xd8") {
        "image/jpeg"
    } else {
        "image/svg+xml"
    }
}

impl<P: CredentialProvider> Connection<P> {
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

    /// The project's newest sequence number and lock holder.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`].
    pub async fn status(&self, project: &str) -> Result<pb::ProjectStatus, ClientError> {
        Ok(self
            .client
            .clone()
            .get_project_status(pb::GetProjectStatusRequest {
                project_id: project.to_owned(),
            })
            .await?
            .into_inner())
    }

    /// The server's official risks, to check this engine's numbers against
    /// or to show when the engines differ.
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

/// The unauthenticated part of the API: how to log in, and logging in.
#[derive(Clone)]
pub struct Public {
    client: AccountsClient<Transport>,
}

/// How a server wants its users to log in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginMethod {
    /// User name and password.
    Password,
    /// Company single sign-on.
    Oidc {
        /// Issuer URL.
        issuer: String,
        /// The desktop app's client ID.
        client_id: String,
        /// The API resource, if the provider needs one.
        resource: Option<String>,
    },
    /// Development header login.
    Development,
}

/// The answer of `GetLoginOptions`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginOptions {
    /// The method.
    pub method: LoginMethod,
    /// Shown on the login screen.
    pub server_name: String,
    /// A user name and password are accepted too. For a company login this
    /// follows the admin setting (job 045); the app offers the password form
    /// only when it is set.
    pub password_enabled: bool,
}

/// A password session as the server opened it.
#[derive(Clone, Debug)]
pub struct PasswordSession {
    /// The bearer token. Secret.
    pub token: String,
    /// How long it lasts.
    pub expires_in: Duration,
    /// Who logged in.
    pub user: pb::UserInfo,
}

impl Public {
    /// Connects to `url` with `kind`.
    ///
    /// # Errors
    ///
    /// [`ClientError::Transport`].
    pub async fn connect(url: &str, kind: TransportKind) -> Result<Self, ClientError> {
        let transport = Transport::connect(url, kind).await?;
        Ok(Public {
            client: AccountsClient::new(transport).max_decoding_message_size(64 * 1024),
        })
    }

    /// How to log in to this server.
    ///
    /// # Errors
    ///
    /// Any [`ClientError`]; an old server without the `Accounts` service
    /// answers `UNIMPLEMENTED`.
    pub async fn login_options(&self) -> Result<LoginOptions, ClientError> {
        use pb::get_login_options_response::Method;
        let r = self
            .client
            .clone()
            .get_login_options(pb::GetLoginOptionsRequest {})
            .await?
            .into_inner();
        let method = match r.method() {
            Method::Password => LoginMethod::Password,
            Method::Oidc => LoginMethod::Oidc {
                issuer: r.oidc_issuer.clone(),
                client_id: r.oidc_client_id.clone(),
                resource: Some(r.oidc_resource.clone()).filter(|s| !s.is_empty()),
            },
            Method::Development => LoginMethod::Development,
            Method::Unspecified => {
                return Err(ClientError::Protocol(
                    "the server names no login method".into(),
                ));
            }
        };
        Ok(LoginOptions {
            method,
            server_name: r.server_name,
            password_enabled: r.password_enabled,
        })
    }

    /// Logs in with a user name and password.
    ///
    /// # Errors
    ///
    /// [`ServerCode::InvalidCredentials`](crate::error::ServerCode::InvalidCredentials),
    /// [`ServerCode::Throttled`](crate::error::ServerCode::Throttled), or any
    /// other [`ClientError`].
    pub async fn log_in(
        &self,
        username: &str,
        password: &str,
        client_name: &str,
    ) -> Result<PasswordSession, ClientError> {
        let r = self
            .client
            .clone()
            .log_in(pb::LogInRequest {
                username: username.to_owned(),
                password: password.to_owned(),
                client_name: client_name.to_owned(),
            })
            .await?
            .into_inner();
        if !r.session_token.starts_with("ats_") || r.session_token.len() > 128 {
            return Err(ClientError::Protocol(
                "the session token is malformed".into(),
            ));
        }
        Ok(PasswordSession {
            token: r.session_token,
            expires_in: Duration::from_millis(r.expires_in_ms),
            user: r
                .user
                .ok_or_else(|| ClientError::Protocol("no user in the answer".into()))?,
        })
    }
}

/// Password login credentials: sends the session token as a bearer token.
/// The token lives in memory here; the shell may also keep it in the OS
/// keychain so the next start needs no password.
#[derive(Default)]
pub struct PasswordLogin {
    token: std::sync::RwLock<Option<String>>,
}

impl PasswordLogin {
    /// No session yet: every call answers "login required".
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Uses this session token from now on.
    pub fn set(&self, token: Option<String>) {
        *self
            .token
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = token;
    }

    /// Whether a session token is held.
    #[must_use]
    pub fn has_token(&self) -> bool {
        self.token
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }
}

impl CredentialProvider for PasswordLogin {
    async fn credentials(
        &self,
        _method: &str,
        _url: &str,
    ) -> Result<aegis_auth::Credentials, aegis_auth::AuthError> {
        let token = self
            .token
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or(aegis_auth::AuthError::LoginRequired)?;
        Ok(aegis_auth::Credentials {
            authorization: format!("Bearer {token}"),
            dpop: None,
            extra: Vec::new(),
        })
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
