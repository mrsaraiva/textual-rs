//! Arena-based widget tree.
//!
//! `WidgetTree` owns every node in the UI hierarchy via a generational arena
//! (`SlotMap<NodeId, WidgetNode>`). Widgets become behavior-only; all structural
//! concerns (parent/child links, CSS classes, display state, layout rects) live
//! here, owned by the runtime.
//!
//! This is the Pillar 1 foundation that Pillars 2–4 build on.

use std::collections::{HashSet, VecDeque};

use slotmap::SlotMap;

use crate::node_id::NodeId;
use crate::widgets::Widget;

// ---------------------------------------------------------------------------
// Rect (local to widget tree; will unify with runtime::types::Rect in P1-12)
// ---------------------------------------------------------------------------

/// Axis-aligned rectangle in terminal cells.
///
/// A separate copy from `runtime::types::Rect` because that module is private.
/// The two will be unified when the render pipeline migrates to WidgetTree (P1-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x0: u16,
    pub(crate) y0: u16,
    pub(crate) x1: u16,
    pub(crate) y1: u16,
}

impl Rect {
    pub(crate) const ZERO: Self = Self {
        x0: 0,
        y0: 0,
        x1: 0,
        y1: 0,
    };
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

/// A single node in the arena-based widget tree.
pub struct WidgetNode {
    /// The widget's behavior (render, events, messages).
    pub(crate) widget: Box<dyn Widget>,
    /// Parent in the tree (`None` for the root).
    pub(crate) parent: Option<NodeId>,
    /// Ordered children.
    pub(crate) children: Vec<NodeId>,
    /// Dynamic CSS classes (F14).
    pub(crate) classes: HashSet<String>,
    /// Visibility toggle (F15). When `false`, excluded from layout + render.
    pub(crate) display: bool,
    /// Lifecycle state — `true` after mount, `false` after removal.
    pub(crate) mounted: bool,
    /// Positioned region from layout solver (co-designed with Pillar 2).
    pub(crate) layout_rect: Rect,
    /// Content area after padding/border (co-designed with Pillar 2).
    pub(crate) content_rect: Rect,
}

impl WidgetNode {
    fn new(widget: Box<dyn Widget>) -> Self {
        Self {
            widget,
            parent: None,
            children: Vec::new(),
            classes: HashSet::new(),
            display: true,
            mounted: false,
            layout_rect: Rect::ZERO,
            content_rect: Rect::ZERO,
        }
    }
}

// ---------------------------------------------------------------------------
// Tree
// ---------------------------------------------------------------------------

/// Runtime-owned arena that holds the entire widget hierarchy.
///
/// No `Rc`, no `RefCell`, no circular references — the `SlotMap` provides
/// generational keys (`NodeId`) that detect use-after-remove.
pub struct WidgetTree {
    arena: SlotMap<NodeId, WidgetNode>,
    root: Option<NodeId>,
}

impl WidgetTree {
    /// Create an empty tree.
    pub fn new() -> Self {
        Self {
            arena: SlotMap::new(),
            root: None,
        }
    }

    // -- Accessors ----------------------------------------------------------

    /// The root node, if any.
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    /// Immutable access to a node.
    pub fn get(&self, node: NodeId) -> Option<&WidgetNode> {
        self.arena.get(node)
    }

    /// Mutable access to a node.
    pub fn get_mut(&mut self, node: NodeId) -> Option<&mut WidgetNode> {
        self.arena.get_mut(node)
    }

    /// Whether a node is present in the arena.
    pub fn contains(&self, node: NodeId) -> bool {
        self.arena.contains_key(node)
    }

    /// Number of live nodes.
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    /// Whether the tree has no nodes.
    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    // -- Mutation ------------------------------------------------------------

    /// Set the root widget, replacing any previous root (and its subtree).
    ///
    /// Returns the `NodeId` of the new root.
    pub fn set_root(&mut self, widget: Box<dyn Widget>) -> NodeId {
        // Remove existing root + descendants.
        if let Some(old_root) = self.root.take() {
            self.remove_subtree(old_root);
        }
        let mut node = WidgetNode::new(widget);
        node.mounted = true;
        let id = self.arena.insert(node);
        self.root = Some(id);
        id
    }

    /// Mount a child widget under `parent`. Returns the new node's `NodeId`.
    ///
    /// If the tree is empty (no root), the widget becomes the root and `parent`
    /// is ignored — though callers should prefer `set_root` for clarity.
    pub fn mount(&mut self, parent: NodeId, widget: Box<dyn Widget>) -> NodeId {
        let mut node = WidgetNode::new(widget);
        node.parent = Some(parent);
        node.mounted = true;
        let id = self.arena.insert(node);
        if let Some(parent_node) = self.arena.get_mut(parent) {
            parent_node.children.push(id);
        }
        id
    }

    /// Mount several children under `parent` in order.
    pub fn mount_all(
        &mut self,
        parent: NodeId,
        widgets: Vec<Box<dyn Widget>>,
    ) {
        for w in widgets {
            self.mount(parent, w);
        }
    }

    /// Remove a node and all of its descendants from the tree.
    pub fn remove(&mut self, node: NodeId) {
        // Detach from parent's children list.
        if let Some(parent_id) = self.arena.get(node).and_then(|n| n.parent) {
            if let Some(parent_node) = self.arena.get_mut(parent_id) {
                parent_node.children.retain(|&c| c != node);
            }
        }
        // If this was the root, clear it.
        if self.root == Some(node) {
            self.root = None;
        }
        self.remove_subtree(node);
    }

    /// Remove all children of `parent` (and their descendants), keeping the
    /// parent node itself intact.
    pub fn remove_children(&mut self, parent: NodeId) {
        let child_ids: Vec<NodeId> = self
            .arena
            .get(parent)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        // Clear parent's children vec first.
        if let Some(parent_node) = self.arena.get_mut(parent) {
            parent_node.children.clear();
        }
        for child in child_ids {
            self.remove_subtree(child);
        }
    }

    /// Move `node` from its current parent to `new_parent`.
    ///
    /// The node (and its subtree) is appended as the last child of `new_parent`.
    /// No-op if `node` does not exist.
    pub fn move_node(&mut self, node: NodeId, new_parent: NodeId) {
        // Detach from old parent.
        if let Some(old_parent_id) = self.arena.get(node).and_then(|n| n.parent) {
            if let Some(old_parent) = self.arena.get_mut(old_parent_id) {
                old_parent.children.retain(|&c| c != node);
            }
        }
        // Attach to new parent.
        if let Some(new_parent_node) = self.arena.get_mut(new_parent) {
            new_parent_node.children.push(node);
        }
        if let Some(n) = self.arena.get_mut(node) {
            n.parent = Some(new_parent);
        }
    }

    // -- Class manipulation (P1-08) -----------------------------------------

    /// Add a CSS class to a node.
    pub fn add_class(&mut self, node: NodeId, class: &str) {
        if let Some(n) = self.arena.get_mut(node) {
            n.classes.insert(class.to_string());
        }
    }

    /// Remove a CSS class from a node.
    pub fn remove_class(&mut self, node: NodeId, class: &str) {
        if let Some(n) = self.arena.get_mut(node) {
            n.classes.remove(class);
        }
    }

    /// Toggle a CSS class: add if absent, remove if present. Returns `true` if
    /// the class is now present.
    pub fn toggle_class(&mut self, node: NodeId, class: &str) -> bool {
        if let Some(n) = self.arena.get_mut(node) {
            if n.classes.contains(class) {
                n.classes.remove(class);
                false
            } else {
                n.classes.insert(class.to_string());
                true
            }
        } else {
            false
        }
    }

    /// Check whether a node has a CSS class.
    pub fn has_class(&self, node: NodeId, class: &str) -> bool {
        self.arena
            .get(node)
            .map(|n| n.classes.contains(class))
            .unwrap_or(false)
    }

    /// Replace all CSS classes on a node.
    pub fn set_classes(&mut self, node: NodeId, classes: &[&str]) {
        if let Some(n) = self.arena.get_mut(node) {
            n.classes.clear();
            for c in classes {
                n.classes.insert((*c).to_string());
            }
        }
    }

    // -- Traversal (P1-09) --------------------------------------------------

    /// The parent of `node`, if any.
    pub fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.arena.get(node).and_then(|n| n.parent)
    }

    /// Ordered children of `node`.
    pub fn children(&self, node: NodeId) -> &[NodeId] {
        self.arena
            .get(node)
            .map(|n| n.children.as_slice())
            .unwrap_or(&[])
    }

    /// Ancestor chain from `node` upward (not including `node` itself).
    /// Returns `[parent, grandparent, …, root]`.
    pub fn ancestors(&self, node: NodeId) -> Vec<NodeId> {
        let mut result = Vec::new();
        let mut current = self.parent(node);
        while let Some(id) = current {
            result.push(id);
            current = self.parent(id);
        }
        result
    }

    /// Depth-first (pre-order) walk starting at `root`.
    /// Includes `root` as the first element.
    pub fn walk_depth_first(&self, root: NodeId) -> Vec<NodeId> {
        let mut result = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !self.arena.contains_key(id) {
                continue;
            }
            result.push(id);
            // Push children in reverse so the first child is visited first.
            let children = self.children(id);
            for &child in children.iter().rev() {
                stack.push(child);
            }
        }
        result
    }

    /// Breadth-first walk starting at `root`.
    /// Includes `root` as the first element.
    pub fn walk_breadth_first(&self, root: NodeId) -> Vec<NodeId> {
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(root);
        while let Some(id) = queue.pop_front() {
            if !self.arena.contains_key(id) {
                continue;
            }
            result.push(id);
            for &child in self.children(id) {
                queue.push_back(child);
            }
        }
        result
    }

    // -- Display toggle (P1-10) ---------------------------------------------

    /// Set the display visibility of a node. When `false`, the node (and
    /// descendants) should be excluded from layout and render.
    pub fn set_display(&mut self, node: NodeId, visible: bool) {
        if let Some(n) = self.arena.get_mut(node) {
            n.display = visible;
        }
    }

    /// Whether a node is displayed (default: `true`).
    pub fn is_displayed(&self, node: NodeId) -> bool {
        self.arena.get(node).map(|n| n.display).unwrap_or(false)
    }

    // -- Internal helpers ---------------------------------------------------

    /// Remove a node and all descendants from the arena (no parent detach).
    fn remove_subtree(&mut self, node: NodeId) {
        // Collect all descendant IDs first (BFS), then remove.
        let mut to_remove = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(node);
        while let Some(id) = queue.pop_front() {
            if let Some(n) = self.arena.get(id) {
                for &child in &n.children {
                    queue.push_back(child);
                }
                to_remove.push(id);
            }
        }
        for id in to_remove {
            self.arena.remove(id);
        }
    }
}

impl Default for WidgetTree {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::WidgetId;
    use rich_rs::{Console, ConsoleOptions, Segments};

    /// Minimal widget for testing — holds only an ID and a label for debugging.
    struct TestWidget {
        id: WidgetId,
        label: &'static str,
    }

    impl TestWidget {
        fn new(label: &'static str) -> Self {
            Self {
                id: WidgetId::new(),
                label,
            }
        }

        fn boxed(label: &'static str) -> Box<dyn Widget> {
            Box::new(Self::new(label))
        }
    }

    impl Widget for TestWidget {
        fn id(&self) -> WidgetId {
            self.id
        }

        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            self.label
        }
    }

    // -- Mount / structure ---------------------------------------------------

    #[test]
    fn set_root_creates_single_node() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.root(), Some(root));
        assert!(tree.contains(root));
        let node = tree.get(root).unwrap();
        assert!(node.parent.is_none());
        assert!(node.children.is_empty());
        assert!(node.mounted);
        assert!(node.display);
    }

    #[test]
    fn mount_single_child() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let child = tree.mount(root, TestWidget::boxed("Child"));

        assert_eq!(tree.len(), 2);
        assert_eq!(tree.parent(child), Some(root));
        assert_eq!(tree.children(root), &[child]);
    }

    #[test]
    fn mount_multiple_children() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let a = tree.mount(root, TestWidget::boxed("A"));
        let b = tree.mount(root, TestWidget::boxed("B"));
        let c = tree.mount(root, TestWidget::boxed("C"));

        assert_eq!(tree.children(root), &[a, b, c]);
        assert_eq!(tree.parent(a), Some(root));
        assert_eq!(tree.parent(b), Some(root));
        assert_eq!(tree.parent(c), Some(root));
    }

    #[test]
    fn mount_all_preserves_order() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        tree.mount_all(
            root,
            vec![
                TestWidget::boxed("X"),
                TestWidget::boxed("Y"),
                TestWidget::boxed("Z"),
            ],
        );
        assert_eq!(tree.len(), 4); // root + 3
        assert_eq!(tree.children(root).len(), 3);
    }

    #[test]
    fn mount_nested() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let a = tree.mount(root, TestWidget::boxed("A"));
        let b = tree.mount(a, TestWidget::boxed("B"));

        assert_eq!(tree.parent(b), Some(a));
        assert_eq!(tree.children(a), &[b]);
        assert!(tree.children(b).is_empty());
    }

    // -- Remove --------------------------------------------------------------

    #[test]
    fn remove_leaf() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let child = tree.mount(root, TestWidget::boxed("Child"));
        tree.remove(child);

        assert_eq!(tree.len(), 1);
        assert!(!tree.contains(child));
        assert!(tree.children(root).is_empty());
    }

    #[test]
    fn remove_subtree() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let a = tree.mount(root, TestWidget::boxed("A"));
        let b = tree.mount(a, TestWidget::boxed("B"));
        let c = tree.mount(a, TestWidget::boxed("C"));
        let d = tree.mount(b, TestWidget::boxed("D"));

        tree.remove(a);

        assert_eq!(tree.len(), 1); // only root
        assert!(!tree.contains(a));
        assert!(!tree.contains(b));
        assert!(!tree.contains(c));
        assert!(!tree.contains(d));
        assert!(tree.children(root).is_empty());
    }

    #[test]
    fn remove_root_clears_tree() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let _child = tree.mount(root, TestWidget::boxed("Child"));
        tree.remove(root);

        assert!(tree.is_empty());
        assert!(tree.root().is_none());
    }

    #[test]
    fn remove_children_keeps_parent() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let a = tree.mount(root, TestWidget::boxed("A"));
        let b = tree.mount(root, TestWidget::boxed("B"));
        let c = tree.mount(a, TestWidget::boxed("C"));

        tree.remove_children(root);

        assert_eq!(tree.len(), 1); // only root
        assert!(tree.contains(root));
        assert!(!tree.contains(a));
        assert!(!tree.contains(b));
        assert!(!tree.contains(c));
        assert!(tree.children(root).is_empty());
    }

    // -- Move ----------------------------------------------------------------

    #[test]
    fn move_node_between_parents() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let a = tree.mount(root, TestWidget::boxed("A"));
        let b = tree.mount(root, TestWidget::boxed("B"));
        let child = tree.mount(a, TestWidget::boxed("Child"));

        // Move child from A to B.
        tree.move_node(child, b);

        assert!(tree.children(a).is_empty());
        assert_eq!(tree.children(b), &[child]);
        assert_eq!(tree.parent(child), Some(b));
    }

    #[test]
    fn move_node_with_subtree() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let a = tree.mount(root, TestWidget::boxed("A"));
        let b = tree.mount(root, TestWidget::boxed("B"));
        let c = tree.mount(a, TestWidget::boxed("C"));
        let _d = tree.mount(c, TestWidget::boxed("D"));

        tree.move_node(a, b);

        // A is now under B, and its subtree (C, D) should still be intact.
        assert_eq!(tree.parent(a), Some(b));
        assert_eq!(tree.children(b), &[a]);
        assert_eq!(tree.children(a), &[c]);
        assert_eq!(tree.len(), 5);
    }

    // -- Class manipulation --------------------------------------------------

    #[test]
    fn add_and_has_class() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));

        assert!(!tree.has_class(root, "highlight"));
        tree.add_class(root, "highlight");
        assert!(tree.has_class(root, "highlight"));
    }

    #[test]
    fn remove_class() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        tree.add_class(root, "active");
        tree.remove_class(root, "active");
        assert!(!tree.has_class(root, "active"));
    }

    #[test]
    fn toggle_class() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));

        let now_present = tree.toggle_class(root, "foo");
        assert!(now_present);
        assert!(tree.has_class(root, "foo"));

        let now_present = tree.toggle_class(root, "foo");
        assert!(!now_present);
        assert!(!tree.has_class(root, "foo"));
    }

    #[test]
    fn set_classes_replaces_all() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        tree.add_class(root, "old");
        tree.set_classes(root, &["new1", "new2"]);

        assert!(!tree.has_class(root, "old"));
        assert!(tree.has_class(root, "new1"));
        assert!(tree.has_class(root, "new2"));
    }

    #[test]
    fn class_operations_on_missing_node_are_noop() {
        let mut tree = WidgetTree::new();
        let bogus = slotmap::KeyData::from_ffi(0xDEAD).into();
        // None of these should panic.
        tree.add_class(bogus, "x");
        tree.remove_class(bogus, "x");
        assert!(!tree.toggle_class(bogus, "x"));
        assert!(!tree.has_class(bogus, "x"));
        tree.set_classes(bogus, &["x"]);
    }

    // -- Traversal -----------------------------------------------------------

    #[test]
    fn ancestors_chain() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let a = tree.mount(root, TestWidget::boxed("A"));
        let b = tree.mount(a, TestWidget::boxed("B"));
        let c = tree.mount(b, TestWidget::boxed("C"));

        let anc = tree.ancestors(c);
        assert_eq!(anc, vec![b, a, root]);
    }

    #[test]
    fn ancestors_of_root_is_empty() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        assert!(tree.ancestors(root).is_empty());
    }

    #[test]
    fn walk_depth_first_order() {
        //       R
        //      / \
        //     A   B
        //    / \
        //   C   D
        let mut tree = WidgetTree::new();
        let r = tree.set_root(TestWidget::boxed("R"));
        let a = tree.mount(r, TestWidget::boxed("A"));
        let b = tree.mount(r, TestWidget::boxed("B"));
        let c = tree.mount(a, TestWidget::boxed("C"));
        let d = tree.mount(a, TestWidget::boxed("D"));

        let order = tree.walk_depth_first(r);
        assert_eq!(order, vec![r, a, c, d, b]);
    }

    #[test]
    fn walk_breadth_first_order() {
        //       R
        //      / \
        //     A   B
        //    / \
        //   C   D
        let mut tree = WidgetTree::new();
        let r = tree.set_root(TestWidget::boxed("R"));
        let a = tree.mount(r, TestWidget::boxed("A"));
        let b = tree.mount(r, TestWidget::boxed("B"));
        let c = tree.mount(a, TestWidget::boxed("C"));
        let d = tree.mount(a, TestWidget::boxed("D"));

        let order = tree.walk_breadth_first(r);
        assert_eq!(order, vec![r, a, b, c, d]);
    }

    #[test]
    fn walk_single_node() {
        let mut tree = WidgetTree::new();
        let r = tree.set_root(TestWidget::boxed("R"));
        assert_eq!(tree.walk_depth_first(r), vec![r]);
        assert_eq!(tree.walk_breadth_first(r), vec![r]);
    }

    // -- Display toggle ------------------------------------------------------

    #[test]
    fn display_default_true() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        assert!(tree.is_displayed(root));
    }

    #[test]
    fn set_display_false_and_back() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));

        tree.set_display(root, false);
        assert!(!tree.is_displayed(root));

        tree.set_display(root, true);
        assert!(tree.is_displayed(root));
    }

    #[test]
    fn is_displayed_missing_node_returns_false() {
        let tree = WidgetTree::new();
        let bogus = slotmap::KeyData::from_ffi(0xBEEF).into();
        assert!(!tree.is_displayed(bogus));
    }

    // -- Edge cases ----------------------------------------------------------

    #[test]
    fn empty_tree() {
        let tree = WidgetTree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        assert!(tree.root().is_none());
    }

    #[test]
    fn set_root_replaces_previous() {
        let mut tree = WidgetTree::new();
        let old = tree.set_root(TestWidget::boxed("Old"));
        let _child = tree.mount(old, TestWidget::boxed("Child"));
        assert_eq!(tree.len(), 2);

        let new = tree.set_root(TestWidget::boxed("New"));
        assert_eq!(tree.len(), 1);
        assert!(!tree.contains(old));
        assert_eq!(tree.root(), Some(new));
    }

    #[test]
    fn children_of_missing_node_returns_empty() {
        let tree = WidgetTree::new();
        let bogus = slotmap::KeyData::from_ffi(0xCAFE).into();
        assert!(tree.children(bogus).is_empty());
    }
}
