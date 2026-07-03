//! Deferred widget-command queue (WidgetCtx build, sub-step 1).
//!
//! Handlers run while the runtime holds a live `&mut` borrow of the widget
//! tree (see `routing.rs:169-182`), so a handler cannot mutate a *different*
//! node — or its own DOM identity — in place. Instead every cross-node / DOM
//! side effect is recorded as a [`WidgetCommand`] on a thread-local FIFO and
//! applied later, during the shared post-dispatch flush
//! ([`App::run_event_loop_reactive_phase`]), when no tree borrow is held.
//!
//! This mirrors the reactive queue (`reactive::RUNTIME_REACTIVE_QUEUE`) and is
//! drained by the *same* flush function the reactive queue is — so both the
//! live event loop (`run_widget_tree`) and the headless pump (`headless_pump`)
//! converge commands with zero extra plumbing (the loop-convergence keystone).
//!
//! Commands so far: `AddClass`/`RemoveClass` (step 1), `UpdateWidget` (step 2),
//! and the widget-owned timer commands `RegisterTimer`/`PauseTimer`/
//! `ResumeTimer`/`StopTimer` (step 4). The mount/style/recompose/post commands
//! arrive in step 5.

use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use super::App;
use super::event_loop::InvalidationScope;
use super::types::{DispatchOutcome, PendingInvalidation};
use crate::event::{EventCtx, InvalidationFlags, WidgetCtx};
use crate::node_id::NodeId;
use crate::widgets::Widget;

/// Per-fire timing handed to a widget-owned interval callback
/// ([`WidgetCtx::set_interval`](crate::event::WidgetCtx::set_interval)).
///
/// `elapsed` is the real clock time since this timer's previous fire (since its
/// registration for the first fire), read from the same clock the timer runtime
/// schedules against — so it is deterministic under `Pilot::advance_clock` and
/// drift-free against wall-clock time live. This mirrors Python's
/// `monotonic() - start` derivation: a time-accumulating callback adds
/// `tick.elapsed` per fire rather than a fixed nominal interval, so a coalesced
/// (skipped) backlog fire still advances by the true elapsed time.
#[derive(Debug, Clone, Copy)]
pub struct TimerTick {
    /// Real clock time elapsed since the previous fire (or registration).
    pub elapsed: Duration,
    /// 1-based count of how many times this timer has fired, including this one.
    pub fire_count: u64,
}

/// A widget-owned timer callback: runs against the concrete widget (downcast at
/// enqueue, same pattern as `UpdateWidget`) with a fresh `WidgetCtx` and the
/// per-fire [`TimerTick`] on each fire.
pub(crate) type WidgetTimerCallback =
    Box<dyn FnMut(&mut dyn Widget, &mut WidgetCtx, TimerTick) + Send>;

/// Registry entry for a widget-owned interval timer: its owning node, the
/// downcast-wrapped callback, and the timing state used to derive each fire's
/// [`TimerTick::elapsed`] / [`TimerTick::fire_count`].
pub(crate) struct WidgetTimerEntry {
    pub node: NodeId,
    pub callback: WidgetTimerCallback,
    /// Clock time of the previous fire (or registration), for `elapsed`.
    pub last_fire: Instant,
    /// Number of fires so far (0 until the first fire).
    pub fire_count: u64,
}

/// Target of a deferred command. **Resolved at drain time**, never at enqueue
/// time: the tree is borrowed during the enqueuing handler, and an earlier
/// command in the same flush may have mounted the intended target.
#[derive(Debug, Clone)]
pub(crate) enum CommandTarget {
    /// An already-resolved node identity (generational — a stale id resolves to
    /// `None` at drain and the command is dropped, never panics). Used by
    /// `Handle::update_via` and self-targeting `WidgetCtx::add_class`.
    Node(NodeId),
    /// A CSS selector resolved against the subtree rooted at `root` at drain
    /// time. Used by `WidgetCtx::query_one_id("#id")`.
    Selector { root: NodeId, sel: String },
    /// The single descendant of `root` whose concrete type is `ty`, resolved by
    /// downcast at drain time. Used by the type-form `WidgetCtx::query_one::<W>()`.
    TypeMatch { root: NodeId, ty: TypeId },
}

/// A deferred DOM/cross-node side effect recorded by a handler and applied in
/// the post-dispatch flush.
pub(crate) enum WidgetCommand {
    /// Add a CSS class to the target node.
    AddClass { target: CommandTarget, class: String },
    /// Remove a CSS class from the target node.
    RemoveClass { target: CommandTarget, class: String },
    /// Run a downcast-wrapped closure against the resolved target widget with a
    /// fresh `WidgetCtx`. The closure is erased to `&mut dyn Widget` and
    /// downcasts to its captured concrete type at drain (a miss logs + drops).
    UpdateWidget {
        target: CommandTarget,
        apply: Box<dyn FnOnce(&mut dyn Widget, &mut WidgetCtx) + Send>,
    },
    /// Apply a closure to the target node's inline styles (Python
    /// `widget.styles.<prop> = v`). Homes the post-mount inline-style write path
    /// that `take_inline_style_writethrough` used to stage on the widget struct:
    /// the seed is drained at mount, so a post-mount style write must reach the
    /// arena node record directly. Applied via `WidgetTree::update_styles`.
    UpdateStyles {
        target: CommandTarget,
        apply: Box<dyn FnOnce(&mut crate::widgets::WidgetStyles) + Send>,
    },
    /// Register a widget-owned interval timer on the SAME `TimerRuntime` as
    /// app-level timers (so `enable_manual_timer_clock` / `Pilot::advance_clock`
    /// drive it deterministically). `timer_id` is pre-allocated by
    /// `alloc_widget_timer_id` so `set_interval` can return a stable `TimerHandle`.
    RegisterTimer {
        node: NodeId,
        timer_id: u64,
        interval: Duration,
        paused: bool,
        callback: WidgetTimerCallback,
    },
    /// Pause a timer by id (deferred `TimerHandle::pause`).
    PauseTimer(u64),
    /// Resume a timer by id (deferred `TimerHandle::resume`).
    ResumeTimer(u64),
    /// Stop + forget a timer by id (deferred `TimerHandle::stop`).
    StopTimer(u64),
}

thread_local! {
    /// Monotonic source of widget-owned timer ids, tagged into a high range
    /// (bit 63 set) so they never collide with app-level ids (`[1<<32, ..)`) or
    /// widget message-only ids (`[0, 1<<32)`). `set_interval` allocates here so
    /// it can return a `TimerHandle` synchronously without `&mut App`.
    static WIDGET_TIMER_ID: Cell<u64> = const { Cell::new(0) };
}

/// Bit 63 tag marking a timer id as a widget-owned-callback timer.
const WIDGET_TIMER_TAG: u64 = 1u64 << 63;

/// Allocate a fresh, process-unique widget-owned timer id (UI thread).
pub(crate) fn alloc_widget_timer_id() -> u64 {
    WIDGET_TIMER_ID.with(|c| {
        let next = c.get().wrapping_add(1);
        c.set(next);
        WIDGET_TIMER_TAG | next
    })
}

thread_local! {
    /// FIFO of commands enqueued on the current (UI) thread since the last drain.
    static RUNTIME_COMMAND_QUEUE: RefCell<Vec<WidgetCommand>> = const { RefCell::new(Vec::new()) };
}

/// Enqueue a deferred widget command onto the UI-thread FIFO.
///
/// The queue is thread-local and drained on the UI thread; worker threads must
/// route their side effects through the worker channel instead (a worker that
/// enqueued here would push onto its *own* thread-local queue, which the UI
/// thread never drains — the command would be silently lost). The debug assert
/// catches that misuse while a live event loop is running.
pub(crate) fn enqueue_widget_command(cmd: WidgetCommand) {
    debug_assert!(
        !crate::runtime::tasks::ui_thread_running() || crate::runtime::tasks::is_ui_thread(),
        "WidgetCommand enqueued off the UI thread; workers must use the worker channel"
    );
    RUNTIME_COMMAND_QUEUE.with(|queue| queue.borrow_mut().push(cmd));
}

/// Drain all queued commands (FIFO order preserved).
pub(crate) fn take_widget_commands() -> Vec<WidgetCommand> {
    RUNTIME_COMMAND_QUEUE.with(|queue| std::mem::take(&mut *queue.borrow_mut()))
}

/// Whether the command queue currently holds any pending commands (without
/// draining). Lets the headless pump decide whether the flush has work to do.
pub(crate) fn command_queue_is_nonempty() -> bool {
    RUNTIME_COMMAND_QUEUE.with(|queue| !queue.borrow().is_empty())
}

/// Test/observability hook: drain the deferred command queue and return the
/// `(node, ClassOp)` pairs from `AddClass`/`RemoveClass` commands (dropping any
/// non-class commands). Post-RA2.3 a handler's `ctx.add_class`/`remove_class`
/// enqueues these commands instead of writing the dispatch `EventCtx`'s class-op
/// list, so tests that formerly inspected `outcome.class_ops` drain here instead.
#[doc(hidden)]
pub fn drain_class_commands_for_test() -> Vec<(NodeId, crate::event::ClassOp)> {
    take_widget_commands()
        .into_iter()
        .filter_map(|cmd| match cmd {
            WidgetCommand::AddClass {
                target: CommandTarget::Node(node),
                class,
            } => Some((node, crate::event::ClassOp::Add(class))),
            WidgetCommand::RemoveClass {
                target: CommandTarget::Node(node),
                class,
            } => Some((node, crate::event::ClassOp::Remove(class))),
            _ => None,
        })
        .collect()
}

impl App {
    /// Resolve a [`CommandTarget`] to a live node id, or `None` if the target no
    /// longer exists / does not match (drop + debug log, **never panic**).
    pub(crate) fn resolve_command_target(&self, target: &CommandTarget) -> Option<NodeId> {
        match target {
            CommandTarget::Node(id) => {
                let tree = self.active_widget_tree()?;
                if tree.contains(*id) {
                    Some(*id)
                } else {
                    crate::debug::debug_message(&format!(
                        "[widget-command] dropped: node {id:?} no longer mounted"
                    ));
                    None
                }
            }
            CommandTarget::Selector { root, sel } => {
                let tree = self.active_widget_tree()?;
                match tree.query_one_within(*root, sel) {
                    Ok(node) => Some(node),
                    Err(err) => {
                        crate::debug::debug_message(&format!(
                            "[widget-command] dropped: selector {sel:?} under {root:?} did not \
                             resolve to one node ({err:?})"
                        ));
                        None
                    }
                }
            }
            CommandTarget::TypeMatch { root, ty } => {
                let tree = self.active_widget_tree()?;
                let mut found: Option<NodeId> = None;
                for node in tree.walk_depth_first(*root) {
                    if node == *root {
                        continue;
                    }
                    let Some(n) = tree.get(node) else { continue };
                    if (n.widget.as_ref() as &dyn Any).type_id() == *ty {
                        if found.is_some() {
                            crate::debug::debug_message(&format!(
                                "[widget-command] dropped: query_one::<{ty:?}> under {root:?} \
                                 matched more than one descendant"
                            ));
                            return None;
                        }
                        found = Some(node);
                    }
                }
                if found.is_none() {
                    crate::debug::debug_message(&format!(
                        "[widget-command] dropped: query_one::<{ty:?}> under {root:?} matched no \
                         descendant"
                    ));
                }
                found
            }
        }
    }

    /// Apply one deferred command during the post-dispatch flush.
    pub(crate) fn apply_widget_command(
        &mut self,
        cmd: WidgetCommand,
        pending: &mut PendingInvalidation,
    ) {
        match cmd {
            WidgetCommand::AddClass { target, class } => {
                let Some(node) = self.resolve_command_target(&target) else {
                    return;
                };
                if let Some(tree) = self.active_widget_tree_mut() {
                    tree.add_class(node, &class);
                }
                // A class flip can change descendant display/visibility and other
                // layout-affecting CSS (Python `add_class` -> `refresh(layout=True)`).
                pending.request_flags(InvalidationFlags::layout());
            }
            WidgetCommand::RemoveClass { target, class } => {
                let Some(node) = self.resolve_command_target(&target) else {
                    return;
                };
                if let Some(tree) = self.active_widget_tree_mut() {
                    tree.remove_class(node, &class);
                }
                pending.request_flags(InvalidationFlags::layout());
            }
            WidgetCommand::UpdateWidget { target, apply } => {
                let Some(node) = self.resolve_command_target(&target) else {
                    return;
                };
                self.run_on_node_widget(node, |w, ctx| apply(w, ctx), pending);
            }
            WidgetCommand::UpdateStyles { target, apply } => {
                let Some(node) = self.resolve_command_target(&target) else {
                    return;
                };
                if let Some(tree) = self.active_widget_tree_mut() {
                    tree.update_styles(node, apply);
                }
                // A post-mount inline-style write can change size/spacing/visibility
                // (Python `widget.styles.x = v` -> `refresh(layout=True)`).
                pending.request_flags(InvalidationFlags::layout());
            }
            WidgetCommand::RegisterTimer {
                node,
                timer_id,
                interval,
                paused,
                callback,
            } => {
                // Same TimerRuntime as app timers (TRAP h: manual-clock coverage).
                let last_fire = self.timers.now();
                self.timers
                    .schedule_interval(timer_id, node, interval, None, paused);
                self.widget_timer_callbacks.insert(
                    timer_id,
                    WidgetTimerEntry {
                        node,
                        callback,
                        last_fire,
                        fire_count: 0,
                    },
                );
            }
            WidgetCommand::PauseTimer(id) => {
                self.timers.pause(id);
            }
            WidgetCommand::ResumeTimer(id) => {
                self.timers.resume(id);
            }
            WidgetCommand::StopTimer(id) => {
                self.timers.cancel(id);
                self.widget_timer_callbacks.remove(&id);
            }
        }
    }

    /// Run a closure against the widget at `node` with a fresh `WidgetCtx`, then
    /// drive that node's reactive fixpoint + absorb the synthesized EventCtx.
    /// Shared by `UpdateWidget` and widget-timer fires. Returns `false` if the
    /// node is not present (caller handles the miss).
    ///
    /// TRAP (b): the dispatch-ctx guard wraps the closure call, else
    /// `self.node_id()` returns `NodeId::default()` inside it. TRAP (a): no
    /// `&mut App` crosses into the closure — the `tree.get_mut` borrow is scoped
    /// to the call, and the closure only sees `&mut dyn Widget` + `&mut WidgetCtx`.
    pub(crate) fn run_on_node_widget(
        &mut self,
        node: NodeId,
        run: impl FnOnce(&mut dyn Widget, &mut WidgetCtx),
        pending: &mut PendingInvalidation,
    ) -> bool {
        // Node interaction state for the dispatch guard (so `self.node_id()` /
        // `node_state()` are correct inside the closure).
        let Some(state) = self
            .active_widget_tree()
            .and_then(|t| t.get(node))
            .map(|n| n.state)
        else {
            return false;
        };
        let _guard = super::dispatch_ctx::set_dispatch_recipient(node, state);

        // No live EventCtx exists in the flush; synthesize one (TRAP f: the
        // WidgetCtx borrows it by `&mut`, so its invalidation flags survive to
        // be absorbed below), plus a fresh WidgetCtx carrying a fresh ReactiveCtx.
        let mut synth_event = EventCtx::default();
        synth_event.set_node_id(node);
        let mut wctx = WidgetCtx::new(node, &mut synth_event);

        let mut ran = false;
        if let Some(tree) = self.active_widget_tree_mut()
            && let Some(n) = tree.get_mut(node)
        {
            run(n.widget.as_mut(), &mut wctx);
            ran = true;
        }

        // Drive the target node's reactive fixpoint by enqueueing its recorded
        // changes: the shared flush's rounds loop dispatches the node's
        // `watch_*` in the SAME pass (mirrors `Handle::update_in`).
        let reactive = wctx.into_reactive();
        if reactive.has_changes()
            || reactive.needs_repaint()
            || reactive.needs_layout()
            || reactive.needs_recompose()
            || reactive.needs_styles()
        {
            crate::reactive::enqueue_runtime_reactive_entry(
                crate::reactive::RuntimeReactiveEntry::new(node, reactive),
            );
        }

        // Absorb the synthesized EventCtx's side effects (repaint / messages /
        // class ops the closure may have requested through the EventCtx surface).
        let mut outcome = DispatchOutcome {
            handled: synth_event.handled(),
            repaint_requested: synth_event.repaint_requested(),
            invalidation: synth_event.invalidation(),
            stop_requested: synth_event.stop_requested(),
            messages: synth_event.take_messages(),
            animation_requests: synth_event.take_animation_requests(),
            worker_requests: synth_event.take_worker_requests(),
            recompose_nodes: synth_event.take_recompose_nodes(),
            default_prevented: false,
            class_ops: synth_event.take_class_ops(),
        };
        // PostUp: messages posted from the closure (sender = this node) are
        // stashed for the flush to bubble from the node after its rounds converge.
        if !outcome.messages.is_empty() {
            self.pending_widget_posts
                .append(&mut outcome.messages);
        }
        self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
        ran
    }
}
