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
