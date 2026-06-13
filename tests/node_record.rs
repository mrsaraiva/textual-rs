//! RA-2 acceptance tests: node-record class-op plumbing.
//!
//! These tests correspond to the test plan in SPEC-RA2-node-record.md §Test plan.
//! Each test is introduced in the step that makes it green (step 4 for class-op tests).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, Segments};
use textual::event::{ClassOp, Event, EventCtx};
use textual::keys::KeyEventData;
use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, dispatch_event_tree};

// ---------------------------------------------------------------------------
// Probe widget: records the key event and queues a class op
// ---------------------------------------------------------------------------

struct ClassOpProbe;

impl Widget for ClassOpProbe {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "ClassOpProbe"
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn focusable(&self) -> bool {
        true
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        NodeSeed {
            css_id: Some("class-op-probe".to_string()),
            ..NodeSeed::default()
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if matches!(event, Event::Key(_)) {
            ctx.add_class("-active");
            ctx.set_handled();
        }
    }
}

/// Find a node in the tree by CSS id.
fn find_node_by_css_id(tree: &WidgetTree, css_id: &str) -> Option<NodeId> {
    let root = tree.root()?;
    tree.walk_depth_first(root)
        .into_iter()
        .find(|&id| tree.css_id(id) == Some(css_id))
}

/// Integration test: widget handler calls `ctx.add_class("-active")` on a key
/// event; the class must land on the tree node after the dispatch cycle.
///
/// This verifies the end-to-end class-op plumbing introduced in Step 4:
/// EventCtx → DispatchOutcome.class_ops → tree.add_class.
#[test]
fn event_ctx_class_ops_apply_to_tree() {
    // Wrap in Container so the tree has a real root with children
    let mut root = Container::new().with_child(ClassOpProbe);
    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should have root");

    // Find the ClassOpProbe node by its CSS id (assigned via take_node_seed)
    let probe_node_id =
        find_node_by_css_id(&tree, "class-op-probe").expect("ClassOpProbe should be in tree");

    // Ensure `-active` is not yet on the node
    assert!(
        !tree.has_class(probe_node_id, "-active"),
        "class should not be present before dispatch"
    );

    // Synthesize a key event
    let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let event = Event::Key(KeyEventData::from_crossterm(key_event));

    // Dispatch through the tree with focus on the probe node
    let mut outcome = dispatch_event_tree(&mut tree, Some(probe_node_id), &event);

    // Apply class_ops from the outcome to the tree (mirrors what absorb_outcome does)
    for (node, op) in std::mem::take(&mut outcome.class_ops) {
        match op {
            ClassOp::Add(c) => tree.add_class(node, &c),
            ClassOp::Remove(c) => tree.remove_class(node, &c),
        }
    }

    assert!(
        tree.has_class(probe_node_id, "-active"),
        "class should be present after dispatch + class_ops application"
    );
}

// ---------------------------------------------------------------------------
// Probe widget: queues add then remove (set_class toggle semantics)
// ---------------------------------------------------------------------------

struct SetClassProbe {
    toggle: bool,
}

impl Widget for SetClassProbe {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "SetClassProbe"
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn focusable(&self) -> bool {
        true
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        NodeSeed {
            css_id: Some("set-class-probe".to_string()),
            ..NodeSeed::default()
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if matches!(event, Event::Key(_)) {
            ctx.set_class(self.toggle, "-toggle");
            ctx.set_handled();
        }
    }
}

/// Integration test: `ctx.set_class(true, …)` queues an Add op;
/// `ctx.set_class(false, …)` queues a Remove op.
#[test]
fn event_ctx_set_class_queues_correct_op() {
    fn dispatch_key(toggle: bool) -> (WidgetTree, NodeId) {
        let mut root = Container::new().with_child(SetClassProbe { toggle });
        let mut tree = build_widget_tree_from_root(&mut root).expect("tree built");
        let probe_node_id =
            find_node_by_css_id(&tree, "set-class-probe").expect("SetClassProbe in tree");

        // Pre-seed with the class so Remove has something to remove
        if !toggle {
            tree.add_class(probe_node_id, "-toggle");
        }

        let key_event = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let event = Event::Key(KeyEventData::from_crossterm(key_event));

        let mut outcome = dispatch_event_tree(&mut tree, Some(probe_node_id), &event);
        for (node, op) in std::mem::take(&mut outcome.class_ops) {
            match op {
                ClassOp::Add(c) => tree.add_class(node, &c),
                ClassOp::Remove(c) => tree.remove_class(node, &c),
            }
        }
        (tree, probe_node_id)
    }

    let (tree, id) = dispatch_key(true);
    assert!(
        tree.has_class(id, "-toggle"),
        "Add: class should be present"
    );

    let (tree, id) = dispatch_key(false);
    assert!(
        !tree.has_class(id, "-toggle"),
        "Remove: class should be absent"
    );
}
