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

use crate::actor::{Accepted, Event, OpenData, Subscription};
use crate::auth::Principal;
use crate::error::ServerError;
use crate::ids::ProjectId;
use crate::lease::Grant;
use crate::pb;
use crate::registry::Registry;
use crate::store::{ProjectMeta, ProjectStore};

/// Largest gRPC message the server accepts: a full `SubmitOps` batch
/// (256 ops of up to 64 KiB) plus framing.
pub const MAX_REQUEST_BYTES: usize = 17 * 1024 * 1024;

/// Longest lease token the server reads, in bytes.
const MAX_TOKEN_BYTES: usize = 64;

/// The tonic service over a [`Registry`].
pub struct AegisService<S> {
    registry: Arc<Registry<S>>,
}

impl<S: ProjectStore> AegisService<S> {
    /// Wraps a registry.
    #[must_use]
    pub fn new(registry: Arc<Registry<S>>) -> Self {
        AegisService { registry }
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
        }),
    }
}

/// Splits a snapshot into chunks; the first carries the header. Slices share
/// the snapshot's buffer, so this copies nothing.
fn chunks(data: OpenData, chunk_bytes: usize) -> Vec<Result<pb::OpenProjectChunk, Status>> {
    let chunk_bytes = chunk_bytes.max(1);
    let header = pb::OpenProjectHeader {
        project: Some(summary(&data.meta)),
        last_seq: data.last_seq,
        snapshot_bytes: data.snapshot.len() as u64,
        lock_holder: data.lock_holder.map(|h| h.to_string()).unwrap_or_default(),
    };
    let mut out = Vec::with_capacity(data.snapshot.len() / chunk_bytes + 1);
    let mut header = Some(header);
    let mut offset = 0;
    loop {
        let end = (offset + chunk_bytes).min(data.snapshot.len());
        out.push(Ok(pb::OpenProjectChunk {
            header: header.take(),
            data: data.snapshot.slice(offset..end),
        }));
        offset = end;
        if offset >= data.snapshot.len() {
            return out;
        }
    }
}

type BoxStream<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

/// Forwards a subscription to a client: the lock state, the backlog, then
/// live events, until the client leaves or falls behind (R4).
async fn forward(sub: Subscription, after: u64, out: mpsc::Sender<Result<pb::WatchEvent, Status>>) {
    let Subscription {
        backlog,
        mut live,
        lock_holder,
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
    loop {
        match live.recv().await {
            Ok(event) => {
                if let Event::Op(op) = &event {
                    if op.seq <= last {
                        continue;
                    }
                    last = op.seq;
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
        tokio::spawn(forward(sub, after, tx));
        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
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
            threats: report
                .threats
                .into_iter()
                .map(|t| pb::ThreatRiskEntry {
                    threat_id: t.threat.to_string(),
                    inherent_risk: t.inherent.unwrap_or_default(),
                    residual_risk: t.residual.unwrap_or_default(),
                })
                .collect(),
        }))
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
                Event::Lock(_) => unreachable!(),
            }],
            live,
            lock_holder: Some(Arc::from("alice")),
        };
        // A live event that the backlog already covers is skipped.
        tx.send(op_event(3)).expect("receiver exists");
        tx.send(op_event(4)).expect("receiver exists");
        drop(tx);
        let (out, rx) = mpsc::channel(16);
        forward(sub, 2, out).await;
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
        };
        // Ten events into a buffer of two before the observer reads any.
        for seq in 1..=10 {
            tx.send(op_event(seq)).expect("receiver exists");
        }
        let (out, rx) = mpsc::channel(16);
        forward(sub, 0, out).await;
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
        };
        let (out, rx) = mpsc::channel(1);
        drop(rx);
        tx.send(op_event(1)).expect("receiver exists");
        // Returns instead of waiting for more events.
        tokio::time::timeout(std::time::Duration::from_secs(5), forward(sub, 0, out))
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
