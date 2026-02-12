//! Typed signal/observer pattern for lightweight pub/sub notifications.
//!
//! [`Signal<T>`] provides a direct emitter → subscriber notification mechanism,
//! bypassing the message bus and widget tree propagation. Useful for simple
//! cross-widget notifications where full message routing is unnecessary.
//!
//! # Example
//!
//! ```
//! use textual::signal::{Signal, SignalResponse};
//! use slotmap::SlotMap;
//! use textual::node_id::NodeId;
//!
//! let mut sm = SlotMap::<NodeId, &str>::new();
//! let node = sm.insert("widget-a");
//!
//! let mut sig = Signal::<u32>::new();
//! sig.subscribe(node, |_val| SignalResponse::Continue);
//! assert_eq!(sig.emit(&42), 1);
//! ```

use crate::node_id::NodeId;

/// What a signal handler returns to control notification flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalResponse {
    /// Continue notifying remaining subscribers.
    Continue,
    /// Stop notifying remaining subscribers.
    Stop,
}

/// A typed signal that widgets can subscribe to and emit.
///
/// Unlike messages which bubble through the widget tree, signals are direct:
/// emitter → all subscribers, no propagation semantics. Subscribers are
/// notified in subscription order.
///
/// A single node may hold multiple subscriptions (e.g. with different handler
/// functions). [`unsubscribe`](Signal::unsubscribe) removes **all**
/// subscriptions for the given node.
pub struct Signal<T: Clone + Send + 'static> {
    subscribers: Vec<Subscription<T>>,
}

struct Subscription<T: Clone + Send + 'static> {
    node: NodeId,
    handler: fn(&T) -> SignalResponse,
}

impl<T: Clone + Send + 'static> Signal<T> {
    /// Create a new signal with no subscribers.
    pub fn new() -> Self {
        Self {
            subscribers: Vec::new(),
        }
    }

    /// Subscribe a node to this signal with the given handler.
    ///
    /// A node may subscribe multiple times (with the same or different handlers).
    /// Each subscription is notified independently.
    pub fn subscribe(&mut self, node: NodeId, handler: fn(&T) -> SignalResponse) {
        self.subscribers.push(Subscription { node, handler });
    }

    /// Remove **all** subscriptions for the given node.
    ///
    /// Call this when a node is removed from the widget tree to prevent stale
    /// subscriptions.
    pub fn unsubscribe(&mut self, node: NodeId) {
        self.subscribers.retain(|s| s.node != node);
    }

    /// Emit a value to all subscribers in subscription order.
    ///
    /// Returns the number of subscribers that were actually notified (including
    /// the one that returned [`SignalResponse::Stop`], if any).
    pub fn emit(&self, value: &T) -> usize {
        let mut count = 0;
        for sub in &self.subscribers {
            count += 1;
            if (sub.handler)(value) == SignalResponse::Stop {
                break;
            }
        }
        count
    }

    /// Check whether a node has any active subscription on this signal.
    pub fn is_subscribed(&self, node: NodeId) -> bool {
        self.subscribers.iter().any(|s| s.node == node)
    }

    /// Number of active subscriptions.
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

impl<T: Clone + Send + 'static> Default for Signal<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Helper: create a SlotMap and insert `n` nodes, returning (map, vec-of-ids).
    fn make_nodes(n: usize) -> (SlotMap<NodeId, &'static str>, Vec<NodeId>) {
        let mut sm = SlotMap::new();
        let ids: Vec<NodeId> = (0..n).map(|i| sm.insert(if i == 0 { "a" } else if i == 1 { "b" } else { "c" })).collect();
        (sm, ids)
    }

    #[test]
    fn basic_subscribe_and_emit() {
        let (_sm, ids) = make_nodes(1);
        let mut sig = Signal::<u32>::new();
        sig.subscribe(ids[0], |val| {
            assert_eq!(*val, 42);
            SignalResponse::Continue
        });
        assert_eq!(sig.emit(&42), 1);
    }

    #[test]
    fn multiple_subscribers_all_receive_value() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let (_sm, ids) = make_nodes(3);
        let mut sig = Signal::<u32>::new();

        COUNTER.store(0, Ordering::SeqCst);

        for &id in &ids {
            sig.subscribe(id, |_| {
                COUNTER.fetch_add(1, Ordering::SeqCst);
                SignalResponse::Continue
            });
        }

        let notified = sig.emit(&99);
        assert_eq!(notified, 3);
        assert_eq!(COUNTER.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn subscribers_notified_in_order() {
        use std::sync::Mutex;
        static ORDER: Mutex<Vec<u32>> = Mutex::new(Vec::new());

        let (_sm, ids) = make_nodes(3);
        let mut sig = Signal::<u32>::new();

        ORDER.lock().unwrap().clear();

        // Use the emitted value to identify subscription order.
        // We subscribe node 0 with handler that pushes 0, node 1 → 1, node 2 → 2.
        // Since fn pointers can't capture, we encode the order in the *signal value*
        // differently. Instead, use three distinct handlers:
        sig.subscribe(ids[0], |_| {
            ORDER.lock().unwrap().push(0);
            SignalResponse::Continue
        });
        sig.subscribe(ids[1], |_| {
            ORDER.lock().unwrap().push(1);
            SignalResponse::Continue
        });
        sig.subscribe(ids[2], |_| {
            ORDER.lock().unwrap().push(2);
            SignalResponse::Continue
        });

        sig.emit(&0);
        assert_eq!(*ORDER.lock().unwrap(), vec![0, 1, 2]);
    }

    #[test]
    fn unsubscribe_removes_node() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let (_sm, ids) = make_nodes(2);
        let mut sig = Signal::<u32>::new();

        sig.subscribe(ids[0], |_| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            SignalResponse::Continue
        });
        sig.subscribe(ids[1], |_| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            SignalResponse::Continue
        });

        sig.unsubscribe(ids[0]);

        COUNTER.store(0, Ordering::SeqCst);
        let notified = sig.emit(&1);
        assert_eq!(notified, 1);
        assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
        assert!(!sig.is_subscribed(ids[0]));
        assert!(sig.is_subscribed(ids[1]));
    }

    #[test]
    fn signal_response_stop_halts_remaining() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let (_sm, ids) = make_nodes(3);
        let mut sig = Signal::<u32>::new();

        COUNTER.store(0, Ordering::SeqCst);

        sig.subscribe(ids[0], |_| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            SignalResponse::Continue
        });
        sig.subscribe(ids[1], |_| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            SignalResponse::Stop // stop here
        });
        sig.subscribe(ids[2], |_| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            SignalResponse::Continue
        });

        let notified = sig.emit(&0);
        // First two notified, third skipped.
        assert_eq!(notified, 2);
        assert_eq!(COUNTER.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn emit_with_no_subscribers_returns_zero() {
        let sig = Signal::<String>::new();
        assert_eq!(sig.emit(&"hello".to_string()), 0);
    }

    #[test]
    fn is_subscribed_before_and_after() {
        let (_sm, ids) = make_nodes(1);
        let mut sig = Signal::<u32>::new();

        assert!(!sig.is_subscribed(ids[0]));
        sig.subscribe(ids[0], |_| SignalResponse::Continue);
        assert!(sig.is_subscribed(ids[0]));
        sig.unsubscribe(ids[0]);
        assert!(!sig.is_subscribed(ids[0]));
    }

    #[test]
    fn subscriber_count_tracks_correctly() {
        let (_sm, ids) = make_nodes(2);
        let mut sig = Signal::<u32>::new();

        assert_eq!(sig.subscriber_count(), 0);
        sig.subscribe(ids[0], |_| SignalResponse::Continue);
        assert_eq!(sig.subscriber_count(), 1);
        sig.subscribe(ids[1], |_| SignalResponse::Continue);
        assert_eq!(sig.subscriber_count(), 2);
        sig.unsubscribe(ids[0]);
        assert_eq!(sig.subscriber_count(), 1);
        sig.unsubscribe(ids[1]);
        assert_eq!(sig.subscriber_count(), 0);
    }

    #[test]
    fn multiple_subscriptions_same_node_both_fire() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let (_sm, ids) = make_nodes(1);
        let mut sig = Signal::<u32>::new();

        COUNTER.store(0, Ordering::SeqCst);

        // Same node, two subscriptions.
        sig.subscribe(ids[0], |_| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            SignalResponse::Continue
        });
        sig.subscribe(ids[0], |_| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            SignalResponse::Continue
        });

        assert_eq!(sig.subscriber_count(), 2);
        let notified = sig.emit(&7);
        assert_eq!(notified, 2);
        assert_eq!(COUNTER.load(Ordering::SeqCst), 2);

        // Unsubscribe removes both.
        sig.unsubscribe(ids[0]);
        assert_eq!(sig.subscriber_count(), 0);
    }

    #[test]
    fn default_trait_works() {
        let sig: Signal<f64> = Signal::default();
        assert_eq!(sig.subscriber_count(), 0);
        assert_eq!(sig.emit(&3.14), 0);
    }

    #[test]
    fn handler_receives_correct_value() {
        let (_sm, ids) = make_nodes(1);
        let mut sig = Signal::<String>::new();

        sig.subscribe(ids[0], |val| {
            assert_eq!(val, "expected");
            SignalResponse::Continue
        });

        sig.emit(&"expected".to_string());
    }

    #[test]
    fn stop_on_first_subscriber() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let (_sm, ids) = make_nodes(2);
        let mut sig = Signal::<u32>::new();

        COUNTER.store(0, Ordering::SeqCst);

        sig.subscribe(ids[0], |_| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            SignalResponse::Stop
        });
        sig.subscribe(ids[1], |_| {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            SignalResponse::Continue
        });

        let notified = sig.emit(&0);
        assert_eq!(notified, 1);
        assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn unsubscribe_nonexistent_node_is_noop() {
        let (_sm, ids) = make_nodes(2);
        let mut sig = Signal::<u32>::new();

        sig.subscribe(ids[0], |_| SignalResponse::Continue);
        // Unsubscribe a node that was never subscribed — should be a no-op.
        sig.unsubscribe(ids[1]);
        assert_eq!(sig.subscriber_count(), 1);
        assert!(sig.is_subscribed(ids[0]));
    }
}
