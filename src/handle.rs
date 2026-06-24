//! Typed widget handles (RA-4).
//!
//! A `Handle<W>` names one mounted widget with its concrete Rust type. It wraps
//! the same arena mechanics as `App::with_widget_mut_as` (the escape hatch),
//! adding compile-time typing, loud failure modes, and reactive-watcher wiring.
//!
//! Guidance boundary (enforced in examples/docs): handles are for *imperative
//! widget APIs* — the situations where Python Textual uses `query_one` to call a
//! widget method. Application state belongs in reactive fields / signals (RA-3);
//! do not use `update` to smuggle app state into widgets.

use std::any::type_name;
use std::fmt;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

use crate::compose::ChildDecl;
use crate::node_id::NodeId;
use crate::reactive::{ReactiveCtx, RuntimeReactiveEntry, enqueue_runtime_reactive_entry};
use crate::widget_tree::{QueryError, WidgetTree};
use crate::widgets::Widget;

// ---------------------------------------------------------------------------
// HandleSink
// ---------------------------------------------------------------------------

/// Type-erased callback fired by the mount pipeline with the freshly allocated
/// `(NodeId, tree_id)` of a widget bound to a `HandleSlot`.
pub type HandleSink = Box<dyn FnOnce(NodeId, u64) + Send + Sync>;

// ---------------------------------------------------------------------------
// Internal resolution helpers
// ---------------------------------------------------------------------------

fn resolve_node<W: Widget>(
    tree: &WidgetTree,
    node: NodeId,
    tree_id: u64,
) -> Result<&W, QueryError> {
    if tree.tree_id() != tree_id {
        return Err(QueryError::Unmounted);
    }
    let n = tree.get(node).ok_or(QueryError::Unmounted)?;
    (n.widget.as_ref() as &dyn std::any::Any)
        .downcast_ref::<W>()
        .ok_or_else(|| QueryError::TypeMismatch {
            expected: type_name::<W>(),
            actual: n.widget.style_type(),
        })
}

fn resolve_node_mut<W: Widget>(
    tree: &mut WidgetTree,
    node: NodeId,
    tree_id: u64,
) -> Result<&mut W, QueryError> {
    if tree.tree_id() != tree_id {
        return Err(QueryError::Unmounted);
    }
    let n = tree.get_mut(node).ok_or(QueryError::Unmounted)?;
    // Capture the type string before the mutable downcast borrow; the
    // borrow-checker cannot prove that style_type() inside ok_or_else
    // and the &mut borrow from as_mut() are disjoint without this split.
    let actual_type = n.widget.style_type();
    (n.widget.as_mut() as &mut dyn std::any::Any)
        .downcast_mut::<W>()
        .ok_or(QueryError::TypeMismatch {
            expected: type_name::<W>(),
            actual: actual_type,
        })
}

// ---------------------------------------------------------------------------
// Handle<W>
// ---------------------------------------------------------------------------

/// Typed reference to one mounted widget.
///
/// `PhantomData<fn() -> W>` makes `Handle<W>` `Send + Sync + Copy`
/// independent of `W`'s own auto traits and keeps drop-check out of the picture.
pub struct Handle<W: Widget> {
    node: NodeId,
    tree_id: u64,
    _widget: PhantomData<fn() -> W>,
}

impl<W: Widget> Clone for Handle<W> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<W: Widget> Copy for Handle<W> {}

impl<W: Widget> PartialEq for Handle<W> {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node && self.tree_id == other.tree_id
    }
}

impl<W: Widget> Eq for Handle<W> {}

impl<W: Widget> std::hash::Hash for Handle<W> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.node.hash(state);
        self.tree_id.hash(state);
    }
}

impl<W: Widget> fmt::Debug for Handle<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Handle<{}>({:?} in tree {})",
            type_name::<W>(),
            self.node,
            self.tree_id
        )
    }
}

impl<W: Widget> Handle<W> {
    /// Crate-internal constructor; public acquisition goes through checked paths.
    pub(crate) fn new(node: NodeId, tree_id: u64) -> Self {
        Self {
            node,
            tree_id,
            _widget: PhantomData,
        }
    }

    /// Arena identity (for interop with NodeId-based APIs, e.g. focus, messages).
    pub fn node_id(self) -> NodeId {
        self.node
    }

    /// Identity of the owning `WidgetTree` (screens own separate trees).
    pub fn tree_id(self) -> u64 {
        self.tree_id
    }

    /// Checked typed upgrade of a `NodeId` within a specific tree.
    /// `Err(Unmounted)` when absent; `Err(TypeMismatch{..})` on wrong concrete type.
    pub fn resolve(tree: &WidgetTree, node: NodeId) -> Result<Self, QueryError> {
        // Attempt to resolve the node — this validates the type.
        let _widget: &W = resolve_node(tree, node, tree.tree_id())?;
        Ok(Self::new(node, tree.tree_id()))
    }

    /// Whether the handle still names a live node in `tree`.
    pub fn is_mounted_in(self, tree: &WidgetTree) -> bool {
        tree.tree_id() == self.tree_id && tree.contains(self.node)
    }

    /// Read-only typed access against an explicit tree (headless/test seam).
    pub fn read_in<R>(self, tree: &WidgetTree, f: impl FnOnce(&W) -> R) -> Result<R, QueryError> {
        let widget = resolve_node::<W>(tree, self.node, self.tree_id)?;
        Ok(f(widget))
    }

    /// Mutable typed access against an explicit tree. Creates a fresh
    /// `ReactiveCtx` for the closure; if the closure recorded changes or
    /// repaint/layout flags, enqueues a `RuntimeReactiveEntry` so the runtime
    /// reactive phase dispatches `watch_*` callbacks (same path as event
    /// handlers, src/runtime/event_loop.rs:4282-4370).
    pub fn update_in<R>(
        self,
        tree: &mut WidgetTree,
        f: impl FnOnce(&mut W, &mut ReactiveCtx) -> R,
    ) -> Result<R, QueryError> {
        let mut rctx = ReactiveCtx::new(self.node);
        let out = {
            let widget = resolve_node_mut::<W>(tree, self.node, self.tree_id)?;
            f(widget, &mut rctx)
        };
        if rctx.has_changes() || rctx.needs_repaint() || rctx.needs_layout() {
            enqueue_runtime_reactive_entry(RuntimeReactiveEntry::new(self.node, rctx));
        }
        Ok(out)
    }

    /// App-level read-only access over the active tree (screen-stack aware).
    ///
    /// Typed wrapper over the same arena access as `with_widget_mut_as`;
    /// for imperative widget APIs. Application state belongs in reactive
    /// fields/signals (RA-3).
    pub fn read<R>(self, app: &crate::runtime::App, f: impl FnOnce(&W) -> R) -> Result<R, QueryError> {
        app.handle_read(self, f)
    }

    /// App-level mutable access: tree-level update + automatic subtree repaint.
    ///
    /// Creates a fresh `ReactiveCtx`; changes flow into the runtime reactive
    /// phase so `watch_*` callbacks fire normally. Always requests a subtree
    /// repaint after mutation (mirrors Python's implicit refresh on mutation).
    pub fn update<R>(
        self,
        app: &mut crate::runtime::App,
        f: impl FnOnce(&mut W, &mut ReactiveCtx) -> R,
    ) -> Result<R, QueryError> {
        app.handle_update(self, f)
    }

    /// Whether the handle still names a live node in the active tree.
    pub fn is_mounted(self, app: &crate::runtime::App) -> bool {
        app.handle_is_mounted(self)
    }
}

// ---------------------------------------------------------------------------
// HandleSlot<W>
// ---------------------------------------------------------------------------

/// Cell filled by the mount pipeline with the bound widget's identity.
/// Cheap to clone (Arc); always reflects the most recent mount.
pub struct HandleSlot<W: Widget> {
    cell: Arc<Mutex<Option<(NodeId, u64)>>>,
    _widget: PhantomData<fn() -> W>,
}

impl<W: Widget> Clone for HandleSlot<W> {
    fn clone(&self) -> Self {
        Self {
            cell: Arc::clone(&self.cell),
            _widget: PhantomData,
        }
    }
}

impl<W: Widget> Default for HandleSlot<W> {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: Widget> fmt::Debug for HandleSlot<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let filled = self
            .cell
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some();
        write!(f, "HandleSlot<{}>(filled: {})", type_name::<W>(), filled)
    }
}

impl<W: Widget> HandleSlot<W> {
    /// Create a new, unfilled slot.
    pub fn new() -> Self {
        Self {
            cell: Arc::new(Mutex::new(None)),
            _widget: PhantomData,
        }
    }

    /// `None` until the bound widget has been mounted.
    pub fn get(&self) -> Option<Handle<W>> {
        let guard = self.cell.lock().unwrap_or_else(|e| e.into_inner());
        guard.map(|(node, tree_id)| Handle::new(node, tree_id))
    }

    /// `Err(QueryError::Unmounted)` until the bound widget has been mounted.
    pub fn handle(&self) -> Result<Handle<W>, QueryError> {
        self.get().ok_or(QueryError::Unmounted)
    }

    /// Bind `widget` to this slot in a `compose()` declaration.
    /// The returned decl can be further configured (`.with_id()`, …).
    pub fn bind(&self, widget: W) -> ChildDecl {
        let mut decl = ChildDecl::new(Box::new(widget));
        decl.handle_sink = Some(self.make_sink());
        decl
    }

    /// Sink consumed by the mount pipeline. Overwrites on refire (remount).
    pub(crate) fn make_sink(&self) -> HandleSink {
        let cell = Arc::clone(&self.cell);
        Box::new(move |node, tree_id| {
            *cell.lock().unwrap_or_else(|e| e.into_inner()) = Some((node, tree_id));
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactive::{ReactiveChange, ReactiveFlags, take_runtime_reactive_entries};
    use crate::widget_tree::WidgetTree;
    use rich_rs::{Console, ConsoleOptions, Segments};

    /// Minimal widget for testing.
    struct Probe {
        value: u32,
    }

    impl Probe {
        fn new(value: u32) -> Self {
            Self { value }
        }

        fn boxed(value: u32) -> Box<dyn Widget> {
            Box::new(Self::new(value))
        }
    }

    impl Widget for Probe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            "Probe"
        }
    }

    /// Different widget type for type-mismatch tests.
    struct Other;

    impl Widget for Other {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            "Other"
        }
    }

    fn build_tree_with_probe(value: u32) -> (WidgetTree, NodeId) {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Probe::boxed(value));
        (tree, root)
    }

    #[test]
    fn resolve_ok_and_node_id_roundtrip() {
        let (tree, root) = build_tree_with_probe(42);
        let handle = Handle::<Probe>::resolve(&tree, root).unwrap();
        assert_eq!(handle.node_id(), root);
        assert_eq!(handle.tree_id(), tree.tree_id());
    }

    #[test]
    fn resolve_type_mismatch() {
        let (tree, root) = build_tree_with_probe(1);
        let result = Handle::<Other>::resolve(&tree, root);
        match result {
            Err(QueryError::TypeMismatch { expected: _, actual }) => {
                assert_eq!(actual, "Probe");
            }
            other => panic!("expected TypeMismatch, got {:?}", other),
        }
    }

    #[test]
    fn read_in_after_remove_is_unmounted() {
        let (mut tree, root) = build_tree_with_probe(10);
        let handle = Handle::<Probe>::resolve(&tree, root).unwrap();
        tree.remove(root);
        let result = handle.read_in(&tree, |p| p.value);
        assert_eq!(result, Err(QueryError::Unmounted));
    }

    #[test]
    fn cross_tree_access_is_unmounted() {
        // Build tree A and get a handle.
        let (tree_a, root_a) = build_tree_with_probe(1);
        let handle_a = Handle::<Probe>::resolve(&tree_a, root_a).unwrap();

        // Build tree B with the same insert order (same NodeId slot likely).
        let (tree_b, _root_b) = build_tree_with_probe(2);

        // Handle from tree A must not resolve in tree B.
        let result = handle_a.read_in(&tree_b, |p| p.value);
        assert_eq!(result, Err(QueryError::Unmounted));
    }

    #[test]
    fn update_in_enqueues_runtime_reactive_entry_when_changes_recorded() {
        // Drain any prior entries from other tests on this thread.
        let _ = take_runtime_reactive_entries();

        let (mut tree, root) = build_tree_with_probe(0);
        let handle = Handle::<Probe>::resolve(&tree, root).unwrap();

        let _ = handle.update_in(&mut tree, |probe, ctx| {
            ctx.record_change(
                "value",
                ReactiveFlags::default(),
                Box::new(0u32),
                Box::new(1u32),
            );
            probe.value = 1;
        });

        let entries = take_runtime_reactive_entries();
        assert_eq!(entries.len(), 1, "one entry should be enqueued");
        assert_eq!(entries[0].node_id(), root);

        // Run without dispatch and verify the result carries repaint flag.
        let mut entries = entries;
        let result = entries[0].run_without_dispatch();
        assert!(result.needs_repaint, "ReactiveFlags::default has repaint=true");

        // Verify the widget was actually mutated by the closure.
        let val = handle.read_in(&tree, |p| p.value).unwrap();
        assert_eq!(val, 1);
    }

    #[test]
    fn update_in_without_changes_enqueues_nothing() {
        let _ = take_runtime_reactive_entries();

        let (mut tree, root) = build_tree_with_probe(5);
        let handle = Handle::<Probe>::resolve(&tree, root).unwrap();

        // Closure does nothing — no record_change, no request_repaint.
        let _ = handle.update_in(&mut tree, |_probe, _ctx| {});

        let entries = take_runtime_reactive_entries();
        assert_eq!(entries.len(), 0, "no entry should be enqueued when nothing changed");
    }

    // Compile-time trait bounds check.
    fn assert_copy_send_sync<T: Copy + Send + Sync>() {}
    fn assert_clone_send_sync<T: Clone + Send + Sync>() {}

    #[test]
    fn handle_is_copy_send_sync() {
        assert_copy_send_sync::<Handle<Probe>>();
        assert_clone_send_sync::<HandleSlot<Probe>>();
    }

    #[test]
    fn handle_slot_unfilled_returns_none() {
        let slot: HandleSlot<Probe> = HandleSlot::new();
        assert!(slot.get().is_none());
        assert_eq!(slot.handle(), Err(QueryError::Unmounted));
    }

    #[test]
    fn handle_slot_make_sink_fills_on_call() {
        let slot: HandleSlot<Probe> = HandleSlot::new();
        let (tree, root) = build_tree_with_probe(7);
        let sink = slot.make_sink();
        sink(root, tree.tree_id());
        let h = slot.get().unwrap();
        assert_eq!(h.node_id(), root);
        assert_eq!(h.tree_id(), tree.tree_id());
    }

    #[test]
    fn handle_slot_latest_mount_wins() {
        let slot: HandleSlot<Probe> = HandleSlot::new();
        let (tree1, root1) = build_tree_with_probe(1);
        let (tree2, root2) = build_tree_with_probe(2);

        let sink1 = slot.make_sink();
        sink1(root1, tree1.tree_id());
        assert_eq!(slot.get().unwrap().node_id(), root1);

        // Second mount overwrites.
        let sink2 = slot.make_sink();
        sink2(root2, tree2.tree_id());
        assert_eq!(slot.get().unwrap().node_id(), root2);
    }

    #[test]
    fn is_mounted_in_true_while_alive() {
        let (tree, root) = build_tree_with_probe(0);
        let handle = Handle::<Probe>::resolve(&tree, root).unwrap();
        assert!(handle.is_mounted_in(&tree));
    }

    #[test]
    fn is_mounted_in_false_after_remove() {
        let (mut tree, root) = build_tree_with_probe(0);
        let handle = Handle::<Probe>::resolve(&tree, root).unwrap();
        tree.remove(root);
        assert!(!handle.is_mounted_in(&tree));
    }

    // Verify that update_in dispatches watchers via runtime entry.
    // This test lives here (in-crate) because it needs pub(crate) access to
    // WidgetNode.widget via tree.get_mut().
    #[test]
    fn update_in_dispatches_watchers_via_runtime_entry() {
        use crate::reactive::ReactiveWidget;
        use std::sync::{Arc, Mutex};

        // Widget that records reactive dispatches.
        struct Watcher {
            watched: Arc<Mutex<Vec<(u32, u32)>>>,
        }

        impl Widget for Watcher {
            fn render(&self, _: &Console, _: &ConsoleOptions) -> Segments {
                Segments::new()
            }
            fn style_type(&self) -> &'static str {
                "Watcher"
            }
            fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
                Some(self)
            }
        }

        impl ReactiveWidget for Watcher {
            fn reactive_dispatch(&mut self, changes: &[ReactiveChange], _ctx: &mut ReactiveCtx) {
                for c in changes {
                    if c.field_name == "value" {
                        let old = *c.old_value.downcast_ref::<u32>().unwrap();
                        let new = *c.new_value.downcast_ref::<u32>().unwrap();
                        self.watched.lock().unwrap().push((old, new));
                    }
                }
            }
        }

        let _ = take_runtime_reactive_entries();

        let log: Arc<Mutex<Vec<(u32, u32)>>> = Arc::new(Mutex::new(Vec::new()));
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(Watcher { watched: Arc::clone(&log) }));

        let handle = Handle::<Watcher>::resolve(&tree, root).unwrap();

        let _ = handle.update_in(&mut tree, |_w, ctx| {
            ctx.record_change(
                "value",
                ReactiveFlags::default(),
                Box::new(0u32),
                Box::new(99u32),
            );
        });

        let mut entries = take_runtime_reactive_entries();
        assert_eq!(entries.len(), 1);

        // Mirror the event loop pattern (src/runtime/event_loop.rs:4325-4332).
        let node = tree.get_mut(root).unwrap();
        entries[0].run_with_dispatch(|changes, ctx| {
            if let Some(rw) = node.widget.reactive_widget() {
                rw.reactive_dispatch(changes, ctx);
            }
        });

        let dispatched = log.lock().unwrap().clone();
        assert_eq!(dispatched, vec![(0u32, 99u32)]);
    }
}
