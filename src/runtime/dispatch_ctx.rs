use crate::node_id::NodeId;
use crate::widgets::NodeState;
use std::cell::Cell;

thread_local! {
    static DISPATCH_NODE: Cell<Option<(NodeId, NodeState)>> = const { Cell::new(None) };
}

pub(crate) struct DispatchRecipientGuard {
    previous: Option<(NodeId, NodeState)>,
}

impl Drop for DispatchRecipientGuard {
    fn drop(&mut self) {
        DISPATCH_NODE.with(|cell| cell.set(self.previous));
    }
}

pub(crate) fn set_dispatch_recipient(node_id: NodeId, state: NodeState) -> DispatchRecipientGuard {
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
