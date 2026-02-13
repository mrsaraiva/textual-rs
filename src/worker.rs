//! Worker abstraction for background tasks with lifecycle management.
//!
//! Provides cooperative cancellation, exclusive mode (cancel-previous semantics),
//! and per-owner cleanup — modelled after Python Textual's `Worker` system.
//!
//! The runtime owns a [`WorkerRegistry`] that tracks all active workers.
//! Widgets request workers via [`EventCtx`](crate::event::EventCtx); actual
//! task spawning is handled by the runtime (deferred to a future sprint).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

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

// ── WorkerRegistry ────────────────────────────────────────────────────

/// Tracks all active workers.
///
/// Owned by the runtime; **not** thread-safe (single event-loop access).
pub struct WorkerRegistry {
    workers: Vec<WorkerEntry>,
}

impl WorkerRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            workers: Vec::new(),
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
            }
        }
    }

    /// Cancel a specific worker by ID.
    pub fn cancel(&mut self, id: WorkerId) {
        if let Some(entry) = self.find_mut(id) {
            if !entry.state.is_finished() {
                entry.cancel_token.cancel();
                entry.state = WorkerState::Cancelled;
            }
        }
    }

    /// Cancel every worker owned by `owner` (e.g. when a widget is unmounted).
    pub fn cancel_by_owner(&mut self, owner: NodeId) {
        for entry in &mut self.workers {
            if entry.owner == owner && !entry.state.is_finished() {
                entry.cancel_token.cancel();
                entry.state = WorkerState::Cancelled;
            }
        }
    }

    /// Query the current state of a worker.
    pub fn state(&self, id: WorkerId) -> Option<&WorkerState> {
        self.find(id).map(|e| &e.state)
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
}

impl Default for WorkerRegistry {
    fn default() -> Self {
        Self::new()
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
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_id::node_id_from_ffi;

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
    }

    #[test]
    fn registry_complete_error() {
        let mut reg = WorkerRegistry::new();
        let owner = node_id_from_ffi(1);
        let (id, _) = reg.register(owner, None, None);
        reg.set_running(id);
        reg.complete(id, Err("boom".into()));
        assert_eq!(
            reg.state(id),
            Some(&WorkerState::Error("boom".into()))
        );
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

        let (id1, token1) =
            reg.register(owner, Some("fetch".into()), Some("first".into()));
        reg.set_running(id1);

        // Register a second worker with the same exclusive key + owner.
        let (id2, _token2) =
            reg.register(owner, Some("fetch".into()), Some("second".into()));

        assert_eq!(reg.state(id1), Some(&WorkerState::Cancelled));
        assert!(token1.is_cancelled());
        assert_eq!(reg.state(id2), Some(&WorkerState::Pending));
    }

    #[test]
    fn registry_exclusive_different_owner_no_cancel() {
        let mut reg = WorkerRegistry::new();
        let owner_a = node_id_from_ffi(1);
        let owner_b = node_id_from_ffi(2);

        let (id1, token1) =
            reg.register(owner_a, Some("fetch".into()), None);
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
        };
        assert_eq!(req.owner, owner);
        assert_eq!(req.exclusive_key.as_deref(), Some("load"));
        assert_eq!(req.name.as_deref(), Some("data-loader"));
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
}
