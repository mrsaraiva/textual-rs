use textual::prelude::*;
use textual::reactive::{ReactiveChange, ReactiveWidget};

// ---------------------------------------------------------------------------
// Probe widget used across tests
// ---------------------------------------------------------------------------

struct Probe {
    value: u32,
    watched: std::sync::Arc<std::sync::Mutex<Vec<(u32, u32)>>>,
}

impl Probe {
    fn new(value: u32) -> Self {
        Self {
            value,
            watched: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

impl Widget for Probe {
    fn render(&self, _: &rich_rs::Console, _: &rich_rs::ConsoleOptions) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "Probe"
    }

    fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }
}

impl ReactiveWidget for Probe {
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

// ---------------------------------------------------------------------------
// Helper container widget that uses compose() with slot.bind()
// ---------------------------------------------------------------------------

struct ProbeContainer {
    slot: HandleSlot<Probe>,
    probe_value: u32,
    probe_id: Option<String>,
}

impl ProbeContainer {
    fn new(slot: HandleSlot<Probe>, value: u32) -> Self {
        Self {
            slot,
            probe_value: value,
            probe_id: None,
        }
    }

    fn with_probe_id(mut self, id: &str) -> Self {
        self.probe_id = Some(id.to_string());
        self
    }
}

impl Widget for ProbeContainer {
    fn render(&self, _: &rich_rs::Console, _: &rich_rs::ConsoleOptions) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "ProbeContainer"
    }

    fn compose(&mut self) -> ComposeResult {
        let probe = Probe::new(self.probe_value);
        let decl = self.slot.bind(probe);
        let decl = if let Some(id) = &self.probe_id {
            decl.with_id(id)
        } else {
            decl
        };
        vec![decl]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn app_root_child_handle_slot_fills_on_build() {
    let slot: HandleSlot<Probe> = HandleSlot::new();
    assert!(slot.get().is_none(), "slot unfilled before build");

    let mut root = AppRoot::new().with_child_handle(Probe::new(42), &slot);

    let tree = build_widget_tree_from_root(&mut root).expect("tree should build");

    let handle = slot.handle().expect("slot should be filled after build");
    let val = handle
        .read_in(&tree, |p| p.value)
        .expect("read_in should succeed");
    assert_eq!(val, 42);
}

#[test]
fn slot_unfilled_before_build_is_unmounted() {
    let slot: HandleSlot<Probe> = HandleSlot::new();
    assert_eq!(slot.handle(), Err(QueryError::Unmounted));
}

#[test]
fn child_decl_bind_fills_slot() {
    let slot: HandleSlot<Probe> = HandleSlot::new();

    let container = ProbeContainer::new(slot.clone(), 7).with_probe_id("probe");
    let mut root = AppRoot::new().with_child(container);

    let tree = build_widget_tree_from_root(&mut root).expect("tree should build");

    let handle = slot.handle().expect("slot filled after build");

    // The mounted node carries id "probe" — verify via query.
    let queried = tree.query_one("#probe").expect("query should find #probe node");
    assert_eq!(
        queried,
        handle.node_id(),
        "slot node_id should match queried #probe"
    );

    let val = handle
        .read_in(&tree, |p| p.value)
        .expect("read_in should succeed");
    assert_eq!(val, 7);
}

#[test]
fn slot_tracks_latest_mount() {
    let slot: HandleSlot<Probe> = HandleSlot::new();

    // Build tree 1.
    let mut root1 = AppRoot::new().with_child_handle(Probe::new(1), &slot);
    let _tree1 = build_widget_tree_from_root(&mut root1).expect("tree1 should build");
    let handle1 = slot.handle().expect("slot filled from tree1");

    // Build tree 2 binding the same slot.
    let mut root2 = AppRoot::new().with_child_handle(Probe::new(2), &slot);
    let tree2 = build_widget_tree_from_root(&mut root2).expect("tree2 should build");

    // Slot now reflects tree 2.
    let handle2 = slot.handle().expect("slot filled from tree2");
    assert_eq!(handle2.tree_id(), tree2.tree_id());
    let val2 = handle2
        .read_in(&tree2, |p| p.value)
        .expect("read_in on tree2 should succeed");
    assert_eq!(val2, 2);

    // Old handle is unmounted in tree2 (tree_id mismatch).
    assert_eq!(
        handle1.read_in(&tree2, |p| p.value),
        Err(QueryError::Unmounted)
    );
}

#[test]
fn typed_mismatch_is_loud() {
    let slot: HandleSlot<Probe> = HandleSlot::new();
    let mut root = AppRoot::new().with_child_handle(Probe::new(0), &slot);
    let tree = build_widget_tree_from_root(&mut root).expect("tree should build");

    let probe_node = slot.handle().expect("slot filled").node_id();

    // Attempt to resolve as Spacer — should fail with TypeMismatch.
    let result = Handle::<Spacer>::resolve(&tree, probe_node);
    match result {
        Err(QueryError::TypeMismatch { actual, .. }) => {
            assert_eq!(actual, "Probe");
        }
        other => panic!("expected TypeMismatch, got {:?}", other),
    }
}

#[test]
fn stale_after_remove() {
    let slot: HandleSlot<Probe> = HandleSlot::new();
    let mut root = AppRoot::new().with_child_handle(Probe::new(5), &slot);
    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should build");

    let handle = slot.handle().expect("slot filled");
    let node = handle.node_id();

    tree.remove(node);

    assert_eq!(
        handle.read_in(&tree, |p| p.value),
        Err(QueryError::Unmounted)
    );
    assert_eq!(
        handle.update_in(&mut tree, |_p, _ctx| {}),
        Err(QueryError::Unmounted)
    );
}
