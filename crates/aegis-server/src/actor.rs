//! One task per open project (rule C1).
//!
//! The actor owns the [`Project`], its [`ProjectEvalCache`] and the edit
//! lease. Everything else talks to it through a bounded mailbox (R4), so ops are applied one at a time in arrival order and no
//! lock protects the project.
//!
//! # Accepting ops
//!
//! `SubmitOps` is atomic. The actor checks the lease and the base sequence,
//! decodes every op (capped, R3), applies them in order, stores them, and
//! only then answers:
//!
//! 1. any op fails to apply: the already applied ops are undone with their
//!    inverses, nothing is stored;
//! 2. the store refuses the append: all ops are undone, so memory is never
//!    ahead of durable state.
//!
//! # CPU work
//!
//! Applying one op is O(size of the op) plus an incremental cache update
//! (rule A1), cheap enough to run on the actor task. Serializing the whole
//! project is O(project size), so it moves the project into
//! `spawn_blocking` and back (R5); the mailbox queues meanwhile.
//!
//! # Lifetime
//!
//! The actor unloads itself after `idle_unload` without a lease, writing a final snapshot first. A new request loads it again.

use std::sync::Arc;
use std::time::Duration;

use aegis_model::{OpCodec, Project, ProjectEvalCache, ThreatId};
use bytes::Bytes;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;

use crate::auth::Principal;
use crate::config::ServerConfig;
use crate::error::ServerError;
use crate::ids::ProjectId;
use crate::lease::{Grant, LeaseState};
use crate::store::{FigureMeta, ProjectMeta, ProjectStore, StoreFault, StoredOp, StoredSnapshot};

/// A serialized project and the sequence number it is current as of.
#[derive(Debug)]
pub struct OpenData {
    /// Static facts.
    pub meta: ProjectMeta,
    /// The snapshot reflects ops `1..=last_seq`.
    pub last_seq: u64,
    /// `Project::save` output.
    pub snapshot: Bytes,
    /// Current lease holder, if any.
    pub lock_holder: Option<Arc<str>>,
}

/// A project's newest sequence number and who holds its lease.
#[derive(Debug)]
pub struct Status {
    /// Sequence number of the last accepted op.
    pub last_seq: u64,
    /// Current lease holder, if any.
    pub lock_holder: Option<Arc<str>>,
}

/// The official risk of each threat scenario at one sequence number.
#[derive(Debug, PartialEq, Eq)]
pub struct RiskReport {
    /// The sequence number the numbers are current as of.
    pub as_of_seq: u64,
    /// Threats in arena order. `None`: not computable (for example no
    /// feasibility or no damage scenario).
    pub threats: Vec<ThreatRiskRow>,
}

/// One threat's risk keys.
#[derive(Debug, PartialEq, Eq)]
pub struct ThreatRiskRow {
    /// The threat.
    pub threat: ThreatId,
    /// Inherent risk level key.
    pub inherent: Option<String>,
    /// Residual risk level key.
    pub residual: Option<String>,
}

type Reply<T> = oneshot::Sender<Result<T, ServerError>>;

enum Cmd {
    Open(Reply<OpenData>),
    Acquire(Principal, Reply<Grant>),
    Renew(Principal, String, Reply<Grant>),
    Release(Principal, String, Reply<()>),
    Break(Principal, Reply<()>),
    Submit {
        who: Principal,
        token: String,
        base_seq: u64,
        ops: Vec<Bytes>,
        reply: Reply<u64>,
    },
    Status(Reply<Status>),
    Risks(Reply<RiskReport>),
    Rename {
        who: Principal,
        token: String,
        name: String,
        reply: Reply<ProjectMeta>,
    },
    CheckLease {
        who: Principal,
        token: String,
        reply: Reply<()>,
    },
    PutFigure {
        who: Principal,
        token: String,
        meta: FigureMeta,
        data: Arc<Vec<u8>>,
        max_count: usize,
        max_total_bytes: u64,
        reply: Reply<()>,
    },
    UpdateFigure {
        who: Principal,
        token: String,
        id: uuid::Uuid,
        caption: String,
        target: String,
        reply: Reply<FigureMeta>,
    },
    DeleteFigure {
        who: Principal,
        token: String,
        id: uuid::Uuid,
        reply: Reply<()>,
    },
    Delete(Reply<()>),
    Shutdown(Reply<()>),
}

/// A cloneable address of a project actor.
#[derive(Clone, Debug)]
pub struct ProjectHandle {
    id: ProjectId,
    tx: mpsc::Sender<Cmd>,
    wait: Duration,
    stopped: watch::Receiver<bool>,
}

impl ProjectHandle {
    /// The project this handle addresses.
    #[must_use]
    pub fn id(&self) -> ProjectId {
        self.id
    }

    /// Whether the actor has stopped. A stopped handle must be replaced.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Waits until the actor task has finished its final snapshot and exited.
    /// Callers replacing a closed handle wait on this so at most one actor
    /// touches storage for a project at a time.
    pub async fn wait_stopped(&self) {
        let mut rx = self.stopped.clone();
        let _ = rx.wait_for(|done| *done).await;
    }

    async fn call<T>(&self, make: impl FnOnce(Reply<T>) -> Cmd) -> Result<T, ServerError> {
        let (reply, answer) = oneshot::channel();
        // Bounded mailbox, bounded wait (R4, R6).
        match tokio::time::timeout(self.wait, self.tx.send(make(reply))).await {
            Err(_) => return Err(ServerError::Busy),
            Ok(Err(_)) => return Err(ServerError::Unloaded),
            Ok(Ok(())) => {}
        }
        answer.await.map_err(|_| ServerError::Unloaded)?
    }

    /// Serializes the project for a client.
    ///
    /// # Errors
    ///
    /// [`ServerError::Busy`], [`ServerError::Unloaded`], or an internal error.
    pub async fn open(&self) -> Result<OpenData, ServerError> {
        self.call(Cmd::Open).await
    }

    /// Grants the edit lease to `who`.
    ///
    /// # Errors
    ///
    /// [`ServerError::LockHeld`] if someone else holds it.
    pub async fn acquire(&self, who: Principal) -> Result<Grant, ServerError> {
        self.call(|r| Cmd::Acquire(who, r)).await
    }

    /// Renews the lease.
    ///
    /// # Errors
    ///
    /// [`ServerError::LockInvalid`] or [`ServerError::NotLocked`].
    pub async fn renew(&self, who: Principal, token: String) -> Result<Grant, ServerError> {
        self.call(|r| Cmd::Renew(who, token, r)).await
    }

    /// Gives the lease up.
    ///
    /// # Errors
    ///
    /// [`ServerError::LockInvalid`] or [`ServerError::NotLocked`].
    pub async fn release(&self, who: Principal, token: String) -> Result<(), ServerError> {
        self.call(|r| Cmd::Release(who, token, r)).await
    }

    /// Breaks the lease (admin).
    ///
    /// # Errors
    ///
    /// [`ServerError::NotAdmin`].
    pub async fn break_lock(&self, who: Principal) -> Result<(), ServerError> {
        self.call(|r| Cmd::Break(who, r)).await
    }

    /// Validates, applies, and stores a batch of ops atomically. Returns
    /// the sequence number of the last accepted op.
    ///
    /// # Errors
    ///
    /// Lease errors, [`ServerError::StaleBase`], [`ServerError::MalformedOp`],
    /// [`ServerError::RejectedOp`], or a storage failure.
    pub async fn submit(
        &self,
        who: Principal,
        token: String,
        base_seq: u64,
        ops: Vec<Bytes>,
    ) -> Result<u64, ServerError> {
        self.call(|reply| Cmd::Submit {
            who,
            token,
            base_seq,
            ops,
            reply,
        })
        .await
    }

    /// The newest sequence number and the lease holder. Cheap: nothing is
    /// serialized.
    ///
    /// # Errors
    ///
    /// [`ServerError::Busy`] or [`ServerError::Unloaded`].
    pub async fn status(&self) -> Result<Status, ServerError> {
        self.call(Cmd::Status).await
    }

    /// Computes the official risks.
    ///
    /// # Errors
    ///
    /// [`ServerError::Busy`] or [`ServerError::Unloaded`].
    pub async fn risks(&self) -> Result<RiskReport, ServerError> {
        self.call(Cmd::Risks).await
    }

    /// Renames the project. Needs the lease, like any change.
    ///
    /// # Errors
    ///
    /// Lease errors or a storage failure.
    pub async fn rename(
        &self,
        who: Principal,
        token: String,
        name: String,
    ) -> Result<ProjectMeta, ServerError> {
        self.call(|reply| Cmd::Rename {
            who,
            token,
            name,
            reply,
        })
        .await
    }

    /// Verifies that `who` holds the edit lease with `token`.
    ///
    /// # Errors
    ///
    /// [`ServerError::NotLocked`] or [`ServerError::LockInvalid`].
    pub async fn check_lease(&self, who: Principal, token: String) -> Result<(), ServerError> {
        self.call(|reply| Cmd::CheckLease { who, token, reply })
            .await
    }

    /// Stores a figure while serializing the lease check with the write.
    pub async fn put_figure(
        &self,
        who: Principal,
        token: String,
        meta: FigureMeta,
        data: Arc<Vec<u8>>,
        max_count: usize,
        max_total_bytes: u64,
    ) -> Result<(), ServerError> {
        self.call(|reply| Cmd::PutFigure {
            who,
            token,
            meta,
            data,
            max_count,
            max_total_bytes,
            reply,
        })
        .await
    }

    /// Updates a figure while serializing the lease check with the write.
    pub async fn update_figure(
        &self,
        who: Principal,
        token: String,
        id: uuid::Uuid,
        caption: String,
        target: String,
    ) -> Result<FigureMeta, ServerError> {
        self.call(|reply| Cmd::UpdateFigure {
            who,
            token,
            id,
            caption,
            target,
            reply,
        })
        .await
    }

    /// Deletes a figure while serializing the lease check with the write.
    pub async fn delete_figure(
        &self,
        who: Principal,
        token: String,
        id: uuid::Uuid,
    ) -> Result<(), ServerError> {
        self.call(|reply| Cmd::DeleteFigure {
            who,
            token,
            id,
            reply,
        })
        .await
    }

    /// Deletes the project from storage and stops the actor. Refused while
    /// anyone holds the lease.
    ///
    /// # Errors
    ///
    /// [`ServerError::LockHeld`] or a storage failure.
    pub async fn delete(&self) -> Result<(), ServerError> {
        self.call(Cmd::Delete).await
    }

    /// Writes a final snapshot and stops the actor.
    ///
    /// # Errors
    ///
    /// [`ServerError::Unloaded`] if it had already stopped.
    pub async fn shutdown(&self) -> Result<(), ServerError> {
        self.call(Cmd::Shutdown).await
    }
}

/// A project rebuilt from storage, ready to be run by an actor.
pub struct Loaded {
    meta: ProjectMeta,
    project: Project,
    cache: ProjectEvalCache,
    seq: u64,
    snapshot_seq: u64,
}

impl Loaded {
    /// Rebuilds a project from its snapshot and the ops after it.
    ///
    /// CPU-bound, O(snapshot bytes + ops): call it from `spawn_blocking`.
    ///
    /// # Errors
    ///
    /// [`ServerError::Store`] with [`StoreFault::Corrupt`] if the snapshot
    /// does not load, an op does not decode or apply, or the sequence has a
    /// gap.
    pub fn rebuild(
        loaded: crate::store::LoadedProject,
        methodology: Arc<aegis_model::Methodology>,
    ) -> Result<Self, ServerError> {
        let corrupt = |what: &str| ServerError::Store(StoreFault::Corrupt(what.to_owned()));
        let mut project = Project::load(loaded.snapshot.bytes.as_slice(), Arc::clone(&methodology))
            .map_err(|e| corrupt(&format!("snapshot: {e}")))?;
        let codec = OpCodec::new(methodology);
        let mut seq = loaded.snapshot.seq;
        for stored in &loaded.ops {
            seq += 1;
            if stored.seq != seq {
                return Err(corrupt(&format!("expected op {seq}, found {}", stored.seq)));
            }
            let op = codec
                .decode(&stored.op)
                .map_err(|e| corrupt(&format!("op {seq}: {e}")))?;
            project
                .apply(op)
                .map_err(|e| corrupt(&format!("op {seq}: {e}")))?;
        }
        let cache = project.eval_cache();
        Ok(Loaded {
            meta: loaded.meta,
            project,
            cache,
            seq,
            snapshot_seq: loaded.snapshot.seq,
        })
    }

    /// The project as rebuilt (tests compare it with what clients hold).
    #[must_use]
    pub fn project(&self) -> &Project {
        &self.project
    }
}

/// Starts the actor task for a rebuilt project and returns its address.
pub fn spawn<S: ProjectStore>(
    loaded: Loaded,
    store: Arc<S>,
    config: &ServerConfig,
) -> ProjectHandle {
    let (tx, rx) = mpsc::channel(config.mailbox);
    let (stopped_tx, stopped) = watch::channel(false);
    let id = loaded.meta.id;
    let codec = OpCodec::new(Arc::clone(loaded.project.methodology()));
    let now = Instant::now();
    let actor = Actor {
        id,
        meta: loaded.meta,
        project: loaded.project,
        cache: loaded.cache,
        codec,
        store,
        seq: loaded.seq,
        snapshot_seq: loaded.snapshot_seq,
        lease: LeaseState::new(config.lease),
        idle_unload: config.idle_unload,
        last_activity: now,
        idle_check_at: now + config.idle_unload,
        snapshot_every: config.snapshot_every.max(1),
        max_batch: config.max_batch_ops,
        wall_clock: config.wall_clock,
        broken: false,
        deleted: false,
    };
    tokio::spawn(actor.run(rx, stopped_tx));
    ProjectHandle {
        id,
        tx,
        wait: config.mailbox_wait,
        stopped,
    }
}

struct StopGuard(watch::Sender<bool>);

impl Drop for StopGuard {
    fn drop(&mut self) {
        let _ = self.0.send(true);
    }
}

enum Flow {
    Continue,
    Stop,
    Shutdown(Reply<()>),
}

struct Actor<S> {
    id: ProjectId,
    meta: ProjectMeta,
    project: Project,
    cache: ProjectEvalCache,
    codec: OpCodec,
    store: Arc<S>,
    /// Sequence number of the last accepted op.
    seq: u64,
    /// Sequence number the stored snapshot is current as of.
    snapshot_seq: u64,
    lease: LeaseState,
    idle_unload: Duration,
    last_activity: Instant,
    idle_check_at: Instant,
    snapshot_every: u64,
    max_batch: usize,
    wall_clock: fn() -> u64,
    /// Memory no longer matches storage; stop and let the next request
    /// reload.
    broken: bool,
    /// The project was deleted; stop without writing a snapshot.
    deleted: bool,
}

impl<S: ProjectStore> Actor<S> {
    async fn run(mut self, mut rx: mpsc::Receiver<Cmd>, stopped_tx: watch::Sender<bool>) {
        let stop_guard = StopGuard(stopped_tx);
        let mut shutdown_reply = None;
        loop {
            let wake = self.next_wake();
            let cmd = tokio::select! {
                cmd = rx.recv() => cmd,
                () = tokio::time::sleep_until(wake) => {
                    if matches!(self.on_timer(), Flow::Stop) {
                        break;
                    }
                    continue;
                }
            };
            let Some(cmd) = cmd else { break };
            self.touch();
            match self.handle(cmd).await {
                Flow::Continue => {}
                Flow::Stop => break,
                Flow::Shutdown(reply) => {
                    shutdown_reply = Some(reply);
                    break;
                }
            }
            if self.broken {
                break;
            }
            self.maintain().await;
            if self.broken {
                break;
            }
        }
        // Close the mailbox first so late callers fail fast and wait for exit.
        rx.close();
        if !self.broken && !self.deleted {
            self.snapshot_now().await;
        }
        // Signal waiters only after the final snapshot completes, then answer
        // the shutdown caller so it observes both a closed mailbox and a stored snapshot.
        drop(stop_guard);
        if let Some(reply) = shutdown_reply {
            let _ = reply.send(Ok(()));
        }
        tracing::debug!(project = %self.id, "project unloaded");
    }

    fn touch(&mut self) {
        let now = Instant::now();
        self.last_activity = now;
        self.idle_check_at = now + self.idle_unload;
    }

    /// The earliest moment something happens without a command: the lease
    /// expires, or the idle check is due.
    fn next_wake(&self) -> Instant {
        self.lease
            .expiry()
            .map_or(self.idle_check_at, |e| e.min(self.idle_check_at))
    }

    fn on_timer(&mut self) -> Flow {
        let now = Instant::now();
        // An expired lease is dropped here; nobody needs to be told.
        let _ = self.lease.sweep(now);
        if now >= self.idle_check_at {
            if self.lease.expiry().is_none() {
                return Flow::Stop;
            }
            self.idle_check_at = now + self.idle_unload;
        }
        Flow::Continue
    }

    async fn handle(&mut self, cmd: Cmd) -> Flow {
        match cmd {
            Cmd::Open(reply) => {
                let r = self.open().await;
                let _ = reply.send(r);
            }
            Cmd::Acquire(who, reply) => {
                let seq = self.seq;
                let r = self
                    .lease
                    .acquire(Instant::now(), &who)
                    .map(|g| Grant { last_seq: seq, ..g });
                let _ = reply.send(r);
            }
            Cmd::Renew(who, token, reply) => {
                let seq = self.seq;
                let r = self
                    .lease
                    .renew(Instant::now(), &who, &token)
                    .map(|g| Grant { last_seq: seq, ..g });
                let _ = reply.send(r);
            }
            Cmd::Release(who, token, reply) => {
                let r = self.lease.release(Instant::now(), &who, &token).map(|_| ());
                let _ = reply.send(r);
            }
            Cmd::Break(who, reply) => {
                let r = self.lease.break_lease(Instant::now(), &who);
                let _ = reply.send(r.map(|_| ()));
            }
            Cmd::Submit {
                who,
                token,
                base_seq,
                ops,
                reply,
            } => {
                let r = self.submit(&who, &token, base_seq, &ops).await;
                let _ = reply.send(r);
            }
            Cmd::Status(reply) => {
                let _ = reply.send(Ok(Status {
                    last_seq: self.seq,
                    lock_holder: self.lease.holder(Instant::now()).cloned(),
                }));
            }
            Cmd::Risks(reply) => {
                let _ = reply.send(Ok(self.risks()));
            }
            Cmd::Rename {
                who,
                token,
                name,
                reply,
            } => {
                let r = match self.lease.require(Instant::now(), &who, &token) {
                    Ok(()) => match self.store.rename(self.id, &name).await {
                        Ok(()) => {
                            self.meta.name = name;
                            Ok(self.meta.clone())
                        }
                        Err(fault) => Err(ServerError::Store(fault)),
                    },
                    Err(e) => Err(e),
                };
                let _ = reply.send(r);
            }
            Cmd::CheckLease { who, token, reply } => {
                let _ = reply.send(self.lease.require(Instant::now(), &who, &token));
            }
            Cmd::PutFigure {
                who,
                token,
                meta,
                data,
                max_count,
                max_total_bytes,
                reply,
            } => {
                let result = match self.lease.require(Instant::now(), &who, &token) {
                    Ok(()) => self
                        .store
                        .put_figure(&meta, data.as_slice(), max_count, max_total_bytes)
                        .await
                        .map_err(ServerError::Store),
                    Err(error) => Err(error),
                };
                let _ = reply.send(result);
            }
            Cmd::UpdateFigure {
                who,
                token,
                id,
                caption,
                target,
                reply,
            } => {
                let result = match self.lease.require(Instant::now(), &who, &token) {
                    Ok(()) => match self
                        .store
                        .update_figure(self.id, id, &caption, &target)
                        .await
                    {
                        Ok(Some(meta)) => Ok(meta),
                        Ok(None) => Err(ServerError::InvalidRequest("unknown figure")),
                        Err(error) => Err(ServerError::Store(error)),
                    },
                    Err(error) => Err(error),
                };
                let _ = reply.send(result);
            }
            Cmd::DeleteFigure {
                who,
                token,
                id,
                reply,
            } => {
                let result = match self.lease.require(Instant::now(), &who, &token) {
                    Ok(()) => match self.store.delete_figure(self.id, id).await {
                        Ok(true) => Ok(()),
                        Ok(false) => Err(ServerError::InvalidRequest("unknown figure")),
                        Err(error) => Err(ServerError::Store(error)),
                    },
                    Err(error) => Err(error),
                };
                let _ = reply.send(result);
            }
            Cmd::Delete(reply) => {
                if let Some(holder) = self.lease.holder(Instant::now()) {
                    let _ = reply.send(Err(ServerError::LockHeld {
                        holder: holder.to_string(),
                    }));
                    return Flow::Continue;
                }
                match self.store.delete(self.id).await {
                    Ok(()) => {
                        // Nothing is left to snapshot into.
                        self.deleted = true;
                        let _ = reply.send(Ok(()));
                        return Flow::Stop;
                    }
                    Err(fault) => {
                        let _ = reply.send(Err(ServerError::Store(fault)));
                    }
                }
            }
            Cmd::Shutdown(reply) => {
                // Close the mailbox and write the final snapshot in `run` before
                // answering, so the caller observes a stopped actor and stored state.
                return Flow::Shutdown(reply);
            }
        }
        Flow::Continue
    }

    /// Runs `f` on the blocking pool with the project moved in and back (R5).
    async fn blocking<R: Send + 'static>(
        &mut self,
        f: impl FnOnce(&Project) -> R + Send + 'static,
    ) -> Result<R, ServerError> {
        let placeholder = Project::new(Arc::clone(self.project.methodology()));
        let project = std::mem::replace(&mut self.project, placeholder);
        match tokio::task::spawn_blocking(move || {
            let out = f(&project);
            (project, out)
        })
        .await
        {
            Ok((project, out)) => {
                self.project = project;
                Ok(out)
            }
            Err(e) => {
                tracing::error!(project = %self.id, error = %e, "blocking task failed");
                self.broken = true;
                Err(ServerError::Internal)
            }
        }
    }

    async fn serialize(&mut self) -> Result<Vec<u8>, ServerError> {
        let id = self.id;
        let saved = self
            .blocking(|p| {
                let mut out = Vec::new();
                p.save(&mut out).map(|()| out)
            })
            .await?;
        saved.map_err(|e| {
            tracing::error!(project = %id, error = %e, "snapshot encoding failed");
            ServerError::Internal
        })
    }

    async fn open(&mut self) -> Result<OpenData, ServerError> {
        let snapshot = self.serialize().await?;
        Ok(OpenData {
            meta: self.meta.clone(),
            last_seq: self.seq,
            snapshot: Bytes::from(snapshot),
            lock_holder: self.lease.holder(Instant::now()).cloned(),
        })
    }

    async fn submit(
        &mut self,
        who: &Principal,
        token: &str,
        base_seq: u64,
        raw: &[Bytes],
    ) -> Result<u64, ServerError> {
        self.lease.require(Instant::now(), who, token)?;
        if base_seq != self.seq {
            return Err(ServerError::StaleBase {
                base: base_seq,
                current: self.seq,
            });
        }
        if raw.is_empty() {
            return Err(ServerError::InvalidRequest("no ops"));
        }
        if raw.len() > self.max_batch {
            return Err(ServerError::InvalidRequest("too many ops"));
        }

        // Decode everything before touching the project (R3, Q4).
        let mut ops = Vec::with_capacity(raw.len());
        for (index, bytes) in raw.iter().enumerate() {
            let op = self
                .codec
                .decode(bytes)
                .map_err(|source| ServerError::MalformedOp { index, source })?;
            ops.push(op);
        }

        // Apply in order; keep the canonical bytes and the inverses.
        let timestamp_ms = (self.wall_clock)();
        let mut stored = Vec::with_capacity(ops.len());
        let mut inverses = Vec::with_capacity(ops.len());
        for (index, op) in ops.into_iter().enumerate() {
            let canonical = match self.codec.encode(&op) {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::error!(project = %self.id, error = %e, "decoded op does not re-encode");
                    self.rollback(inverses);
                    return Err(ServerError::Internal);
                }
            };
            match self.project.apply_cached(op, &mut self.cache) {
                Ok(inverse) => inverses.push(inverse),
                Err(source) => {
                    self.rollback(inverses);
                    return Err(ServerError::RejectedOp { index, source });
                }
            }
            stored.push(StoredOp {
                seq: self.seq + 1 + index as u64,
                actor: who.subject.to_string(),
                timestamp_ms,
                op: canonical,
            });
        }

        if let Err(fault) = self.store.append(self.id, &stored).await {
            self.rollback(inverses);
            return Err(ServerError::Store(fault));
        }
        if self.broken {
            return Err(ServerError::Internal);
        }

        self.seq += stored.len() as u64;
        Ok(self.seq)
    }

    /// Undoes applied ops, newest first. If an inverse is refused the
    /// project can no longer be trusted: stop and reload from storage.
    fn rollback(&mut self, inverses: Vec<aegis_model::Op>) {
        for inverse in inverses.into_iter().rev() {
            if let Err(e) = self.project.apply_cached(inverse, &mut self.cache) {
                tracing::error!(project = %self.id, error = %e, "rollback failed; unloading project");
                self.broken = true;
                return;
            }
        }
    }

    fn risks(&self) -> RiskReport {
        let m = self.project.methodology();
        let key = |risk: Result<&aegis_model::ThreatRisk, aegis_model::ModelError>| {
            risk.ok()
                .and_then(aegis_model::ThreatRisk::max_risk)
                .and_then(|level| m.risk_level_key(level))
                .map(str::to_owned)
        };
        let threats = self
            .project
            .threat_scenarios()
            .map(|(_, id, _)| ThreatRiskRow {
                threat: id,
                inherent: key(self.cache.evaluate_risk(&self.project, id)),
                residual: key(self.cache.evaluate_residual_risk(&self.project, id)),
            })
            .collect();
        RiskReport {
            as_of_seq: self.seq,
            threats,
        }
    }

    /// Snapshot when enough ops have piled up since the last one.
    async fn maintain(&mut self) {
        if self.seq - self.snapshot_seq >= self.snapshot_every {
            self.snapshot_now().await;
        }
    }

    /// Writes a snapshot at the current sequence number. A failure is
    /// logged, not fatal: the op log still holds the truth.
    async fn snapshot_now(&mut self) {
        if self.seq == self.snapshot_seq {
            return;
        }
        let seq = self.seq;
        let Ok(bytes) = self.serialize().await else {
            return;
        };
        let snapshot = StoredSnapshot { seq, bytes };
        match self.store.save_snapshot(self.id, &snapshot).await {
            Ok(()) => self.snapshot_seq = seq,
            Err(e) => tracing::warn!(project = %self.id, error = %e, "snapshot not saved"),
        }
    }
}
