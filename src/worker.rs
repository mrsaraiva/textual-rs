//! Worker abstraction for background tasks with lifecycle management.
//!
//! Provides cooperative cancellation, exclusive mode (cancel-previous semantics),
//! and per-owner cleanup — modelled after Python Textual's `Worker` system.
//!
//! The runtime owns a [`WorkerRegistry`] that tracks all active workers.
//! Widgets request workers via [`EventCtx`](crate::event::EventCtx); actual
//! task spawning/execution is handled by the runtime event loop.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use crate::node_id::NodeId;

// ── WorkerId ──────────────────────────────────────────────────────────

/// Unique identifier for a background worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorkerId(u64);

/// Global monotonic counter for worker IDs.
static NEXT_WORKER_ID: AtomicU64 = AtomicU64::new(1);

impl WorkerId {
    /// Allocate a new, globally-unique worker ID.
    pub fn new() -> Self {
        Self(NEXT_WORKER_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Return the raw numeric value (useful for logging/debug).
    #[inline]
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl Default for WorkerId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for WorkerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Worker({})", self.0)
    }
}

// ── WorkerState ───────────────────────────────────────────────────────

/// Lifecycle states for a background worker.
#[derive(Clone, Debug, PartialEq)]
pub enum WorkerState {
    /// Registered but not yet executing.
    Pending,
    /// Actively running.
    Running,
    /// Cancelled (cooperatively or by exclusive-mode replacement).
    Cancelled,
    /// Completed successfully.
    Success,
    /// Completed with an error.
    Error(String),
}

impl WorkerState {
    /// `true` for terminal states (`Cancelled`, `Success`, `Error`).
    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Cancelled | Self::Success | Self::Error(_))
    }
}

// ── CancellationToken ─────────────────────────────────────────────────

/// Cooperative cancellation token.
///
/// Workers check [`is_cancelled`](Self::is_cancelled) periodically to stop
/// early. Cloning a token shares the same underlying flag (`Arc<AtomicBool>`),
/// so both the registry and the worker closure see the same state.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Create a new token that is *not* cancelled.
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signal cancellation.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Check whether cancellation has been signalled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

// ── WorkerEntry (registry-internal) ───────────────────────────────────

/// Registry-internal record for a single worker.
struct WorkerEntry {
    id: WorkerId,
    owner: NodeId,
    state: WorkerState,
    cancel_token: CancellationToken,
    exclusive_key: Option<String>,
    #[allow(dead_code)]
    name: Option<String>,
}

type WorkerJob = Box<dyn FnOnce(CancellationToken) -> Result<(), String> + Send + 'static>;

#[derive(Debug)]
struct WorkerCompletion {
    id: WorkerId,
    state: WorkerState,
}

// ── WorkerRegistry ────────────────────────────────────────────────────

/// Tracks all active workers.
///
/// Owned by the runtime; **not** thread-safe (single event-loop access).
pub struct WorkerRegistry {
    workers: Vec<WorkerEntry>,
    completion_tx: Sender<WorkerCompletion>,
    completion_rx: Receiver<WorkerCompletion>,
    pending_changes: Vec<WorkerStateChanged>,
}

impl WorkerRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        let (completion_tx, completion_rx) = mpsc::channel();
        Self {
            workers: Vec::new(),
            completion_tx,
            completion_rx,
            pending_changes: Vec::new(),
        }
    }

    /// Register a new worker.
    ///
    /// If `exclusive_key` is `Some`, any existing worker with the same key
    /// **and** the same `owner` is cancelled first.
    ///
    /// Returns `(WorkerId, CancellationToken)` — the token should be moved
    /// into the spawned task so it can check for cancellation.
    pub fn register(
        &mut self,
        owner: NodeId,
        exclusive_key: Option<String>,
        name: Option<String>,
    ) -> (WorkerId, CancellationToken) {
        // Exclusive mode: cancel previous worker with same key+owner.
        if let Some(ref key) = exclusive_key {
            for entry in &mut self.workers {
                if entry.owner == owner
                    && entry.exclusive_key.as_deref() == Some(key)
                    && !entry.state.is_finished()
                {
                    entry.cancel_token.cancel();
                    entry.state = WorkerState::Cancelled;
                    self.pending_changes.push(WorkerStateChanged {
                        worker_id: entry.id,
                        state: WorkerState::Cancelled,
                    });
                }
            }
        }

        let id = WorkerId::new();
        let token = CancellationToken::new();

        self.workers.push(WorkerEntry {
            id,
            owner,
            state: WorkerState::Pending,
            cancel_token: token.clone(),
            exclusive_key,
            name,
        });

        (id, token)
    }

    /// Transition a worker to [`WorkerState::Running`].
    pub fn set_running(&mut self, id: WorkerId) {
        if let Some(entry) = self.find_mut(id) {
            if entry.state == WorkerState::Pending {
                entry.state = WorkerState::Running;
            }
        }
    }

    /// Mark a worker as completed.
    ///
    /// `result` — `Ok(())` for success, `Err(msg)` for error.
    pub fn complete(&mut self, id: WorkerId, result: Result<(), String>) {
        if let Some(entry) = self.find_mut(id) {
            if !entry.state.is_finished() {
                entry.state = match result {
                    Ok(()) => WorkerState::Success,
                    Err(msg) => WorkerState::Error(msg),
                };
                let state = entry.state.clone();
                self.pending_changes.push(WorkerStateChanged {
                    worker_id: id,
                    state,
                });
            }
        }
    }

    /// Cancel a specific worker by ID.
    pub fn cancel(&mut self, id: WorkerId) {
        if let Some(entry) = self.find_mut(id) {
            if !entry.state.is_finished() {
                entry.cancel_token.cancel();
                entry.state = WorkerState::Cancelled;
                self.pending_changes.push(WorkerStateChanged {
                    worker_id: id,
                    state: WorkerState::Cancelled,
                });
            }
        }
    }

    /// Cancel every worker owned by `owner` (e.g. when a widget is unmounted).
    pub fn cancel_by_owner(&mut self, owner: NodeId) {
        for entry in &mut self.workers {
            if entry.owner == owner && !entry.state.is_finished() {
                entry.cancel_token.cancel();
                entry.state = WorkerState::Cancelled;
                self.pending_changes.push(WorkerStateChanged {
                    worker_id: entry.id,
                    state: WorkerState::Cancelled,
                });
            }
        }
    }

    /// Spawn and track a background worker task.
    ///
    /// The worker state is moved to `Running` immediately, then to a terminal
    /// state once the background job reports completion.
    pub fn spawn(
        &mut self,
        owner: NodeId,
        exclusive_key: Option<String>,
        name: Option<String>,
        job: WorkerJob,
    ) -> WorkerId {
        let (id, token) = self.register(owner, exclusive_key, name);
        self.set_running(id);

        let completion_tx = self.completion_tx.clone();
        std::thread::spawn(move || {
            if token.is_cancelled() {
                let _ = completion_tx.send(WorkerCompletion {
                    id,
                    state: WorkerState::Cancelled,
                });
                return;
            }

            let result = job(token.clone());
            let state = if token.is_cancelled() {
                WorkerState::Cancelled
            } else {
                match result {
                    Ok(()) => WorkerState::Success,
                    Err(error) => WorkerState::Error(error),
                }
            };

            let _ = completion_tx.send(WorkerCompletion { id, state });
        });

        id
    }

    /// Drain completion queue and return pending terminal state notifications.
    pub fn drain_state_changes(&mut self) -> Vec<WorkerStateChanged> {
        self.drain_completion_queue_nonblocking();
        std::mem::take(&mut self.pending_changes)
    }

    /// Query the current state of a worker.
    pub fn state(&self, id: WorkerId) -> Option<&WorkerState> {
        self.find(id).map(|e| &e.state)
    }

    /// Return the owning widget node for a worker id, when present.
    pub(crate) fn owner(&self, id: WorkerId) -> Option<NodeId> {
        self.find(id).map(|e| e.owner)
    }

    /// Return IDs of all workers that are not in a terminal state.
    pub fn active_workers(&self) -> Vec<WorkerId> {
        self.workers
            .iter()
            .filter(|e| !e.state.is_finished())
            .map(|e| e.id)
            .collect()
    }

    /// Remove all workers that are in a terminal state.
    pub fn cleanup(&mut self) {
        self.workers.retain(|e| !e.state.is_finished());
    }

    // ── Private helpers ───────────────────────────────────────────────

    fn find(&self, id: WorkerId) -> Option<&WorkerEntry> {
        self.workers.iter().find(|e| e.id == id)
    }

    fn find_mut(&mut self, id: WorkerId) -> Option<&mut WorkerEntry> {
        self.workers.iter_mut().find(|e| e.id == id)
    }

    fn drain_completion_queue_nonblocking(&mut self) {
        loop {
            match self.completion_rx.try_recv() {
                Ok(completion) => self.apply_completion(completion),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    fn apply_completion(&mut self, completion: WorkerCompletion) {
        if let Some(entry) = self.find_mut(completion.id) {
            if !entry.state.is_finished() {
                entry.state = completion.state.clone();
                self.pending_changes.push(WorkerStateChanged {
                    worker_id: completion.id,
                    state: completion.state,
                });
            }
        }
    }
}

impl Default for WorkerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── WorkerStateChanged ───────────────────────────────────────────────

/// Notification delivered when a worker transitions to a terminal state.
///
/// The runtime produces one of these after a worker reaches `Success`,
/// `Error`, or `Cancelled`, then converts it to a routed runtime message.
#[derive(Clone, Debug)]
pub struct WorkerStateChanged {
    pub worker_id: WorkerId,
    pub state: WorkerState,
}

// ── Runtime helper ──────────────────────────────────────────────────

/// Process a batch of [`WorkerRequest`]s against a [`WorkerRegistry`].
///
/// For each request the helper spawns an async worker job that executes
/// [`WorkerRequestPayload`]. This function is non-blocking: it never waits
/// for new completions and only drains completions already available.
///
/// Returns pending [`WorkerStateChanged`] notifications (including
/// synchronous exclusive cancellations and any async completions available
/// at call time).
pub(crate) fn process_worker_requests(
    registry: &mut WorkerRegistry,
    requests: Vec<WorkerRequest>,
) -> Vec<WorkerStateChanged> {
    for req in requests {
        registry.spawn(
            req.owner,
            req.exclusive_key,
            req.name,
            Box::new(move |token| req.payload.execute(token)),
        );
    }
    registry.drain_state_changes()
}

/// Payload that defines meaningful, deterministic worker behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerRequestPayload {
    /// Perform deterministic CPU work with optional delay and error.
    ComputeDigest {
        /// Input bytes mixed into a simple deterministic checksum.
        input: String,
        /// Number of digest rounds to execute (minimum 1 in execution).
        rounds: u16,
        /// Optional pause between rounds for cancellation/concurrency tests.
        delay_per_round_ms: u64,
        /// If set, return this terminal error after all rounds complete.
        fail_with: Option<String>,
    },
}

impl WorkerRequestPayload {
    /// Execute the requested worker payload.
    pub fn execute(self, token: CancellationToken) -> Result<(), String> {
        match self {
            Self::ComputeDigest {
                input,
                rounds,
                delay_per_round_ms,
                fail_with,
            } => {
                let rounds = rounds.max(1);
                let mut digest = 0xcbf2_9ce4_8422_2325_u64;
                for _ in 0..rounds {
                    if token.is_cancelled() {
                        return Ok(());
                    }
                    for byte in input.as_bytes() {
                        digest ^= u64::from(*byte);
                        digest = digest.wrapping_mul(0x100_0000_01b3);
                    }
                    if delay_per_round_ms > 0 {
                        thread::sleep(Duration::from_millis(delay_per_round_ms));
                    }
                }
                if token.is_cancelled() {
                    return Ok(());
                }
                if let Some(error) = fail_with {
                    return Err(error);
                }
                let _ = digest;
                Ok(())
            }
        }
    }
}

impl Default for WorkerRequestPayload {
    fn default() -> Self {
        Self::ComputeDigest {
            input: "worker".to_string(),
            rounds: 1,
            delay_per_round_ms: 0,
            fail_with: None,
        }
    }
}

// ── WorkerRequest (EventCtx integration) ──────────────────────────────

/// A request from a widget to spawn a background worker.
///
/// Created via [`EventCtx::request_worker`] / [`EventCtx::request_exclusive_worker`].
/// The runtime collects these after event dispatch and feeds them to
/// [`WorkerRegistry::register`].
#[derive(Debug, Clone)]
pub struct WorkerRequest {
    /// Widget that requested the worker.
    pub owner: NodeId,
    /// If `Some`, enables exclusive mode (cancel-previous semantics).
    pub exclusive_key: Option<String>,
    /// Optional descriptive name for debugging.
    pub name: Option<String>,
    /// Work definition executed by the spawned worker.
    pub payload: WorkerRequestPayload,
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_id::node_id_from_ffi;
    use std::thread;
    use std::time::{Duration, Instant};

    fn wait_for_changes(
        reg: &mut WorkerRegistry,
        mut expected: usize,
        timeout: Duration,
    ) -> Vec<WorkerStateChanged> {
        let deadline = Instant::now() + timeout;
        let mut out = Vec::new();
        while expected > 0 && Instant::now() < deadline {
            let mut batch = reg.drain_state_changes();
            if !batch.is_empty() {
                expected = expected.saturating_sub(batch.len());
                out.append(&mut batch);
            } else {
                thread::sleep(Duration::from_millis(1));
            }
        }
        out
    }

    fn wait_for_request_changes(
        reg: &mut WorkerRegistry,
        mut expected: usize,
        timeout: Duration,
    ) -> Vec<WorkerStateChanged> {
        let deadline = Instant::now() + timeout;
        let mut out = Vec::new();
        while expected > 0 && Instant::now() < deadline {
            let mut batch = process_worker_requests(reg, Vec::new());
            if !batch.is_empty() {
                expected = expected.saturating_sub(batch.len());
                out.append(&mut batch);
            } else {
                thread::sleep(Duration::from_millis(1));
            }
        }
        out
    }

    // ── WorkerId ──────────────────────────────────────────────────────

    #[test]
    fn worker_id_unique() {
        let a = WorkerId::new();
        let b = WorkerId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn worker_id_equality() {
        let a = WorkerId::new();
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn worker_id_display() {
        let id = WorkerId::new();
        let s = format!("{id}");
        assert!(s.starts_with("Worker("));
    }

    #[test]
    fn worker_id_raw() {
        let id = WorkerId::new();
        assert!(id.raw() > 0);
    }

    // ── CancellationToken ─────────────────────────────────────────────

    #[test]
    fn cancellation_token_new_is_not_cancelled() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancellation_token_cancel_sets_flag() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancellation_token_clone_shares_state() {
        let a = CancellationToken::new();
        let b = a.clone();
        assert!(!b.is_cancelled());
        a.cancel();
        assert!(b.is_cancelled());
    }

    // ── WorkerState ───────────────────────────────────────────────────

    #[test]
    fn worker_state_is_finished() {
        assert!(!WorkerState::Pending.is_finished());
        assert!(!WorkerState::Running.is_finished());
        assert!(WorkerState::Cancelled.is_finished());
        assert!(WorkerState::Success.is_finished());
        assert!(WorkerState::Error("oops".into()).is_finished());
    }

    // ── WorkerRegistry: lifecycle ─────────────────────────────────────

    #[test]
    fn registry_register_returns_pending() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);
        let (id, _token) = reg.register(owner, None, None);
        assert_eq!(reg.state(id), Some(&WorkerState::Pending));
    }

    #[test]
    fn registry_set_running() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);
        let (id, _) = reg.register(owner, None, None);
        reg.set_running(id);
        assert_eq!(reg.state(id), Some(&WorkerState::Running));
    }

    #[test]
    fn registry_complete_success() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);
        let (id, _) = reg.register(owner, None, None);
        reg.set_running(id);
        reg.complete(id, Ok(()));
        assert_eq!(reg.state(id), Some(&WorkerState::Success));
        let changes = reg.drain_state_changes();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].worker_id, id);
        assert_eq!(changes[0].state, WorkerState::Success);
    }

    #[test]
    fn registry_complete_error() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);
        let (id, _) = reg.register(owner, None, None);
        reg.set_running(id);
        reg.complete(id, Err("boom".into()));
        assert_eq!(reg.state(id), Some(&WorkerState::Error("boom".into())));
        let changes = reg.drain_state_changes();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].worker_id, id);
        assert_eq!(changes[0].state, WorkerState::Error("boom".into()));
    }

    #[test]
    fn registry_cancel() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);
        let (id, token) = reg.register(owner, None, None);
        reg.set_running(id);
        reg.cancel(id);
        assert_eq!(reg.state(id), Some(&WorkerState::Cancelled));
        assert!(token.is_cancelled());
        let changes = reg.drain_state_changes();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].worker_id, id);
        assert_eq!(changes[0].state, WorkerState::Cancelled);
    }

    #[test]
    fn registry_cancel_already_finished_is_noop() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);
        let (id, token) = reg.register(owner, None, None);
        reg.complete(id, Ok(()));
        reg.cancel(id); // should not overwrite Success
        assert_eq!(reg.state(id), Some(&WorkerState::Success));
        assert!(!token.is_cancelled());
    }

    // ── WorkerRegistry: exclusive mode ────────────────────────────────

    #[test]
    fn registry_exclusive_cancels_previous() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);

        let (id1, token1) = reg.register(owner, Some("fetch".into()), Some("first".into()));
        reg.set_running(id1);

        // Register a second worker with the same exclusive key + owner.
        let (id2, _token2) = reg.register(owner, Some("fetch".into()), Some("second".into()));

        assert_eq!(reg.state(id1), Some(&WorkerState::Cancelled));
        assert!(token1.is_cancelled());
        assert_eq!(reg.state(id2), Some(&WorkerState::Pending));
        let changes = reg.drain_state_changes();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].worker_id, id1);
        assert_eq!(changes[0].state, WorkerState::Cancelled);
    }

    #[test]
    fn registry_exclusive_different_owner_no_cancel() {
        let mut reg = WorkerRegistry::new();
        let owner_a = node_id_from_ffi(1);
        let owner_b = node_id_from_ffi(2);

        let (id1, token1) = reg.register(owner_a, Some("fetch".into()), None);
        reg.set_running(id1);

        // Different owner, same key — should NOT cancel.
        let (_id2, _) = reg.register(owner_b, Some("fetch".into()), None);

        assert_eq!(reg.state(id1), Some(&WorkerState::Running));
        assert!(!token1.is_cancelled());
    }

    #[test]
    fn registry_exclusive_does_not_cancel_finished() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);

        let (id1, _) = reg.register(owner, Some("fetch".into()), None);
        reg.complete(id1, Ok(()));

        // Previous worker already finished — register should succeed without issues.
        let (id2, _) = reg.register(owner, Some("fetch".into()), None);
        assert_eq!(reg.state(id1), Some(&WorkerState::Success));
        assert_eq!(reg.state(id2), Some(&WorkerState::Pending));
    }

    // ── WorkerRegistry: cancel_by_owner ───────────────────────────────

    #[test]
    fn registry_cancel_by_owner() {
        let mut reg = WorkerRegistry::new();
        let owner_a = node_id_from_ffi(1);
        let owner_b = node_id_from_ffi(2);

        let (id1, token1) = reg.register(owner_a, None, Some("a1".into()));
        let (id2, token2) = reg.register(owner_a, None, Some("a2".into()));
        let (id3, token3) = reg.register(owner_b, None, Some("b1".into()));
        reg.set_running(id1);
        reg.set_running(id2);
        reg.set_running(id3);

        reg.cancel_by_owner(owner_a);

        assert_eq!(reg.state(id1), Some(&WorkerState::Cancelled));
        assert_eq!(reg.state(id2), Some(&WorkerState::Cancelled));
        assert_eq!(reg.state(id3), Some(&WorkerState::Running));
        assert!(token1.is_cancelled());
        assert!(token2.is_cancelled());
        assert!(!token3.is_cancelled());
        let changes = reg.drain_state_changes();
        assert_eq!(changes.len(), 2);
        assert!(
            changes
                .iter()
                .any(|c| c.worker_id == id1 && c.state == WorkerState::Cancelled)
        );
        assert!(
            changes
                .iter()
                .any(|c| c.worker_id == id2 && c.state == WorkerState::Cancelled)
        );
    }

    // ── WorkerRegistry: active_workers ────────────────────────────────

    #[test]
    fn registry_active_workers() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);

        let (id1, _) = reg.register(owner, None, None);
        let (id2, _) = reg.register(owner, None, None);
        let (id3, _) = reg.register(owner, None, None);
        reg.set_running(id1);
        reg.complete(id2, Ok(()));

        let active = reg.active_workers();
        assert!(active.contains(&id1));
        assert!(!active.contains(&id2));
        assert!(active.contains(&id3)); // Pending is active
    }

    // ── WorkerRegistry: cleanup ───────────────────────────────────────

    #[test]
    fn registry_cleanup_removes_finished() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);

        let (id1, _) = reg.register(owner, None, None);
        let (id2, _) = reg.register(owner, None, None);
        let (id3, _) = reg.register(owner, None, None);

        reg.set_running(id1);
        reg.complete(id1, Ok(()));
        reg.cancel(id2);
        // id3 still Pending

        reg.cleanup();

        assert_eq!(reg.state(id1), None); // removed
        assert_eq!(reg.state(id2), None); // removed
        assert_eq!(reg.state(id3), Some(&WorkerState::Pending)); // kept
    }

    #[test]
    fn registry_cleanup_on_empty_is_noop() {
        let mut reg = WorkerRegistry::new();
        reg.cleanup(); // should not panic
        assert!(reg.active_workers().is_empty());
    }

    // ── WorkerRequest ─────────────────────────────────────────────────

    #[test]
    fn worker_request_construction() {
        let owner = node_id_from_ffi(42);
        let req = WorkerRequest {
            owner,
            exclusive_key: Some("load".into()),
            name: Some("data-loader".into()),
            payload: WorkerRequestPayload::ComputeDigest {
                input: "dataset".into(),
                rounds: 2,
                delay_per_round_ms: 0,
                fail_with: None,
            },
        };
        assert_eq!(req.owner, owner);
        assert_eq!(req.exclusive_key.as_deref(), Some("load"));
        assert_eq!(req.name.as_deref(), Some("data-loader"));
        assert_eq!(
            req.payload,
            WorkerRequestPayload::ComputeDigest {
                input: "dataset".into(),
                rounds: 2,
                delay_per_round_ms: 0,
                fail_with: None,
            }
        );
    }

    // ── State transition edge cases ───────────────────────────────────

    #[test]
    fn set_running_only_from_pending() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);
        let (id, _) = reg.register(owner, None, None);
        reg.set_running(id);
        reg.complete(id, Ok(()));

        // set_running on a finished worker should be a no-op.
        reg.set_running(id);
        assert_eq!(reg.state(id), Some(&WorkerState::Success));
    }

    #[test]
    fn complete_on_cancelled_is_noop() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);
        let (id, _) = reg.register(owner, None, None);
        reg.cancel(id);
        reg.complete(id, Ok(()));
        assert_eq!(reg.state(id), Some(&WorkerState::Cancelled));
    }

    #[test]
    fn unknown_worker_id_returns_none() {
        let reg = WorkerRegistry::new();
        let bogus = WorkerId::new();
        assert_eq!(reg.state(bogus), None);
    }

    // ── WorkerStateChanged ───────────────────────────────────────────

    #[test]
    fn worker_state_changed_construction() {
        let id = WorkerId::new();
        let changed = WorkerStateChanged {
            worker_id: id,
            state: WorkerState::Success,
        };
        assert_eq!(changed.worker_id, id);
        assert_eq!(changed.state, WorkerState::Success);
    }

    #[test]
    fn worker_state_changed_debug_format() {
        let id = WorkerId::new();
        let changed = WorkerStateChanged {
            worker_id: id,
            state: WorkerState::Error("oops".into()),
        };
        let s = format!("{changed:?}");
        assert!(s.contains("WorkerStateChanged"));
        assert!(s.contains("oops"));
    }

    #[test]
    fn worker_state_changed_clone() {
        let id = WorkerId::new();
        let original = WorkerStateChanged {
            worker_id: id,
            state: WorkerState::Cancelled,
        };
        let cloned = original.clone();
        assert_eq!(cloned.worker_id, id);
        assert_eq!(cloned.state, WorkerState::Cancelled);
    }

    // ── process_worker_requests ──────────────────────────────────────

    #[test]
    fn process_worker_requests_empty() {
        let mut reg = WorkerRegistry::new();
        let changes = process_worker_requests(&mut reg, vec![]);
        assert!(changes.is_empty());
        assert!(reg.active_workers().is_empty());
    }

    #[test]
    fn process_worker_requests_registers_and_completes() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(10);
        let requests = vec![WorkerRequest {
            owner,
            exclusive_key: None,
            name: Some("bg-task".into()),
            payload: WorkerRequestPayload::ComputeDigest {
                input: "ok".into(),
                rounds: 4,
                delay_per_round_ms: 0,
                fail_with: None,
            },
        }];
        let mut changes = process_worker_requests(&mut reg, requests);
        changes.extend(wait_for_request_changes(
            &mut reg,
            1usize.saturating_sub(changes.len()),
            Duration::from_millis(100),
        ));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].state, WorkerState::Success);
        assert_eq!(reg.state(changes[0].worker_id), Some(&WorkerState::Success));
    }

    #[test]
    fn process_worker_requests_success_and_error_payloads() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(5);
        let requests = vec![
            WorkerRequest {
                owner,
                exclusive_key: None,
                name: Some("a".into()),
                payload: WorkerRequestPayload::ComputeDigest {
                    input: "alpha".into(),
                    rounds: 3,
                    delay_per_round_ms: 0,
                    fail_with: None,
                },
            },
            WorkerRequest {
                owner,
                exclusive_key: None,
                name: Some("b".into()),
                payload: WorkerRequestPayload::ComputeDigest {
                    input: "beta".into(),
                    rounds: 3,
                    delay_per_round_ms: 0,
                    fail_with: Some("payload-fail".into()),
                },
            },
            WorkerRequest {
                owner,
                exclusive_key: None,
                name: None,
                payload: WorkerRequestPayload::default(),
            },
        ];
        let mut changes = process_worker_requests(&mut reg, requests);
        changes.extend(wait_for_request_changes(
            &mut reg,
            3usize.saturating_sub(changes.len()),
            Duration::from_millis(150),
        ));

        assert_eq!(changes.len(), 3);
        assert_eq!(
            changes
                .iter()
                .filter(|c| c.state == WorkerState::Success)
                .count(),
            2
        );
        assert!(
            changes
                .iter()
                .any(|c| c.state == WorkerState::Error("payload-fail".into()))
        );
    }

    #[test]
    fn process_worker_requests_exclusive_cancels_previous() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(7);

        // First: register a worker manually, leave it running.
        let (prev_id, prev_token) = reg.register(owner, Some("search".into()), Some("old".into()));
        reg.set_running(prev_id);

        // Then: process an exclusive request with the same key.
        let requests = vec![WorkerRequest {
            owner,
            exclusive_key: Some("search".into()),
            name: Some("new".into()),
            payload: WorkerRequestPayload::default(),
        }];
        let mut changes = process_worker_requests(&mut reg, requests);
        changes.extend(wait_for_request_changes(
            &mut reg,
            2usize.saturating_sub(changes.len()),
            Duration::from_millis(150),
        ));

        // The previous worker should have been cancelled.
        assert_eq!(reg.state(prev_id), Some(&WorkerState::Cancelled));
        assert!(prev_token.is_cancelled());

        // One cancellation + one completion should be emitted.
        assert_eq!(changes.len(), 2);
        assert!(
            changes
                .iter()
                .any(|c| c.worker_id == prev_id && c.state == WorkerState::Cancelled)
        );
        assert!(changes.iter().any(|c| c.state == WorkerState::Success));
    }

    #[test]
    fn process_worker_requests_cleanup_removes_finished() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(3);
        let requests = vec![WorkerRequest {
            owner,
            exclusive_key: None,
            name: None,
            payload: WorkerRequestPayload::default(),
        }];
        let mut changes = process_worker_requests(&mut reg, requests);
        changes.extend(wait_for_request_changes(
            &mut reg,
            1usize.saturating_sub(changes.len()),
            Duration::from_millis(100),
        ));
        assert_eq!(changes.len(), 1);
        // Before cleanup: worker is in registry (Success state).
        assert!(reg.state(changes[0].worker_id).is_some());
        // After cleanup: removed.
        reg.cleanup();
        assert!(reg.state(changes[0].worker_id).is_none());
        assert!(reg.active_workers().is_empty());
    }

    #[test]
    fn process_worker_requests_drains_from_event_ctx() {
        use crate::event::EventCtx;

        let owner = node_id_from_ffi(20);
        let mut ctx = EventCtx::default();
        ctx.set_node_id(owner);
        ctx.request_worker(Some("fetch"));
        ctx.request_exclusive_worker("load", Some("loader"));

        let reqs = ctx.take_worker_requests();
        assert_eq!(reqs.len(), 2);

        let mut reg = WorkerRegistry::new();
        let mut changes = process_worker_requests(&mut reg, reqs);
        changes.extend(wait_for_request_changes(
            &mut reg,
            2usize.saturating_sub(changes.len()),
            Duration::from_millis(150),
        ));
        assert_eq!(changes.len(), 2);

        // Second take should be empty.
        let reqs2 = ctx.take_worker_requests();
        assert!(reqs2.is_empty());
    }

    #[test]
    fn process_worker_requests_cancelled_worker_reports_terminal_cancelled() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(21);
        let requests = vec![WorkerRequest {
            owner,
            exclusive_key: None,
            name: Some("slow".into()),
            payload: WorkerRequestPayload::ComputeDigest {
                input: "slow".into(),
                rounds: 50,
                delay_per_round_ms: 2,
                fail_with: None,
            },
        }];
        let _ = process_worker_requests(&mut reg, requests);
        let worker_id = *reg
            .active_workers()
            .first()
            .expect("expected spawned worker");

        reg.cancel(worker_id);
        let mut changes = process_worker_requests(&mut reg, Vec::new());
        changes.extend(wait_for_request_changes(
            &mut reg,
            1usize.saturating_sub(changes.len()),
            Duration::from_millis(150),
        ));

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].worker_id, worker_id);
        assert_eq!(changes[0].state, WorkerState::Cancelled);
        assert_eq!(reg.state(worker_id), Some(&WorkerState::Cancelled));
    }

    #[test]
    fn registry_spawn_reports_success() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(31);
        let id = reg.spawn(owner, None, Some("ok".into()), Box::new(|_token| Ok(())));
        assert_eq!(reg.state(id), Some(&WorkerState::Running));

        let changes = wait_for_changes(&mut reg, 1, Duration::from_millis(100));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].worker_id, id);
        assert_eq!(changes[0].state, WorkerState::Success);
        assert_eq!(reg.state(id), Some(&WorkerState::Success));
    }

    #[test]
    fn registry_spawn_reports_error() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(32);
        let id = reg.spawn(
            owner,
            None,
            Some("fail".into()),
            Box::new(|_token| Err("boom".into())),
        );
        let changes = wait_for_changes(&mut reg, 1, Duration::from_millis(100));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].worker_id, id);
        assert_eq!(changes[0].state, WorkerState::Error("boom".into()));
        assert_eq!(reg.state(id), Some(&WorkerState::Error("boom".into())));
    }

    #[test]
    fn registry_cancel_wins_over_late_completion() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(33);
        let id = reg.spawn(
            owner,
            None,
            Some("slow".into()),
            Box::new(|token| {
                for _ in 0..20 {
                    if token.is_cancelled() {
                        return Ok(());
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(())
            }),
        );
        reg.cancel(id);

        let changes = wait_for_changes(&mut reg, 1, Duration::from_millis(120));
        assert!(
            changes
                .iter()
                .any(|c| c.worker_id == id && c.state == WorkerState::Cancelled)
        );
        assert_eq!(reg.state(id), Some(&WorkerState::Cancelled));
    }

    #[test]
    fn registry_concurrent_workers_keep_terminal_states_deterministic() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(41);

        let fast_ok = reg.spawn(owner, None, Some("fast-ok".into()), Box::new(|_t| Ok(())));
        let fast_err = reg.spawn(
            owner,
            None,
            Some("fast-err".into()),
            Box::new(|_t| Err("e-fast".into())),
        );
        let slow_ok = reg.spawn(
            owner,
            None,
            Some("slow-ok".into()),
            Box::new(|token| {
                for _ in 0..8 {
                    if token.is_cancelled() {
                        return Ok(());
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(())
            }),
        );

        reg.cancel(slow_ok);

        let changes = wait_for_changes(&mut reg, 3, Duration::from_millis(150));
        assert_eq!(changes.len(), 3);
        assert!(
            changes
                .iter()
                .any(|c| c.worker_id == fast_ok && c.state == WorkerState::Success)
        );
        assert!(
            changes
                .iter()
                .any(|c| c.worker_id == fast_err && c.state == WorkerState::Error("e-fast".into()))
        );
        assert!(
            changes
                .iter()
                .any(|c| c.worker_id == slow_ok && c.state == WorkerState::Cancelled)
        );

        assert_eq!(reg.state(fast_ok), Some(&WorkerState::Success));
        assert_eq!(
            reg.state(fast_err),
            Some(&WorkerState::Error("e-fast".into()))
        );
        assert_eq!(reg.state(slow_ok), Some(&WorkerState::Cancelled));
    }

    #[test]
    fn registry_exclusive_replacement_cancels_previous_during_running_job() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(44);

        let first = reg.spawn(
            owner,
            Some("search".into()),
            Some("first".into()),
            Box::new(|token| {
                for _ in 0..25 {
                    if token.is_cancelled() {
                        return Ok(());
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(())
            }),
        );

        let second = reg.spawn(
            owner,
            Some("search".into()),
            Some("second".into()),
            Box::new(|_token| Ok(())),
        );

        let changes = wait_for_changes(&mut reg, 2, Duration::from_millis(200));
        assert_eq!(changes.len(), 2);
        assert!(
            changes
                .iter()
                .any(|c| c.worker_id == first && c.state == WorkerState::Cancelled)
        );
        assert!(
            changes
                .iter()
                .any(|c| c.worker_id == second && c.state == WorkerState::Success)
        );

        assert_eq!(reg.state(first), Some(&WorkerState::Cancelled));
        assert_eq!(reg.state(second), Some(&WorkerState::Success));
    }
}
