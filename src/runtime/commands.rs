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

/// An erased closure applied to a resolved target widget with a fresh `WidgetCtx`
/// ([`WidgetCommand::UpdateWidget`]).
pub(crate) type WidgetApply = Box<dyn FnOnce(&mut dyn Widget, &mut WidgetCtx) + Send>;

/// An erased closure applied to a target node's inline styles
/// ([`WidgetCommand::UpdateStyles`]).
pub(crate) type StyleApply = Box<dyn FnOnce(&mut crate::widgets::WidgetStyles) + Send>;

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

/// Which tree a deferred command's target lives in, resolved at drain time
/// (Phase B2 of the cross-screen design; the owned twin of the public
/// [`ScreenRef`](crate::runtime::ScreenRef)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TreeScope {
    /// The active tree at drain time (top screen, else app root).
    Active,
    /// The base app-root tree, regardless of pushed screens.
    AppRoot,
    /// The topmost stacked screen with this `Screen::name()` or mode name.
    Name(String),
    /// The exact live tree with this `WidgetTree::tree_id()`.
    Tree(u64),
}

impl TreeScope {
    /// Owned scope from a public [`ScreenRef`](crate::runtime::ScreenRef)
    /// (enqueued targets must not borrow the caller's screen name).
    pub(crate) fn from_screen_ref(screen: crate::runtime::ScreenRef<'_>) -> Self {
        match screen {
            crate::runtime::ScreenRef::Active => TreeScope::Active,
            crate::runtime::ScreenRef::AppRoot => TreeScope::AppRoot,
            crate::runtime::ScreenRef::Name(name) => TreeScope::Name(name.to_string()),
            crate::runtime::ScreenRef::Tree(id) => TreeScope::Tree(id),
        }
    }

    /// The scope of the tree currently being dispatched into: its exact
    /// `tree_id` inside a dispatch scope (so a drain while a different screen
    /// is on top cannot alias a same-keyed node of another tree), else the
    /// active tree at drain (the pre-scope resolution, kept for enqueues made
    /// outside any dispatch).
    pub(crate) fn dispatching() -> Self {
        match crate::runtime::dispatch_ctx::dispatch_tree_id() {
            Some(id) => TreeScope::Tree(id),
            None => TreeScope::Active,
        }
    }
}

/// The node a scoped selector/type query is rooted at, inside the scoped tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootRef {
    /// A concrete node of the scoped tree (self-rooted `WidgetCtx::query_one*`:
    /// descendants of the querying widget only).
    Node(NodeId),
    /// The scoped tree's root (whole-tree search; the cross-screen
    /// `query_one_on` form, where the querying widget's own node belongs to a
    /// different tree).
    TreeRoot,
}

/// Target of a deferred command. **Resolved at drain time**, never at enqueue
/// time: the tree is borrowed during the enqueuing handler, and an earlier
/// command in the same flush may have mounted the intended target.
#[derive(Debug, Clone)]
pub(crate) enum CommandTarget {
    /// An already-resolved node identity (generational — a stale id resolves to
    /// `None` at drain and the command is dropped, never panics). Used by
    /// `Handle::update_via` and self-targeting `WidgetCtx::add_class`.
    ///
    /// `tree` is the owning `WidgetTree::tree_id()` when known (`Handle`s carry
    /// it; ctx paths stamp the dispatching tree's id). Screens own separate
    /// trees whose slotmap keys collide (same index + generation), so a bare
    /// `NodeId` captured on one tree passes `contains()` against another tree
    /// and would mutate an unrelated widget. `None` means "the active tree at
    /// drain time" (the pre-stamp resolution, kept for enqueue sites that
    /// legitimately target the active tree). Since Phase B2 a stamped target
    /// whose (live) owning tree is not active APPLIES in the owning tree; a
    /// dead owning tree still drops.
    Node { node: NodeId, tree: Option<u64> },
    /// A CSS selector resolved within `scope`'s tree under `root` at drain
    /// time. Used by `WidgetCtx::query_one_id("#id")` (dispatching scope,
    /// self-rooted) and `query_one_on` (screen scope, tree-rooted).
    Selector {
        scope: TreeScope,
        root: RootRef,
        sel: String,
    },
    /// The single descendant under `root` in `scope`'s tree whose concrete
    /// type is `ty`, resolved by downcast at drain time. Used by the type-form
    /// `WidgetCtx::query_one::<W>()`.
    TypeMatch {
        scope: TreeScope,
        root: RootRef,
        ty: TypeId,
    },
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
        apply: WidgetApply,
    },
    /// Apply a closure to the target node's inline styles (Python
    /// `widget.styles.<prop> = v`). Homes the post-mount inline-style write path
    /// that the former inline-style write-through hook staged on the widget:
    /// the seed is drained at mount, so a post-mount style write must reach the
    /// arena node record directly. Applied via `WidgetTree::update_styles`.
    UpdateStyles {
        target: CommandTarget,
        apply: StyleApply,
    },
    /// Register a widget-owned timer on the SAME `TimerRuntime` as app-level
    /// timers (so `enable_manual_timer_clock` / `Pilot::advance_clock` drive it
    /// deterministically). `timer_id` is pre-allocated by
    /// `alloc_widget_timer_id` so `set_interval` / `set_timer` can return a
    /// stable `TimerHandle`. `repeat = None` repeats forever
    /// (`WidgetCtx::set_interval`); `Some(1)` is a one-shot
    /// (`WidgetCtx::set_timer`).
    RegisterTimer {
        node: NodeId,
        timer_id: u64,
        interval: Duration,
        paused: bool,
        repeat: Option<u64>,
        callback: WidgetTimerCallback,
    },
    /// Bubble a message from its sender node during the shared flush (PostUp).
    /// Homes messages posted from a build-time `on_mount` (fired by
    /// `WidgetTree::fire_mount_callbacks`, where no `App` exists to absorb the
    /// synth `EventCtx`'s messages) — e.g. `Select`/`ListView` initial-selection
    /// messages. The flush pushes it onto `pending_widget_posts`, which
    /// `run_event_loop_reactive_phase` bubbles from the sender.
    PostMessage(crate::message::MessageEvent),
    /// Absorb a full [`DispatchOutcome`] captured where no `App` existed to
    /// absorb the synthesized `EventCtx` (a build-time `on_mount` fired by
    /// `WidgetTree::fire_mount_callbacks`). Applied by the flush via
    /// `App::absorb_outcome`, so worker requests, animation requests, recompose
    /// nodes, class ops, invalidation flags and stop requests all land exactly
    /// as if the handler had run under a live dispatch; messages keep their
    /// PostUp semantics via `pending_widget_posts` (same routing as
    /// [`WidgetCommand::PostMessage`]).
    ///
    /// `node` is debug labeling only — the apply arm must NOT gate on node
    /// liveness: a worker requested at mount outlives its node if an early
    /// recompose unmounted it before the flush (Python parity: workers outlive
    /// their owner unless cancelled).
    AbsorbOutcome {
        node: NodeId,
        outcome: Box<DispatchOutcome>,
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
                target: CommandTarget::Node { node, .. },
                class,
            } => Some((node, crate::event::ClassOp::Add(class))),
            WidgetCommand::RemoveClass {
                target: CommandTarget::Node { node, .. },
                class,
            } => Some((node, crate::event::ClassOp::Remove(class))),
            _ => None,
        })
        .collect()
}

/// Test/observability hook: drain the deferred command queue and return the
/// `MessageEvent`s a build-time `on_mount` posted, dropping any non-message
/// commands. Post-RA2.3 a widget's mount-time messages route through this
/// queue instead of a dedicated drain; since the Gap 6 fix they ride the
/// per-node `AbsorbOutcome` bundle (alongside `PostMessage`, which other
/// enqueue sites still use), so both carriers are drained here.
#[doc(hidden)]
pub fn drain_mount_posts_for_test() -> Vec<crate::message::MessageEvent> {
    take_widget_commands()
        .into_iter()
        .flat_map(|cmd| match cmd {
            WidgetCommand::PostMessage(message) => vec![message],
            WidgetCommand::AbsorbOutcome { outcome, .. } => outcome.messages,
            _ => Vec::new(),
        })
        .collect()
}

/// Test/observability hook: drain the deferred command queue and return the
/// `(node, outcome)` bundles from `AbsorbOutcome` commands (those a build-time
/// `on_mount` staged via `WidgetTree::fire_mount_callbacks`), dropping any
/// other commands.
#[doc(hidden)]
pub fn drain_absorb_outcomes_for_test() -> Vec<(NodeId, DispatchOutcome)> {
    take_widget_commands()
        .into_iter()
        .filter_map(|cmd| match cmd {
            WidgetCommand::AbsorbOutcome { node, outcome } => Some((node, *outcome)),
            _ => None,
        })
        .collect()
}

impl App {
    /// Resolve a [`TreeScope`] to the `tree_id` of a live tree, or `None` when
    /// no live tree matches (drop + debug log at the caller, never panic).
    fn resolve_tree_scope(&self, scope: &TreeScope) -> Option<u64> {
        use crate::runtime::ScreenRef;
        match scope {
            TreeScope::Active => self.resolve_screen_ref(ScreenRef::Active),
            TreeScope::AppRoot => self.resolve_screen_ref(ScreenRef::AppRoot),
            TreeScope::Name(name) => self.resolve_screen_ref(ScreenRef::Name(name)),
            TreeScope::Tree(id) => self.resolve_screen_ref(ScreenRef::Tree(*id)),
        }
    }

    /// Resolve a [`CommandTarget`] to `(owning tree_id, live node id)`, or
    /// `None` if the target no longer exists / does not match (drop + debug
    /// log, **never panic**). Two-step since Phase B2: resolve the tree scope
    /// to a concrete live tree, then the node/selector/type within it. A scope
    /// whose screen was popped before the flush degrades to the same
    /// drop-with-log failure mode as a stale node.
    pub(crate) fn resolve_command_target(&self, target: &CommandTarget) -> Option<(u64, NodeId)> {
        match target {
            CommandTarget::Node { node, tree } => {
                let tree_id = match tree {
                    None => self.active_widget_tree()?.tree_id(),
                    Some(id) => *id,
                };
                let Some(owning) = self.tree_by_id(tree_id) else {
                    crate::debug::debug_message(&format!(
                        "[widget-command] dropped: node {node:?}'s owning tree \
                         {tree_id} no longer exists"
                    ));
                    return None;
                };
                if owning.contains(*node) {
                    Some((tree_id, *node))
                } else {
                    crate::debug::debug_message(&format!(
                        "[widget-command] dropped: node {node:?} no longer mounted"
                    ));
                    None
                }
            }
            CommandTarget::Selector { scope, root, sel } => {
                let Some(tree_id) = self.resolve_tree_scope(scope) else {
                    crate::debug::debug_message(&format!(
                        "[widget-command] dropped: selector {sel:?} scope {scope:?} \
                         resolves to no live tree (screen popped before flush?)"
                    ));
                    return None;
                };
                let tree = self.tree_by_id(tree_id)?;
                let resolved = match root {
                    RootRef::TreeRoot => tree.query_one(sel),
                    RootRef::Node(root) => tree.query_one_within(*root, sel),
                };
                match resolved {
                    Ok(node) => Some((tree_id, node)),
                    Err(err) => {
                        crate::debug::debug_message(&format!(
                            "[widget-command] dropped: selector {sel:?} under {root:?} in tree \
                             {tree_id} did not resolve to one node ({err:?})"
                        ));
                        None
                    }
                }
            }
            CommandTarget::TypeMatch { scope, root, ty } => {
                let Some(tree_id) = self.resolve_tree_scope(scope) else {
                    crate::debug::debug_message(&format!(
                        "[widget-command] dropped: query_one::<{ty:?}> scope {scope:?} \
                         resolves to no live tree (screen popped before flush?)"
                    ));
                    return None;
                };
                let tree = self.tree_by_id(tree_id)?;
                let root_node = match root {
                    RootRef::TreeRoot => tree.root()?,
                    RootRef::Node(root) => {
                        if !tree.contains(*root) {
                            crate::debug::debug_message(&format!(
                                "[widget-command] dropped: query_one::<{ty:?}> root {root:?} \
                                 no longer mounted in tree {tree_id}"
                            ));
                            return None;
                        }
                        *root
                    }
                };
                let mut found: Option<NodeId> = None;
                for node in tree.walk_depth_first(root_node) {
                    if node == root_node {
                        continue;
                    }
                    let Some(n) = tree.get(node) else { continue };
                    if (n.widget.as_ref() as &dyn Any).type_id() == *ty {
                        if found.is_some() {
                            crate::debug::debug_message(&format!(
                                "[widget-command] dropped: query_one::<{ty:?}> under \
                                 {root_node:?} matched more than one descendant"
                            ));
                            return None;
                        }
                        found = Some(node);
                    }
                }
                if found.is_none() {
                    crate::debug::debug_message(&format!(
                        "[widget-command] dropped: query_one::<{ty:?}> under {root_node:?} \
                         matched no descendant"
                    ));
                }
                found.map(|node| (tree_id, node))
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
                let Some((tree_id, node)) = self.resolve_command_target(&target) else {
                    return;
                };
                if let Some(tree) = self.tree_by_id_mut(tree_id) {
                    tree.add_class(node, &class);
                }
                // A class flip can change descendant display/visibility and other
                // layout-affecting CSS (Python `add_class` -> `refresh(layout=True)`).
                // The layout flag invalidates all content regions, so a
                // non-active-tree apply repaints every visible layer too.
                pending.request_flags(InvalidationFlags::layout());
            }
            WidgetCommand::RemoveClass { target, class } => {
                let Some((tree_id, node)) = self.resolve_command_target(&target) else {
                    return;
                };
                if let Some(tree) = self.tree_by_id_mut(tree_id) {
                    tree.remove_class(node, &class);
                }
                pending.request_flags(InvalidationFlags::layout());
            }
            WidgetCommand::UpdateWidget { target, apply } => {
                let Some((tree_id, node)) = self.resolve_command_target(&target) else {
                    return;
                };
                // Scoped apply: the closure runs against the widget in its
                // OWNING tree (cross-screen semantics + invalidation handled by
                // the scoped node-update path).
                self.run_on_node_widget_r_in(Some(tree_id), node, |w, ctx| apply(w, ctx), pending);
            }
            WidgetCommand::UpdateStyles { target, apply } => {
                let Some((tree_id, node)) = self.resolve_command_target(&target) else {
                    return;
                };
                if let Some(tree) = self.tree_by_id_mut(tree_id) {
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
                repeat,
                callback,
            } => {
                // Same TimerRuntime as app timers (TRAP h: manual-clock coverage).
                let last_fire = self.timers.now();
                self.timers
                    .schedule_interval(timer_id, node, interval, repeat, paused);
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
            WidgetCommand::PostMessage(message) => {
                // Bubble from the sender during the flush's PostUp pass.
                self.pending_widget_posts.push(message);
            }
            WidgetCommand::AbsorbOutcome { node, mut outcome } => {
                crate::debug::debug_message(&format!(
                    "[widget-command] absorb build-time on_mount outcome for {node:?}"
                ));
                // Messages keep their PostUp semantics (`absorb_outcome` does
                // not route messages): stage them for the flush to bubble from
                // their sender, exactly as `PostMessage` does.
                let messages = std::mem::take(&mut outcome.messages);
                self.pending_widget_posts.extend(messages);
                // Everything else (worker requests, animation + style-animation
                // requests, recompose nodes, class ops, invalidation, sticky
                // stop) lands through the same absorb path a live dispatch uses.
                // Deliberately NOT gated on `node` liveness (see the variant doc).
                self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
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
    /// Non-generic bool wrapper over [`run_on_node_widget_r`], kept for the
    /// `()`-returning call sites (timer fires, `UpdateWidget`, `on_mount`).
    pub(crate) fn run_on_node_widget(
        &mut self,
        node: NodeId,
        run: impl FnOnce(&mut dyn Widget, &mut WidgetCtx),
        pending: &mut PendingInvalidation,
    ) -> bool {
        self.run_on_node_widget_r(node, |w, ctx| run(w, ctx), pending)
            .is_some()
    }

    /// Value-returning twin of [`run_on_node_widget`]: run `run` against the
    /// widget at `node` with a fresh `WidgetCtx`, returning its result (or `None`
    /// when the node is absent — the `Option<R>` contract `with_widget_mut`
    /// relies on). Drives the node's reactive fixpoint + absorbs the synthesized
    /// EventCtx exactly as the bool wrapper does.
    ///
    /// TRAP (b): the dispatch-ctx guard wraps the closure call, else
    /// `self.node_id()` returns `NodeId::default()` inside it. TRAP (a): no
    /// `&mut App` crosses into the closure — the `tree.get_mut` borrow is scoped
    /// to the call, and the closure only sees `&mut dyn Widget` + `&mut WidgetCtx`.
    pub(crate) fn run_on_node_widget_r<R>(
        &mut self,
        node: NodeId,
        run: impl FnOnce(&mut dyn Widget, &mut WidgetCtx) -> R,
        pending: &mut PendingInvalidation,
    ) -> Option<R> {
        self.run_on_node_widget_r_in(None, node, run, pending)
    }

    /// Tree-scoped twin of [`run_on_node_widget_r`](Self::run_on_node_widget_r):
    /// `tree` is `None` for the active tree (today's behavior, byte-for-byte)
    /// or `Some(tree_id)` for an exact live tree, enabling cross-screen applies
    /// (design note "mount_and_cross_screen", Phase B1/B2).
    ///
    /// Cross-tree semantics when the scoped tree is NOT the active tree:
    /// - The closure runs against the widget in its OWNING tree; commands it
    ///   enqueues are stamped with that tree's id (the dispatch-tree guard), so
    ///   they resolve against the right tree at the next drain.
    /// - Class ops recorded on the synth EventCtx are applied to the owning
    ///   tree here (absorb_outcome would apply them to the active tree, which
    ///   is the slotmap-key aliasing hazard).
    /// - The reactive entry is NOT enqueued: runtime reactive dispatch
    ///   (`dispatch_runtime_reactive_entries` / `with_node_widget_taken_dyn`)
    ///   is active-tree-scoped, so a cross-tree entry would alias a same-keyed
    ///   node. First cut restricts cross-screen closures to direct mutation
    ///   plus repaint; owning-tree watcher routing is a Phase B3 follow-up.
    /// - Messages posted from the closure are dropped with a loud debug log:
    ///   `pending_widget_posts` bubbles against the active tree, which would
    ///   misroute or alias. Owning-tree PostUp bubbling is a B3 follow-up.
    /// - A full relayout + repaint is requested so the compositor repaints
    ///   every visible layer (an update behind a translucent screen shows
    ///   immediately; behind an opaque screen it is state-only until reveal).
    pub(crate) fn run_on_node_widget_r_in<R>(
        &mut self,
        tree: Option<u64>,
        node: NodeId,
        run: impl FnOnce(&mut dyn Widget, &mut WidgetCtx) -> R,
        pending: &mut PendingInvalidation,
    ) -> Option<R> {
        // Node interaction state for the dispatch guard (so `self.node_id()` /
        // `node_state()` are correct inside the closure).
        let (state, tree_id) = {
            let scoped = self.scoped_tree(tree)?;
            (scoped.get(node)?.state, scoped.tree_id())
        };
        let is_active_tree = self
            .active_widget_tree()
            .is_some_and(|t| t.tree_id() == tree_id);
        let _guard = super::dispatch_ctx::set_dispatch_recipient(node, state);
        // Stamp the dispatching tree so `CommandTarget::Node`s enqueued from the
        // closure carry their owning tree's identity (aliasing guard).
        let _tree_guard = super::dispatch_ctx::set_dispatch_tree(tree_id);

        // No live EventCtx exists in the flush; synthesize one (TRAP f: the
        // WidgetCtx borrows it by `&mut`, so its invalidation flags survive to
        // be absorbed below), plus a fresh WidgetCtx carrying a fresh ReactiveCtx.
        let mut synth_event = EventCtx::default();
        synth_event.set_node_id(node);
        let mut wctx = WidgetCtx::new(node, &mut synth_event);

        let mut result: Option<R> = None;
        if let Some(scoped) = self.scoped_tree_mut(tree)
            && let Some(n) = scoped.get_mut(node)
        {
            result = Some(run(n.widget.as_mut(), &mut wctx));
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
            || reactive.has_messages()
        {
            if is_active_tree {
                crate::reactive::enqueue_runtime_reactive_entry(
                    crate::reactive::RuntimeReactiveEntry::new(node, reactive),
                );
            } else {
                // Runtime reactive dispatch is active-tree-scoped; enqueueing a
                // cross-tree node would alias a same-keyed active-tree node.
                // Direct widget mutation already happened; the full
                // invalidation below carries the visible effect. Watcher
                // routing for cross-screen closures is a B3 follow-up.
                crate::debug::debug_message(&format!(
                    "[cross-screen] reactive changes recorded on node {node:?} of \
                     non-active tree {tree_id} were not watcher-dispatched \
                     (cross-screen watch_* routing is a deferred follow-up)"
                ));
            }
        }

        // Absorb the synthesized EventCtx's side effects (repaint / messages /
        // class ops the closure may have requested through the EventCtx surface).
        let mut outcome = DispatchOutcome::from_event_ctx(&mut synth_event);
        if is_active_tree {
            // PostUp: messages posted from the closure (sender = this node) are
            // stashed for the flush to bubble from the node after its rounds
            // converge.
            if !outcome.messages.is_empty() {
                self.pending_widget_posts.append(&mut outcome.messages);
            }
        } else {
            // Messages would bubble against the ACTIVE tree (misroute/alias);
            // drop loudly. Owning-tree PostUp bubbling is a B3 follow-up.
            if !outcome.messages.is_empty() {
                crate::debug::debug_message(&format!(
                    "[cross-screen] DROPPED {} message(s) posted from a closure \
                     targeting node {node:?} of non-active tree {tree_id} \
                     (owning-tree message bubbling is a deferred follow-up)",
                    outcome.messages.len()
                ));
                outcome.messages.clear();
            }
            // Class ops must land on the OWNING tree (absorb_outcome applies
            // them to the active tree).
            let class_ops = std::mem::take(&mut outcome.class_ops);
            if !class_ops.is_empty() {
                if let Some(owning) = self.tree_by_id_mut(tree_id) {
                    for (op_node, op) in class_ops {
                        match op {
                            crate::event::ClassOp::Add(c) => owning.add_class(op_node, &c),
                            crate::event::ClassOp::Remove(c) => owning.remove_class(op_node, &c),
                        }
                    }
                }
                pending.request_flags(InvalidationFlags::layout());
            }
            // The recompose machinery resolves nodes against the active tree;
            // a cross-tree recompose request would alias. Drop loudly (B3).
            let recompose = std::mem::take(&mut outcome.recompose_nodes);
            if !recompose.is_empty() {
                crate::debug::debug_message(&format!(
                    "[cross-screen] DROPPED {} recompose request(s) from a closure \
                     targeting non-active tree {tree_id} (cross-screen recompose \
                     is a deferred follow-up)",
                    recompose.len()
                ));
            }
            // Repaint whatever is visible: the compositor re-renders every
            // visible layer under a full invalidation, so a base-screen update
            // behind a translucent modal shows immediately.
            if result.is_some() {
                pending.request_flags(InvalidationFlags::layout());
                pending.request_full_content();
            }
        }
        self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
        result
    }
}
