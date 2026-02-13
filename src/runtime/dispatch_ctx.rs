use crate::node_id::NodeId;
use std::cell::Cell;

thread_local! {
    static DISPATCH_RECIPIENT: Cell<Option<NodeId>> = const { Cell::new(None) };
}

pub(crate) struct DispatchRecipientGuard {
    previous: Option<NodeId>,
}

impl Drop for DispatchRecipientGuard {
    fn drop(&mut self) {
        DISPATCH_RECIPIENT.with(|cell| cell.set(self.previous));
    }
}

pub(crate) fn set_dispatch_recipient(node_id: NodeId) -> DispatchRecipientGuard {
    let previous = DISPATCH_RECIPIENT.with(|cell| {
        let prev = cell.get();
        cell.set(Some(node_id));
        prev
    });
    DispatchRecipientGuard { previous }
}

pub(crate) fn dispatch_recipient() -> Option<NodeId> {
    DISPATCH_RECIPIENT.with(Cell::get)
}
