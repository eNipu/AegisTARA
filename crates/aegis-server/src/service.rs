//! The gRPC service: requests in, actor calls out.
//!
//! This layer holds no project state. Each handler does the same four
//! things: read the caller's [`Principal`], parse and bound the request
//! (Q4), call the project's actor through the [`Registry`], and map the
//! result or [`ServerError`] to a response or status.

use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use crate::accounts::service::{no_event_log, password_login_off, user_info};

/// Events returned when the request names no limit.
const DEFAULT_EVENTS: usize = 200;
use crate::accounts::{Accounts, AuditKind, EventLog, UserUpdate, bearer};
use crate::actor::{Accepted, Event, OpenData, RiskReport, Subscription};
use crate::auth::{AuthMethod, Principal};
use crate::error::ServerError;
use crate::ids::ProjectId;
use crate::lease::Grant;
use crate::license::LicenseGate;
use crate::pb;
use crate::registry::{Registry, missing};
use crate::store::{BaselineMeta, ProjectMeta, ProjectStore};

/// Largest gRPC message the server accepts. A `SubmitOps` batch has at most
/// 1024 ops (`max_batch_ops`); typical ops are a few hundred bytes, and this
/// cap bounds the whole request whatever the op sizes.
pub const MAX_REQUEST_BYTES: usize = 17 * 1024 * 1024;

/// Longest lease token the server reads, in bytes.
const MAX_TOKEN_BYTES: usize = 64;

/// Longest export kind recorded, in bytes.
const MAX_EXPORT_KIND_BYTES: usize = 64;

/// The tonic service over a [`Registry`].
pub struct AegisService<S> {
    registry: Arc<Registry<S>>,
    /// Password accounts, when the server runs with `AEGIS_AUTH=password`.
    accounts: Option<Arc<Accounts>>,
    /// The event log. Present with password accounts and, for company login
    /// or development servers, through [`with_event_log`](Self::with_event_log).
    events: Option<EventLog>,
}

impl<S: ProjectStore> AegisService<S> {
    /// Wraps a registry.
    #[must_use]
    pub fn new(registry: Arc<Registry<S>>) -> Self {
        AegisService {
            registry,
            accounts: None,
            events: None,
        }
    }

    /// Adds password accounts: enables the user administration calls and
    /// `ChangePassword`, and puts logins and exports in their event log.
    #[must_use]
    pub fn with_accounts(mut self, accounts: Arc<Accounts>) -> Self {
        self.events = Some(accounts.event_log());
        self.accounts = Some(accounts);
        self
    }

    /// Gives a server without password accounts (company login only, or
    /// development login) an event log: exports, baselines and project
    /// deletions are recorded, and an administrator can read them.
    #[must_use]
    pub fn with_event_log(mut self, events: EventLog) -> Self {
        self.events = Some(events);
        self
    }

    /// The event log, if the server keeps one.
    #[must_use]
    pub fn event_log(&self) -> Option<&EventLog> {
        self.events.as_ref()
    }

    fn accounts(&self) -> Result<&Accounts, Status> {
        self.accounts.as_deref().ok_or_else(password_login_off)
    }

    fn admin_accounts<T>(&self, request: &Request<T>) -> Result<(&Accounts, Principal), Status> {
        let who = Principal::of(request)?.clone();
        if !who.admin {
            return Err(ServerError::NotAdmin.into());
        }
        Ok((self.accounts()?, who))
    }

    /// The switch as clients see it. Without password accounts (company
    /// login only) password login is simply off.
    fn server_settings(&self) -> pb::ServerSettings {
        match &self.accounts {
            Some(a) => pb::ServerSettings {
                password_login_enabled: a.password_login_enabled(),
                company_login_available: a.company_login_available(),
            },
            None => pb::ServerSettings {
                password_login_enabled: false,
                company_login_available: true,
            },
        }
    }

    async fn audit(&self, who: &Principal, kind: AuditKind, target: &str, detail: &str) {
        match &self.events {
            Some(log) => log.record(&who.subject, kind, target, detail).await,
            // Without an event log the event goes to the server log.
            None => tracing::info!(
                actor = %who.subject,
                kind = kind.as_str(),
                target,
                detail,
                "audit event"
            ),
        }
    }

    /// The service ready to mount on a tonic server, with the request size
    /// cap applied (R6).
    #[must_use]
    pub fn into_server(self) -> pb::aegis_server::AegisServer<Self> {
        pb::aegis_server::AegisServer::new(self).max_decoding_message_size(MAX_REQUEST_BYTES)
    }
}

fn project_id(text: &str) -> Result<ProjectId, ServerError> {
    text.parse().map_err(|_| ServerError::BadProjectId)
}

fn token(text: &str) -> Result<String, ServerError> {
    if text.is_empty() || text.len() > MAX_TOKEN_BYTES {
        return Err(ServerError::InvalidRequest("lock token"));
    }
    Ok(text.to_owned())
}

fn summary(meta: &ProjectMeta) -> pb::ProjectSummary {
    pb::ProjectSummary {
        project_id: meta.id.to_string(),
        name: meta.name.clone(),
        methodology_key: meta.methodology_key.clone(),
        methodology_version: meta.methodology_version.clone(),
        created_ms: meta.created_ms,
    }
}

fn lease(project: ProjectId, grant: &Grant) -> pb::Lease {
    pb::Lease {
        project_id: project.to_string(),
        token: grant.token.as_str().to_owned(),
        holder: grant.holder.to_string(),
        expires_in_ms: u64::try_from(grant.expires_in.as_millis()).unwrap_or(u64::MAX),
        last_seq: grant.last_seq,
    }
}

fn accepted(a: &Accepted) -> pb::AcceptedOp {
    pb::AcceptedOp {
        seq: a.seq,
        actor: a.actor.to_string(),
        timestamp_ms: a.timestamp_ms,
        op: a.op.clone(),
    }
}

fn watch_event(event: &Event) -> pb::WatchEvent {
    use pb::watch_event::Event as E;
    pb::WatchEvent {
        event: Some(match event {
            Event::Op(op) => E::Op(accepted(op)),
            Event::Lock(holder) => E::Lock(pb::LockEvent {
                holder: holder.to_string(),
            }),
            Event::Risks(report) => E::Risks(risks_event(report)),
        }),
    }
}

fn risk_entries(report: &RiskReport) -> Vec<pb::ThreatRiskEntry> {
    report
        .threats
        .iter()
        .map(|t| pb::ThreatRiskEntry {
            threat_id: t.threat.to_string(),
            inherent_risk: t.inherent.clone().unwrap_or_default(),
            residual_risk: t.residual.clone().unwrap_or_default(),
        })
        .collect()
}

fn risks_event(report: &RiskReport) -> pb::RisksEvent {
    pb::RisksEvent {
        as_of_seq: report.as_of_seq,
        threats: risk_entries(report),
    }
}

fn baseline_info(b: &BaselineMeta) -> pb::BaselineInfo {
    pb::BaselineInfo {
        baseline_id: b.id.to_string(),
        project_id: b.project.to_string(),
        name: b.name.clone(),
        seq: b.seq,
        created_by: b.created_by.clone(),
        created_ms: b.created_ms,
        snapshot_bytes: b.snapshot_bytes,
    }
}

/// The op kind of canonical op bytes (`{"op":"add_asset",...}`), in
/// `PascalCase` as the API reports it. Bounded by the op size (64 KiB).
fn op_kind(bytes: &[u8]) -> String {
    #[derive(serde::Deserialize)]
    struct Kind {
        op: String,
    }
    serde_json::from_slice::<Kind>(bytes).map_or_else(
        |_| "Unknown".to_owned(),
        |k| {
            k.op.split('_')
                .map(|w| {
                    let mut c = w.chars();
                    c.next()
                        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                        .unwrap_or_default()
                })
                .collect()
        },
    )
}

/// Splits a snapshot into chunks; the first carries the header. Slices share
/// the snapshot's buffer, so this copies nothing.
fn chunks(data: OpenData, chunk_bytes: usize) -> Vec<Result<pb::OpenProjectChunk, Status>> {
    let header = pb::OpenProjectHeader {
        project: Some(summary(&data.meta)),
        last_seq: data.last_seq,
        snapshot_bytes: data.snapshot.len() as u64,
        lock_holder: data.lock_holder.map(|h| h.to_string()).unwrap_or_default(),
    };
    chunk_snapshot(header, &data.snapshot, chunk_bytes)
}

fn chunk_snapshot(
    header: pb::OpenProjectHeader,
    snapshot: &Bytes,
    chunk_bytes: usize,
) -> Vec<Result<pb::OpenProjectChunk, Status>> {
    let chunk_bytes = chunk_bytes.max(1);
    let mut out = Vec::with_capacity(snapshot.len() / chunk_bytes + 1);
    let mut header = Some(header);
    let mut offset = 0;
    loop {
        let end = (offset + chunk_bytes).min(snapshot.len());
        out.push(Ok(pb::OpenProjectChunk {
            header: header.take(),
            data: snapshot.slice(offset..end),
        }));
        offset = end;
        if offset >= snapshot.len() {
            return out;
        }
    }
}

type BoxStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

/// Forwards a subscription to a client: the lock state, the backlog, then
/// live events, until the client leaves or falls behind (R4).
async fn forward(
    sub: Subscription,
    after: u64,
    heartbeat: std::time::Duration,
    out: mpsc::Sender<Result<pb::WatchEvent, Status>>,
) {
    let Subscription {
        backlog,
        mut live,
        lock_holder,
        risks,
    } = sub;
    let first = Event::Lock(lock_holder.unwrap_or_else(|| Arc::from("")));
    if out.send(Ok(watch_event(&first))).await.is_err() {
        return;
    }
    let mut last = after;
    for op in backlog {
        last = op.seq;
        if out.send(Ok(watch_event(&Event::Op(op)))).await.is_err() {
            return;
        }
    }
    // The risks as of now come after the backlog ops they include.
    if let Some(report) = risks
        && out
            .send(Ok(watch_event(&Event::Risks(report))))
            .await
            .is_err()
    {
        return;
    }
    loop {
        let Ok(received) = tokio::time::timeout(heartbeat, live.recv()).await else {
            let beat = pb::WatchEvent {
                event: Some(pb::watch_event::Event::Heartbeat(pb::Heartbeat {})),
            };
            if out.send(Ok(beat)).await.is_err() {
                return;
            }
            continue;
        };
        match received {
            Ok(event) => {
                match &event {
                    Event::Op(op) => {
                        if op.seq <= last {
                            continue;
                        }
                        last = op.seq;
                    }
                    // Numbers older than what the observer has seen are stale.
                    Event::Risks(report) if report.as_of_seq < last => continue,
                    Event::Risks(_) | Event::Lock(_) => {}
                }
                if out.send(Ok(watch_event(&event))).await.is_err() {
                    return;
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                let _ = out
                    .send(Err(ServerError::OutOfRange { after: last }.into()))
                    .await;
                return;
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

#[tonic::async_trait]
impl<S: ProjectStore> pb::aegis_server::Aegis for AegisService<S> {
    type OpenProjectStream = BoxStream<pb::OpenProjectChunk>;
    type WatchStream = BoxStream<pb::WatchEvent>;
    type GetBaselineStream = BoxStream<pb::OpenProjectChunk>;

    async fn get_server_info(
        &self,
        request: Request<pb::GetServerInfoRequest>,
    ) -> Result<Response<pb::GetServerInfoResponse>, Status> {
        Principal::of(&request)?;
        let methodologies = self
            .registry
            .methodologies()
            .iter()
            .map(|h| pb::MethodologyInfo {
                key: h.methodology.key().to_owned(),
                version: h.methodology.version().to_owned(),
                label: h.methodology.label().to_owned(),
            })
            .collect();
        Ok(Response::new(pb::GetServerInfoResponse {
            server_version: env!("CARGO_PKG_VERSION").to_owned(),
            engine_version: aegis_model::ENGINE_VERSION.to_owned(),
            methodologies,
            lease_ms: u64::try_from(self.registry.config().lease.as_millis()).unwrap_or(u64::MAX),
            license: self.registry.license().map(LicenseGate::info),
        }))
    }

    async fn get_methodology(
        &self,
        request: Request<pb::GetMethodologyRequest>,
    ) -> Result<Response<pb::GetMethodologyResponse>, Status> {
        Principal::of(&request)?;
        let key = &request.get_ref().key;
        let hosted = self
            .registry
            .methodologies()
            .get(key)
            .ok_or(ServerError::UnknownMethodology)?;
        Ok(Response::new(pb::GetMethodologyResponse {
            key: hosted.methodology.key().to_owned(),
            version: hosted.methodology.version().to_owned(),
            pack: Bytes::copy_from_slice(&hosted.pack),
        }))
    }

    async fn create_project(
        &self,
        request: Request<pb::CreateProjectRequest>,
    ) -> Result<Response<pb::CreateProjectResponse>, Status> {
        Principal::of(&request)?;
        let req = request.get_ref();
        let meta = self
            .registry
            .create(&req.name, &req.methodology_key)
            .await?;
        Ok(Response::new(pb::CreateProjectResponse {
            project: Some(summary(&meta)),
        }))
    }

    async fn list_projects(
        &self,
        request: Request<pb::ListProjectsRequest>,
    ) -> Result<Response<pb::ListProjectsResponse>, Status> {
        Principal::of(&request)?;
        let projects = self.registry.list().await?;
        Ok(Response::new(pb::ListProjectsResponse {
            projects: projects.iter().map(summary).collect(),
        }))
    }

    async fn open_project(
        &self,
        request: Request<pb::OpenProjectRequest>,
    ) -> Result<Response<Self::OpenProjectStream>, Status> {
        Principal::of(&request)?;
        let id = project_id(&request.get_ref().project_id)?;
        let data = self
            .registry
            .with(id, |h| async move { h.open().await })
            .await?;
        let items = chunks(data, self.registry.config().chunk_bytes);
        Ok(Response::new(Box::pin(tokio_stream::iter(items))))
    }

    async fn acquire_lock(
        &self,
        request: Request<pb::LockRequest>,
    ) -> Result<Response<pb::Lease>, Status> {
        let who = Principal::of(&request)?.clone();
        let id = project_id(&request.get_ref().project_id)?;
        // Taking the lease is what uses a seat. Reading is never gated.
        if let Some(gate) = self.registry.license() {
            gate.admit_editor(&who.subject)?;
        }
        let grant = self
            .registry
            .with(id, |h| {
                let who = who.clone();
                async move { h.acquire(who).await }
            })
            .await?;
        Ok(Response::new(lease(id, &grant)))
    }

    async fn renew_lock(
        &self,
        request: Request<pb::RenewLockRequest>,
    ) -> Result<Response<pb::Lease>, Status> {
        let who = Principal::of(&request)?.clone();
        let id = project_id(&request.get_ref().project_id)?;
        let tok = token(&request.get_ref().token)?;
        let grant = self
            .registry
            .with(id, |h| {
                let (who, tok) = (who.clone(), tok.clone());
                async move { h.renew(who, tok).await }
            })
            .await?;
        Ok(Response::new(lease(id, &grant)))
    }

    async fn release_lock(
        &self,
        request: Request<pb::ReleaseLockRequest>,
    ) -> Result<Response<pb::ReleaseLockResponse>, Status> {
        let who = Principal::of(&request)?.clone();
        let id = project_id(&request.get_ref().project_id)?;
        let tok = token(&request.get_ref().token)?;
        self.registry
            .with(id, |h| {
                let (who, tok) = (who.clone(), tok.clone());
                async move { h.release(who, tok).await }
            })
            .await?;
        Ok(Response::new(pb::ReleaseLockResponse {}))
    }

    async fn break_lock(
        &self,
        request: Request<pb::LockRequest>,
    ) -> Result<Response<pb::ReleaseLockResponse>, Status> {
        let who = Principal::of(&request)?.clone();
        if !who.admin {
            return Err(ServerError::NotAdmin.into());
        }
        let id = project_id(&request.get_ref().project_id)?;
        self.registry
            .with(id, |h| {
                let who = who.clone();
                async move { h.break_lock(who).await }
            })
            .await?;
        Ok(Response::new(pb::ReleaseLockResponse {}))
    }

    async fn submit_ops(
        &self,
        request: Request<pb::SubmitOpsRequest>,
    ) -> Result<Response<pb::SubmitOpsResponse>, Status> {
        let who = Principal::of(&request)?.clone();
        let req = request.into_inner();
        let id = project_id(&req.project_id)?;
        let tok = token(&req.lock_token)?;
        let (base_seq, ops) = (req.base_seq, req.ops);
        let last_seq = self
            .registry
            .with(id, |h| {
                let (who, tok, ops) = (who.clone(), tok.clone(), ops.clone());
                async move { h.submit(who, tok, base_seq, ops).await }
            })
            .await?;
        Ok(Response::new(pb::SubmitOpsResponse { last_seq }))
    }

    async fn watch(
        &self,
        request: Request<pb::WatchRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        Principal::of(&request)?;
        let id = project_id(&request.get_ref().project_id)?;
        let after = request.get_ref().after_seq;
        let sub = self
            .registry
            .with(id, |h| async move { h.subscribe(after).await })
            .await?;
        let (tx, rx) = mpsc::channel(self.registry.config().observer_buffer.max(1));
        tokio::spawn(forward(sub, after, self.registry.config().heartbeat, tx));
        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    async fn verify_audit(
        &self,
        request: Request<pb::VerifyAuditRequest>,
    ) -> Result<Response<pb::VerifyAuditResponse>, Status> {
        if !Principal::of(&request)?.admin {
            return Err(ServerError::NotAdmin.into());
        }
        let id = project_id(&request.get_ref().project_id)?;
        let report = self.registry.verify_audit(id).await?;
        Ok(Response::new(pb::VerifyAuditResponse {
            intact: report.is_intact(),
            verified_ops: report.verified,
            first_bad_seq: report.first_bad.unwrap_or(0),
            head_hash: Bytes::copy_from_slice(&report.head),
        }))
    }

    async fn get_risks(
        &self,
        request: Request<pb::GetRisksRequest>,
    ) -> Result<Response<pb::GetRisksResponse>, Status> {
        Principal::of(&request)?;
        let id = project_id(&request.get_ref().project_id)?;
        let report = self
            .registry
            .with(id, |h| async move { h.risks().await })
            .await?;
        Ok(Response::new(pb::GetRisksResponse {
            as_of_seq: report.as_of_seq,
            threats: risk_entries(&report),
        }))
    }

    async fn who_am_i(
        &self,
        request: Request<pb::WhoAmIRequest>,
    ) -> Result<Response<pb::WhoAmIResponse>, Status> {
        let who = Principal::of(&request)?;
        let display_name = match (who.method, &self.accounts) {
            (AuthMethod::Password, Some(a)) => a.display_name(&who.subject),
            _ => None,
        }
        .unwrap_or_else(|| who.subject.to_string());
        Ok(Response::new(pb::WhoAmIResponse {
            subject: who.subject.to_string(),
            display_name,
            admin: who.admin,
            method: who.method.as_str().to_owned(),
        }))
    }

    async fn log_out(
        &self,
        request: Request<pb::LogOutRequest>,
    ) -> Result<Response<pb::LogOutResponse>, Status> {
        let who = Principal::of(&request)?;
        let headers = request.metadata().clone().into_headers();
        if who.method == AuthMethod::Password
            && let Some(accounts) = &self.accounts
            && let Some(token) = bearer(&headers)
        {
            accounts.log_out(token).await?;
        }
        Ok(Response::new(pb::LogOutResponse {}))
    }

    async fn change_password(
        &self,
        request: Request<pb::ChangePasswordRequest>,
    ) -> Result<Response<pb::ChangePasswordResponse>, Status> {
        let who = Principal::of(&request)?.clone();
        if who.method != AuthMethod::Password {
            return Err(password_login_off());
        }
        let accounts = self.accounts()?;
        let headers = request.metadata().clone().into_headers();
        let keep = bearer(&headers).map(str::to_owned);
        let req = request.into_inner();
        accounts
            .change_password(
                &who.subject,
                &req.current_password,
                &req.new_password,
                keep.as_deref(),
            )
            .await?;
        Ok(Response::new(pb::ChangePasswordResponse {}))
    }

    async fn get_server_settings(
        &self,
        request: Request<pb::GetServerSettingsRequest>,
    ) -> Result<Response<pb::ServerSettings>, Status> {
        Principal::of(&request)?;
        Ok(Response::new(self.server_settings()))
    }

    async fn set_password_login(
        &self,
        request: Request<pb::SetPasswordLoginRequest>,
    ) -> Result<Response<pb::ServerSettings>, Status> {
        let (accounts, who) = self.admin_accounts(&request)?;
        accounts
            .set_password_login(&who.subject, request.get_ref().enabled)
            .await?;
        Ok(Response::new(self.server_settings()))
    }

    async fn list_events(
        &self,
        request: Request<pb::ListEventsRequest>,
    ) -> Result<Response<pb::ListEventsResponse>, Status> {
        let who = Principal::of(&request)?.clone();
        if !who.admin {
            return Err(ServerError::NotAdmin.into());
        }
        let log = self.events.as_ref().ok_or_else(no_event_log)?;
        let limit = match request.get_ref().limit {
            0 => DEFAULT_EVENTS,
            n => usize::try_from(n).unwrap_or(usize::MAX),
        };
        let events = log
            .recent(limit)
            .await
            .map_err(crate::accounts::AccountError::from)?;
        Ok(Response::new(pb::ListEventsResponse {
            events: events
                .into_iter()
                .map(|e| pb::EventInfo {
                    at_ms: e.at_ms,
                    actor: e.actor,
                    kind: e.kind.as_str().to_owned(),
                    target: e.target,
                    detail: e.detail,
                })
                .collect(),
        }))
    }

    async fn list_users(
        &self,
        request: Request<pb::ListUsersRequest>,
    ) -> Result<Response<pb::ListUsersResponse>, Status> {
        let (accounts, _) = self.admin_accounts(&request)?;
        Ok(Response::new(pb::ListUsersResponse {
            users: accounts
                .list_users()
                .iter()
                .map(|(u, n)| user_info(u, *n))
                .collect(),
        }))
    }

    async fn create_user(
        &self,
        request: Request<pb::CreateUserRequest>,
    ) -> Result<Response<pb::UserInfo>, Status> {
        let (accounts, who) = self.admin_accounts(&request)?;
        let req = request.get_ref();
        let user = accounts
            .create_user(
                &who.subject,
                &req.username,
                &req.display_name,
                &req.password,
                req.admin,
            )
            .await?;
        Ok(Response::new(user_info(&user, 0)))
    }

    async fn update_user(
        &self,
        request: Request<pb::UpdateUserRequest>,
    ) -> Result<Response<pb::UserInfo>, Status> {
        let (accounts, who) = self.admin_accounts(&request)?;
        let req = request.into_inner();
        let change = UserUpdate {
            display_name: req.display_name,
            admin: req.admin,
            disabled: req.disabled,
            password: req.password,
        };
        let user = accounts
            .update_user(&who.subject, &req.username, change)
            .await?;
        let sessions = accounts.user(&user.username).map_or(0, |(_, n)| n);
        Ok(Response::new(user_info(&user, sessions)))
    }

    async fn delete_user(
        &self,
        request: Request<pb::DeleteUserRequest>,
    ) -> Result<Response<pb::DeleteUserResponse>, Status> {
        let (accounts, who) = self.admin_accounts(&request)?;
        if crate::accounts::normalize_username(&request.get_ref().username)
            .is_ok_and(|u| *u == *who.subject)
        {
            return Err(Status::failed_precondition(
                "you cannot delete your own account",
            ));
        }
        accounts
            .delete_user(&who.subject, &request.get_ref().username)
            .await?;
        Ok(Response::new(pb::DeleteUserResponse {}))
    }

    async fn create_baseline(
        &self,
        request: Request<pb::CreateBaselineRequest>,
    ) -> Result<Response<pb::BaselineInfo>, Status> {
        let who = Principal::of(&request)?.clone();
        let req = request.get_ref();
        let id = project_id(&req.project_id)?;
        let meta = self
            .registry
            .create_baseline(id, &req.name, &who.subject)
            .await?;
        self.audit(
            &who,
            AuditKind::BaselineCreated,
            &id.to_string(),
            &meta.id.to_string(),
        )
        .await;
        Ok(Response::new(baseline_info(&meta)))
    }

    async fn list_baselines(
        &self,
        request: Request<pb::ListBaselinesRequest>,
    ) -> Result<Response<pb::ListBaselinesResponse>, Status> {
        Principal::of(&request)?;
        let id = project_id(&request.get_ref().project_id)?;
        let list = self
            .registry
            .store()
            .list_baselines(id)
            .await
            .map_err(|e| missing(e, id))?;
        Ok(Response::new(pb::ListBaselinesResponse {
            baselines: list.iter().map(baseline_info).collect(),
        }))
    }

    async fn get_baseline(
        &self,
        request: Request<pb::GetBaselineRequest>,
    ) -> Result<Response<Self::GetBaselineStream>, Status> {
        Principal::of(&request)?;
        let req = request.get_ref();
        let id = project_id(&req.project_id)?;
        let baseline: uuid::Uuid = req
            .baseline_id
            .parse()
            .map_err(|_| ServerError::InvalidRequest("baseline id"))?;
        let (meta, snapshot) = self
            .registry
            .store()
            .load_baseline(id, baseline)
            .await
            .map_err(|e| missing(e, id))?
            .ok_or(ServerError::InvalidRequest("unknown baseline"))?;
        let header = pb::OpenProjectHeader {
            project: None,
            last_seq: meta.seq,
            snapshot_bytes: snapshot.len() as u64,
            lock_holder: String::new(),
        };
        let items = chunk_snapshot(
            header,
            &Bytes::from(snapshot),
            self.registry.config().chunk_bytes,
        );
        Ok(Response::new(Box::pin(tokio_stream::iter(items))))
    }

    async fn rename_project(
        &self,
        request: Request<pb::RenameProjectRequest>,
    ) -> Result<Response<pb::ProjectSummary>, Status> {
        let who = Principal::of(&request)?.clone();
        let req = request.get_ref();
        let id = project_id(&req.project_id)?;
        let tok = token(&req.lock_token)?;
        let name = self.registry.check_name(&req.name)?.to_owned();
        let meta = self
            .registry
            .with(id, |h| {
                let (who, tok, name) = (who.clone(), tok.clone(), name.clone());
                async move { h.rename(who, tok, name).await }
            })
            .await?;
        Ok(Response::new(summary(&meta)))
    }

    async fn delete_project(
        &self,
        request: Request<pb::DeleteProjectRequest>,
    ) -> Result<Response<pb::DeleteProjectResponse>, Status> {
        let who = Principal::of(&request)?.clone();
        if !who.admin {
            return Err(ServerError::NotAdmin.into());
        }
        let id = project_id(&request.get_ref().project_id)?;
        self.registry.delete(id).await?;
        self.audit(&who, AuditKind::ProjectDeleted, &id.to_string(), "")
            .await;
        Ok(Response::new(pb::DeleteProjectResponse {}))
    }

    async fn list_history(
        &self,
        request: Request<pb::ListHistoryRequest>,
    ) -> Result<Response<pb::ListHistoryResponse>, Status> {
        Principal::of(&request)?;
        let req = request.get_ref();
        let id = project_id(&req.project_id)?;
        let max = self.registry.config().max_history;
        let limit = match req.limit {
            0 => max,
            n => (n as usize).min(max),
        };
        let ops = self
            .registry
            .store()
            .recent_ops(id, limit)
            .await
            .map_err(|e| missing(e, id))?;
        Ok(Response::new(pb::ListHistoryResponse {
            entries: ops
                .iter()
                .map(|o| pb::HistoryEntry {
                    seq: o.seq,
                    actor: o.actor.clone(),
                    timestamp_ms: o.timestamp_ms,
                    kind: op_kind(&o.op),
                })
                .collect(),
        }))
    }

    async fn record_export(
        &self,
        request: Request<pb::RecordExportRequest>,
    ) -> Result<Response<pb::RecordExportResponse>, Status> {
        let who = Principal::of(&request)?.clone();
        let req = request.get_ref();
        let id = project_id(&req.project_id)?;
        let kind = req.kind.as_str();
        if kind.is_empty()
            || kind.len() > MAX_EXPORT_KIND_BYTES
            || !kind.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(ServerError::InvalidRequest("export kind").into());
        }
        self.audit(&who, AuditKind::Export, &id.to_string(), kind)
            .await;
        Ok(Response::new(pb::RecordExportResponse {}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op_event(seq: u64) -> Event {
        Event::Op(Arc::new(Accepted {
            seq,
            actor: Arc::from("a"),
            timestamp_ms: 0,
            op: Bytes::new(),
        }))
    }

    async fn drain(
        mut rx: mpsc::Receiver<Result<pb::WatchEvent, Status>>,
    ) -> Vec<Result<pb::WatchEvent, Status>> {
        let mut out = Vec::new();
        while let Some(item) = rx.recv().await {
            out.push(item);
        }
        out
    }

    fn seq_of(item: &Result<pb::WatchEvent, Status>) -> Option<u64> {
        match item {
            Ok(pb::WatchEvent {
                event: Some(pb::watch_event::Event::Op(op)),
            }) => Some(op.seq),
            _ => None,
        }
    }

    #[tokio::test]
    async fn forward_sends_lock_state_backlog_then_live_without_repeats() {
        let (tx, live) = broadcast::channel(8);
        let sub = Subscription {
            backlog: vec![match op_event(3) {
                Event::Op(a) => a,
                Event::Lock(_) | Event::Risks(_) => unreachable!(),
            }],
            live,
            lock_holder: Some(Arc::from("alice")),
            risks: None,
        };
        // A live event that the backlog already covers is skipped.
        tx.send(op_event(3)).expect("receiver exists");
        tx.send(op_event(4)).expect("receiver exists");
        drop(tx);
        let (out, rx) = mpsc::channel(16);
        forward(sub, 2, std::time::Duration::from_secs(60), out).await;
        let items = drain(rx).await;
        assert!(matches!(
            &items[0],
            Ok(pb::WatchEvent { event: Some(pb::watch_event::Event::Lock(l)) }) if l.holder == "alice"
        ));
        let seqs: Vec<_> = items.iter().filter_map(seq_of).collect();
        assert_eq!(seqs, vec![3, 4]);
    }

    #[tokio::test]
    async fn forward_tells_a_lagging_observer_to_resync() {
        let (tx, live) = broadcast::channel(2);
        let sub = Subscription {
            backlog: vec![],
            live,
            lock_holder: None,
            risks: None,
        };
        // Ten events into a buffer of two before the observer reads any.
        for seq in 1..=10 {
            tx.send(op_event(seq)).expect("receiver exists");
        }
        let (out, rx) = mpsc::channel(16);
        forward(sub, 0, std::time::Duration::from_secs(60), out).await;
        let items = drain(rx).await;
        let last = items.last().expect("something was sent");
        let status = last.as_ref().expect_err("ends with an error");
        assert_eq!(status.code(), tonic::Code::OutOfRange);
    }

    #[tokio::test]
    async fn forward_stops_when_the_client_leaves() {
        let (tx, live) = broadcast::channel(8);
        let sub = Subscription {
            backlog: vec![],
            live,
            lock_holder: None,
            risks: None,
        };
        let (out, rx) = mpsc::channel(1);
        drop(rx);
        tx.send(op_event(1)).expect("receiver exists");
        // Returns instead of waiting for more events.
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            forward(sub, 0, std::time::Duration::from_secs(60), out),
        )
        .await
        .expect("forward ends");
    }

    #[test]
    fn chunks_cover_the_snapshot_and_put_the_header_first() {
        let meta = ProjectMeta {
            id: ProjectId::new_v7(),
            name: "p".into(),
            methodology_key: "k".into(),
            methodology_version: "1".into(),
            created_ms: 0,
        };
        for len in [0usize, 1, 9, 10, 11, 100] {
            let data = OpenData {
                meta: meta.clone(),
                last_seq: 7,
                snapshot: Bytes::from(vec![b'x'; len]),
                lock_holder: None,
            };
            let items = chunks(data, 10);
            let parts: Vec<_> = items.into_iter().map(|c| c.expect("ok")).collect();
            assert!(parts[0].header.as_ref().is_some_and(|h| h.last_seq == 7));
            assert!(parts[1..].iter().all(|c| c.header.is_none()));
            assert_eq!(parts.iter().map(|c| c.data.len()).sum::<usize>(), len);
            assert!(parts.iter().all(|c| c.data.len() <= 10));
        }
    }

    #[test]
    fn tokens_and_ids_are_bounded() {
        assert!(token("").is_err());
        assert!(token(&"x".repeat(MAX_TOKEN_BYTES + 1)).is_err());
        assert!(token("abc").is_ok());
        assert!(project_id("zzz").is_err());
    }
}
