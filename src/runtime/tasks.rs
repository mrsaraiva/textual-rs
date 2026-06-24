use crate::message::{
    AsyncDirectoryEntry, AsyncTaskCancelled, AsyncTaskCompleted, AsyncTaskRequest, AsyncTaskResult,
    MessageEvent,
};
use crate::node_id::NodeId;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::ThreadId;

#[derive(Debug)]
struct RunningTask {
    generation: u64,
    target: NodeId,
    cancel_flag: Arc<AtomicBool>,
}

#[derive(Debug)]
struct TaskCompletion {
    task_id: u64,
    generation: u64,
    target: NodeId,
    result: AsyncTaskResult,
}

#[derive(Debug)]
pub(crate) struct AsyncTaskRuntime {
    running: HashMap<u64, RunningTask>,
    completion_tx: Sender<TaskCompletion>,
    completion_rx: Receiver<TaskCompletion>,
}

impl Default for AsyncTaskRuntime {
    fn default() -> Self {
        let (completion_tx, completion_rx) = mpsc::channel();
        Self {
            running: HashMap::new(),
            completion_tx,
            completion_rx,
        }
    }
}

impl AsyncTaskRuntime {
    pub(crate) fn spawn(
        &mut self,
        task_id: u64,
        target: NodeId,
        request: AsyncTaskRequest,
    ) -> Option<MessageEvent> {
        let (previous_generation, replaced) = if let Some(previous) = self.running.remove(&task_id)
        {
            previous.cancel_flag.store(true, Ordering::Relaxed);
            (previous.generation, {
                let sender = super::App::runtime_message_sender();
                Some(
                    MessageEvent::new(
                        sender,
                        AsyncTaskCancelled {
                            task_id,
                            target: previous.target,
                        },
                    )
                    .with_control(sender),
                )
            })
        } else {
            (0, None)
        };
        let generation = previous_generation + 1;
        let cancel_flag = Arc::new(AtomicBool::new(false));

        self.running.insert(
            task_id,
            RunningTask {
                generation,
                target,
                cancel_flag: Arc::clone(&cancel_flag),
            },
        );

        let tx = self.completion_tx.clone();
        std::thread::spawn(move || {
            if cancel_flag.load(Ordering::Relaxed) {
                return;
            }
            let result = execute_request(request);
            if cancel_flag.load(Ordering::Relaxed) {
                return;
            }
            let _ = tx.send(TaskCompletion {
                task_id,
                generation,
                target,
                result,
            });
        });
        replaced
    }

    pub(crate) fn cancel(&mut self, task_id: u64) -> Option<MessageEvent> {
        let task = self.running.remove(&task_id)?;
        task.cancel_flag.store(true, Ordering::Relaxed);
        {
            let sender = super::App::runtime_message_sender();
            Some(
                MessageEvent::new(
                    sender,
                    AsyncTaskCancelled {
                        task_id,
                        target: task.target,
                    },
                )
                .with_control(sender),
            )
        }
    }

    pub(crate) fn cancel_for_target(&mut self, target: NodeId) -> Vec<MessageEvent> {
        let ids = self
            .running
            .iter()
            .filter_map(|(task_id, task)| (task.target == target).then_some(*task_id))
            .collect::<Vec<_>>();
        let mut cancelled = Vec::new();
        for task_id in ids {
            if let Some(event) = self.cancel(task_id) {
                cancelled.push(event);
            }
        }
        cancelled
    }

    pub(crate) fn drain_completed(&mut self) -> Vec<MessageEvent> {
        let mut out = Vec::new();
        loop {
            let completion = match self.completion_rx.try_recv() {
                Ok(completion) => completion,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            };

            let Some(active) = self.running.get(&completion.task_id) else {
                continue;
            };
            if active.generation != completion.generation {
                continue;
            }

            self.running.remove(&completion.task_id);
            let sender = super::App::runtime_message_sender();
            out.push(
                MessageEvent::new(
                    sender,
                    AsyncTaskCompleted {
                        task_id: completion.task_id,
                        target: completion.target,
                        result: completion.result,
                    },
                )
                .with_control(sender),
            );
        }
        out
    }
}

// ── call_from_thread: synchronous UI-thread dispatch ─────────────────────
//
// Mirrors Python Textual `App.call_from_thread`. A worker thread posts a
// callable onto a global queue; the UI thread (event loop) drains the queue
// once per tick, runs each callable with `&mut App`, and signals completion.
// The worker blocks until its callable has run and its return value is
// available.
//
// Why a global queue (not the `AsyncTaskRuntime`/`WorkerRegistry` mpsc
// channels): those channels ferry *data* (results) back to the UI thread to
// be applied later. `call_from_thread` instead ferries *behavior* — an
// arbitrary closure that must execute *in the app context* (`&mut App`) and
// return a value synchronously to the caller. Worker jobs run on detached
// `std::thread` threads, so this queue must be process-global and `Send`.

/// A type-erased unit of UI-thread work posted by a worker thread.
///
/// The closure is boxed `FnOnce(&mut crate::runtime::App)`; the original
/// caller's return value is captured inside the closure and shipped back to
/// the worker over a oneshot channel before the closure returns.
type CallFromThreadJob = Box<dyn FnOnce(&mut crate::runtime::App) + Send + 'static>;

/// Process-global bridge for `App::call_from_thread`.
///
/// Holds the pending UI-thread jobs and the id of the thread currently running
/// the event loop (used to reject same-thread calls, matching Python's
/// `RuntimeError` when `call_from_thread` is invoked on the app thread).
struct CallFromThreadBridge {
    queue: Mutex<VecDeque<CallFromThreadJob>>,
    /// Thread id of the active event loop, or `0` when no app is running.
    ///
    /// Stored as the raw `ThreadId` hash so it can live in an atomic; we keep
    /// the real `ThreadId` alongside in the mutex for exact comparison.
    ui_thread: Mutex<Option<ThreadId>>,
    /// `true` while an event loop is running and able to drain the queue.
    running: AtomicBool,
    /// Generation counter bumped each time the UI thread (re)registers, so a
    /// blocked worker can detect app shutdown if needed.
    generation: AtomicU64,
}

impl CallFromThreadBridge {
    fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            ui_thread: Mutex::new(None),
            running: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        }
    }
}

fn call_from_thread_bridge() -> &'static CallFromThreadBridge {
    static BRIDGE: OnceLock<CallFromThreadBridge> = OnceLock::new();
    BRIDGE.get_or_init(CallFromThreadBridge::new)
}

/// Register the calling thread as the UI/event-loop thread.
///
/// Called by the event loop at startup. Enables `call_from_thread` and lets it
/// detect (and reject) calls made from the UI thread itself.
pub(crate) fn register_ui_thread() {
    let bridge = call_from_thread_bridge();
    *bridge
        .ui_thread
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(std::thread::current().id());
    bridge.generation.fetch_add(1, Ordering::SeqCst);
    bridge.running.store(true, Ordering::SeqCst);
}

/// Unregister the UI thread when the event loop exits.
///
/// Pending jobs are drained-and-dropped here; the corresponding workers'
/// oneshot receivers see a disconnected channel and their `call_from_thread`
/// calls return the configured shutdown result.
pub(crate) fn unregister_ui_thread() {
    let bridge = call_from_thread_bridge();
    bridge.running.store(false, Ordering::SeqCst);
    *bridge
        .ui_thread
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
    // Drop any pending jobs so blocked workers unblock (their result senders
    // are dropped inside the job closures we discard here).
    bridge
        .queue
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
}

/// `true` when the calling thread is the active UI/event-loop thread.
pub(crate) fn is_ui_thread() -> bool {
    let bridge = call_from_thread_bridge();
    matches!(
        *bridge.ui_thread.lock().unwrap_or_else(|e| e.into_inner()),
        Some(id) if id == std::thread::current().id()
    )
}

/// Whether an event loop is currently registered to drain the queue.
pub(crate) fn ui_thread_running() -> bool {
    call_from_thread_bridge().running.load(Ordering::SeqCst)
}

/// Errors that prevent a [`crate::runtime::App::call_from_thread`] from being
/// dispatched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallFromThreadError {
    /// No event loop is running to service the callable.
    NotRunning,
    /// Called from the UI thread itself (would deadlock).
    SameThread,
    /// The app shut down before the callable could run.
    Disconnected,
}

impl std::fmt::Display for CallFromThreadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRunning => write!(f, "App is not running"),
            Self::SameThread => write!(
                f,
                "call_from_thread must run in a different thread from the app"
            ),
            Self::Disconnected => write!(f, "App shut down before the callable could run"),
        }
    }
}

impl std::error::Error for CallFromThreadError {}

/// Post `job` to the UI thread and block until it has executed there.
///
/// The closure runs with `&mut App` on the event-loop thread and its return
/// value is shipped back to this (worker) thread. Mirrors Python
/// `App.call_from_thread(callback, *args)` which posts a coroutine onto the
/// app loop and blocks on the resulting `Future`.
///
/// Returns `Err` (without blocking) if no app is running or if called on the
/// UI thread, and `Err(Disconnected)` if the app shuts down before the job
/// runs.
pub(crate) fn call_from_thread<F, R>(job: F) -> Result<R, CallFromThreadError>
where
    F: FnOnce(&mut crate::runtime::App) -> R + Send + 'static,
    R: Send + 'static,
{
    let bridge = call_from_thread_bridge();
    if !ui_thread_running() {
        return Err(CallFromThreadError::NotRunning);
    }
    if is_ui_thread() {
        return Err(CallFromThreadError::SameThread);
    }

    let (result_tx, result_rx) = mpsc::channel::<R>();
    let boxed: CallFromThreadJob = Box::new(move |app: &mut crate::runtime::App| {
        let value = job(app);
        // Worker may already be gone (timed out / detached); ignore send error.
        let _ = result_tx.send(value);
    });

    bridge
        .queue
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push_back(boxed);

    // Block until the UI thread runs the job and sends the result, or until the
    // app shuts down and drops the job (closing the channel).
    result_rx.recv().map_err(|_| CallFromThreadError::Disconnected)
}

/// Drain and return all pending `call_from_thread` jobs.
///
/// Invoked by the event loop once per tick. Each returned job must be run with
/// `&mut App` to unblock its worker.
pub(crate) fn drain_call_from_thread_jobs() -> Vec<CallFromThreadJob> {
    let bridge = call_from_thread_bridge();
    let mut queue = bridge.queue.lock().unwrap_or_else(|e| e.into_inner());
    queue.drain(..).collect()
}

// ── push_screen_wait: worker-suspending screen push ──────────────────────
//
// Mirrors Python Textual `App.push_screen_wait(screen)`:
//
//     result = await self.push_screen_wait(QuestionScreen(...))
//
// which (via `push_screen(..., wait_for_dismiss=True)`) pushes a screen and
// suspends the calling `@work` worker on an `asyncio.Future`. The future is
// resolved by the screen's result callback when the screen dismisses with a
// value, and the worker resumes with that value.
//
// Rust mapping: a threaded worker (the `tasks`/`worker` subsystem) calls this
// from inside its job closure. We coordinate with the UI thread exactly the way
// `call_from_thread` does — post the push onto the UI thread, where it runs with
// `&mut App` — but instead of completing immediately, the pushed screen's result
// callback ferries the eventual `ScreenResult` back over a oneshot channel. The
// worker blocks on that channel until the screen dismisses, then resumes with
// the result (the analogue of awaiting the future).

use crate::screen::{Screen, ScreenResult};

/// Errors that prevent a [`crate::runtime::App::push_screen_wait`] from
/// suspending the calling worker on a screen result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushScreenWaitError {
    /// No event loop is running to mount/drive the screen.
    NotRunning,
    /// Called on the UI/event-loop thread instead of a worker thread.
    ///
    /// Mirrors Python's `NoActiveWorker`: `push_screen_wait` may only be awaited
    /// from inside a worker, never on the app thread (it would deadlock the loop
    /// it is waiting on).
    NoActiveWorker,
    /// The app shut down before the screen was dismissed.
    Disconnected,
}

impl std::fmt::Display for PushScreenWaitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRunning => write!(f, "App is not running"),
            Self::NoActiveWorker => write!(
                f,
                "push_screen_wait must be called from a worker thread, not the app thread"
            ),
            Self::Disconnected => write!(f, "App shut down before the screen was dismissed"),
        }
    }
}

impl std::error::Error for PushScreenWaitError {}

/// Push `screen` onto the screen stack from a worker thread and block until the
/// screen is dismissed, returning the dismiss [`ScreenResult`].
///
/// The push runs on the UI thread (via the `call_from_thread` queue) with a
/// result callback that ships the `ScreenResult` back over a oneshot channel.
/// This (worker) thread then blocks on that channel until the screen dismisses
/// — mirroring Python `await self.push_screen_wait(screen)` suspending a worker
/// on the screen-result future.
///
/// Returns `Err` without pushing if no app is running or if called on the UI
/// thread, and `Err(Disconnected)` if the app shuts down before dismissal.
pub(crate) fn push_screen_wait(
    screen: Box<dyn Screen>,
) -> Result<ScreenResult, PushScreenWaitError> {
    if !ui_thread_running() {
        return Err(PushScreenWaitError::NotRunning);
    }
    // Python raises `NoActiveWorker` when `wait_for_dismiss` is requested outside
    // a worker. On the UI thread, blocking on dismissal would deadlock the loop
    // that must drain the dismissal — so reject it the same way.
    if is_ui_thread() {
        return Err(PushScreenWaitError::NoActiveWorker);
    }

    // Oneshot channel carrying the eventual dismiss result from the UI thread
    // (where the screen callback fires) back to this worker thread.
    let (result_tx, result_rx) = mpsc::channel::<ScreenResult>();

    // Post the push onto the UI thread. `push_screen_with_callback` registers a
    // callback the runtime invokes when the screen is dismissed (popped); that
    // callback forwards the result over our channel.
    let push = move |app: &mut crate::runtime::App| {
        app.push_screen_with_callback(
            screen,
            Box::new(move |result: ScreenResult| {
                // Worker may already be gone (app shutdown / detached); ignore.
                let _ = result_tx.send(result);
            }),
        );
    };

    // `call_from_thread` itself blocks only until the push has run on the UI
    // thread (fast); the screen lifetime is governed by the dismissal channel.
    call_from_thread(push).map_err(|err| match err {
        CallFromThreadError::NotRunning => PushScreenWaitError::NotRunning,
        CallFromThreadError::SameThread => PushScreenWaitError::NoActiveWorker,
        CallFromThreadError::Disconnected => PushScreenWaitError::Disconnected,
    })?;

    // Suspend the worker until the screen is dismissed (the callback sends), or
    // the app shuts down (callback dropped → channel closed → Disconnected).
    result_rx
        .recv()
        .map_err(|_| PushScreenWaitError::Disconnected)
}

fn execute_request(request: AsyncTaskRequest) -> AsyncTaskResult {
    match request {
        AsyncTaskRequest::ReadDirectory { path, show_hidden } => {
            read_directory_request(path, show_hidden)
        }
        AsyncTaskRequest::Sleep { duration, label } => {
            let start = std::time::Instant::now();
            std::thread::sleep(duration);
            AsyncTaskResult::SleepFinished {
                label,
                elapsed: start.elapsed(),
            }
        }
    }
}

fn read_directory_request(path: String, show_hidden: bool) -> AsyncTaskResult {
    let path_buf = PathBuf::from(&path);
    let read_dir = match fs::read_dir(&path_buf) {
        Ok(read_dir) => read_dir,
        Err(error) => {
            return AsyncTaskResult::Failed {
                path,
                error: error.to_string(),
            };
        }
    };

    let mut entries = Vec::new();
    for entry in read_dir.flatten() {
        let entry_path = entry.path();
        let Some(name) = entry_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        entries.push(AsyncDirectoryEntry {
            path: entry_path.display().to_string(),
            label: name.to_string(),
            is_dir,
        });
    }

    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
            .then_with(|| left.label.cmp(&right.label))
    });

    AsyncTaskResult::DirectoryEntries { path, entries }
}

#[cfg(test)]
mod tests {
    use super::AsyncTaskRuntime;
    use crate::message::{AsyncTaskCancelled, AsyncTaskCompleted, AsyncTaskRequest};
    use crate::node_id::node_id_from_ffi;
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    struct TempTreeDir {
        path: PathBuf,
    }

    impl TempTreeDir {
        fn new(label: &str) -> Self {
            let mut path = std::env::temp_dir();
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock before epoch")
                .as_nanos();
            path.push(format!("textual-rs-{label}-{}-{stamp}", std::process::id()));
            fs::create_dir_all(&path).expect("create temp test directory");
            Self { path }
        }
    }

    impl Drop for TempTreeDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn wait_for_messages(runtime: &mut AsyncTaskRuntime) -> Vec<crate::message::MessageEvent> {
        for _ in 0..200 {
            let drained = runtime.drain_completed();
            if !drained.is_empty() {
                return drained;
            }
            thread::sleep(Duration::from_millis(1));
        }
        Vec::new()
    }

    #[test]
    fn read_directory_task_completes_with_directory_entries() {
        let temp = TempTreeDir::new("async-task-complete");
        fs::create_dir_all(temp.path.join("nested")).expect("create nested dir");
        fs::write(temp.path.join("alpha.txt"), "alpha").expect("write file");

        let target_id = node_id_from_ffi(10);
        let mut runtime = AsyncTaskRuntime::default();
        runtime.spawn(
            7,
            target_id,
            AsyncTaskRequest::ReadDirectory {
                path: temp.path.display().to_string(),
                show_hidden: false,
            },
        );

        let messages = wait_for_messages(&mut runtime);
        assert_eq!(messages.len(), 1);
        {
            let m = messages[0].downcast_ref::<AsyncTaskCompleted>().unwrap();
            assert_eq!(m.task_id, 7);
            assert_eq!(m.target, target_id);
        }
    }

    // ── call_from_thread bridge ───────────────────────────────────────

    use super::{
        CallFromThreadError, call_from_thread, drain_call_from_thread_jobs, is_ui_thread,
        register_ui_thread, ui_thread_running, unregister_ui_thread,
    };
    use std::sync::Mutex as StdMutex;
    use std::sync::mpsc;

    /// Serialize bridge tests: the bridge is process-global, so concurrent
    /// register/unregister from parallel tests would interfere.
    static BRIDGE_TEST_LOCK: StdMutex<()> = StdMutex::new(());

    #[test]
    fn call_from_thread_not_running_returns_error_without_blocking() {
        let _guard = BRIDGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Ensure no app registered.
        unregister_ui_thread();
        assert!(!ui_thread_running());
        let result = call_from_thread(|_app| 7_u32);
        assert_eq!(result, Err(CallFromThreadError::NotRunning));
    }

    #[test]
    fn call_from_thread_same_thread_is_rejected() {
        let _guard = BRIDGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        register_ui_thread();
        assert!(is_ui_thread());
        // Calling on the UI thread itself must not deadlock; it errors instead.
        let result = call_from_thread(|_app| 1_u32);
        assert_eq!(result, Err(CallFromThreadError::SameThread));
        unregister_ui_thread();
    }

    #[test]
    fn call_from_thread_round_trips_value_and_runs_with_app() {
        let _guard = BRIDGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // This thread plays the role of the UI/event-loop thread, holding the
        // single `&mut App`.
        let mut app = crate::runtime::App::new().expect("app should initialize");
        register_ui_thread();
        assert!(ui_thread_running());

        // A worker thread posts a callable via `call_from_thread` and blocks for
        // its result. The callable receives `&mut App` and returns a value.
        let (enqueued_tx, enqueued_rx) = mpsc::channel::<()>();
        let worker = std::thread::spawn(move || {
            // Signal that we're about to enqueue, then make the blocking call.
            // (The signal is best-effort; the real synchronization is the
            // blocking `recv` inside `call_from_thread`.)
            let _ = enqueued_tx.send(());
            // The closure proves it runs *on the UI thread* by asserting
            // `is_ui_thread()` from inside, and exercises real `&mut App` access.
            call_from_thread(|app| {
                assert!(
                    is_ui_thread(),
                    "callable must run on the UI/event-loop thread"
                );
                // Touch the app to prove `&mut App` access is real.
                let _ = app.theme_name().to_string();
                40 + 2_i64
            })
        });

        // Wait for the worker to start, then drain+run jobs on this UI thread
        // until the worker's blocking call has been serviced.
        enqueued_rx.recv().expect("worker started");
        let worker_value = {
            let mut value = None;
            for _ in 0..2000 {
                for job in drain_call_from_thread_jobs() {
                    job(&mut app);
                }
                if worker.is_finished() {
                    value = Some(worker.join().expect("worker joined"));
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            value.expect("worker completed within timeout")
        };

        assert_eq!(worker_value, Ok(42));
        unregister_ui_thread();
    }

    #[test]
    fn unregister_drops_pending_jobs_and_unblocks_worker() {
        let _guard = BRIDGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        register_ui_thread();

        // Worker posts a job but the UI thread never drains it — instead the app
        // "shuts down" (unregister). The worker must unblock with Disconnected.
        let worker = std::thread::spawn(move || call_from_thread(|_app| 99_u8));

        // Give the worker a moment to enqueue, then shut down without draining.
        std::thread::sleep(std::time::Duration::from_millis(20));
        unregister_ui_thread();

        let result = worker.join().expect("worker joined");
        // The worker must not block past shutdown. Depending on the exact
        // interleaving it either enqueued before shutdown (job dropped →
        // Disconnected) or observed `running == false` first (NotRunning).
        // Both are non-blocking terminal outcomes; the invariant under test is
        // "shutdown never leaves a worker blocked forever".
        assert!(
            matches!(
                result,
                Err(CallFromThreadError::Disconnected) | Err(CallFromThreadError::NotRunning)
            ),
            "worker must unblock on shutdown, got {result:?}"
        );
    }

    #[test]
    fn cancelling_running_task_emits_cancelled_and_suppresses_completion() {
        let temp = TempTreeDir::new("async-task-cancel");
        fs::write(temp.path.join("alpha.txt"), "alpha").expect("write file");

        let target_id = node_id_from_ffi(22);
        let mut runtime = AsyncTaskRuntime::default();
        runtime.spawn(
            5,
            target_id,
            AsyncTaskRequest::ReadDirectory {
                path: temp.path.display().to_string(),
                show_hidden: false,
            },
        );

        let cancelled = runtime.cancel(5).expect("cancelled message");
        {
            let m = cancelled.downcast_ref::<AsyncTaskCancelled>().unwrap();
            assert_eq!(m.task_id, 5);
            assert_eq!(m.target, target_id);
        }

        let messages = wait_for_messages(&mut runtime);
        assert!(messages.is_empty());
    }

    // ── push_screen_wait ──────────────────────────────────────────────

    use super::{PushScreenWaitError, push_screen_wait};
    use crate::message::ButtonPressed;
    use crate::screen::{Screen, ScreenMessageCtx, ScreenResult};
    use crate::widgets::Widget;

    /// A screen mirroring Python `QuestionScreen(Screen[bool])`: a press of the
    /// `#yes` button dismisses with `true`, any other with `false`.
    struct AnswerScreen;

    impl Screen for AnswerScreen {
        fn name(&self) -> &str {
            "AnswerScreen"
        }

        fn compose(&self) -> Box<dyn Widget> {
            struct Body;
            impl Widget for Body {
                fn render(
                    &self,
                    _c: &rich_rs::Console,
                    _o: &rich_rs::ConsoleOptions,
                ) -> rich_rs::Segments {
                    rich_rs::Segments::new()
                }
                fn style_type(&self) -> &'static str {
                    "AnswerScreenBody"
                }
            }
            Box::new(Body)
        }

        fn on_button_pressed(
            &mut self,
            pressed: &ButtonPressed,
            _control: crate::node_id::NodeId,
            ctx: &mut ScreenMessageCtx,
        ) {
            ctx.dismiss(pressed.button_id.as_deref() == Some("yes"));
        }
    }

    /// `push_screen_wait` outside a running app errors without blocking.
    #[test]
    fn push_screen_wait_not_running_errors() {
        let _guard = BRIDGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unregister_ui_thread();
        assert!(!ui_thread_running());
        let result = push_screen_wait(Box::new(AnswerScreen));
        assert!(matches!(result, Err(PushScreenWaitError::NotRunning)));
    }

    /// Called on the UI thread itself, `push_screen_wait` is rejected as
    /// `NoActiveWorker` (Python parity) rather than deadlocking.
    #[test]
    fn push_screen_wait_on_ui_thread_is_rejected() {
        let _guard = BRIDGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        register_ui_thread();
        assert!(is_ui_thread());
        let result = push_screen_wait(Box::new(AnswerScreen));
        assert!(matches!(result, Err(PushScreenWaitError::NoActiveWorker)));
        unregister_ui_thread();
    }

    /// End-to-end: a worker thread calls `push_screen_wait`, the UI thread drives
    /// the push (via the call_from_thread queue) and then a button press on the
    /// screen dismisses it with a value; the worker must resume with that value.
    ///
    /// This plays the role of the event loop on the test (UI) thread: it drains
    /// the call_from_thread queue (running the push with `&mut App`), then
    /// dispatches a real `ButtonPressed` into the active screen tree — exercising
    /// `Screen::on_button_pressed` → `ctx.dismiss(..)` — and drains screen
    /// dismissals, which pops the screen and fires the result callback that
    /// resumes the worker.
    #[test]
    fn push_screen_wait_resumes_worker_with_dismiss_value() {
        let _guard = BRIDGE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let mut app = crate::runtime::App::new().expect("app should initialize");
        register_ui_thread();
        assert!(ui_thread_running());

        // Worker thread: push the screen and suspend on its dismissal. The
        // returned ScreenResult is mapped to the bool the test asserts on.
        let (started_tx, started_rx) = mpsc::channel::<()>();
        let worker = std::thread::spawn(move || {
            let _ = started_tx.send(());
            let result = push_screen_wait(Box::new(AnswerScreen))?;
            Ok::<bool, PushScreenWaitError>(match result {
                ScreenResult::Value(v) => *v.downcast::<bool>().unwrap_or_default(),
                ScreenResult::Dismissed => false,
            })
        });

        started_rx.recv().expect("worker started");

        // Phase 1: drain the call_from_thread queue until the push has run on the
        // UI thread (the screen is now on the stack).
        let mut pushed = false;
        for _ in 0..2000 {
            for job in drain_call_from_thread_jobs() {
                job(&mut app);
            }
            if app.screen_count() == 1 {
                pushed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(pushed, "worker's push_screen_wait should mount the screen");
        // The worker is now suspended on the dismissal; it has not finished.
        assert!(!worker.is_finished(), "worker must block until dismissed");

        // Phase 2: simulate a #yes button press routed into the active screen
        // tree. The screen's on_button_pressed stages a dismissal; draining it
        // pops the screen and fires the callback that resumes the worker.
        let yes = crate::node_id::node_id_from_ffi(7);
        let message = crate::message::MessageEvent::new(
            yes,
            ButtonPressed {
                description: "Yes".into(),
                button_id: Some("yes".into()),
            },
        )
        .with_control(yes);
        {
            let tree = app
                .active_widget_tree_mut()
                .expect("active screen tree should exist");
            let _ = crate::runtime::dispatch_message_queue_tree(tree, vec![message]);
        }
        app.drain_screen_dismissals();
        assert_eq!(
            app.screen_count(),
            0,
            "screen should be popped after dismiss"
        );

        // Phase 3: the worker resumes with the dismiss value.
        let value = {
            let mut got = None;
            for _ in 0..2000 {
                if worker.is_finished() {
                    got = Some(worker.join().expect("worker joined"));
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            got.expect("worker resumed after dismissal")
        };
        assert_eq!(value, Ok(true), "worker resumes with the dismiss value");

        unregister_ui_thread();
    }
}
