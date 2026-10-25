//! One task per open project (rule C1).
//!
//! The actor owns the [`Project`], its [`ProjectEvalCache`], the edit lease,
//! and the observer channel. Everything else talks to it through a bounded
//! mailbox (R4), so ops are applied one at a time in arrival order and no
//! lock protects the project.
//!
//! # Accepting ops
//!
//! `SubmitOps` is atomic. The actor checks the lease and the base sequence,
//! decodes every op (capped, R3), applies them in order, stores them, and
//! only then answers and tells observers:
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
//! The actor unloads itself after `idle_unload` without a lease or
//! observers, writing a final snapshot first. A new request loads it again.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use aegis_model::{OpCodec, Project, ProjectEvalCache, ThreatId};
use bytes::Bytes;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::Instant;

use crate::auth::Principal;
use crate::config::ServerConfig;
use crate::error::ServerError;
use crate::ids::ProjectId;
use crate::lease::{Grant, LeaseState};
use crate::store::{ProjectMeta, ProjectStore, StoreFault, StoredOp, StoredSnapshot};

/// One accepted op, shared between the recent-ops ring and every observer.
#[derive(Debug, PartialEq, Eq)]
pub struct Accepted {
    /// Position in the project's history.
    pub seq: u64,
    /// Subject that submitted the op.
    pub actor: Arc<str>,
    /// Acceptance time, milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
    /// Canonical op bytes (`OpCodec::encode`).
    pub op: Bytes,
}

/// What observers are told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// An op was accepted.
    Op(Arc<Accepted>),
    /// The lease changed hands. Empty holder: the lock is free.
    Lock(Arc<str>),
}

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

/// An observer's start: ops it missed, then live events.
#[derive(Debug)]
pub struct Subscription {
    /// Ops with `seq > after_seq` that were already accepted.
    pub backlog: Vec<Arc<Accepted>>,
    /// Events from now on. Nothing is missed or repeated between the two.
    pub live: broadcast::Receiver<Event>,
    /// Lease holder at subscription time.
    pub lock_holder: Option<Arc<str>>,
}

/// The official risk of each threat scenario at one sequence number.
#[derive(Debug)]
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
    Subscribe(u64, Reply<Subscription>),
    Risks(Reply<RiskReport>),
    Shutdown(Reply<()>),
}

/// A cloneable address of a project actor.
#[derive(Clone, Debug)]
pub struct ProjectHandle {
    id: ProjectId,
    tx: mpsc::Sender<Cmd>,
    wait: Duration,
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

    /// Starts observing from `after_seq`.
    ///
    /// # Errors
    ///
    /// [`ServerError::OutOfRange`] if the ops after `after_seq` are no
    /// longer in memory or `after_seq` is in the future.
    pub async fn subscribe(&self, after_seq: u64) -> Result<Subscription, ServerError> {
        self.call(|r| Cmd::Subscribe(after_seq, r)).await
    }

    /// Computes the official risks.
    ///
    /// # Errors
    ///
    /// [`ServerError::Busy`] or [`ServerError::Unloaded`].
    pub async fn risks(&self) -> Result<RiskReport, ServerError> {
        self.call(Cmd::Risks).await
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
    let (events, _) = broadcast::channel(config.observer_buffer.max(1));
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
        recent: VecDeque::with_capacity(config.recent_ops.min(1024)),
        events,
        idle_unload: config.idle_unload,
        last_activity: now,
        idle_check_at: now + config.idle_unload,
        snapshot_every: config.snapshot_every.max(1),
        recent_cap: config.recent_ops,
        max_batch: config.max_batch_ops,
        wall_clock: config.wall_clock,
        broken: false,
    };
    tokio::spawn(actor.run(rx));
    ProjectHandle {
        id,
        tx,
        wait: config.mailbox_wait,
    }
}

enum Flow {
    Continue,
    Stop,
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
    /// The last `recent_cap` accepted ops, oldest first, contiguous.
    recent: VecDeque<Arc<Accepted>>,
    events: broadcast::Sender<Event>,
    idle_unload: Duration,
    last_activity: Instant,
    idle_check_at: Instant,
    snapshot_every: u64,
    recent_cap: usize,
    max_batch: usize,
    wall_clock: fn() -> u64,
    /// Memory no longer matches storage; stop and let the next request
    /// reload.
    broken: bool,
}

impl<S: ProjectStore> Actor<S> {
    async fn run(mut self, mut rx: mpsc::Receiver<Cmd>) {
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
            let flow = self.handle(cmd).await;
            if self.broken || matches!(flow, Flow::Stop) {
                break;
            }
            self.maintain().await;
            if self.broken {
                break;
            }
        }
        // Close the mailbox first so late callers fail fast and reload.
        rx.close();
        if !self.broken {
            self.snapshot_now().await;
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
        if self.lease.sweep(now).is_some() {
            self.emit(Event::Lock(Arc::from("")));
        }
        if now >= self.idle_check_at {
            let unused = self.lease.expiry().is_none() && self.events.receiver_count() == 0;
            if unused {
                return Flow::Stop;
            }
            self.idle_check_at = now + self.idle_unload;
        }
        Flow::Continue
    }

    fn emit(&self, event: Event) {
        // No receivers is normal; the send error only says so.
        let _ = self.events.send(event);
    }

    async fn handle(&mut self, cmd: Cmd) -> Flow {
        match cmd {
            Cmd::Open(reply) => {
                let r = self.open().await;
                let _ = reply.send(r);
            }
            Cmd::Acquire(who, reply) => {
                let r = self.lease.acquire(Instant::now(), &who);
                if r.is_ok() {
                    self.emit(Event::Lock(Arc::clone(&who.subject)));
                }
                let _ = reply.send(r);
            }
            Cmd::Renew(who, token, reply) => {
                let _ = reply.send(self.lease.renew(Instant::now(), &who, &token));
            }
            Cmd::Release(who, token, reply) => {
                let r = self.lease.release(Instant::now(), &who, &token).map(|_| ());
                if r.is_ok() {
                    self.emit(Event::Lock(Arc::from("")));
                }
                let _ = reply.send(r);
            }
            Cmd::Break(who, reply) => {
                let r = self.lease.break_lease(Instant::now(), &who);
                if matches!(r, Ok(Some(_))) {
                    self.emit(Event::Lock(Arc::from("")));
                }
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
            Cmd::Subscribe(after, reply) => {
                let _ = reply.send(self.subscribe(after));
            }
            Cmd::Risks(reply) => {
                let _ = reply.send(Ok(self.risks()));
            }
            Cmd::Shutdown(reply) => {
                // Snapshot before answering, so a caller that waits for the
                // reply knows the state is stored.
                self.snapshot_now().await;
                let _ = reply.send(Ok(()));
                return Flow::Stop;
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

    fn subscribe(&self, after: u64) -> Result<Subscription, ServerError> {
        if after > self.seq {
            return Err(ServerError::OutOfRange { after });
        }
        // The ring holds seq (first..=self.seq); older ops are gone.
        let first_held = self.recent.front().map_or(self.seq + 1, |a| a.seq);
        if after + 1 < first_held {
            return Err(ServerError::OutOfRange { after });
        }
        let backlog = self
            .recent
            .iter()
            .filter(|a| a.seq > after)
            .map(Arc::clone)
            .collect();
        Ok(Subscription {
            backlog,
            live: self.events.subscribe(),
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

        for op in stored {
            let accepted = Arc::new(Accepted {
                seq: op.seq,
                actor: Arc::clone(&who.subject),
                timestamp_ms: op.timestamp_ms,
                op: Bytes::from(op.op),
            });
            self.seq = accepted.seq;
            if self.recent_cap > 0 {
                if self.recent.len() == self.recent_cap {
                    self.recent.pop_front();
                }
                self.recent.push_back(Arc::clone(&accepted));
            }
            self.emit(Event::Op(accepted));
        }
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
