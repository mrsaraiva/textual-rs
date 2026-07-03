//! P1G-13 gate: integration tests for tree-mode focus and hover lifecycle.
//!
//! All tests exercise the arena-tree pipeline:
//! - `build_widget_tree_from_root` — build the tree
//! - `dispatch_event_tree` / `dispatch_event_to_target_tree` — dispatch events
//! - `focused_node_id_tree` — verify focus state

use std::sync::{Arc, Mutex};

use rich_rs::{Console, ConsoleOptions, Segments};
use textual::compose;
use textual::event::{BlurEvent, FocusEvent};
use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, dispatch_event_to_target_tree, dispatch_event_tree, focused_node_id_tree, run_layout_pass};

// ---------------------------------------------------------------------------
// Test probe: focus lifecycle tracking
// ---------------------------------------------------------------------------

/// Lightweight focusable widget that records focus/blur transitions to a
/// shared sink.  Handles `Event::Focus` and `Event::Blur` so that
/// `dispatch_event_to_target_tree` can drive focus changes through the tree.
struct TreeFocusProbe {
    id: &'static str,
    focused: bool,
    sink: Arc<Mutex<Vec<String>>>,
}

impl TreeFocusProbe {
    fn new(id: &'static str, sink: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            id,
            focused: false,
            sink,
        }
    }

    fn set_focus(&mut self, focused: bool) {
        if self.focused != focused {
            self.focused = focused;
            self.sink
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("{}:{focused}", self.id));
        }
    }
}

impl Widget for TreeFocusProbe {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn focusable(&self) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        match event {
            Event::Focus(_) => {
                self.set_focus(true);
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::Blur(_) => {
                self.set_focus(false);
                ctx.request_repaint();
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Test probe: hover lifecycle tracking
// ---------------------------------------------------------------------------

/// Lightweight hoverable widget that records hover enter/leave to a shared
/// sink.  Handles `Event::Enter` and `Event::Leave`.
struct TreeHoverProbe {
    id: &'static str,
    hovered: bool,
    sink: Arc<Mutex<Vec<String>>>,
}

impl TreeHoverProbe {
    fn new(id: &'static str, sink: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            id,
            hovered: false,
            sink,
        }
    }

    fn set_hovered(&mut self, hovered: bool) {
        if self.hovered != hovered {
            self.hovered = hovered;
            self.sink
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("{}:{hovered}", self.id));
        }
    }
}

impl Widget for TreeHoverProbe {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn focusable(&self) -> bool {
        true
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        match event {
            Event::Enter(_) => {
                self.set_hovered(true);
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::Leave(_) => {
                self.set_hovered(false);
                ctx.request_repaint();
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

// ===========================================================================
// P1G-13(a): Focus dispatch crosses wrapper boundaries in the arena tree.
//
// NOTE: Runtime-level Tab cycling (FocusNext/FocusPrev) is handled by
// `App::move_focus_auto`, which is pub(crate).  These tests validate the
// prerequisite: Focus/Blur events dispatched through the tree reach widgets
// across nested wrapper boundaries (Container, Vertical, VerticalScroll).
// ===========================================================================

#[test]
fn p1g13_focus_crosses_wrapper_boundary_via_tree_dispatch() {
    // Tree: Container -> Vertical -> [ProbeA, VerticalScroll -> [ProbeB, ProbeC]]
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new().with_child(
        Vertical::new()
            .with_child(TreeFocusProbe::new("A", sink.clone()))
            .with_child(
                VerticalScroll::new()
                    .with_child(TreeFocusProbe::new("B", sink.clone()))
                    .with_child(TreeFocusProbe::new("C", sink.clone()))
                    .height(5),
            ),
    );

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should have children");
    run_layout_pass(&mut tree, (40, 12));

    let root_id = tree.root().unwrap();
    let all_nodes = tree.walk_depth_first(root_id);

    // Collect focusable leaf nodes (our probes) in depth-first order.
    let focusable: Vec<NodeId> = all_nodes
        .iter()
        .copied()
        .filter(|&id| {
            tree.get(id)
                .map(|_| tree.children(id).is_empty()) // leaf nodes
                .unwrap_or(false)
        })
        .collect();

    // We expect at least 3 focusable leaves (A, B, C).
    assert!(
        focusable.len() >= 3,
        "expected at least 3 focusable leaves; got {} in {focusable:?}",
        focusable.len()
    );

    let probe_a = focusable[0];
    let probe_b = focusable[1];
    let probe_c = focusable[2];

    // Focus probe A via tree dispatch.
    let outcome_a = dispatch_event_to_target_tree(
        &mut tree,
        probe_a,
        &Event::Focus(FocusEvent { node: probe_a }),
    );
    assert!(
        outcome_a.handled,
        "Focus event on probe A should be handled"
    );
    assert_eq!(
        focused_node_id_tree(&tree),
        Some(probe_a),
        "focused_node_id_tree should return probe A"
    );

    // Transfer focus to probe B (across VerticalScroll boundary).
    dispatch_event_to_target_tree(
        &mut tree,
        probe_a,
        &Event::Blur(BlurEvent { node: probe_a }),
    );
    let outcome_b = dispatch_event_to_target_tree(
        &mut tree,
        probe_b,
        &Event::Focus(FocusEvent { node: probe_b }),
    );
    assert!(
        outcome_b.handled,
        "Focus event on probe B should be handled"
    );
    assert_eq!(
        focused_node_id_tree(&tree),
        Some(probe_b),
        "focus should cross the VerticalScroll wrapper boundary to probe B"
    );

    // Transfer focus to probe C (same wrapper, next sibling).
    dispatch_event_to_target_tree(
        &mut tree,
        probe_b,
        &Event::Blur(BlurEvent { node: probe_b }),
    );
    let outcome_c = dispatch_event_to_target_tree(
        &mut tree,
        probe_c,
        &Event::Focus(FocusEvent { node: probe_c }),
    );
    assert!(
        outcome_c.handled,
        "Focus event on probe C should be handled"
    );
    assert_eq!(
        focused_node_id_tree(&tree),
        Some(probe_c),
        "focus should move to probe C"
    );

    // Verify sink recorded the full traversal.
    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"A:true".to_string()),
        "probe A should have received focus; events={events:?}"
    );
    assert!(
        events.contains(&"A:false".to_string()),
        "probe A should have lost focus; events={events:?}"
    );
    assert!(
        events.contains(&"B:true".to_string()),
        "probe B should have received focus across wrapper boundary; events={events:?}"
    );
    assert!(
        events.contains(&"C:true".to_string()),
        "probe C should have received focus; events={events:?}"
    );
}

#[test]
fn p1g13_focus_traverses_deep_wrapper_chain() {
    // Tree: Container -> VerticalScroll -> Container -> [ProbeA, ProbeB]
    // Tests focus traversal through a deep wrapper chain.
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new().with_child(
        VerticalScroll::new()
            .with_child(
                Container::new()
                    .with_child(TreeFocusProbe::new("deep_A", sink.clone()))
                    .with_child(TreeFocusProbe::new("deep_B", sink.clone())),
            )
            .height(8),
    );

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should have children");
    run_layout_pass(&mut tree, (40, 12));

    let root_id = tree.root().unwrap();
    let all_nodes = tree.walk_depth_first(root_id);

    // Find leaf nodes (probes).
    let leaves: Vec<NodeId> = all_nodes
        .iter()
        .copied()
        .filter(|&id| tree.children(id).is_empty())
        .collect();

    assert!(
        leaves.len() >= 2,
        "expected at least 2 leaves in deep chain; got {} in {leaves:?}",
        leaves.len()
    );

    let deep_a = leaves[0];
    let deep_b = leaves[1];

    // Focus deep_A.
    dispatch_event_to_target_tree(
        &mut tree,
        deep_a,
        &Event::Focus(FocusEvent { node: deep_a }),
    );
    assert_eq!(focused_node_id_tree(&tree), Some(deep_a));

    // Transfer to deep_B through wrapper chain.
    dispatch_event_to_target_tree(&mut tree, deep_a, &Event::Blur(BlurEvent { node: deep_a }));
    dispatch_event_to_target_tree(
        &mut tree,
        deep_b,
        &Event::Focus(FocusEvent { node: deep_b }),
    );
    assert_eq!(
        focused_node_id_tree(&tree),
        Some(deep_b),
        "focus should traverse deep wrapper chain to deep_B"
    );

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"deep_A:true".to_string()),
        "deep_A should gain focus; events={events:?}"
    );
    assert!(
        events.contains(&"deep_A:false".to_string()),
        "deep_A should lose focus; events={events:?}"
    );
    assert!(
        events.contains(&"deep_B:true".to_string()),
        "deep_B should gain focus across wrappers; events={events:?}"
    );
}

// ===========================================================================
// P1G-13(b): Pointer hover enter/leave updates pseudo-class state and repaint
// ===========================================================================

#[test]
fn p1g13_hover_enter_requests_repaint_via_tree_dispatch() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(TreeHoverProbe::new("btn1", sink.clone()))
        .with_child(TreeHoverProbe::new("btn2", sink.clone()));

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should have children");
    run_layout_pass(&mut tree, (40, 10));

    let root_id = tree.root().unwrap();
    let children: Vec<NodeId> = tree.children(root_id).to_vec();
    assert!(
        children.len() >= 2,
        "expected at least 2 children under root"
    );

    let btn1 = children[0];
    let _btn2 = children[1];

    // Hover enter on btn1.
    let enter_outcome = dispatch_event_to_target_tree(
        &mut tree,
        btn1,
        &Event::Enter(MouseEnterEvent {
            screen_x: 5,
            screen_y: 0,
            x: 5,
            y: 0,
        }),
    );
    assert!(
        enter_outcome.repaint_requested,
        "hover enter should request repaint"
    );

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"btn1:true".to_string()),
        "btn1 should be marked hovered after Enter; events={events:?}"
    );
}

#[test]
fn p1g13_hover_leave_clears_state_via_tree_dispatch() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(TreeHoverProbe::new("h1", sink.clone()))
        .with_child(TreeHoverProbe::new("h2", sink.clone()));

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should have children");
    run_layout_pass(&mut tree, (40, 10));

    let root_id = tree.root().unwrap();
    let children: Vec<NodeId> = tree.children(root_id).to_vec();
    let h1 = children[0];

    // Enter then leave.
    dispatch_event_to_target_tree(
        &mut tree,
        h1,
        &Event::Enter(MouseEnterEvent {
            screen_x: 5,
            screen_y: 0,
            x: 5,
            y: 0,
        }),
    );
    let leave_outcome = dispatch_event_to_target_tree(
        &mut tree,
        h1,
        &Event::Leave(MouseLeaveEvent {
            screen_x: 5,
            screen_y: 0,
            x: 5,
            y: 0,
        }),
    );
    assert!(
        leave_outcome.repaint_requested,
        "hover leave should request repaint"
    );

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"h1:true".to_string()),
        "h1 should have been hovered; events={events:?}"
    );
    assert!(
        events.contains(&"h1:false".to_string()),
        "h1 hover should be cleared after Leave; events={events:?}"
    );
}

#[test]
fn p1g13_hover_transfer_between_siblings_via_tree_dispatch() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(TreeHoverProbe::new("top", sink.clone()))
        .with_child(TreeHoverProbe::new("bottom", sink.clone()));

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should have children");
    run_layout_pass(&mut tree, (40, 10));

    let root_id = tree.root().unwrap();
    let children: Vec<NodeId> = tree.children(root_id).to_vec();
    let top = children[0];
    let bottom = children[1];

    // Hover top.
    dispatch_event_to_target_tree(
        &mut tree,
        top,
        &Event::Enter(MouseEnterEvent {
            screen_x: 5,
            screen_y: 0,
            x: 5,
            y: 0,
        }),
    );

    // Move hover: leave top, enter bottom.
    dispatch_event_to_target_tree(
        &mut tree,
        top,
        &Event::Leave(MouseLeaveEvent {
            screen_x: 5,
            screen_y: 0,
            x: 5,
            y: 0,
        }),
    );
    dispatch_event_to_target_tree(
        &mut tree,
        bottom,
        &Event::Enter(MouseEnterEvent {
            screen_x: 5,
            screen_y: 1,
            x: 5,
            y: 1,
        }),
    );

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"top:true".to_string()),
        "top should have been hovered; events={events:?}"
    );
    assert!(
        events.contains(&"top:false".to_string()),
        "top hover should be cleared; events={events:?}"
    );
    assert!(
        events.contains(&"bottom:true".to_string()),
        "bottom should become hovered; events={events:?}"
    );
}

// ===========================================================================
// P1G-13(c): Focus transfer clears previous focused widget
// ===========================================================================

#[test]
fn p1g13_focus_transfer_clears_previous_in_separate_branches() {
    // Tree: Container -> [Vertical -> ProbeLeft, Vertical -> ProbeRight]
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(Vertical::new().with_child(TreeFocusProbe::new("left", sink.clone())))
        .with_child(Vertical::new().with_child(TreeFocusProbe::new("right", sink.clone())));

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should have children");
    run_layout_pass(&mut tree, (40, 10));

    let root_id = tree.root().unwrap();
    let all_nodes = tree.walk_depth_first(root_id);

    // Find leaf focusable probes.
    let leaves: Vec<NodeId> = all_nodes
        .iter()
        .copied()
        .filter(|&id| tree.children(id).is_empty())
        .collect();
    assert!(
        leaves.len() >= 2,
        "expected at least 2 leaves; got {leaves:?}"
    );

    let left = leaves[0];
    let right = leaves[1];

    // Focus left probe.
    dispatch_event_to_target_tree(&mut tree, left, &Event::Focus(FocusEvent { node: left }));
    assert_eq!(
        focused_node_id_tree(&tree),
        Some(left),
        "left probe should have focus"
    );

    // Transfer focus: blur left, focus right.
    dispatch_event_to_target_tree(&mut tree, left, &Event::Blur(BlurEvent { node: left }));
    dispatch_event_to_target_tree(&mut tree, right, &Event::Focus(FocusEvent { node: right }));

    assert_eq!(
        focused_node_id_tree(&tree),
        Some(right),
        "right probe should have focus after transfer"
    );

    // Verify sink: left gained then lost, right gained.
    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"left:true".to_string()),
        "left should have gained focus; events={events:?}"
    );
    assert!(
        events.contains(&"left:false".to_string()),
        "left should have lost focus after transfer; events={events:?}"
    );
    assert!(
        events.contains(&"right:true".to_string()),
        "right should have gained focus; events={events:?}"
    );

    // Verify ordering: left:true before left:false before right:true.
    let left_true_pos = events.iter().position(|e| e == "left:true").unwrap();
    let left_false_pos = events.iter().position(|e| e == "left:false").unwrap();
    let right_true_pos = events.iter().position(|e| e == "right:true").unwrap();
    assert!(
        left_true_pos < left_false_pos && left_false_pos < right_true_pos,
        "focus events should be ordered: left:true < left:false < right:true; events={events:?}"
    );
}

#[test]
fn p1g13_focus_transfer_no_dual_focus() {
    // Verify that after transfer, only one node reports has_focus == true.
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(TreeFocusProbe::new("one", sink.clone()))
        .with_child(TreeFocusProbe::new("two", sink.clone()))
        .with_child(TreeFocusProbe::new("three", sink.clone()));

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should have children");
    run_layout_pass(&mut tree, (40, 10));

    let root_id = tree.root().unwrap();
    let children: Vec<NodeId> = tree.children(root_id).to_vec();
    assert!(children.len() >= 3);

    let one = children[0];
    let two = children[1];
    let three = children[2];

    // Focus "one".
    dispatch_event_to_target_tree(&mut tree, one, &Event::Focus(FocusEvent { node: one }));
    assert_eq!(focused_node_id_tree(&tree), Some(one));

    // Transfer: one -> two.
    dispatch_event_to_target_tree(&mut tree, one, &Event::Blur(BlurEvent { node: one }));
    dispatch_event_to_target_tree(&mut tree, two, &Event::Focus(FocusEvent { node: two }));
    assert_eq!(focused_node_id_tree(&tree), Some(two));

    // Transfer: two -> three.
    dispatch_event_to_target_tree(&mut tree, two, &Event::Blur(BlurEvent { node: two }));
    dispatch_event_to_target_tree(&mut tree, three, &Event::Focus(FocusEvent { node: three }));
    assert_eq!(
        focused_node_id_tree(&tree),
        Some(three),
        "only 'three' should be focused"
    );
}

// ===========================================================================
// Additional: tree-wide focus tracking with focused_node_id_tree
// ===========================================================================

#[test]
fn p1g13_focused_node_id_tree_returns_none_when_no_focus() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(TreeFocusProbe::new("x", sink.clone()))
        .with_child(TreeFocusProbe::new("y", sink.clone()));

    let tree = build_widget_tree_from_root(&mut root).expect("tree should have children");

    assert_eq!(
        focused_node_id_tree(&tree),
        None,
        "no node should be focused initially"
    );
}

#[test]
fn p1g13_focused_node_id_tree_tracks_single_focus() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(TreeFocusProbe::new("alpha", sink.clone()))
        .with_child(Label::new("not focusable"))
        .with_child(TreeFocusProbe::new("beta", sink.clone()));

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should have children");
    run_layout_pass(&mut tree, (40, 10));

    let root_id = tree.root().unwrap();
    let children: Vec<NodeId> = tree.children(root_id).to_vec();

    // Alpha is first child, beta is third.
    let alpha = children[0];

    dispatch_event_to_target_tree(&mut tree, alpha, &Event::Focus(FocusEvent { node: alpha }));
    assert_eq!(
        focused_node_id_tree(&tree),
        Some(alpha),
        "focused_node_id_tree should track the focused probe"
    );

    // Verify non-focusable Label doesn't interfere.
    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"alpha:true".to_string()),
        "alpha should record focus; events={events:?}"
    );
}

// ===========================================================================
// Structural: build_widget_tree_from_root produces correct tree shape
// ===========================================================================

#[test]
fn p1g13_tree_structure_matches_widget_hierarchy() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(TreeFocusProbe::new("A", sink.clone()))
        .with_child(
            Vertical::new()
                .with_child(TreeFocusProbe::new("B", sink.clone()))
                .with_child(TreeFocusProbe::new("C", sink)),
        );

    let tree = build_widget_tree_from_root(&mut root).expect("tree should build");
    let root_id = tree.root().unwrap();

    // Root (TreeStub) should have 2 children: ProbeA and Vertical.
    let top_children = tree.children(root_id);
    assert_eq!(
        top_children.len(),
        2,
        "root should have 2 top-level children"
    );

    // Second child (Vertical) should have 2 children: ProbeB and ProbeC.
    let vertical = top_children[1];
    let vertical_children = tree.children(vertical);
    assert_eq!(
        vertical_children.len(),
        2,
        "Vertical container should have 2 children"
    );

    // All leaf nodes should be focusable probes.
    let probe_a = top_children[0];
    let probe_b = vertical_children[0];
    let probe_c = vertical_children[1];

    assert!(
        tree.children(probe_a).is_empty(),
        "probe A should be a leaf"
    );
    assert!(
        tree.children(probe_b).is_empty(),
        "probe B should be a leaf"
    );
    assert!(
        tree.children(probe_c).is_empty(),
        "probe C should be a leaf"
    );
}

// ===========================================================================
// Event dispatch through tree: key events reach focused widget
// ===========================================================================

#[test]
fn p1g13_key_event_dispatched_to_focused_node_via_tree() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    /// Probe that records key events.
    struct KeyProbe {
        focused: bool,
        keys: Arc<Mutex<Vec<String>>>,
    }

    impl KeyProbe {
        fn new(keys: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                focused: false,
                keys,
            }
        }

        fn set_focus(&mut self, focused: bool) {
            self.focused = focused;
        }
    }

    impl Widget for KeyProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }
        fn layout_height(&self) -> Option<usize> {
            Some(1)
        }
        fn focusable(&self) -> bool {
            true
        }
        fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
            match event {
                Event::Focus(_) => {
                    self.set_focus(true);
                    ctx.set_handled();
                }
                Event::Key(key_data) => {
                    self.keys
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(key_data.key.clone());
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    let keys = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(Label::new("header"))
        .with_child(KeyProbe::new(keys.clone()));

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should have children");
    run_layout_pass(&mut tree, (40, 10));

    let root_id = tree.root().unwrap();
    let children: Vec<NodeId> = tree.children(root_id).to_vec();
    let probe_id = children[1]; // KeyProbe is the second child.

    // Focus the probe.
    dispatch_event_to_target_tree(
        &mut tree,
        probe_id,
        &Event::Focus(FocusEvent { node: probe_id }),
    );
    assert_eq!(focused_node_id_tree(&tree), Some(probe_id));

    // Dispatch a key event to the focused node.
    let tab_event = Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
    )));
    let outcome = dispatch_event_tree(&mut tree, Some(probe_id), &tab_event);
    assert!(
        outcome.handled,
        "key event should be handled by focused probe"
    );

    let recorded = keys.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        !recorded.is_empty(),
        "focused probe should receive the key event; recorded={recorded:?}"
    );
}

#[test]
fn p1g13_buttons_advanced_like_chain_focus_transfer_is_single_owner() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root =
        Dock::new().push_fill(ScrollView::new(Horizontal::new().with_compose(compose![
            VerticalScroll::new().with_compose(compose![
                TreeFocusProbe::new("left_a", sink.clone()),
                TreeFocusProbe::new("left_b", sink.clone()),
            ]),
            VerticalScroll::new().with_compose(compose![
                TreeFocusProbe::new("right_a", sink.clone()),
                TreeFocusProbe::new("right_b", sink.clone()),
            ]),
        ])));

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should have children");
    run_layout_pass(&mut tree, (80, 20));

    let root_id = tree.root().unwrap();
    let leaves: Vec<NodeId> = tree
        .walk_depth_first(root_id)
        .into_iter()
        .filter(|&id| tree.children(id).is_empty())
        .collect();
    assert!(
        leaves.len() >= 4,
        "expected at least four focus probes in wrapper chain, got {}",
        leaves.len()
    );

    let mut left_a = None;
    let mut right_a = None;
    for leaf in leaves {
        let before_len = sink.lock().unwrap_or_else(|e| e.into_inner()).len();
        let outcome = dispatch_event_to_target_tree(
            &mut tree,
            leaf,
            &Event::Focus(FocusEvent { node: leaf }),
        );
        let focused = focused_node_id_tree(&tree);
        let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let new_events = &events[before_len..];
        if outcome.handled && focused == Some(leaf) {
            if new_events.iter().any(|event| event == "left_a:true") {
                left_a = Some(leaf);
            }
            if new_events.iter().any(|event| event == "right_a:true") {
                right_a = Some(leaf);
            }
        }
        if let Some(current) = focused {
            dispatch_event_to_target_tree(
                &mut tree,
                current,
                &Event::Blur(BlurEvent { node: current }),
            );
        }
        if left_a.is_some() && right_a.is_some() {
            break;
        }
    }
    let left_a = left_a.expect("left_a probe node should be discoverable in tree leaves");
    let right_a = right_a.expect("right_a probe node should be discoverable in tree leaves");

    dispatch_event_to_target_tree(
        &mut tree,
        left_a,
        &Event::Focus(FocusEvent { node: left_a }),
    );
    assert_eq!(focused_node_id_tree(&tree), Some(left_a));

    dispatch_event_to_target_tree(&mut tree, left_a, &Event::Blur(BlurEvent { node: left_a }));
    dispatch_event_to_target_tree(
        &mut tree,
        right_a,
        &Event::Focus(FocusEvent { node: right_a }),
    );
    assert_eq!(focused_node_id_tree(&tree), Some(right_a));

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(events.contains(&"left_a:true".to_string()));
    assert!(events.contains(&"left_a:false".to_string()));
    assert!(events.contains(&"right_a:true".to_string()));
}
