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

use std::cell::RefCell;

use super::App;
use super::types::PendingInvalidation;
use crate::event::InvalidationFlags;
use crate::node_id::NodeId;

/// Target of a deferred command. **Resolved at drain time**, never at enqueue
/// time: the tree is borrowed during the enqueuing handler, and an earlier
/// command in the same flush may have mounted the intended target.
#[derive(Debug, Clone)]
// Variants are constructed by tests in this sub-step and by `WidgetCtx` /
// `Handle::update_via` in sub-step 2; the allow is removed there.
#[allow(dead_code)]
pub(crate) enum CommandTarget {
    /// An already-resolved node identity (generational — a stale id resolves to
    /// `None` at drain and the command is dropped, never panics).
    Node(NodeId),
    /// A CSS selector resolved against the subtree rooted at `root` at drain
    /// time. Used by `WidgetCtx::query_one("#id")` (sub-step 2).
    Selector { root: NodeId, sel: String },
}

/// A deferred DOM/cross-node side effect recorded by a handler and applied in
/// the post-dispatch flush.
// Enqueued by tests here and by `WidgetCtx` in sub-step 2; allow removed there.
#[allow(dead_code)]
pub(crate) enum WidgetCommand {
    /// Add a CSS class to the target node.
    AddClass { target: CommandTarget, class: String },
    /// Remove a CSS class from the target node.
    RemoveClass { target: CommandTarget, class: String },
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
// Called by tests here and by `WidgetCtx` in sub-step 2; allow removed there.
#[allow(dead_code)]
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
        }
    }
}
