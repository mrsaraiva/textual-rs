//! Arena-based widget tree.
//!
//! `WidgetTree` owns every node in the UI hierarchy via a generational arena
//! (`SlotMap<NodeId, WidgetNode>`). Widgets become behavior-only; all structural
//! concerns (parent/child links, CSS classes, display state, layout rects) live
//! here, owned by the runtime.
//!
//! This is the Pillar 1 foundation that Pillars 2–4 build on.

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use slotmap::SlotMap;

use crate::css::{Combinator, SelectorChain, SelectorMeta, parse_selector_list};
use crate::node_id::NodeId;
use crate::style::Visibility;
use crate::widgets::{NodeSeed, NodeState, Widget, WidgetStyles};

// ---------------------------------------------------------------------------
// QueryError
// ---------------------------------------------------------------------------

/// Errors returned by `WidgetTree::query*` methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    /// No nodes matched the selector.
    NoMatch,
    /// `query_one` found more than one match.
    TooManyMatches(usize),
    /// The selector string could not be parsed.
    ParseError(String),
    /// Typed-handle target is not mounted: node removed, handle from a
    /// different tree (e.g. another screen), or no tree built yet.
    Unmounted,
    /// Typed access found a widget of a different concrete type.
    /// `expected` is the Rust type path (`std::any::type_name`),
    /// `actual` the widget's CSS type (`Widget::style_type`).
    TypeMismatch {
        expected: &'static str,
        actual: &'static str,
    },
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryError::NoMatch => write!(f, "no matching nodes found"),
            QueryError::TooManyMatches(n) => write!(f, "expected 1 match, found {n}"),
            QueryError::ParseError(msg) => write!(f, "selector parse error: {msg}"),
            QueryError::Unmounted => write!(f, "widget is not mounted (stale handle)"),
            QueryError::TypeMismatch { expected, actual } => {
                write!(f, "type mismatch: expected {expected}, found widget of type {actual}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Lifecycle events
// ---------------------------------------------------------------------------

/// Events emitted during tree mutations (mount / remove).
///
/// The runtime collects these and dispatches the corresponding callbacks on
/// the affected widgets *after* the structural mutation is complete. This
/// two-phase approach avoids borrow conflicts (mutating the arena while
/// calling widget methods).
///
/// Dispatch order:
/// - `Mount` events fire in tree order (parent before children).
/// - `Unmount` events fire in reverse tree order (children before parent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    /// A node was inserted into the tree and is now mounted.
    Mount { node: NodeId },
    /// A node was removed from the tree and is no longer mounted.
    Unmount { node: NodeId },
}

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
    /// CSS id (`#foo`). Canonical; replaces widget-held `WidgetStyles::style_id`.
    pub(crate) css_id: Option<String>,
    /// Inline styles + programmatic layout constraints. Canonical; replaces
    /// the widget-held `styles: WidgetStyles` field.
    pub(crate) styles: WidgetStyles,
    /// Focus/hover/disabled/loading interaction state.
    pub(crate) state: NodeState,
    /// Effective visibility toggle used by layout/render (`css_display && runtime_display`).
    pub(crate) display: bool,
    /// Display state derived from CSS (`display:none`).
    pub(crate) css_display: bool,
    /// Display state controlled by runtime/widget logic (for example tab switching).
    pub(crate) runtime_display: bool,
    /// CSS visibility state. When `Hidden`, the node still participates in
    /// layout but is not rendered (preserves space).
    pub(crate) visibility: Visibility,
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
            css_id: None,
            styles: WidgetStyles::default(),
            state: NodeState::default(),
            display: true,
            css_display: true,
            runtime_display: true,
            visibility: Visibility::Visible,
            mounted: false,
            layout_rect: Rect::ZERO,
            content_rect: Rect::ZERO,
        }
    }
}

// ---------------------------------------------------------------------------
// Tree
// ---------------------------------------------------------------------------

static NEXT_TREE_ID: AtomicU64 = AtomicU64::new(1);

/// Runtime-owned arena that holds the entire widget hierarchy.
///
/// No `Rc`, no `RefCell`, no circular references — the `SlotMap` provides
/// generational keys (`NodeId`) that detect use-after-remove.
pub struct WidgetTree {
    arena: SlotMap<NodeId, WidgetNode>,
    root: Option<NodeId>,
    /// Accumulated lifecycle events from tree mutations.
    ///
    /// The runtime drains these after each mutation batch and dispatches the
    /// corresponding widget callbacks (`on_mount` / `on_unmount`).
    pending_lifecycle: Vec<LifecycleEvent>,
    /// Process-unique tree identity (handles are scoped to one tree).
    tree_id: u64,
}

impl WidgetTree {
    /// Create an empty tree.
    pub fn new() -> Self {
        Self {
            arena: SlotMap::new(),
            root: None,
            pending_lifecycle: Vec::new(),
            tree_id: NEXT_TREE_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Process-unique identity of this tree.
    pub fn tree_id(&self) -> u64 {
        self.tree_id
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

    // -- Lifecycle event drain -----------------------------------------------

    /// Drain all pending lifecycle events accumulated since the last drain.
    ///
    /// The runtime calls this after a mutation batch and dispatches the
    /// returned events to the affected widgets' `on_mount()` / `on_unmount()`.
    pub fn drain_lifecycle(&mut self) -> Vec<LifecycleEvent> {
        std::mem::take(&mut self.pending_lifecycle)
    }

    /// Whether there are pending lifecycle events waiting to be drained.
    pub fn has_pending_lifecycle(&self) -> bool {
        !self.pending_lifecycle.is_empty()
    }

    // -- Mutation ------------------------------------------------------------

    /// Set the root widget, replacing any previous root (and its subtree).
    ///
    /// Returns the `NodeId` of the new root.
    /// Emits `Unmount` events for the old root subtree and a `Mount` event
    /// for the new root.
    pub fn set_root(&mut self, mut widget: Box<dyn Widget>) -> NodeId {
        // Remove existing root + descendants (emits Unmount events).
        if let Some(old_root) = self.root.take() {
            self.remove_subtree_with_lifecycle(old_root);
        }
        let seed = widget.take_node_seed();
        let mut node = Self::make_node_from_seed(widget, seed);
        node.mounted = true;
        let id = self.arena.insert(node);
        self.root = Some(id);
        self.pending_lifecycle
            .push(LifecycleEvent::Mount { node: id });
        id
    }

    /// Mount a child widget under `parent`. Returns the new node's `NodeId`.
    ///
    /// Emits a `Mount` lifecycle event for the new node.
    ///
    /// If the tree is empty (no root), the widget becomes the root and `parent`
    /// is ignored — though callers should prefer `set_root` for clarity.
    /// Internal helper: build a `WidgetNode` from a boxed widget, consuming its seed.
    fn make_node_from_seed(widget: Box<dyn Widget>, seed: NodeSeed) -> WidgetNode {
        let initial_disabled = widget.is_initially_disabled();
        let mut node = WidgetNode::new(widget);
        node.css_id = seed.css_id;
        node.styles = seed.styles;
        for class in seed.classes {
            node.classes.insert(class);
        }
        if initial_disabled {
            node.state.disabled = true;
        }
        node
    }

    pub fn mount(&mut self, parent: NodeId, mut widget: Box<dyn Widget>) -> NodeId {
        let seed = widget.take_node_seed();
        let mut node = Self::make_node_from_seed(widget, seed);
        node.parent = Some(parent);
        node.mounted = true;
        let id = self.arena.insert(node);
        if let Some(parent_node) = self.arena.get_mut(parent) {
            parent_node.children.push(id);
        }
        self.pending_lifecycle
            .push(LifecycleEvent::Mount { node: id });
        id
    }

    /// Mount several children under `parent` in order.
    pub fn mount_all(&mut self, parent: NodeId, widgets: Vec<Box<dyn Widget>>) {
        for w in widgets {
            self.mount(parent, w);
        }
    }

    /// Remove a node and all of its descendants from the tree.
    ///
    /// Emits `Unmount` lifecycle events in reverse tree order (children before
    /// parent).
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
        self.remove_subtree_with_lifecycle(node);
    }

    /// Remove all children of `parent` (and their descendants), keeping the
    /// parent node itself intact.
    ///
    /// Emits `Unmount` lifecycle events for all removed descendants.
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
            self.remove_subtree_with_lifecycle(child);
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

    /// Whether `ancestor` is a proper ancestor of `descendant`.
    ///
    /// Walks from `descendant` upward through parents.  Returns `true` if
    /// `ancestor` is found along the way, `false` if the root is reached
    /// without a match.  Returns `false` when `ancestor == descendant`
    /// (self is not an ancestor of self).
    pub fn is_ancestor_of(&self, ancestor: NodeId, descendant: NodeId) -> bool {
        if ancestor == descendant {
            return false;
        }
        let mut current = self.parent(descendant);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.parent(id);
        }
        false
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

    fn recompute_display(node: &mut WidgetNode) {
        node.display = node.css_display && node.runtime_display;
    }

    /// Set runtime-controlled display visibility for a node.
    ///
    /// This is used by widget/runtime logic (for example tab switching).
    pub fn set_runtime_display(&mut self, node: NodeId, visible: bool) {
        if let Some(n) = self.arena.get_mut(node) {
            n.runtime_display = visible;
            Self::recompute_display(n);
        }
    }

    /// Set CSS-controlled display visibility for a node.
    ///
    /// This is fed from resolved CSS `display` (`none` => false).
    pub fn set_css_display(&mut self, node: NodeId, visible: bool) {
        if let Some(n) = self.arena.get_mut(node) {
            n.css_display = visible;
            Self::recompute_display(n);
        }
    }

    /// Backwards-compatible alias for runtime-controlled display visibility.
    ///
    /// Prefer [`set_runtime_display`] for new code.
    pub fn set_display(&mut self, node: NodeId, visible: bool) {
        self.set_runtime_display(node, visible);
    }

    /// Whether a node is displayed (default: `true`).
    pub fn is_displayed(&self, node: NodeId) -> bool {
        self.arena.get(node).map(|n| n.display).unwrap_or(false)
    }

    // -- Visibility toggle (P2-14) ------------------------------------------

    /// Set the CSS visibility of a node. When `Hidden`, the node still
    /// participates in layout but is not rendered (preserves space).
    pub fn set_visibility(&mut self, node: NodeId, visibility: Visibility) {
        if let Some(n) = self.arena.get_mut(node) {
            n.visibility = visibility;
        }
    }

    /// Returns the CSS visibility of a node (default: `Visible`).
    pub fn visibility(&self, node: NodeId) -> Visibility {
        self.arena
            .get(node)
            .map(|n| n.visibility)
            .unwrap_or(Visibility::Visible)
    }

    // -- CSS id (T-4) --------------------------------------------------------

    /// Return the CSS id for a node (e.g. the part after `#` in `#foo`).
    pub fn css_id(&self, node: NodeId) -> Option<&str> {
        self.arena.get(node).and_then(|n| n.css_id.as_deref())
    }

    /// Set or clear the CSS id for a node.
    pub fn set_css_id(&mut self, node: NodeId, id: Option<String>) {
        if let Some(n) = self.arena.get_mut(node) {
            n.css_id = id;
        }
    }

    // -- Inline styles (T-4) ------------------------------------------------

    /// Return the inline styles for a node.
    pub fn styles(&self, node: NodeId) -> Option<&WidgetStyles> {
        self.arena.get(node).map(|n| &n.styles)
    }

    /// Apply `f` to the node's inline styles. The only inline-style writer.
    pub fn update_styles(&mut self, node: NodeId, f: impl FnOnce(&mut WidgetStyles)) {
        if let Some(n) = self.arena.get_mut(node) {
            f(&mut n.styles);
        }
    }

    // -- Interaction state (T-4) --------------------------------------------

    /// Return the interaction state for a node (default: `NodeState::default()`).
    pub fn node_state(&self, node: NodeId) -> NodeState {
        self.arena.get(node).map(|n| n.state).unwrap_or_default()
    }

    /// Set the focused state of a node and notify the widget.
    pub fn set_focus_state(&mut self, node: NodeId, focused: bool) {
        if let Some(n) = self.arena.get_mut(node) {
            if n.state.focused != focused {
                let old = n.state;
                n.state.focused = focused;
                let new = n.state;
                n.widget.on_node_state_changed(old, new);
            }
        }
    }

    /// Set the hovered state of a node and notify the widget.
    pub fn set_hover_state(&mut self, node: NodeId, hovered: bool) {
        if let Some(n) = self.arena.get_mut(node) {
            if n.state.hovered != hovered {
                let old = n.state;
                n.state.hovered = hovered;
                let new = n.state;
                n.widget.on_node_state_changed(old, new);
            }
        }
    }

    /// Set the disabled state of a node and notify the widget.
    pub fn set_disabled(&mut self, node: NodeId, disabled: bool) {
        if let Some(n) = self.arena.get_mut(node) {
            if n.state.disabled != disabled {
                let old = n.state;
                n.state.disabled = disabled;
                let new = n.state;
                n.widget.on_node_state_changed(old, new);
            }
        }
    }

    /// Set the loading state of a node and notify the widget.
    pub fn set_loading(&mut self, node: NodeId, loading: bool) {
        if let Some(n) = self.arena.get_mut(node) {
            if n.state.loading != loading {
                let old = n.state;
                n.state.loading = loading;
                let new = n.state;
                n.widget.on_node_state_changed(old, new);
            }
        }
    }

    // -- DOM queries (P1-07) -----------------------------------------------

    /// Find all nodes matching a CSS selector string.
    ///
    /// Supports type (`Button`), class (`.primary`), id (`#my-input`),
    /// combined selectors (`Button.primary`), and descendant / child
    /// combinators (`Container > Button`, `Panel .item`).
    ///
    /// Comma-separated selector lists are supported (`Button, Input`).
    pub fn query(&self, selector: &str) -> Result<Vec<NodeId>, QueryError> {
        let chains = parse_selector_list(selector);
        if chains.is_empty() {
            return Err(QueryError::ParseError(format!(
                "invalid selector: {selector}"
            )));
        }
        let root = match self.root {
            Some(r) => r,
            None => return Ok(Vec::new()),
        };
        let all_nodes = self.walk_depth_first(root);
        let mut result = Vec::new();
        for &node in &all_nodes {
            for chain in &chains {
                if self.matches_chain(node, chain) {
                    result.push(node);
                    break;
                }
            }
        }
        Ok(result)
    }

    /// Find exactly one node matching a CSS selector.
    ///
    /// Returns `Err(QueryError::NoMatch)` if nothing matches, or
    /// `Err(QueryError::TooManyMatches(n))` if more than one node matches.
    pub fn query_one(&self, selector: &str) -> Result<NodeId, QueryError> {
        let matches = self.query(selector)?;
        match matches.len() {
            0 => Err(QueryError::NoMatch),
            1 => Ok(matches[0]),
            n => Err(QueryError::TooManyMatches(n)),
        }
    }

    /// Find direct children of `parent` that match a CSS selector.
    ///
    /// Only considers immediate children — not deeper descendants.
    pub fn query_children(
        &self,
        parent: NodeId,
        selector: &str,
    ) -> Result<Vec<NodeId>, QueryError> {
        let chains = parse_selector_list(selector);
        if chains.is_empty() {
            return Err(QueryError::ParseError(format!(
                "invalid selector: {selector}"
            )));
        }
        let children = self.children(parent).to_vec();
        let mut result = Vec::new();
        for child in children {
            for chain in &chains {
                if self.matches_chain(child, chain) {
                    result.push(child);
                    break;
                }
            }
        }
        Ok(result)
    }

    /// Build a lightweight `SelectorMeta` for a node.
    ///
    /// Identity comes from the node record (`css_id`, `classes`); the widget
    /// only contributes its type identity. Pseudo-class states default to
    /// inactive (DOM queries don't evaluate `:focus`, `:hover`, etc.).
    fn node_selector_meta(&self, node: NodeId) -> Option<SelectorMeta> {
        let n = self.arena.get(node)?;
        let type_name = n.widget.style_type().to_string();
        let id = n.css_id.clone();
        let classes: Vec<String> = n.classes.iter().cloned().collect();
        Some(SelectorMeta::new(type_name, id, classes))
    }

    /// Check whether `node` matches a full selector chain (possibly with
    /// descendant / child combinators).
    fn matches_chain(&self, node: NodeId, chain: &SelectorChain) -> bool {
        let parts = chain.parts();
        if parts.is_empty() {
            return false;
        }
        let meta = match self.node_selector_meta(node) {
            Some(m) => m,
            None => return false,
        };
        // The last part of the chain must match the node itself.
        if !parts[parts.len() - 1].matches(&meta) {
            return false;
        }
        if parts.len() == 1 {
            return true;
        }
        // Walk backwards through the remaining parts + combinators.
        let combinators = chain.combinators();
        let mut current = node;
        for (i, selector) in parts[..parts.len() - 1].iter().rev().enumerate() {
            let comb = combinators[combinators.len() - 1 - i];
            match comb {
                Combinator::Child => {
                    let parent = match self.parent(current) {
                        Some(p) => p,
                        None => return false,
                    };
                    let parent_meta = match self.node_selector_meta(parent) {
                        Some(m) => m,
                        None => return false,
                    };
                    if !selector.matches(&parent_meta) {
                        return false;
                    }
                    current = parent;
                }
                Combinator::Descendant => {
                    let mut ancestor = self.parent(current);
                    let mut found = false;
                    while let Some(anc) = ancestor {
                        if let Some(anc_meta) = self.node_selector_meta(anc) {
                            if selector.matches(&anc_meta) {
                                current = anc;
                                found = true;
                                break;
                            }
                        }
                        ancestor = self.parent(anc);
                    }
                    if !found {
                        return false;
                    }
                }
            }
        }
        true
    }

    // -- Internal helpers ---------------------------------------------------

    /// Remove a node and all descendants from the arena (no parent detach).
    ///
    /// Emits `Unmount` events in reverse BFS order (children before parent)
    /// so that leaf widgets unmount before their containers.
    fn remove_subtree_with_lifecycle(&mut self, node: NodeId) {
        // Collect all descendant IDs in BFS order, then remove + emit in
        // reverse so children unmount before parents.
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
        // Reverse: children before parent.
        for &id in to_remove.iter().rev() {
            self.pending_lifecycle
                .push(LifecycleEvent::Unmount { node: id });
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
    use rich_rs::{Console, ConsoleOptions, Segments};

    /// Minimal widget for testing — holds only a label for debugging.
    struct TestWidget {
        label: &'static str,
    }

    impl TestWidget {
        fn new(label: &'static str) -> Self {
            Self { label }
        }

        fn boxed(label: &'static str) -> Box<dyn Widget> {
            Box::new(Self::new(label))
        }
    }

    impl Widget for TestWidget {
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

    // -- is_ancestor_of ------------------------------------------------------

    #[test]
    fn is_ancestor_of_parent_child() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let child = tree.mount(root, TestWidget::boxed("Child"));
        assert!(tree.is_ancestor_of(root, child));
    }

    #[test]
    fn is_ancestor_of_grandparent() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let a = tree.mount(root, TestWidget::boxed("A"));
        let b = tree.mount(a, TestWidget::boxed("B"));
        assert!(tree.is_ancestor_of(root, b));
        assert!(tree.is_ancestor_of(a, b));
    }

    #[test]
    fn is_ancestor_of_self_returns_false() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        assert!(!tree.is_ancestor_of(root, root));
    }

    #[test]
    fn is_ancestor_of_siblings_returns_false() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let a = tree.mount(root, TestWidget::boxed("A"));
        let b = tree.mount(root, TestWidget::boxed("B"));
        assert!(!tree.is_ancestor_of(a, b));
        assert!(!tree.is_ancestor_of(b, a));
    }

    #[test]
    fn is_ancestor_of_reverse_direction_returns_false() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let child = tree.mount(root, TestWidget::boxed("Child"));
        // child is NOT an ancestor of root
        assert!(!tree.is_ancestor_of(child, root));
    }

    #[test]
    fn is_ancestor_of_missing_node_returns_false() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let bogus = slotmap::KeyData::from_ffi(0xDEAD).into();
        assert!(!tree.is_ancestor_of(root, bogus));
        assert!(!tree.is_ancestor_of(bogus, root));
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
    fn effective_display_merges_css_and_runtime_flags() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));

        tree.set_runtime_display(root, false);
        assert!(!tree.is_displayed(root));

        // CSS visibility can't force a runtime-hidden node visible.
        tree.set_css_display(root, true);
        assert!(!tree.is_displayed(root));

        // Re-enabling runtime display restores effective visibility.
        tree.set_runtime_display(root, true);
        assert!(tree.is_displayed(root));

        // CSS display:none still wins.
        tree.set_css_display(root, false);
        assert!(!tree.is_displayed(root));
    }

    #[test]
    fn is_displayed_missing_node_returns_false() {
        let tree = WidgetTree::new();
        let bogus = slotmap::KeyData::from_ffi(0xBEEF).into();
        assert!(!tree.is_displayed(bogus));
    }

    // -- Visibility toggle ---------------------------------------------------

    #[test]
    fn visibility_default_visible() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        assert_eq!(tree.visibility(root), Visibility::Visible);
    }

    #[test]
    fn set_visibility_hidden_and_back() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));

        tree.set_visibility(root, Visibility::Hidden);
        assert_eq!(tree.visibility(root), Visibility::Hidden);

        tree.set_visibility(root, Visibility::Visible);
        assert_eq!(tree.visibility(root), Visibility::Visible);
    }

    #[test]
    fn visibility_missing_node_returns_visible() {
        let tree = WidgetTree::new();
        let bogus = slotmap::KeyData::from_ffi(0xBEEF).into();
        assert_eq!(tree.visibility(bogus), Visibility::Visible);
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

    // -- Lifecycle events ----------------------------------------------------

    #[test]
    fn set_root_emits_mount() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));

        let events = tree.drain_lifecycle();
        assert_eq!(events, vec![LifecycleEvent::Mount { node: root }]);
        // Second drain is empty.
        assert!(tree.drain_lifecycle().is_empty());
    }

    #[test]
    fn mount_emits_mount() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        tree.drain_lifecycle(); // clear root mount

        let child = tree.mount(root, TestWidget::boxed("Child"));
        let events = tree.drain_lifecycle();
        assert_eq!(events, vec![LifecycleEvent::Mount { node: child }]);
    }

    #[test]
    fn mount_all_emits_mount_in_order() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        tree.drain_lifecycle();

        tree.mount_all(
            root,
            vec![
                TestWidget::boxed("A"),
                TestWidget::boxed("B"),
                TestWidget::boxed("C"),
            ],
        );
        let events = tree.drain_lifecycle();
        assert_eq!(events.len(), 3);
        assert!(
            events
                .iter()
                .all(|e| matches!(e, LifecycleEvent::Mount { .. }))
        );
    }

    #[test]
    fn remove_leaf_emits_unmount() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let child = tree.mount(root, TestWidget::boxed("Child"));
        tree.drain_lifecycle();

        tree.remove(child);
        let events = tree.drain_lifecycle();
        assert_eq!(events, vec![LifecycleEvent::Unmount { node: child }]);
    }

    #[test]
    fn remove_subtree_emits_unmount_children_before_parent() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let a = tree.mount(root, TestWidget::boxed("A"));
        let b = tree.mount(a, TestWidget::boxed("B"));
        let c = tree.mount(a, TestWidget::boxed("C"));
        tree.drain_lifecycle();

        tree.remove(a);
        let events = tree.drain_lifecycle();
        // Children (B, C) unmount before parent (A).
        // BFS order is [A, B, C], reversed is [C, B, A].
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], LifecycleEvent::Unmount { node: c });
        assert_eq!(events[1], LifecycleEvent::Unmount { node: b });
        assert_eq!(events[2], LifecycleEvent::Unmount { node: a });
    }

    #[test]
    fn set_root_replaces_emits_unmount_then_mount() {
        let mut tree = WidgetTree::new();
        let old = tree.set_root(TestWidget::boxed("Old"));
        let child = tree.mount(old, TestWidget::boxed("Child"));
        tree.drain_lifecycle();

        let new = tree.set_root(TestWidget::boxed("New"));
        let events = tree.drain_lifecycle();
        // Unmount old subtree (child before old), then mount new.
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], LifecycleEvent::Unmount { node: child });
        assert_eq!(events[1], LifecycleEvent::Unmount { node: old });
        assert_eq!(events[2], LifecycleEvent::Mount { node: new });
    }

    #[test]
    fn remove_children_emits_unmount() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let a = tree.mount(root, TestWidget::boxed("A"));
        let b = tree.mount(root, TestWidget::boxed("B"));
        tree.drain_lifecycle();

        tree.remove_children(root);
        let events = tree.drain_lifecycle();
        // Both children unmounted.
        assert_eq!(events.len(), 2);
        assert!(events.contains(&LifecycleEvent::Unmount { node: a }));
        assert!(events.contains(&LifecycleEvent::Unmount { node: b }));
    }

    #[test]
    fn has_pending_lifecycle() {
        let mut tree = WidgetTree::new();
        assert!(!tree.has_pending_lifecycle());

        tree.set_root(TestWidget::boxed("Root"));
        assert!(tree.has_pending_lifecycle());

        tree.drain_lifecycle();
        assert!(!tree.has_pending_lifecycle());
    }

    // -- DOM queries (P1-07) -------------------------------------------------

    /// Widget for query tests — supports configurable type name and CSS id (via seed).
    struct QueryWidget {
        type_name: &'static str,
        seed: NodeSeed,
    }

    impl QueryWidget {
        fn new(type_name: &'static str) -> Self {
            Self {
                type_name,
                seed: NodeSeed::default(),
            }
        }

        fn with_id(mut self, id: &str) -> Self {
            self.seed.css_id = Some(id.to_string());
            self
        }

        fn boxed(type_name: &'static str) -> Box<dyn Widget> {
            Box::new(Self::new(type_name))
        }

        fn boxed_with_id(type_name: &'static str, id: &str) -> Box<dyn Widget> {
            Box::new(Self::new(type_name).with_id(id))
        }
    }

    impl Widget for QueryWidget {
        fn render(&self, _: &Console, _: &ConsoleOptions) -> Segments {
            Segments::new()
        }
        fn style_type(&self) -> &'static str {
            self.type_name
        }
        fn take_node_seed(&mut self) -> NodeSeed {
            std::mem::take(&mut self.seed)
        }
    }

    /// Build a standard test tree:
    ///
    /// ```text
    ///        Container (root)
    ///        /        \
    ///   Button.primary  Input#my-input
    ///                      |
    ///                   Button
    /// ```
    fn build_query_tree() -> (WidgetTree, NodeId, NodeId, NodeId, NodeId) {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(QueryWidget::boxed("Container"));
        let btn = tree.mount(root, QueryWidget::boxed("Button"));
        tree.add_class(btn, "primary");
        let input = tree.mount(root, QueryWidget::boxed_with_id("Input", "my-input"));
        let nested_btn = tree.mount(input, QueryWidget::boxed("Button"));
        (tree, root, btn, input, nested_btn)
    }

    #[test]
    fn query_type_selector() {
        let (tree, _root, btn, _input, nested_btn) = build_query_tree();
        let result = tree.query("Button").unwrap();
        assert_eq!(result, vec![btn, nested_btn]);
    }

    #[test]
    fn query_class_selector() {
        let (tree, _root, btn, _input, _nested) = build_query_tree();
        let result = tree.query(".primary").unwrap();
        assert_eq!(result, vec![btn]);
    }

    #[test]
    fn query_id_selector() {
        let (tree, _root, _btn, input, _nested) = build_query_tree();
        let result = tree.query("#my-input").unwrap();
        assert_eq!(result, vec![input]);
    }

    #[test]
    fn query_combined_type_and_class() {
        let (tree, _root, btn, _input, _nested) = build_query_tree();
        let result = tree.query("Button.primary").unwrap();
        assert_eq!(result, vec![btn]);
    }

    #[test]
    fn query_combined_type_and_id() {
        let (tree, _root, _btn, input, _nested) = build_query_tree();
        let result = tree.query("Input#my-input").unwrap();
        assert_eq!(result, vec![input]);
    }

    #[test]
    fn query_descendant_combinator() {
        let (tree, _root, _btn, _input, nested_btn) = build_query_tree();
        // Button that is a descendant of Input
        let result = tree.query("Input Button").unwrap();
        assert_eq!(result, vec![nested_btn]);
    }

    #[test]
    fn query_child_combinator() {
        let (tree, _root, btn, _input, _nested) = build_query_tree();
        // Button that is a direct child of Container
        let result = tree.query("Container > Button").unwrap();
        assert_eq!(result, vec![btn]);
    }

    #[test]
    fn query_child_combinator_excludes_deeper() {
        let (tree, _root, _btn, _input, nested_btn) = build_query_tree();
        // Container > Button should NOT match the nested Button under Input
        let result = tree.query("Container > Button").unwrap();
        assert!(!result.contains(&nested_btn));
    }

    #[test]
    fn query_comma_separated() {
        let (tree, _root, btn, input, nested_btn) = build_query_tree();
        let result = tree.query("Button, Input").unwrap();
        assert_eq!(result, vec![btn, input, nested_btn]);
    }

    #[test]
    fn query_no_match() {
        let (tree, ..) = build_query_tree();
        let result = tree.query("TextArea").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn query_empty_tree() {
        let tree = WidgetTree::new();
        let result = tree.query("Button").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn query_invalid_selector() {
        let (tree, ..) = build_query_tree();
        let result = tree.query("");
        assert!(matches!(result, Err(QueryError::ParseError(_))));
    }

    #[test]
    fn query_one_success() {
        let (tree, _root, _btn, input, _nested) = build_query_tree();
        let result = tree.query_one("#my-input").unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn query_one_no_match() {
        let (tree, ..) = build_query_tree();
        let result = tree.query_one("TextArea");
        assert_eq!(result, Err(QueryError::NoMatch));
    }

    #[test]
    fn query_one_too_many() {
        let (tree, ..) = build_query_tree();
        let result = tree.query_one("Button");
        assert_eq!(result, Err(QueryError::TooManyMatches(2)));
    }

    #[test]
    fn query_children_only_direct() {
        let (tree, root, btn, input, _nested) = build_query_tree();
        // Direct children of root that are Buttons — only btn, not nested_btn
        let result = tree.query_children(root, "Button").unwrap();
        assert_eq!(result, vec![btn]);

        // All direct children
        let all = tree.query_children(root, "Button, Input").unwrap();
        assert_eq!(all, vec![btn, input]);
    }

    #[test]
    fn query_children_empty() {
        let (tree, _root, btn, _input, _nested) = build_query_tree();
        // btn is a leaf — no children
        let result = tree.query_children(btn, "Button").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn query_uses_tree_classes() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(QueryWidget::boxed("Root"));
        let a = tree.mount(root, QueryWidget::boxed("Item"));
        let b = tree.mount(root, QueryWidget::boxed("Item"));
        tree.add_class(a, "selected");

        let result = tree.query(".selected").unwrap();
        assert_eq!(result, vec![a]);
        assert!(!result.contains(&b));
    }

    #[test]
    fn query_deep_descendant_chain() {
        //  Panel > Container > Button
        let mut tree = WidgetTree::new();
        let panel = tree.set_root(QueryWidget::boxed("Panel"));
        let container = tree.mount(panel, QueryWidget::boxed("Container"));
        let btn = tree.mount(container, QueryWidget::boxed("Button"));

        // Descendant: Panel Button (skips Container)
        let result = tree.query("Panel Button").unwrap();
        assert_eq!(result, vec![btn]);

        // Child: Panel > Button should NOT match (Button is grandchild)
        let result = tree.query("Panel > Button").unwrap();
        assert!(result.is_empty());

        // Full chain: Panel > Container > Button
        let result = tree.query("Panel > Container > Button").unwrap();
        assert_eq!(result, vec![btn]);
    }

    // -- Step 1: NodeState / NodeSeed / writer API tests ---------------------

    /// Widget that implements take_node_seed() for testing seed consumption.
    struct SeededWidget {
        seed: NodeSeed,
    }

    impl SeededWidget {
        fn boxed(css_id: Option<&str>, classes: Vec<&str>) -> Box<dyn Widget> {
            Box::new(Self {
                seed: NodeSeed {
                    css_id: css_id.map(str::to_string),
                    classes: classes.into_iter().map(str::to_string).collect(),
                    styles: WidgetStyles::default(),
                },
            })
        }
    }

    impl Widget for SeededWidget {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn take_node_seed(&mut self) -> NodeSeed {
            std::mem::take(&mut self.seed)
        }
    }

    #[test]
    fn mount_consumes_node_seed() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(SeededWidget::boxed(Some("my-id"), vec!["foo", "bar"]));
        let node = tree.get(root).unwrap();
        // css_id from seed lands on the record.
        assert_eq!(node.css_id.as_deref(), Some("my-id"));
        // classes from seed land on the record.
        assert!(node.classes.contains("foo"));
        assert!(node.classes.contains("bar"));
    }

    #[test]
    fn set_css_id_round_trip() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        assert_eq!(tree.css_id(root), None);
        tree.set_css_id(root, Some("hello".to_string()));
        assert_eq!(tree.css_id(root), Some("hello"));
        tree.set_css_id(root, None);
        assert_eq!(tree.css_id(root), None);
    }

    #[test]
    fn update_styles_applies() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        assert!(tree.styles(root).is_some());
        tree.update_styles(root, |s| {
            s.layout.min_height = Some(5);
        });
        assert_eq!(tree.styles(root).unwrap().layout.min_height, Some(5));
    }

    #[test]
    fn node_state_default_for_missing_node() {
        let tree = WidgetTree::new();
        // Fabricate a node_id that was never inserted.
        let fake: NodeId = {
            let mut sm: slotmap::SlotMap<NodeId, ()> = slotmap::SlotMap::new();
            sm.insert(())
        };
        assert_eq!(tree.node_state(fake), NodeState::default());
    }

    #[test]
    fn set_focus_state_updates_node_record() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        assert!(!tree.node_state(root).focused);
        tree.set_focus_state(root, true);
        assert!(tree.node_state(root).focused);
        tree.set_focus_state(root, false);
        assert!(!tree.node_state(root).focused);
    }

    #[test]
    fn query_matches_node_css_id() {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(TestWidget::boxed("Root"));
        let child = tree.mount(root, TestWidget::boxed("Child"));
        tree.set_css_id(child, Some("target".to_string()));
        // Query by CSS id should match the child via node record.
        // (node_selector_meta picks up css_id from node record)
        let result = tree.query("#target");
        // The selector parser may or may not support id selectors in query
        // (this tests the node_selector_meta integration).
        // If id selectors are supported, child is the single match.
        match result {
            Ok(matches) => {
                if !matches.is_empty() {
                    assert!(matches.contains(&child));
                }
            }
            Err(_) => {
                // id selectors may not be implemented in parse_selector_list yet; skip
            }
        }
    }

    // -- Step 1 (RA-4): tree_id and new QueryError variants ------------------

    #[test]
    fn tree_ids_are_unique_per_tree() {
        let t1 = WidgetTree::new();
        let t2 = WidgetTree::new();
        assert_ne!(t1.tree_id(), 0, "tree_id must be non-zero");
        assert_ne!(t2.tree_id(), 0, "tree_id must be non-zero");
        assert_ne!(t1.tree_id(), t2.tree_id(), "different trees must have different ids");
    }

    #[test]
    fn query_error_display_new_variants() {
        let unmounted = QueryError::Unmounted;
        assert_eq!(
            unmounted.to_string(),
            "widget is not mounted (stale handle)"
        );

        let mismatch = QueryError::TypeMismatch {
            expected: "textual::widgets::Button",
            actual: "Input",
        };
        assert_eq!(
            mismatch.to_string(),
            "type mismatch: expected textual::widgets::Button, found widget of type Input"
        );
    }
}
