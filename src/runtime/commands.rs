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
//! Only `AddClass` / `RemoveClass` exist in this sub-step; `UpdateWidget`
//! (sub-step 2) and the mount/style/recompose/post commands (sub-step 5) are
//! added as the build proceeds.

use std::any::{Any, TypeId};
use std::cell::RefCell;

use super::App;
use super::event_loop::InvalidationScope;
use super::types::{DispatchOutcome, PendingInvalidation};
use crate::event::{EventCtx, InvalidationFlags, WidgetCtx};
use crate::node_id::NodeId;
use crate::widgets::Widget;

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
                self.run_update_widget(node, apply, pending);
            }
        }
    }

    /// Run an `UpdateWidget` closure against the resolved target node during the
    /// flush. TRAP (b): the dispatch-ctx guard must wrap the closure call, else
    /// `self.node_id()` returns `NodeId::default()` inside it. TRAP (a): no
    /// `&mut App` crosses into the closure — the `tree.get_mut` borrow is scoped
    /// to the call, and the closure only sees `&mut dyn Widget` + `&mut WidgetCtx`.
    fn run_update_widget(
        &mut self,
        node: NodeId,
        apply: Box<dyn FnOnce(&mut dyn Widget, &mut WidgetCtx) + Send>,
        pending: &mut PendingInvalidation,
    ) {
        // Node interaction state for the dispatch guard (so `self.node_id()` /
        // `node_state()` are correct inside the closure).
        let state = self
            .active_widget_tree()
            .and_then(|t| t.get(node))
            .map(|n| n.state)
            .unwrap_or_default();
        let _guard = super::dispatch_ctx::set_dispatch_recipient(node, state);

        // No live EventCtx exists in the flush; synthesize one (TRAP f: the
        // WidgetCtx borrows it by `&mut`, so its invalidation flags survive to
        // be absorbed below), plus a fresh WidgetCtx carrying a fresh ReactiveCtx.
        let mut synth_event = EventCtx::default();
        synth_event.set_node_id(node);
        let mut wctx = WidgetCtx::new(node, &mut synth_event);

        if let Some(tree) = self.active_widget_tree_mut()
            && let Some(n) = tree.get_mut(node)
        {
            apply(n.widget.as_mut(), &mut wctx);
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
        if !outcome.messages.is_empty() {
            // Routing messages posted from an update closure needs the
            // `PostUp` command (sub-step 5); until then, surface the drop.
            crate::debug::debug_message(&format!(
                "[widget-command] UpdateWidget on {node:?} posted {} message(s) that are not yet \
                 routed (PostUp lands in a later step)",
                outcome.messages.len()
            ));
        }
        self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
    }
}
