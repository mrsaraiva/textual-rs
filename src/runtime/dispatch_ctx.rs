use crate::node_id::NodeId;
use crate::widgets::NodeState;
use std::cell::Cell;

thread_local! {
    static DISPATCH_NODE: Cell<Option<(NodeId, NodeState)>> = const { Cell::new(None) };
}

/// Guard returned by [`set_dispatch_recipient`]. Restores the previous
/// dispatch recipient when dropped (RAII).
pub struct DispatchRecipientGuard {
    previous: Option<(NodeId, NodeState)>,
}

impl Drop for DispatchRecipientGuard {
    fn drop(&mut self) {
        DISPATCH_NODE.with(|cell| cell.set(self.previous));
    }
}

/// Set the current dispatch recipient for the current thread.
///
/// Returns a guard that restores the previous recipient when dropped.
/// Primarily used by the runtime event loop and renderer; exposed for
/// widget-render tests that need to simulate hover/focus/disabled state.
pub fn set_dispatch_recipient(node_id: NodeId, state: NodeState) -> DispatchRecipientGuard {
    let previous = DISPATCH_NODE.with(|cell| {
        let prev = cell.get();
        cell.set(Some((node_id, state)));
        prev
    });
    DispatchRecipientGuard { previous }
}

pub(crate) fn dispatch_recipient() -> Option<NodeId> {
    DISPATCH_NODE.with(|cell| cell.get().map(|(id, _)| id))
}

pub(crate) fn dispatch_node_state() -> Option<NodeState> {
    DISPATCH_NODE.with(|cell| cell.get().map(|(_, state)| state))
}

thread_local! {
    /// `WidgetTree::tree_id()` of the tree currently being dispatched into,
    /// set alongside the dispatch recipient by every handler-invoking scope
    /// that holds a concrete tree. Read by the `WidgetCtx` enqueue paths so a
    /// deferred `CommandTarget::Node` carries its owning tree's identity
    /// (screens own separate trees whose slotmap keys collide; an unstamped
    /// node id drained against a different tree would alias an unrelated
    /// widget — design note "mount_and_cross_screen", section 2.1).
    static DISPATCH_TREE: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Guard returned by [`set_dispatch_tree`]. Restores the previous dispatching
/// tree id when dropped (RAII).
pub(crate) struct DispatchTreeGuard {
    previous: Option<u64>,
}

impl Drop for DispatchTreeGuard {
    fn drop(&mut self) {
        DISPATCH_TREE.with(|cell| cell.set(self.previous));
    }
}

/// Set the tree id of the tree currently being dispatched into (RAII scope).
pub(crate) fn set_dispatch_tree(tree_id: u64) -> DispatchTreeGuard {
    let previous = DISPATCH_TREE.with(|cell| {
        let prev = cell.get();
        cell.set(Some(tree_id));
        prev
    });
    DispatchTreeGuard { previous }
}

/// Tree id of the tree currently being dispatched into, if any. `None` outside
/// a dispatch scope; enqueue sites then stamp `None` = "active tree at drain",
/// today's (pre-stamp) resolution.
pub(crate) fn dispatch_tree_id() -> Option<u64> {
    DISPATCH_TREE.with(|cell| cell.get())
}
