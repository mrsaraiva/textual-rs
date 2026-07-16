//! Tree stable-node-identity tests.
//!
//! Ports of the Python Textual test files, adapted to the Rust id-based API
//! (Python raises become typed `TreeError` results; Python `TreeNode` object
//! methods become `Tree` methods taking a `TreeNodeId`):
//! - `tests/tree/test_tree_get_node_by_id.py`
//! - `tests/tree/test_tree_node_parent.py`
//! - `tests/tree/test_tree_node_children.py`
//! - `tests/tree/test_tree_node_add.py` (anchor-node forms; the Python index
//!   forms stay expressible via `children_of(parent)[i]`)
//! - `tests/tree/test_tree_clearing.py`
//! - `tests/tree/test_tree_node_label.py`
//! - `tests/tree/test_node_refresh.py` (render essence: a relabel is what the
//!   next render paints; Rust has no `render_label` override to instrument)
//! - `tests/tree/test_tree_cursor.py` / `test_tree_messages.py` structural
//!   halves (the message-capture halves live as unit tests in
//!   `src/widgets/tree/mod.rs`; `EventCtx::take_messages` is crate-private)
//!
//! Plus the residual design-decision pins from the key-identity spec:
//! subtree-removal purge, `clear()` anti-parity, Clone key preservation,
//! DirectoryTree cursor-on-path across rebuild, and the null-key miss.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use slotmap::SlotMap;
use textual::event::EventCtx;
use textual::message::MessageEvent;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::dispatch_ctx::set_dispatch_recipient;

// ── test_tree_get_node_by_id.py ───────────────────────────────────────────

#[test]
fn get_tree_node_by_id() {
    let mut tree = Tree::new(vec![TreeNode::new("Anakin")]);
    let root = tree.root_id().expect("root");
    let child = tree.add(root, TreeNode::new("Leia")).unwrap();
    let grandchild = tree.add(child, TreeNode::new("Ben")).unwrap();

    assert_eq!(tree.get_node_by_id(root).unwrap().id(), root);
    assert_eq!(tree.get_node_by_id(child).unwrap().id(), child);
    assert_eq!(tree.get_node_by_id(grandchild).unwrap().id(), grandchild);

    // Python probes `grandchild.id + 1000` for UnknownNodeID; the generational
    // arena's exact equivalent is a removed node's id no longer resolving.
    tree.remove(grandchild).unwrap();
    assert_eq!(
        tree.get_node_by_id(grandchild),
        Err(TreeError::UnknownNode(grandchild))
    );
}

/// Residual pin: the null key (`TreeNodeId::default()`) is syntactically
/// valid and guaranteed to miss every lookup (extends the bogus-key pin in
/// `src/node_id.rs` to the new key type).
#[test]
fn null_key_misses_every_lookup() {
    let tree = Tree::new(vec![TreeNode::new("Root").with_child(TreeNode::new("Child"))]);
    let null = TreeNodeId::default();
    assert!(tree.node(null).is_none());
    assert_eq!(tree.get_node_by_id(null), Err(TreeError::UnknownNode(null)));
    assert_eq!(tree.parent_of(null), None);
    assert!(tree.children_of(null).is_empty());
    assert!(!tree.is_root(null));
    assert_eq!(tree.label_of(null), None);
    assert_eq!(tree.line_of(null), None);
}

// ── test_tree_node_parent.py ──────────────────────────────────────────────

#[test]
fn tree_node_parent() {
    let mut tree = Tree::new(vec![TreeNode::new("Anakin")]);
    let root = tree.root_id().expect("root");
    let child = tree.add(root, TreeNode::new("Leia")).unwrap();
    let grandchild = tree.add(child, TreeNode::new("Ben")).unwrap();

    assert!(tree.root().unwrap().parent().is_none());
    assert_eq!(tree.parent_of(grandchild), Some(child));
    assert_eq!(tree.parent_of(child), Some(root));
    assert_eq!(
        tree.get_node_by_id(grandchild).unwrap().parent().map(|p| p.id()),
        Some(child)
    );
}

// ── test_tree_node_children.py ────────────────────────────────────────────

#[test]
fn tree_node_children() {
    const CHILDREN: usize = 23;
    let mut tree = Tree::new(vec![TreeNode::new("Root")]);
    let root = tree.root_id().expect("root");
    for child in 0..CHILDREN {
        tree.add(root, TreeNode::new(child.to_string())).unwrap();
    }
    assert_eq!(tree.children_of(root).len(), CHILDREN);
    let labels: Vec<String> = tree
        .root()
        .unwrap()
        .children()
        .map(|c| c.label().to_string())
        .collect();
    assert_eq!(
        labels,
        (0..CHILDREN).map(|n| n.to_string()).collect::<Vec<_>>()
    );
    let first = tree.children_of(root)[0];
    let last = *tree.children_of(root).last().unwrap();
    assert_eq!(tree.label_of(first), Some("0"));
    assert_eq!(tree.label_of(last), Some((CHILDREN - 1).to_string().as_str()));
    // The Python "children acts immutable" assertions are type-level in Rust:
    // `children_of` returns `&[TreeNodeId]`.
}

// ── test_tree_node_add.py (anchor-node forms) ─────────────────────────────

#[test]
fn tree_node_add_before_node() {
    let mut tree = Tree::new(vec![TreeNode::new("root")]);
    let root = tree.root_id().expect("root");
    let node = tree.add(root, TreeNode::new("node")).unwrap();
    let before_node = tree.add_before(node, TreeNode::new("before node")).unwrap();
    tree.add_before(before_node, TreeNode::new("first")).unwrap();
    // "after first" goes right before "before node" (which now has "first"
    // ahead of it).
    tree.add_before(before_node, TreeNode::new("after first")).unwrap();
    let after_node = tree.add_after(node, TreeNode::new("after node")).unwrap();
    let last = tree.add_after(after_node, TreeNode::new("last")).unwrap();
    tree.add_before(last, TreeNode::new("before last")).unwrap();

    let labels: Vec<&str> = tree
        .children_of(root)
        .iter()
        .map(|&id| tree.label_of(id).unwrap())
        .collect();
    assert_eq!(
        labels,
        vec![
            "first",
            "after first",
            "before node",
            "node",
            "after node",
            "before last",
            "last",
        ]
    );
}

#[test]
fn tree_node_add_after_node() {
    let mut tree = Tree::new(vec![TreeNode::new("root")]);
    let root = tree.root_id().expect("root");
    let node = tree.add(root, TreeNode::new("node")).unwrap();
    let after_node = tree.add_after(node, TreeNode::new("after node")).unwrap();
    let first = tree.add_before(node, TreeNode::new("first")).unwrap();
    let after_first = tree.add_after(first, TreeNode::new("after first")).unwrap();
    tree.add_after(after_first, TreeNode::new("before node")).unwrap();
    let before_last = tree.add_after(after_node, TreeNode::new("before last")).unwrap();
    tree.add_after(before_last, TreeNode::new("last")).unwrap();

    let labels: Vec<&str> = tree
        .children_of(root)
        .iter()
        .map(|&id| tree.label_of(id).unwrap())
        .collect();
    assert_eq!(
        labels,
        vec![
            "first",
            "after first",
            "before node",
            "node",
            "after node",
            "before last",
            "last",
        ]
    );
}

#[test]
fn tree_node_add_relative_to_unknown_node_errors() {
    // Python: adding before/after a removed node raises AddNodeError.
    let mut tree = Tree::new(vec![TreeNode::new("root")]);
    let root = tree.root_id().expect("root");
    let removed = tree.add(root, TreeNode::new("removed node")).unwrap();
    tree.remove(removed).unwrap();
    assert_eq!(
        tree.add_before(removed, TreeNode::new("node")),
        Err(TreeError::InvalidAnchor(removed))
    );
    assert_eq!(
        tree.add_after(removed, TreeNode::new("node")),
        Err(TreeError::InvalidAnchor(removed))
    );
}

#[test]
fn tree_node_add_relative_to_root_errors() {
    // Roots have no node-addressed sibling insertion (Python AddNodeError for
    // a non-child anchor).
    let mut tree = Tree::new(vec![TreeNode::new("root")]);
    let root = tree.root_id().expect("root");
    assert_eq!(
        tree.add_before(root, TreeNode::new("node")),
        Err(TreeError::InvalidAnchor(root))
    );
    assert_eq!(
        tree.add_after(root, TreeNode::new("node")),
        Err(TreeError::InvalidAnchor(root))
    );
}

#[test]
fn tree_node_add_leaf_before_or_after() {
    let mut tree = Tree::new(vec![TreeNode::new("root")]);
    let root = tree.root_id().expect("root");
    let leaf = tree.add_leaf(root, "leaf").unwrap();
    tree.add_before(leaf, TreeNode::new("before leaf")).unwrap();
    tree.add_after(leaf, TreeNode::new("after leaf")).unwrap();
    let first_existing = tree.children_of(root)[0];
    tree.add_before(first_existing, TreeNode::new("first")).unwrap();
    let last_existing = *tree.children_of(root).last().unwrap();
    tree.add_after(last_existing, TreeNode::new("last")).unwrap();

    let labels: Vec<&str> = tree
        .children_of(root)
        .iter()
        .map(|&id| tree.label_of(id).unwrap())
        .collect();
    assert_eq!(
        labels,
        vec!["first", "before leaf", "leaf", "after leaf", "last"]
    );
}

#[test]
fn add_seed_inserts_whole_subtree() {
    let mut tree = Tree::new(vec![TreeNode::new("root")]);
    let root = tree.root_id().expect("root");
    let subtree = TreeNode::new("branch")
        .expanded(true)
        .with_child(TreeNode::new("leaf a"))
        .with_child(TreeNode::new("leaf b").with_child(TreeNode::new("deep")));
    let branch = tree.add(root, subtree).unwrap();
    assert_eq!(tree.parent_of(branch), Some(root));
    let kids = tree.children_of(branch).to_vec();
    assert_eq!(kids.len(), 2);
    assert_eq!(tree.label_of(kids[0]), Some("leaf a"));
    assert_eq!(tree.children_of(kids[1]).len(), 1);
}

// ── test_tree_clearing.py ─────────────────────────────────────────────────

/// Python TreeClearApp fixture: root "White Sun" (data) with two planets,
/// each holding two moons.
fn verse_tree() -> Tree {
    let mut tree = Tree::new(vec![
        TreeNode::new("White Sun").with_data("star").expanded(true),
    ]);
    let root = tree.root_id().expect("root");
    let londinium = tree
        .add(root, TreeNode::new("Londinium").with_data("planet"))
        .unwrap();
    tree.add(londinium, TreeNode::new("Balkerne").with_data("moon")).unwrap();
    tree.add(londinium, TreeNode::new("Colchester").with_data("moon")).unwrap();
    let sihnon = tree
        .add(root, TreeNode::new("Sihnon").with_data("planet"))
        .unwrap();
    tree.add(sihnon, TreeNode::new("Airen").with_data("moon")).unwrap();
    tree.add(sihnon, TreeNode::new("Xiaojie").with_data("moon")).unwrap();
    tree
}

#[test]
fn tree_simple_clear() {
    let mut tree = verse_tree();
    let root = tree.root_id().unwrap();
    assert!(tree.children_of(root).len() > 1);
    tree.clear();
    assert_eq!(tree.children_of(root).len(), 0);
    assert_eq!(tree.root().unwrap().label(), "White Sun");
    assert_eq!(tree.root().unwrap().data(), Some("star"));
}

/// Residual pin: `clear()` PURGES cleared descendants, so their ids no longer
/// resolve. DELIBERATE divergence from Python, whose `clear()` leaves
/// `_tree_nodes` stale and resets the id counter (`_tree.py:924-945`), letting
/// stale ids resolve to unrelated nodes.
#[test]
fn tree_clear_purges_descendant_ids() {
    let mut tree = verse_tree();
    let root = tree.root_id().unwrap();
    let planet = tree.children_of(root)[0];
    let moon = tree.children_of(planet)[0];
    tree.clear();
    assert!(tree.node(planet).is_none());
    assert!(tree.node(moon).is_none());
    assert_eq!(tree.get_node_by_id(moon), Err(TreeError::UnknownNode(moon)));
}

#[test]
fn tree_reset_with_label() {
    let mut tree = verse_tree();
    tree.reset("Jiangyin");
    let root = tree.root().expect("root survives reset");
    assert_eq!(root.child_count(), 0);
    assert_eq!(root.label(), "Jiangyin");
    assert_eq!(root.data(), None);
}

#[test]
fn tree_reset_with_label_and_data() {
    let mut tree = verse_tree();
    tree.reset_with_data("Jiangyin", Some("planet".to_string()));
    let root = tree.root().expect("root survives reset");
    assert_eq!(root.child_count(), 0);
    assert_eq!(root.label(), "Jiangyin");
    assert_eq!(root.data(), Some("planet"));
}

#[test]
fn remove_node() {
    let mut tree = verse_tree();
    let root = tree.root_id().unwrap();
    assert_eq!(tree.children_of(root).len(), 2);
    tree.remove(tree.children_of(root)[0]).unwrap();
    assert_eq!(tree.children_of(root).len(), 1);
}

/// Residual pin: subtree removal purges ALL descendant slots (mirrors
/// `_tree.py:482-492`): removing a node invalidates its grandchildren's ids.
#[test]
fn remove_node_purges_descendants() {
    let mut tree = verse_tree();
    let root = tree.root_id().unwrap();
    let planet = tree.children_of(root)[0];
    let moon = tree.children_of(planet)[1];
    tree.remove(planet).unwrap();
    assert!(tree.node(planet).is_none());
    assert!(
        tree.node(moon).is_none(),
        "descendant slots must be purged with their ancestor"
    );
}

#[test]
fn remove_node_children() {
    let mut tree = verse_tree();
    let root = tree.root_id().unwrap();
    let planet = tree.children_of(root)[0];
    assert_eq!(tree.children_of(planet).len(), 2);
    tree.remove_children(planet).unwrap();
    assert_eq!(tree.children_of(root).len(), 2);
    assert_eq!(tree.children_of(planet).len(), 0);
}

#[test]
fn tree_remove_children_of_root() {
    let mut tree = verse_tree();
    let root = tree.root_id().unwrap();
    assert!(tree.children_of(root).len() > 1);
    tree.remove_children(root).unwrap();
    assert_eq!(tree.children_of(root).len(), 0);
}

#[test]
fn attempt_to_remove_root() {
    let mut tree = verse_tree();
    let root = tree.root_id().unwrap();
    assert_eq!(tree.remove(root), Err(TreeError::RemoveRoot));
    assert!(tree.node(root).is_some(), "failed removal must not purge");
}

// ── test_tree_node_label.py ───────────────────────────────────────────────

#[test]
fn tree_node_label_via_tree() {
    let mut tree = Tree::new(vec![TreeNode::new("Xenomorph Lifecycle")]);
    let root = tree.root_id().unwrap();
    let node = tree.add(root, TreeNode::new("Facehugger")).unwrap();
    assert_eq!(tree.label_of(node), Some("Facehugger"));
    tree.set_label(node, "Chestbuster").unwrap();
    assert_eq!(tree.label_of(node), Some("Chestbuster"));
    assert_eq!(
        tree.set_label(TreeNodeId::default(), "nope"),
        Err(TreeError::UnknownNode(TreeNodeId::default()))
    );
}

// ── test_node_refresh.py (render essence) ─────────────────────────────────

#[test]
fn relabel_is_painted_on_next_render() {
    let mut tree = Tree::new(vec![
        TreeNode::new("Root")
            .expanded(true)
            .with_child(TreeNode::new("Child")),
    ]);
    tree.on_layout(30, 3);
    let child = tree.root().unwrap().child_ids()[0];
    tree.set_label(child, "Renamed").unwrap();

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (30, 3);
    options.max_width = 30;
    options.max_height = 3;
    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().any(|line| line.contains("Renamed")));
    assert!(!lines.iter().any(|line| line.contains("Child")));
}

// ── Cursor follows the node (test_tree_cursor.py structural half) ─────────

#[test]
fn cursor_follows_node_across_sibling_insertion() {
    let mut tree = Tree::new(vec![
        TreeNode::new("Root")
            .expanded(true)
            .with_child(TreeNode::new("target")),
    ]);
    tree.on_layout(30, 6);
    let target = tree.root().unwrap().child_ids()[0];
    tree.move_cursor(Some(target));
    assert_eq!(tree.selected(), 1);

    // Inserting a sibling ABOVE the cursor shifts the projected line, but the
    // cursor keeps pointing at the same node (this is the bug the arena
    // fixes: a usize cursor would silently point at the new sibling).
    tree.add_before(target, TreeNode::new("intruder")).unwrap();
    assert_eq!(tree.cursor_node_id(), Some(target));
    assert_eq!(tree.selected(), 2);
    assert_eq!(tree.node_at_line(2), Some(target));
    assert_eq!(tree.line_of(target), Some(2));
}

#[test]
fn cursor_moves_to_visible_ancestor_when_hidden() {
    let mut tree = Tree::new(vec![
        TreeNode::new("Root").expanded(true).with_child(
            TreeNode::new("branch")
                .expanded(true)
                .with_child(TreeNode::new("deep")),
        ),
    ]);
    tree.on_layout(30, 6);
    let branch = tree.root().unwrap().child_ids()[0];
    let deep = tree.children_of(branch)[0];
    tree.move_cursor(Some(deep));
    assert_eq!(tree.selected(), 2);

    tree.collapse(branch).unwrap();
    // The hidden cursor projects onto (and re-anchors to) the collapsed
    // ancestor.
    assert_eq!(tree.selected(), 1);
    assert_eq!(tree.node_at_line(1), Some(branch));
}

// ── Clone key preservation pin ────────────────────────────────────────────

/// Residual pin: `Tree` is `Clone` and slotmap clones preserve keys — ids
/// captured from the original must resolve to the equivalent node in the
/// clone, so a future storage change cannot silently break clone-identity.
#[test]
fn clone_preserves_node_ids() {
    let mut tree = Tree::new(vec![TreeNode::new("Root")]);
    let root = tree.root_id().unwrap();
    let child = tree.add(root, TreeNode::new("Child")).unwrap();
    let grandchild = tree.add(child, TreeNode::new("Grandchild")).unwrap();

    let clone = tree.clone();
    assert_eq!(clone.label_of(child), Some("Child"));
    assert_eq!(clone.label_of(grandchild), Some("Grandchild"));
    assert_eq!(clone.parent_of(grandchild), Some(child));

    // Divergence after cloning stays independent.
    tree.set_label(child, "Renamed").unwrap();
    assert_eq!(clone.label_of(child), Some("Child"));
}

// ── DirectoryTree cursor-on-path pin (spec 3.1.2 scope decision) ──────────

struct TempTreeDir {
    path: PathBuf,
}

impl TempTreeDir {
    fn new(label: &str) -> Self {
        let mut path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        path.push(format!("textual-rs-{label}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).expect("create temp test directory");
        Self { path }
    }
}

impl Drop for TempTreeDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn make_node_id() -> NodeId {
    let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
    sm.insert(())
}

/// DirectoryTree rebuilds its ENTIRE inner Tree on every toggle/load (fresh
/// arena, fresh ids). This pin guards the spec 3.1.2 scope decision: the
/// index/path internals must keep the cursor on the same PATH across that
/// wholesale rebuild, even though arena ids are regenerated per rebuild.
///
/// Flow: expanding "aaa" spawns an async load; the cursor then moves to
/// "zzz.txt"; when the load completes, the rebuild inserts "aaa/inner.txt"
/// ABOVE the cursor, shifting its line, and the cursor must stay on
/// "zzz.txt" by path.
#[test]
fn directory_tree_rebuild_keeps_cursor_on_path() {
    let temp = TempTreeDir::new("tree-identity-cursor-path");
    let nested = temp.path.join("aaa");
    fs::create_dir_all(&nested).expect("create nested dir");
    fs::write(nested.join("inner.txt"), "inner").expect("write nested file");
    fs::write(temp.path.join("zzz.txt"), "zzz").expect("write file");

    let mut tree = DirectoryTree::new(&temp.path);
    let _guard = set_dispatch_recipient(
        make_node_id(),
        NodeState {
            focused: true,
            ..Default::default()
        },
    );
    tree.on_layout(50, 10);

    // Expand "aaa" (index 1, lazy-loaded): spawns async load task 1 and
    // rebuilds anchored on the toggled entry.
    let mut ctx = EventCtx::default();
    {
        let mut w =
            textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        tree.on_message(
            &MessageEvent::new(
                tree.tree_id(),
                TreeNodeToggled {
                    index: 1,
                    label: "aaa".to_string(),
                    expanded: true,
                    node_id: TreeNodeId::default(),
                },
            ),
            &mut w,
        );
    }

    // Entries while the load is pending: root(0), aaa(1), zzz.txt(2).
    // Move the cursor onto zzz.txt.
    let down = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    for _ in 0..2 {
        let mut ctx = EventCtx::default();
        let mut w =
            textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        tree.on_event(&Event::Key(down.clone()), &mut w);
    }
    assert_eq!(
        tree.selected_path().map(|p| p.to_path_buf()),
        Some(temp.path.join("zzz.txt"))
    );

    // Deliver the load result: the wholesale rebuild inserts aaa/inner.txt
    // ABOVE the cursor line.
    let mut ctx = EventCtx::default();
    {
        let mut w =
            textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx);
        tree.on_message(
            &MessageEvent::new(
                NodeId::default(),
                AsyncTaskCompleted {
                    task_id: 1,
                    target: NodeId::default(),
                    result: AsyncTaskResult::DirectoryEntries {
                        path: nested.display().to_string(),
                        entries: vec![AsyncDirectoryEntry {
                            path: nested.join("inner.txt").display().to_string(),
                            label: "inner.txt".to_string(),
                            is_dir: false,
                        }],
                    },
                },
            ),
            &mut w,
        );
    }

    // The cursor stayed on the same PATH (zzz.txt), now at a shifted line.
    assert_eq!(
        tree.selected_path().map(|p| p.to_path_buf()),
        Some(temp.path.join("zzz.txt"))
    );
}
