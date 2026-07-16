use crossterm::event::{KeyCode, KeyModifiers};
use textual_macros::widget;

use crate::event::{Action, Event};
use crate::message::*;

use crate::action::ParsedAction;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

use super::{BindingDecl, NodeSeed, ScrollView, Widget};

mod node;
mod render;

pub use node::{NodeRef, TreeError, TreeNode, TreeNodeId};
use node::TreeNodeData;
use render::VisibleNode;

#[derive(Debug, Clone)]
#[widget(Focus, Interactive, Layout, Scrollable, Components)]
pub struct Tree {
    /// Per-widget node arena: the slotmap IS the id registry (Python's
    /// `_tree_nodes` dict + `_new_id()` counter folded into insertion).
    nodes: slotmap::SlotMap<TreeNodeId, TreeNodeData>,
    /// Ordered root ids (multi-root superset of Python's single root).
    roots: Vec<TreeNodeId>,
    /// Node-anchored cursor (Python's `_cursor_node`). The visible-line
    /// projection is derived per frame; see [`Tree::selected`].
    cursor: Option<TreeNodeId>,
    offset: usize,
    hovered_index: Option<usize>,
    pressed_activation_index: Option<usize>,
    viewport_height: usize,
    scroll_step: usize,
    /// Whether the root node(s) are visible. Default: true.
    show_root: bool,
    /// Whether tree guide lines (│, ├, └) are drawn. Default: true.
    show_guides: bool,
    /// Indentation width per tree level in cells. Clamped to [2, 10]. Default: 4.
    guide_depth: usize,
    /// When `true`, the expand/collapse twisty (`▼`/`▶`) is not rendered.
    ///
    /// Used by [`DirectoryTree`](super::DirectoryTree), which (like Python
    /// Textual) overrides `render_label` so the folder/file emoji *is* the node
    /// prefix and replaces the twisty entirely. With the twisty hidden, the
    /// toggle hit-zone collapses onto the guide region (the emoji prefix that
    /// follows is part of the label and stays click-to-toggle for directories).
    hide_twisty: bool,
    seed: NodeSeed,
}

impl Tree {
    crate::seed_ident_methods!();

    pub fn new(roots: Vec<TreeNode>) -> Self {
        let mut nodes = slotmap::SlotMap::with_key();
        let root_ids: Vec<TreeNodeId> = roots
            .into_iter()
            .map(|seed| Self::insert_seed(&mut nodes, None, seed))
            .collect();
        let cursor = root_ids.first().copied();
        Self {
            nodes,
            roots: root_ids,
            cursor,
            offset: 0,
            hovered_index: None,
            pressed_activation_index: None,
            viewport_height: 1,
            scroll_step: 1,
            show_root: true,
            show_guides: true,
            guide_depth: 4,
            hide_twisty: false,
            seed: NodeSeed::default(),
        }
    }

    /// Flatten a declarative [`TreeNode`] seed (and its subtree) into the
    /// arena, returning the subtree root's id.
    fn insert_seed(
        nodes: &mut slotmap::SlotMap<TreeNodeId, TreeNodeData>,
        parent: Option<TreeNodeId>,
        seed: TreeNode,
    ) -> TreeNodeId {
        let TreeNode {
            label,
            expanded,
            allow_expand,
            disabled,
            component_classes,
            children,
            data,
        } = seed;
        let id = nodes.insert(TreeNodeData {
            label,
            data,
            expanded,
            allow_expand,
            disabled,
            component_classes,
            parent,
            children: Vec::new(),
        });
        for child in children {
            let child_id = Self::insert_seed(nodes, Some(id), child);
            nodes[id].children.push(child_id);
        }
        id
    }

    /// Remove `id` and its whole subtree from the arena (does NOT touch the
    /// parent's child list; callers own that edge).
    fn purge_subtree(&mut self, id: TreeNodeId) {
        if let Some(data) = self.nodes.remove(id) {
            for child in data.children {
                self.purge_subtree(child);
            }
        }
        if self.cursor == Some(id) {
            self.cursor = None;
        }
    }

    /// Hide the expand/collapse twisty (`▼`/`▶`).
    ///
    /// Mirrors Python Textual's `DirectoryTree`, where `render_label` is
    /// overridden so the folder/file emoji replaces the twisty prefix.
    pub fn set_hide_twisty(&mut self, value: bool) {
        self.hide_twisty = value;
    }

    /// Whether the expand/collapse twisty is hidden.
    pub fn twisty_hidden(&self) -> bool {
        self.hide_twisty
    }

    // ── Cursor projection ────────────────────────────────────────────────

    /// Project the node-anchored cursor onto the given visible-node stream.
    ///
    /// A hidden cursor node (collapsed ancestor) projects onto its nearest
    /// visible ancestor; a missing/None cursor projects to line 0 (matching
    /// the previous line-cursor default of highlighting the first row).
    fn selected_line_in(&self, nodes: &[VisibleNode]) -> usize {
        let Some(mut cursor) = self.cursor else {
            return 0;
        };
        loop {
            if let Some(line) = nodes.iter().position(|n| n.id == cursor) {
                return line;
            }
            match self.nodes.get(cursor).and_then(|n| n.parent) {
                Some(parent) => cursor = parent,
                None => return 0,
            }
        }
    }

    /// Current cursor line (projection of the node-anchored cursor).
    fn selected_line(&self) -> usize {
        self.selected_line_in(&self.visible_nodes())
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    /// The cursor position as a visible-line index (Python `cursor_line`).
    ///
    /// This is a per-frame projection of the node-anchored cursor
    /// ([`Tree::cursor_node_id`]); it shifts when nodes above the cursor are
    /// inserted/removed/expanded, while the cursor keeps following its node.
    pub fn selected(&self) -> usize {
        self.selected_line()
    }

    pub fn showing_root(&self) -> bool {
        self.show_root
    }

    pub fn showing_guides(&self) -> bool {
        self.show_guides
    }

    pub fn guide_depth(&self) -> usize {
        self.guide_depth
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `selected` (always_update: fires even when value unchanged).
    ///
    /// Matches Python's `cursor_line = var(-1, always_update=True)` — setting
    /// the cursor to the same position still triggers scroll-into-view and repaint.
    /// Line-oriented entry point (spec 3.1.1): resolves the line to a node id
    /// for the node-anchored cursor, while the reactive record keeps the
    /// `usize` visible-line projection as its old/new values, computed at
    /// record time (watchers keep observing line numbers, like Python's
    /// `cursor_line` watcher).
    pub fn set_selected(&mut self, index: usize, ctx: &mut ReactiveCtx) {
        let nodes = self.visible_nodes();
        if nodes.is_empty() {
            self.cursor = None;
            self.offset = 0;
            return;
        }
        let old = self.selected_line_in(&nodes);
        let new_selected = index.min(nodes.len() - 1);
        self.cursor = Some(nodes[new_selected].id);
        self.ensure_visible();
        ctx.record_change(
            "selected",
            ReactiveFlags::reactive_always_update(),
            Box::new(old),
            Box::new(new_selected),
        );
    }

    /// Reactive setter for `show_root`.
    pub fn set_show_root(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.show_root != value {
            let old = self.show_root;
            self.show_root = value;
            self.clamp_offsets();
            ctx.record_change(
                "show_root",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    /// Reactive setter for `show_guides`.
    pub fn set_show_guides(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.show_guides != value {
            let old = self.show_guides;
            self.show_guides = value;
            ctx.record_change(
                "show_guides",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    /// Reactive setter for `guide_depth`.
    pub fn set_guide_depth(&mut self, value: usize, ctx: &mut ReactiveCtx) {
        let clamped = value.clamp(2, 10);
        if self.guide_depth != clamped {
            let old = self.guide_depth;
            self.guide_depth = clamped;
            ctx.record_change(
                "guide_depth",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(clamped),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_show_root(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        // Offset clamping is done in the setter; layout flag triggers re-layout.
        self.clamp_offsets();
    }

    // ── Root access ────────────────────────────────────────────────────

    /// Read-only view of the root node (first root).
    ///
    /// Mirrors Python's `tree.root` property. Python's Tree always has exactly
    /// one root; Rust's multi-root Vec is an implementation detail.
    pub fn root(&self) -> Option<NodeRef<'_>> {
        self.roots.first().map(|&id| NodeRef { tree: self, id })
    }

    /// Stable id of the root node (first root).
    pub fn root_id(&self) -> Option<TreeNodeId> {
        self.roots.first().copied()
    }

    /// Stable ids of all roots, in order.
    pub fn root_ids(&self) -> &[TreeNodeId] {
        &self.roots
    }

    // ── Lookup and topology (Python `_tree.py` identity API) ───────────

    /// Read-only view of a node by id, or `None` for a stale/unknown id
    /// (non-erroring twin of [`Tree::get_node_by_id`]).
    pub fn node(&self, id: TreeNodeId) -> Option<NodeRef<'_>> {
        self.nodes.contains_key(id).then_some(NodeRef { tree: self, id })
    }

    /// Look up a node by id (Python `get_node_by_id`, typed
    /// `TreeError::UnknownNode` instead of `UnknownNodeID`).
    pub fn get_node_by_id(&self, id: TreeNodeId) -> Result<NodeRef<'_>, TreeError> {
        self.node(id).ok_or(TreeError::UnknownNode(id))
    }

    /// The parent of `id`, or `None` for a root or unknown id.
    pub fn parent_of(&self, id: TreeNodeId) -> Option<TreeNodeId> {
        self.nodes.get(id).and_then(|n| n.parent)
    }

    /// The ordered children of `id` (empty for a leaf or unknown id).
    pub fn children_of(&self, id: TreeNodeId) -> &[TreeNodeId] {
        self.nodes
            .get(id)
            .map(|n| n.children.as_slice())
            .unwrap_or(&[])
    }

    /// The sibling list containing `id`: its parent's children, or the root
    /// list for a root.
    fn sibling_list(&self, id: TreeNodeId) -> &[TreeNodeId] {
        match self.nodes.get(id).and_then(|n| n.parent) {
            Some(parent) => self.children_of(parent),
            None if self.nodes.contains_key(id) => &self.roots,
            None => &[],
        }
    }

    /// The next sibling of `id`, if any (Python `next_sibling`).
    pub fn next_sibling(&self, id: TreeNodeId) -> Option<TreeNodeId> {
        let siblings = self.sibling_list(id);
        let pos = siblings.iter().position(|&s| s == id)?;
        siblings.get(pos + 1).copied()
    }

    /// The previous sibling of `id`, if any (Python `previous_sibling`).
    pub fn previous_sibling(&self, id: TreeNodeId) -> Option<TreeNodeId> {
        let siblings = self.sibling_list(id);
        let pos = siblings.iter().position(|&s| s == id)?;
        pos.checked_sub(1).and_then(|p| siblings.get(p)).copied()
    }

    /// Whether `id` is a live root node (Python `is_root`).
    pub fn is_root(&self, id: TreeNodeId) -> bool {
        self.nodes
            .get(id)
            .is_some_and(|n| n.parent.is_none())
    }

    /// Whether `id` is the last of its siblings (Python `is_last`).
    pub fn is_last(&self, id: TreeNodeId) -> bool {
        self.sibling_list(id).last() == Some(&id)
    }

    // ── Per-node accessors (replace the retired `root_mut()` surgery) ──

    /// The label of `id`, if it resolves.
    pub fn label_of(&self, id: TreeNodeId) -> Option<&str> {
        self.nodes.get(id).map(|n| n.label.as_str())
    }

    /// Set the label of `id` (Python `node.set_label`).
    pub fn set_label(&mut self, id: TreeNodeId, label: impl Into<String>) -> Result<(), TreeError> {
        match self.nodes.get_mut(id) {
            Some(node) => {
                node.label = label.into();
                Ok(())
            }
            None => Err(TreeError::UnknownNode(id)),
        }
    }

    /// The user data of `id`, if it resolves and has data.
    pub fn data_of(&self, id: TreeNodeId) -> Option<&str> {
        self.nodes.get(id).and_then(|n| n.data.as_deref())
    }

    /// Set the user data of `id`.
    pub fn set_data(&mut self, id: TreeNodeId, data: Option<String>) -> Result<(), TreeError> {
        match self.nodes.get_mut(id) {
            Some(node) => {
                node.data = data;
                Ok(())
            }
            None => Err(TreeError::UnknownNode(id)),
        }
    }

    /// Expand `id` (Python `node.expand()`).
    pub fn expand(&mut self, id: TreeNodeId) -> Result<(), TreeError> {
        match self.nodes.get_mut(id) {
            Some(node) => {
                node.expanded = true;
                Ok(())
            }
            None => Err(TreeError::UnknownNode(id)),
        }
    }

    /// Collapse `id` (Python `node.collapse()`).
    pub fn collapse(&mut self, id: TreeNodeId) -> Result<(), TreeError> {
        match self.nodes.get_mut(id) {
            Some(node) => {
                node.expanded = false;
                Ok(())
            }
            None => Err(TreeError::UnknownNode(id)),
        }
    }

    /// Toggle expansion of `id` (Python `node.toggle()`).
    pub fn toggle_node(&mut self, id: TreeNodeId) -> Result<bool, TreeError> {
        match self.nodes.get_mut(id) {
            Some(node) => {
                node.expanded = !node.expanded;
                Ok(node.expanded)
            }
            None => Err(TreeError::UnknownNode(id)),
        }
    }

    /// Set whether `id` can be expanded by the user.
    pub fn set_allow_expand(&mut self, id: TreeNodeId, value: bool) -> Result<(), TreeError> {
        match self.nodes.get_mut(id) {
            Some(node) => {
                node.allow_expand = value;
                Ok(())
            }
            None => Err(TreeError::UnknownNode(id)),
        }
    }

    // ── Key-based CRUD (Python `_tree.py:359-508`) ──────────────────────

    /// Add a seed (and its whole subtree) as the last child of `parent`,
    /// returning the subtree root's id (Python `node.add`).
    pub fn add(&mut self, parent: TreeNodeId, node: TreeNode) -> Result<TreeNodeId, TreeError> {
        if !self.nodes.contains_key(parent) {
            return Err(TreeError::UnknownNode(parent));
        }
        let id = Self::insert_seed(&mut self.nodes, Some(parent), node);
        self.nodes[parent].children.push(id);
        Ok(id)
    }

    /// Add a seed as a leaf child of `parent` (Python `node.add_leaf`).
    pub fn add_leaf(
        &mut self,
        parent: TreeNodeId,
        label: impl Into<String>,
    ) -> Result<TreeNodeId, TreeError> {
        self.add(parent, TreeNode::new(label))
    }

    /// Insert a seed as a sibling of `sibling`, immediately before it
    /// (Python `add(before=node)`). The anchor must be a live non-root node;
    /// a root anchor is `TreeError::InvalidAnchor` (roots have no
    /// node-addressed sibling insertion; use the index form via
    /// `children_of(parent)`), a stale anchor is `TreeError::InvalidAnchor`
    /// like Python's `AddNodeError` for a removed anchor.
    pub fn add_before(
        &mut self,
        sibling: TreeNodeId,
        node: TreeNode,
    ) -> Result<TreeNodeId, TreeError> {
        self.add_relative(sibling, node, 0)
    }

    /// Insert a seed as a sibling of `sibling`, immediately after it
    /// (Python `add(after=node)`).
    pub fn add_after(
        &mut self,
        sibling: TreeNodeId,
        node: TreeNode,
    ) -> Result<TreeNodeId, TreeError> {
        self.add_relative(sibling, node, 1)
    }

    fn add_relative(
        &mut self,
        sibling: TreeNodeId,
        node: TreeNode,
        offset: usize,
    ) -> Result<TreeNodeId, TreeError> {
        let Some(parent) = self.nodes.get(sibling).and_then(|n| n.parent) else {
            return Err(TreeError::InvalidAnchor(sibling));
        };
        let position = self.nodes[parent]
            .children
            .iter()
            .position(|&c| c == sibling)
            .ok_or(TreeError::InvalidAnchor(sibling))?;
        let id = Self::insert_seed(&mut self.nodes, Some(parent), node);
        self.nodes[parent].children.insert(position + offset, id);
        Ok(id)
    }

    /// Remove `id` and its whole subtree (Python `node.remove()`).
    ///
    /// Refuses roots with `TreeError::RemoveRoot`. All descendant slots are
    /// purged, so stale ids reliably miss every lookup afterwards.
    pub fn remove(&mut self, id: TreeNodeId) -> Result<(), TreeError> {
        let Some(node) = self.nodes.get(id) else {
            return Err(TreeError::UnknownNode(id));
        };
        let Some(parent) = node.parent else {
            return Err(TreeError::RemoveRoot);
        };
        self.nodes[parent].children.retain(|&c| c != id);
        self.purge_subtree(id);
        self.clamp_offsets();
        Ok(())
    }

    /// Remove all children of `id`, keeping the node itself
    /// (Python `node.remove_children()`).
    pub fn remove_children(&mut self, id: TreeNodeId) -> Result<(), TreeError> {
        let Some(node) = self.nodes.get_mut(id) else {
            return Err(TreeError::UnknownNode(id));
        };
        let children = std::mem::take(&mut node.children);
        for child in children {
            self.purge_subtree(child);
        }
        self.clamp_offsets();
        Ok(())
    }

    // ── API methods (QW-19) ──────────────────────────────────────────────

    /// Clear all children under the root node.
    ///
    /// Mirrors Python's `tree.clear()` which preserves the root node (label,
    /// data, expanded state) and only removes its children.
    ///
    /// DELIBERATE divergence from Python: cleared descendants are purged from
    /// the arena, so their ids no longer resolve (Python's `clear()` leaves
    /// `_tree_nodes` stale and resets the id counter, letting stale ids
    /// resolve to unrelated nodes).
    pub fn clear(&mut self) {
        if let Some(&root) = self.roots.first() {
            let _ = self.remove_children(root);
            self.cursor = Some(root);
        } else {
            self.cursor = None;
        }
        self.offset = 0;
        self.hovered_index = None;
        self.pressed_activation_index = None;
    }

    /// Clear the tree and reset the root node with a new label
    /// (data cleared). Mirrors Python's `tree.reset(label)`.
    pub fn reset(&mut self, label: impl Into<String>) {
        self.reset_with_data(label, None);
    }

    /// Clear the tree and reset the root node with a new label and data.
    ///
    /// Mirrors Python's `tree.reset(label, data)`.
    pub fn reset_with_data(&mut self, label: impl Into<String>, data: Option<String>) {
        self.nodes.clear();
        let root = self.nodes.insert(TreeNodeData {
            label: label.into(),
            data,
            expanded: false,
            allow_expand: false,
            disabled: false,
            component_classes: Vec::new(),
            parent: None,
            children: Vec::new(),
        });
        self.roots = vec![root];
        self.cursor = Some(root);
        self.offset = 0;
        self.hovered_index = None;
        self.pressed_activation_index = None;
    }

    /// Append a root node without clearing existing ones.
    ///
    /// Resets cursor/selection since the tree structure changed.
    pub fn add_root(&mut self, node: TreeNode) {
        let id = Self::insert_seed(&mut self.nodes, None, node);
        self.roots.push(id);
        self.cursor = self.roots.first().copied();
        self.offset = 0;
    }

    /// Toggle `show_root` without reactive dispatch.
    ///
    /// For use from app-level hooks (`on_key_with_app`) where no `ReactiveCtx`
    /// is available.  Repaint must be requested separately via `ctx.request_repaint()`.
    pub fn toggle_show_root(&mut self) {
        self.show_root = !self.show_root;
        self.cursor = self.visible_nodes().first().map(|n| n.id);
        self.offset = 0;
    }

    /// Non-reactive setter for `show_root`.
    ///
    /// For use in construction contexts where no `ReactiveCtx` is available
    /// (e.g. building a Tree inside MarkdownTableOfContents).
    pub fn set_show_root_plain(&mut self, value: bool) {
        self.show_root = value;
    }

    /// Programmatically select a node by visible line: moves the cursor and
    /// emits `TreeNodeSelected` (line-oriented convenience; Python
    /// `move_cursor_to_line` + select).
    pub fn select_node(&mut self, node_index: usize, ctx: &mut crate::event::WidgetCtx) {
        let nodes = self.visible_nodes();
        let total = nodes.len();
        if total == 0 || node_index >= total {
            return;
        }
        self.cursor = Some(nodes[node_index].id);
        self.ensure_visible();
        self.emit_selected(ctx, &nodes);
        self.emit_highlighted(ctx, &nodes);
    }

    // ── Node-anchored cursor (Python `_tree.py:962-1003`) ──────────────

    /// Stable id of the cursor node, if any.
    pub fn cursor_node_id(&self) -> Option<TreeNodeId> {
        self.cursor.filter(|&id| self.nodes.contains_key(id))
    }

    /// Move the cursor to a node (or to the first visible node for `None`,
    /// matching Python `move_cursor(None)` resetting to the top). Does not
    /// emit messages; use [`Tree::select_node_by_id`] to also select.
    pub fn move_cursor(&mut self, node: Option<TreeNodeId>) {
        match node {
            Some(id) if self.nodes.contains_key(id) => self.cursor = Some(id),
            _ => self.cursor = self.visible_nodes().first().map(|n| n.id),
        }
        self.ensure_visible();
    }

    /// Move the cursor to `id` and emit `TreeNodeSelected` +
    /// `TreeNodeHighlighted` (Python `select_node(node)`).
    pub fn select_node_by_id(
        &mut self,
        id: TreeNodeId,
        ctx: &mut crate::event::WidgetCtx,
    ) -> Result<(), TreeError> {
        if !self.nodes.contains_key(id) {
            return Err(TreeError::UnknownNode(id));
        }
        self.cursor = Some(id);
        self.ensure_visible();
        let nodes = self.visible_nodes();
        self.emit_selected(ctx, &nodes);
        self.emit_highlighted(ctx, &nodes);
        Ok(())
    }

    /// The node currently rendered at visible line `line`, if any (bridges
    /// the line-oriented and id-oriented APIs).
    pub fn node_at_line(&self, line: usize) -> Option<TreeNodeId> {
        self.visible_nodes().get(line).map(|n| n.id)
    }

    /// The current visible line of `id`, or `None` when the node is unknown
    /// or hidden inside a collapsed ancestor.
    pub fn line_of(&self, id: TreeNodeId) -> Option<usize> {
        self.visible_nodes().iter().position(|n| n.id == id)
    }

    /// Expand or collapse all nodes. If any expandable node is expanded, collapse all;
    /// otherwise expand all.
    pub fn toggle_all(&mut self) {
        let any_expanded = self
            .nodes
            .values()
            .any(|node| node.is_expandable() && node.expanded);
        let target = !any_expanded;
        for node in self.nodes.values_mut() {
            if node.is_expandable() {
                node.expanded = target;
            }
        }
        self.ensure_visible();
    }

    /// Enable or disable auto-expand. When enabled, all nodes start expanded.
    ///
    /// Mirrors Python's `Tree.auto_expand = True`. Expands all existing nodes
    /// and marks them so future additions also start expanded.
    pub fn set_auto_expand(&mut self, expand: bool) {
        if expand {
            for node in self.nodes.values_mut() {
                if node.is_expandable() {
                    node.expanded = true;
                }
            }
        }
        self.ensure_visible();
    }

    /// Scroll to make the node at `node_index` visible, moving it to the center
    /// of the viewport when possible.
    ///
    /// Mirrors Python's `Tree.scroll_to_node()`.
    pub fn scroll_to_node(&mut self, node_index: usize) {
        let total = self.visible_count();
        if total == 0 || node_index >= total {
            return;
        }
        // Center the node in the viewport.
        let half = self.viewport_height / 2;
        self.offset = node_index.saturating_sub(half);
        let max = ScrollView::line_max_offset(total, self.viewport_height.max(1));
        self.offset = self.offset.min(max);
    }

    /// Get the label of the currently highlighted (cursor) node, if any.
    ///
    /// Mirrors Python's `Tree.cursor_node` property.
    pub fn cursor_node(&self) -> Option<String> {
        let nodes = self.visible_nodes();
        let line = self.selected_line_in(&nodes);
        nodes.get(line).map(|n| n.label.clone())
    }

    fn visible_count(&self) -> usize {
        self.visible_nodes().len()
    }

    fn selectable_count(&self) -> usize {
        self.visible_nodes()
            .iter()
            .filter(|node| !node.disabled)
            .count()
    }

    fn max_offset(&self) -> usize {
        ScrollView::line_max_offset(self.visible_count(), self.viewport_height.max(1))
    }

    fn clamp_offsets(&mut self) {
        let nodes = self.visible_nodes();
        let total = nodes.len();
        if total == 0 {
            self.offset = 0;
            self.hovered_index = None;
            self.pressed_activation_index = None;
            return;
        }
        // Re-anchor the cursor to its projected line (hidden cursor nodes
        // land on their nearest visible ancestor; disabled ones move to the
        // closest selectable line).
        let mut line = self.selected_line_in(&nodes).min(total - 1);
        if nodes.get(line).is_some_and(|node| node.disabled) {
            if let Some(next) = self.closest_selectable(line, 1, &nodes) {
                line = next;
            } else if let Some(prev) = self.closest_selectable(line, -1, &nodes) {
                line = prev;
            }
        }
        self.cursor = nodes.get(line).map(|n| n.id);
        self.offset = self.offset.min(self.max_offset());
        if let Some(index) = self.hovered_index {
            if index >= total {
                self.hovered_index = None;
            }
        }
    }

    fn ensure_visible(&mut self) {
        self.clamp_offsets();
        let nodes = self.visible_nodes();
        if nodes.is_empty() {
            return;
        }
        let selected = self.selected_line_in(&nodes);
        let viewport = self.viewport_height.max(1);
        if selected < self.offset {
            self.offset = selected;
        } else if selected >= self.offset + viewport {
            self.offset = selected + 1 - viewport;
        }
        self.offset = self.offset.min(self.max_offset());
    }

    fn emit_selected(&self, ctx: &mut crate::event::WidgetCtx, nodes: &[VisibleNode]) {
        let selected = self.selected_line_in(nodes);
        if let Some(node) = nodes.get(selected) {
            if node.disabled {
                return;
            }
            ctx.post_message(TreeNodeSelected {
                index: selected,
                label: node.label.clone(),
                data: node.data.clone(),
                node_id: node.id,
            });
        }
    }

    fn emit_activated(&self, ctx: &mut crate::event::WidgetCtx, index: usize, nodes: &[VisibleNode]) {
        if let Some(node) = nodes.get(index) {
            if node.disabled {
                return;
            }
            ctx.post_message(TreeNodeActivated {
                index,
                label: node.label.clone(),
                data: node.data.clone(),
                node_id: node.id,
            });
        }
    }

    fn emit_highlighted(&self, ctx: &mut crate::event::WidgetCtx, nodes: &[VisibleNode]) {
        let selected = self.selected_line_in(nodes);
        if let Some(node) = nodes.get(selected) {
            ctx.post_message(TreeNodeHighlighted {
                index: selected,
                label: node.label.clone(),
                node_id: node.id,
            });
        }
    }

    fn emit_toggled(
        &self,
        ctx: &mut crate::event::WidgetCtx,
        index: usize,
        node_id: TreeNodeId,
        label: String,
        expanded: bool,
    ) {
        ctx.post_message(TreeNodeToggled {
            index,
            label: label.clone(),
            expanded,
            node_id,
        });
        if expanded {
            ctx.post_message(TreeNodeExpanded { index, label, node_id });
        } else {
            ctx.post_message(TreeNodeCollapsed { index, label, node_id });
        }
    }

    fn select_index(&mut self, index: usize, ctx: &mut crate::event::WidgetCtx) {
        let nodes = self.visible_nodes();
        let total = nodes.len();
        if total == 0 {
            return;
        }
        let selected = self.selected_line_in(&nodes);
        let next = self
            .closest_selectable(index, 1, &nodes)
            .or_else(|| self.closest_selectable(index, -1, &nodes))
            .unwrap_or(selected.min(total - 1));
        if next != selected {
            self.cursor = nodes.get(next).map(|n| n.id);
            self.ensure_visible();
            self.emit_selected(ctx, &nodes);
            self.emit_highlighted(ctx, &nodes);
            ctx.request_repaint();
        }
    }

    fn move_selection(&mut self, delta: isize, ctx: &mut crate::event::WidgetCtx) {
        let nodes = self.visible_nodes();
        let total = nodes.len();
        if total == 0 || self.selectable_count() == 0 {
            return;
        }
        let current = self.selected_line_in(&nodes) as isize;
        let max = (total - 1) as isize;
        let mut next = (current + delta).clamp(0, max) as usize;
        let step = if delta >= 0 { 1 } else { -1 };
        while next < total && nodes[next].disabled {
            let probe = next as isize + step;
            if probe < 0 || probe > max {
                return;
            }
            next = probe as usize;
        }
        self.select_index(next, ctx);
    }

    fn page_step(&self) -> usize {
        self.viewport_height.saturating_sub(1).max(1)
    }

    fn toggle_selected(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let nodes = self.visible_nodes();
        let selected = self.selected_line_in(&nodes);
        self.toggle_line(selected, nodes, ctx);
    }

    fn toggle_index(&mut self, index: usize, ctx: &mut crate::event::WidgetCtx) {
        let nodes = self.visible_nodes();
        self.toggle_line(index, nodes, ctx);
    }

    fn toggle_line(
        &mut self,
        index: usize,
        nodes: Vec<VisibleNode>,
        ctx: &mut crate::event::WidgetCtx,
    ) {
        let Some(info) = nodes.get(index).cloned() else {
            return;
        };
        if info.disabled || !info.expandable {
            return;
        }
        let mut expanded = info.expanded;
        if let Some(node) = self.nodes.get_mut(info.id) {
            node.expanded = !node.expanded;
            expanded = node.expanded;
        }
        self.ensure_visible();
        self.emit_toggled(ctx, index, info.id, info.label, expanded);
        ctx.request_repaint();
    }

    fn collapse_or_parent(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let nodes = self.visible_nodes();
        let Some(info) = nodes.get(self.selected_line_in(&nodes)).cloned() else {
            return;
        };
        if info.disabled {
            return;
        }
        if info.expandable && info.expanded {
            self.toggle_selected(ctx);
            return;
        }
        if info.path.len() <= 1 {
            return;
        }
        let parent_path = &info.path[..info.path.len() - 1];
        if let Some(parent_index) = nodes
            .iter()
            .position(|node| node.path.as_slice() == parent_path)
        {
            self.select_index(parent_index, ctx);
        }
    }

    fn expand_or_child(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let nodes = self.visible_nodes();
        let selected = self.selected_line_in(&nodes);
        let Some(info) = nodes.get(selected).cloned() else {
            return;
        };
        if info.disabled || !info.expandable {
            return;
        }
        if !info.expanded {
            self.toggle_selected(ctx);
            return;
        }
        let current_depth = info.depth;
        if let Some(child_index) =
            (selected + 1..nodes.len()).find(|idx| nodes[*idx].depth == current_depth + 1)
        {
            self.select_index(child_index, ctx);
        }
    }

    // ── Shift-key navigation (QW-22) ───────────────────────────────────

    /// Move cursor to the previous sibling; if none, move to parent.
    fn cursor_previous_sibling(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let nodes = self.visible_nodes();
        let selected = self.selected_line_in(&nodes);
        let Some(info) = nodes.get(selected) else {
            return;
        };
        let target_depth = info.depth;
        let parent_path: Vec<usize> = if info.path.len() > 1 {
            info.path[..info.path.len() - 1].to_vec()
        } else {
            Vec::new()
        };

        for i in (0..selected).rev() {
            let n = &nodes[i];
            // Same parent and depth → sibling
            if n.depth == target_depth
                && n.path.len() == info.path.len()
                && (parent_path.is_empty() || n.path[..n.path.len() - 1] == parent_path)
                && !n.disabled
            {
                self.select_index(i, ctx);
                return;
            }
            // Reached a shallower node → that's the parent
            if n.depth < target_depth {
                if !n.disabled {
                    self.select_index(i, ctx);
                }
                return;
            }
        }
    }

    /// Move cursor to the next sibling.
    ///
    /// Python Textual semantics: if no next sibling exists at the current level,
    /// climb to the parent and try *its* next sibling instead.
    fn cursor_next_sibling(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let nodes = self.visible_nodes();
        let selected = self.selected_line_in(&nodes);
        let Some(info) = nodes.get(selected) else {
            return;
        };
        let target_depth = info.depth;
        let parent_path: Vec<usize> = if info.path.len() > 1 {
            info.path[..info.path.len() - 1].to_vec()
        } else {
            Vec::new()
        };

        for (i, n) in nodes.iter().enumerate().skip(selected + 1) {
            if n.depth == target_depth
                && n.path.len() == info.path.len()
                && (parent_path.is_empty() || n.path[..n.path.len() - 1] == parent_path)
                && !n.disabled
            {
                self.select_index(i, ctx);
                return;
            }
            // Past this subtree — no more siblings at current level.
            if n.depth < target_depth {
                break;
            }
        }

        // No next sibling found — climb to parent's next sibling (Python semantics).
        if info.path.len() > 1 {
            if let Some(parent_idx) = nodes.iter().position(|n| n.path.as_slice() == parent_path) {
                // Find parent's next sibling.
                let parent_depth = nodes[parent_idx].depth;
                let parent_parent_path: Vec<usize> = if parent_path.len() > 1 {
                    parent_path[..parent_path.len() - 1].to_vec()
                } else {
                    Vec::new()
                };
                for (i, n) in nodes.iter().enumerate().skip(parent_idx + 1) {
                    if n.depth == parent_depth
                        && n.path.len() == parent_path.len()
                        && (parent_parent_path.is_empty()
                            || n.path[..n.path.len() - 1] == parent_parent_path)
                        && !n.disabled
                    {
                        self.select_index(i, ctx);
                        return;
                    }
                    if n.depth < parent_depth {
                        break;
                    }
                }
            }
        }
    }

    /// Move cursor directly to the parent node.
    fn cursor_parent(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let nodes = self.visible_nodes();
        let Some(info) = nodes.get(self.selected_line_in(&nodes)) else {
            return;
        };
        if info.path.len() <= 1 {
            return;
        }
        let parent_path = &info.path[..info.path.len() - 1];
        if let Some(parent_idx) = nodes.iter().position(|n| n.path.as_slice() == parent_path) {
            self.select_index(parent_idx, ctx);
        }
    }

    /// Toggle expand/collapse on all siblings of the selected node (Python semantics).
    ///
    /// Python Textual's `action_toggle_expand_all` operates on the *siblings* at the
    /// cursor's level: if all expandable siblings are collapsed, expand them all;
    /// otherwise collapse them all.  Each sibling's full subtree is also toggled.
    fn toggle_expand_all_selected(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let nodes = self.visible_nodes();
        let selected = self.selected_line_in(&nodes);
        let Some(info) = nodes.get(selected).cloned() else {
            return;
        };
        // Need a parent to determine siblings (root-level excluded per Python).
        let Some(parent) = self.nodes.get(info.id).and_then(|n| n.parent) else {
            return;
        };
        let siblings: Vec<TreeNodeId> = self.nodes[parent].children.clone();

        // Check if all expandable siblings are collapsed.
        let all_collapsed = siblings.iter().all(|&child| {
            let node = &self.nodes[child];
            !node.is_expandable() || !node.expanded
        });
        let new_state = all_collapsed; // expand all if all collapsed, else collapse all

        for child in siblings {
            self.set_all_expanded(child, new_state);
        }

        self.ensure_visible();
        self.emit_toggled(ctx, selected, info.id, info.label, new_state);
        ctx.request_repaint();
    }

    /// Set `expanded` on `id` and every expandable descendant.
    fn set_all_expanded(&mut self, id: TreeNodeId, value: bool) {
        let Some(node) = self.nodes.get_mut(id) else {
            return;
        };
        if node.is_expandable() {
            node.expanded = value;
        }
        let children = node.children.clone();
        for child in children {
            self.set_all_expanded(child, value);
        }
    }

    fn scroll_offset(&mut self, delta_rows: isize, ctx: &mut crate::event::WidgetCtx) {
        let before = self.offset;
        self.offset = ScrollView::line_scroll_by(
            self.offset,
            delta_rows as i32,
            self.visible_count(),
            self.viewport_height.max(1),
        );
        if self.offset != before {
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn closest_selectable(
        &self,
        index: usize,
        direction: isize,
        nodes: &[VisibleNode],
    ) -> Option<usize> {
        if nodes.is_empty() {
            return None;
        }
        let max = nodes.len().saturating_sub(1) as isize;
        let mut idx = (index as isize).clamp(0, max) as usize;
        if !nodes[idx].disabled {
            return Some(idx);
        }
        let step = if direction >= 0 { 1 } else { -1 };
        loop {
            let next = idx as isize + step;
            if next < 0 || next > max {
                return None;
            }
            idx = next as usize;
            if !nodes[idx].disabled {
                return Some(idx);
            }
        }
    }
}

impl ReactiveWidget for Tree {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            if change.field_name == "show_root" {
                if let (Some(old), Some(new)) = (
                    change.old_value.downcast_ref::<bool>(),
                    change.new_value.downcast_ref::<bool>(),
                ) {
                    self.watch_show_root(old, new, ctx);
                }
            }
        }
    }
}

impl crate::widgets::Focus for Tree {
    fn focusable(&self) -> bool {
        true
    }

    fn action_namespace(&self) -> &str {
        "tree"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("up", "cursor_up", "Move cursor up").hidden(),
            BindingDecl::new("down", "cursor_down", "Move cursor down").hidden(),
            BindingDecl::new("pageup", "scroll_up", "Page up").hidden(),
            BindingDecl::new("pagedown", "scroll_down", "Page down").hidden(),
            BindingDecl::new("home", "scroll_home", "Move to first node").hidden(),
            BindingDecl::new("end", "scroll_end", "Move to last node").hidden(),
            BindingDecl::new("left", "collapse_or_parent", "Collapse or move to parent").hidden(),
            BindingDecl::new("right", "expand_or_child", "Expand or move to child").hidden(),
            BindingDecl::new("enter", "select_cursor", "Activate node").hidden(),
            BindingDecl::new("space", "toggle_node", "Toggle node").hidden(),
            BindingDecl::new("shift+up", "cursor_previous_sibling", "Previous sibling").hidden(),
            BindingDecl::new("shift+down", "cursor_next_sibling", "Next sibling").hidden(),
            BindingDecl::new("shift+left", "cursor_parent", "Go to parent").hidden(),
            BindingDecl::new("shift+right", "toggle_expand_all", "Toggle expand all").hidden(),
            BindingDecl::new("shift+space", "toggle_expand_all", "Toggle expand all").hidden(),
        ]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut crate::event::WidgetCtx) -> bool {
        match action.name.as_str() {
            "cursor_up" => {
                self.move_selection(-1, ctx);
                ctx.set_handled();
                true
            }
            "cursor_down" => {
                self.move_selection(1, ctx);
                ctx.set_handled();
                true
            }
            "scroll_up" => {
                self.move_selection(-(self.page_step() as isize), ctx);
                ctx.set_handled();
                true
            }
            "scroll_down" => {
                self.move_selection(self.page_step() as isize, ctx);
                ctx.set_handled();
                true
            }
            "scroll_home" => {
                self.select_index(0, ctx);
                ctx.set_handled();
                true
            }
            "scroll_end" => {
                let total = self.visible_count();
                if total > 0 {
                    self.select_index(total - 1, ctx);
                }
                ctx.set_handled();
                true
            }
            "collapse_or_parent" => {
                self.collapse_or_parent(ctx);
                ctx.set_handled();
                true
            }
            "expand_or_child" => {
                self.expand_or_child(ctx);
                ctx.set_handled();
                true
            }
            "select_cursor" => {
                let nodes = self.visible_nodes();
                let selected = self.selected_line_in(&nodes);
                self.emit_activated(ctx, selected, &nodes);
                ctx.set_handled();
                true
            }
            "toggle_node" => {
                self.toggle_selected(ctx);
                ctx.set_handled();
                true
            }
            "cursor_previous_sibling" => {
                self.cursor_previous_sibling(ctx);
                ctx.set_handled();
                true
            }
            "cursor_next_sibling" => {
                self.cursor_next_sibling(ctx);
                ctx.set_handled();
                true
            }
            "cursor_parent" => {
                self.cursor_parent(ctx);
                ctx.set_handled();
                true
            }
            "toggle_expand_all" => {
                self.toggle_expand_all_selected(ctx);
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }
}

impl crate::widgets::Interactive for Tree {
    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        if !new.hovered {
            self.hovered_index = None;
        }
    }

    fn on_layout(&mut self, _width: u16, height: u16) {
        self.viewport_height = usize::from(height).max(1);
        self.ensure_visible();
    }

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                let nodes = self.visible_nodes();
                let index = self.offset.saturating_add(mouse.y as usize);
                if let Some(node) = nodes.get(index) {
                    if node.disabled {
                        return;
                    }
                    let twist_col = Self::twisty_hit_max_x(
                        node,
                        self.show_guides,
                        self.guide_depth,
                        self.hide_twisty,
                    );
                    if node.expandable && (mouse.x as usize) <= twist_col {
                        self.pressed_activation_index = None;
                        self.toggle_index(index, ctx);
                    } else {
                        self.select_index(index, ctx);
                        self.pressed_activation_index = Some(index);
                        if self.hovered_index != Some(index) {
                            self.hovered_index = Some(index);
                            ctx.request_repaint();
                        }
                    }
                    ctx.set_handled();
                }
            }
            Event::MouseUp(mouse) if mouse.target.is_some_and(|t| t == self.node_id()) => {
                let index = self.offset.saturating_add(mouse.y as usize);
                let nodes = self.visible_nodes();
                if self.pressed_activation_index == Some(index) {
                    self.emit_activated(ctx, index, &nodes);
                    ctx.set_handled();
                }
                self.pressed_activation_index = None;
            }
            Event::Action(action) if self.node_state().focused => match action {
                Action::ScrollUp => {
                    self.move_selection(-1, ctx);
                    ctx.set_handled();
                }
                Action::ScrollDown => {
                    self.move_selection(1, ctx);
                    ctx.set_handled();
                }
                Action::ScrollPageUp => {
                    self.move_selection(-(self.page_step() as isize), ctx);
                    ctx.set_handled();
                }
                Action::ScrollPageDown => {
                    self.move_selection(self.page_step() as isize, ctx);
                    ctx.set_handled();
                }
                Action::Toggle => {
                    self.toggle_selected(ctx);
                    ctx.set_handled();
                }
                _ => {}
            },
            Event::Key(key) if self.node_state().focused => {
                let shift = key.modifiers.contains(KeyModifiers::SHIFT);
                let shift_handled = if shift {
                    match key.code {
                        KeyCode::Up => {
                            self.cursor_previous_sibling(ctx);
                            true
                        }
                        KeyCode::Down => {
                            self.cursor_next_sibling(ctx);
                            true
                        }
                        KeyCode::Left => {
                            self.cursor_parent(ctx);
                            true
                        }
                        KeyCode::Right => {
                            self.toggle_expand_all_selected(ctx);
                            true
                        }
                        KeyCode::Char(' ') => {
                            self.toggle_expand_all_selected(ctx);
                            true
                        }
                        _ => false,
                    }
                } else {
                    false
                };
                if shift_handled {
                    ctx.set_handled();
                } else {
                    match key.code {
                        KeyCode::Up => {
                            self.move_selection(-1, ctx);
                            ctx.set_handled();
                        }
                        KeyCode::Down => {
                            self.move_selection(1, ctx);
                            ctx.set_handled();
                        }
                        KeyCode::PageUp => {
                            self.move_selection(-(self.page_step() as isize), ctx);
                            ctx.set_handled();
                        }
                        KeyCode::PageDown => {
                            self.move_selection(self.page_step() as isize, ctx);
                            ctx.set_handled();
                        }
                        KeyCode::Home => {
                            self.select_index(0, ctx);
                            ctx.set_handled();
                        }
                        KeyCode::End => {
                            let total = self.visible_count();
                            if total > 0 {
                                self.select_index(total - 1, ctx);
                            }
                            ctx.set_handled();
                        }
                        KeyCode::Left => {
                            self.collapse_or_parent(ctx);
                            ctx.set_handled();
                        }
                        KeyCode::Right => {
                            self.expand_or_child(ctx);
                            ctx.set_handled();
                        }
                        KeyCode::Enter => {
                            let nodes = self.visible_nodes();
                            let selected = self.selected_line_in(&nodes);
                            self.emit_activated(ctx, selected, &nodes);
                            ctx.set_handled();
                        }
                        KeyCode::Char(' ') => {
                            self.toggle_selected(ctx);
                            ctx.set_handled();
                        }
                        _ => {}
                    }
                }
            }
            Event::AppFocus(false) => {
                self.pressed_activation_index = None;
                if self.hovered_index.is_some() {
                    self.hovered_index = None;
                    ctx.request_repaint();
                }
            }
            _ => {}
        }
    }

    fn on_mouse_move(&mut self, _x: u16, y: u16) -> bool {
        let index = self.offset.saturating_add(y as usize);
        let nodes = self.visible_nodes();
        let total = nodes.len();
        let hovered = if index < total && !nodes[index].disabled {
            Some(index)
        } else {
            None
        };
        if hovered != self.hovered_index {
            self.hovered_index = hovered;
            return true;
        }
        false
    }

    fn on_unmount(&mut self) {
        self.hovered_index = None;
        self.pressed_activation_index = None;
    }
}

impl crate::widgets::Layout for Tree {
    fn layout_height(&self) -> Option<usize> {
        Some(self.visible_count().max(1))
    }

    fn content_width(&self) -> Option<usize> {
        let content_width = self.max_line_width();
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        Some(content_width.saturating_add(chrome_lr).max(1))
    }
}

impl crate::widgets::Scrollable for Tree {
    fn on_mouse_scroll(&mut self, _delta_x: i32, delta_y: i32, ctx: &mut crate::event::WidgetCtx) {
        if delta_y == 0 {
            return;
        }
        self.scroll_offset(
            delta_y.saturating_mul(self.scroll_step as i32) as isize,
            ctx,
        );
    }
}


// ── Free helpers for recursive tree operations ──────────────────────────

#[cfg(test)]
mod tests {
    use super::{Tree, TreeNode, TreeNodeId, VisibleNode};
    use crate::event::{Event, EventCtx, MouseDownEvent, MouseUpEvent};
    use crate::keys::KeyEventData;
    use crate::message::*;
    use crate::node_id::NodeId;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
    use crate::widgets::{NodeState, Widget};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rich_rs::Console;
    use slotmap::SlotMap;

    fn make_node_id() -> NodeId {
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    fn focused_state() -> NodeState {
        NodeState {
            focused: true,
            ..Default::default()
        }
    }

    #[test]
    fn highlighted_node_uses_highlight_class_not_hover() {
        let tree = Tree::new(vec![TreeNode::new("Root")]);
        let node = VisibleNode {
            id: TreeNodeId::default(),
            path: vec![0],
            depth: 0,
            label: "Root".to_string(),
            expanded: true,
            disabled: false,
            expandable: false,
            component_classes: Vec::new(),
            data: None,
            is_last_at_depth: vec![true],
        };
        let classes = Tree::node_classes(&node, true, true, true);
        assert!(classes.iter().any(|class| class == "-highlighted"));
        assert!(classes.iter().any(|class| class == "-focus"));
        assert!(!classes.iter().any(|class| class == "-hover"));
        assert!(classes.iter().any(|class| class == "-leaf"));
        let _ = tree;
    }

    #[test]
    fn node_classes_include_component_classes() {
        let node = VisibleNode {
            id: TreeNodeId::default(),
            path: vec![0],
            depth: 0,
            label: "entry.txt".to_string(),
            expanded: false,
            disabled: false,
            expandable: false,
            component_classes: vec![
                "directory-tree--file".to_string(),
                "directory-tree--extension".to_string(),
            ],
            data: None,
            is_last_at_depth: vec![true],
        };
        let classes = Tree::node_classes(&node, false, false, false);
        assert!(classes.iter().any(|class| class == "directory-tree--file"));
        assert!(
            classes
                .iter()
                .any(|class| class == "directory-tree--extension")
        );
    }

    #[test]
    fn enter_activates_selected_node_without_toggling() {
        let mut tree = Tree::new(vec![
            TreeNode::new("Root")
                .expanded(false)
                .with_child(TreeNode::new("Child")),
        ]);
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        tree.on_layout(24, 4);

        let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            tree.on_event(&Event::Key(key), &mut __w);
        }

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is::<TreeNodeActivated>());
        assert_eq!(
            messages[0]
                .downcast_ref::<TreeNodeActivated>()
                .unwrap()
                .index,
            0
        );
        assert_eq!(
            messages[0]
                .downcast_ref::<TreeNodeActivated>()
                .unwrap()
                .label,
            "Root"
        );

        // Enter should not expand/collapse.
        let visible_labels: Vec<String> =
            tree.visible_nodes().into_iter().map(|n| n.label).collect();
        assert_eq!(visible_labels, vec!["Root".to_string()]);
    }

    #[test]
    fn mouse_twisty_click_toggles_without_emitting_activation() {
        let mut tree = Tree::new(vec![
            TreeNode::new("Root")
                .expanded(true)
                .with_child(TreeNode::new("Child")),
        ]);
        tree.on_layout(24, 4);
        let id = NodeId::default();

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            tree.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut __w);
        }
        let messages = ctx.take_messages();
        // emit_toggled now posts TreeNodeToggled + TreeNodeCollapsed (2 messages)
        assert_eq!(messages.len(), 2);
        assert!(messages[0].is::<TreeNodeToggled>());
        assert_eq!(
            messages[0].downcast_ref::<TreeNodeToggled>().unwrap().index,
            0
        );
        assert!(
            !messages[0]
                .downcast_ref::<TreeNodeToggled>()
                .unwrap()
                .expanded
        );
        assert!(messages[1].is::<TreeNodeCollapsed>());
        assert_eq!(
            messages[1]
                .downcast_ref::<TreeNodeCollapsed>()
                .unwrap()
                .index,
            0
        );
    }

    #[test]
    fn mouse_row_click_activates_on_mouse_up() {
        let mut tree = Tree::new(vec![TreeNode::new("Root"), TreeNode::new("Second")]);
        tree.on_layout(24, 4);
        let id = NodeId::default();

        let mut down_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut down_ctx);
            tree.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 1,
                screen_y: 1,
                x: 1,
                y: 1,
            }),
            &mut __w);
        }
        assert!(down_ctx.handled());
        // select_index emits TreeNodeSelected + TreeNodeHighlighted (2 messages)
        assert_eq!(down_ctx.take_messages().len(), 2);

        let mut up_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut up_ctx);
            tree.on_event(
            &Event::MouseUp(MouseUpEvent {
                target: Some(id),
                screen_x: 1,
                screen_y: 1,
                x: 1,
                y: 1,
            }),
            &mut __w);
        }
        let messages = up_ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is::<TreeNodeActivated>());
        assert_eq!(
            messages[0]
                .downcast_ref::<TreeNodeActivated>()
                .unwrap()
                .index,
            1
        );
        assert_eq!(
            messages[0]
                .downcast_ref::<TreeNodeActivated>()
                .unwrap()
                .label,
            "Second"
        );
    }

    #[test]
    fn app_focus_loss_clears_hover_state() {
        let mut tree = Tree::new(vec![TreeNode::new("Root")]);
        assert!(tree.on_mouse_move(0, 0));
        assert_eq!(tree.hovered_index, Some(0));

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            tree.on_event(&Event::AppFocus(false), &mut __w);
        }

        assert_eq!(tree.hovered_index, None);
        assert!(ctx.repaint_requested());
    }

    #[test]
    fn content_width_accounts_for_grapheme_cell_width() {
        let tree = Tree::new(vec![
            TreeNode::new("e\u{0301}"),
            TreeNode::new("👩‍🚀"),
            TreeNode::new("中中"),
        ]);

        let expected = ["e\u{0301}", "👩‍🚀", "中中"]
            .iter()
            .map(|label| rich_rs::cell_len(label))
            .max()
            .unwrap_or(1);
        assert_eq!(tree.content_width(), Some(expected.max(1)));
    }

    #[test]
    fn render_clamps_grapheme_rows_to_viewport_width() {
        let tree = Tree::new(vec![TreeNode::new("👩‍🚀中中e\u{0301}")]);
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (8, 1);
        options.max_width = 8;
        options.max_height = 1;

        let rendered = Widget::render(&tree, &console, &options);
        assert_eq!(rendered.cell_len(), 8);
    }

    #[test]
    fn bindings_are_declared() {
        let tree = Tree::new(vec![TreeNode::new("Root")]);
        let bindings = tree.bindings();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().any(|b| b.action == "cursor_up"));
        assert!(bindings.iter().any(|b| b.action == "cursor_down"));
        assert!(bindings.iter().any(|b| b.action == "toggle_node"));
        assert!(bindings.iter().any(|b| b.action == "select_cursor"));
    }

    #[test]
    fn execute_action_handles_cursor_down() {
        use crate::action::ParsedAction;
        let mut tree = Tree::new(vec![TreeNode::new("Child A"), TreeNode::new("Child B")]);
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "cursor_down".to_string(),
            arguments: vec![],
        };
        assert!({ let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); tree.execute_action(&action, &mut __w) });
    }

    #[test]
    fn add_child_returns_mutable_ref_to_added_node() {
        let mut root = TreeNode::new("Root");
        let child = root.add_child(TreeNode::new("Child"));
        assert_eq!(child.label, "Child");
        assert!(child.children.is_empty());
        assert_eq!(root.children.len(), 1);
    }

    #[test]
    fn add_child_supports_nested_chaining() {
        let mut root = TreeNode::new("Root");
        let child = root.add_child(TreeNode::new("A").allow_expand(true));
        child.add_child(TreeNode::new("A1"));
        child.add_child(TreeNode::new("A2"));

        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].label, "A");
        assert_eq!(root.children[0].children.len(), 2);
        assert_eq!(root.children[0].children[0].label, "A1");
        assert_eq!(root.children[0].children[1].label, "A2");
    }

    #[test]
    fn add_leaf_creates_non_expandable_child() {
        let mut root = TreeNode::new("Root");
        let leaf = root.add_leaf("Leaf");
        assert_eq!(leaf.label, "Leaf");
        assert!(!leaf.allow_expand);
        assert!(leaf.children.is_empty());
        assert_eq!(root.children.len(), 1);
    }

    #[test]
    fn render_produces_multiple_segments_per_row() {
        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let tree = Tree::new(vec![
            TreeNode::new("Root")
                .expanded(true)
                .with_child(TreeNode::new("Child A"))
                .with_child(TreeNode::new("Child B")),
        ]);

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (30, 3);
        options.max_width = 30;
        options.max_height = 3;

        let rendered = Widget::render(&tree, &console, &options);
        // Filter to non-empty, non-newline segments.
        let content_segs: Vec<_> = rendered
            .iter()
            .filter(|s| !s.text.is_empty() && s.text != "\n")
            .collect();
        // With per-segment styling, a row with guides should produce at least 3 segments
        // (marker + guide + twisty+label, or more). Before this change, each row was 1 segment.
        // Row 1 (Child A at depth 1) has: marker("  ") + guide("├── ") + twisty("  ") + label.
        assert!(
            content_segs.len() >= 6,
            "expected at least 6 non-empty segments for 3 rows with per-segment styling, got {}",
            content_segs.len()
        );
    }

    #[test]
    fn hovered_row_background_extends_to_row_end() {
        use crate::render::FrameBuffer;
        use crate::widgets::WidgetRenderable;

        let _guard = crate::css::set_style_context(crate::css::default_widget_stylesheet());

        let make_tree = || {
            Tree::new(vec![
                TreeNode::new("Root")
                    .expanded(true)
                    .allow_expand(true)
                    .with_child(TreeNode::new("Child")),
            ])
        };

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (24, 2);
        options.max_width = 24;
        options.max_height = 2;

        let baseline = make_tree();
        let baseline_buf = FrameBuffer::from_renderable(
            &console,
            &options,
            &WidgetRenderable::new(&baseline),
            None,
        );
        let baseline_bg = baseline_buf
            .get(23, 1)
            .style
            .as_ref()
            .and_then(|style| style.bgcolor);

        let mut hovered = make_tree();
        assert!(hovered.on_mouse_move(2, 1));
        let hovered_buf = FrameBuffer::from_renderable(
            &console,
            &options,
            &WidgetRenderable::new(&hovered),
            None,
        );
        let hovered_bg = hovered_buf
            .get(23, 1)
            .style
            .as_ref()
            .and_then(|style| style.bgcolor);

        assert_ne!(
            hovered_bg, baseline_bg,
            "hovered row should paint hover-line background through trailing cells"
        );
    }

    #[test]
    fn content_width_counts_guides_and_prefixed_labels_for_hidden_root() {
        let mut tree = Tree::new(vec![
            TreeNode::new("Contents")
                .expanded(true)
                .allow_expand(true)
                .with_child(
                    TreeNode::new("Ⅰ Markdown Viewer")
                        .expanded(true)
                        .allow_expand(true)
                        .with_child(TreeNode::new("Ⅱ Litany Against Fear")),
                ),
        ]);
        tree.set_show_root_plain(false);

        let expected = rich_rs::cell_len("└── Ⅱ Litany Against Fear");
        assert_eq!(tree.content_width(), Some(expected.max(1)));
    }

    #[test]
    fn set_selected_always_fires_even_when_unchanged() {
        use crate::reactive::ReactiveCtx;
        let mut tree = Tree::new(vec![TreeNode::new("A"), TreeNode::new("B")]);
        let mut ctx = ReactiveCtx::new(NodeId::default());
        tree.set_selected(0, &mut ctx);
        assert!(
            ctx.has_changes(),
            "set_selected should record a change even for same value"
        );
        assert!(ctx.changes()[0].flags.always_update);
    }

    // ── Phase 1 tests: root access, clear, reset, set_label, expand ──

    #[test]
    fn tree_root_label_mutates_via_id_api() {
        let mut tree = Tree::new(vec![TreeNode::new("Root")]);
        let root_id = tree.root_id().expect("should have a root");
        assert_eq!(tree.label_of(root_id), Some("Root"));
        tree.set_label(root_id, "NewRoot").expect("root id resolves");
        assert_eq!(tree.root().unwrap().label(), "NewRoot");
    }

    #[test]
    fn tree_clear_preserves_root() {
        let mut tree = Tree::new(vec![
            TreeNode::new("Root")
                .with_child(TreeNode::new("A"))
                .with_child(TreeNode::new("B")),
        ]);
        assert_eq!(tree.root().unwrap().child_count(), 2);
        tree.clear();
        // Root preserved, children cleared.
        let root = tree.root().expect("root should survive clear()");
        assert_eq!(root.label(), "Root");
        assert_eq!(root.child_count(), 0);
    }

    #[test]
    fn tree_reset_replaces_root() {
        let mut tree = Tree::new(vec![
            TreeNode::new("Old").with_child(TreeNode::new("Child")),
        ]);
        tree.reset("Fresh");
        let root = tree.root().expect("reset should create a root");
        assert_eq!(root.label(), "Fresh");
        assert_eq!(root.child_count(), 0);
    }

    #[test]
    fn tree_node_set_label_mutates() {
        let mut node = TreeNode::new("original");
        assert_eq!(node.label(), "original");
        node.set_label("changed");
        assert_eq!(node.label(), "changed");
    }

    #[test]
    fn tree_node_expand_collapse() {
        let mut node = TreeNode::new("N");
        // Nodes start collapsed by default; expanded must be set explicitly.
        node = node.with_child(TreeNode::new("C"));
        assert!(!node.is_expanded());
        node.expand();
        assert!(node.is_expanded());
        node.collapse();
        assert!(!node.is_expanded());
    }

    #[test]
    fn tree_label_rich_markup_renders_bold() {
        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let tree = Tree::new(vec![TreeNode::new("[b]key[/b]=value")]);

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (40, 1);
        options.max_width = 40;
        options.max_height = 1;

        let rendered = Widget::render(&tree, &console, &options);
        let text: String = rendered.iter().map(|s| s.text.clone()).collect();
        // Markup tags should be parsed, not rendered literally.
        assert!(
            !text.contains("[b]"),
            "literal [b] tag should not appear: {text:?}"
        );
        assert!(
            text.contains("key"),
            "label text 'key' should appear: {text:?}"
        );
        assert!(
            text.contains("value"),
            "label text 'value' should appear: {text:?}"
        );
    }

    #[test]
    fn tree_label_plain_brackets_render_literally() {
        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let tree = Tree::new(vec![TreeNode::new("[not-markup] value")]);

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (40, 1);
        options.max_width = 40;
        options.max_height = 1;

        let rendered = Widget::render(&tree, &console, &options);
        let text: String = rendered.iter().map(|s| s.text.clone()).collect();
        assert!(
            text.contains("[not-markup]"),
            "plain bracketed labels should render literally: {text:?}"
        );
    }

    #[test]
    fn tree_node_default_not_expanded() {
        let node = TreeNode::new("label");
        assert!(!node.is_expanded(), "new TreeNode must start collapsed");
    }

    #[test]
    fn tree_navigation_bindings_all_hidden() {
        let tree = Tree::new(vec![TreeNode::new("r")]);
        for binding in tree.bindings() {
            assert!(
                !binding.show,
                "Tree binding {:?}/{:?} must be hidden (show=false)",
                binding.key, binding.description
            );
        }
    }

    // ── Key-identity message/cursor ports (Python tests/tree/test_tree_cursor.py
    //    and test_tree_messages.py, adapted to the Rust message model) ──────────

    fn cursor_fixture() -> Tree {
        // Python TreeApp fixture: Tree("tree") with an expanded root and one
        // "leaf" child.
        let mut tree = Tree::new(vec![
            TreeNode::new("tree")
                .expanded(true)
                .with_child(TreeNode::new("leaf")),
        ]);
        tree.on_layout(24, 4);
        tree
    }

    #[test]
    fn select_node_by_id_posts_selected_and_highlighted_with_node_id() {
        let mut tree = cursor_fixture();
        let leaf = tree.root().unwrap().child_ids()[0];
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            tree.select_node_by_id(leaf, &mut __w).expect("live id");
        }
        let messages = ctx.take_messages();
        let selected = messages
            .iter()
            .find_map(|m| m.downcast_ref::<TreeNodeSelected>())
            .expect("TreeNodeSelected posted");
        assert_eq!(selected.node_id, leaf);
        assert_eq!(selected.index, 1);
        let highlighted = messages
            .iter()
            .find_map(|m| m.downcast_ref::<TreeNodeHighlighted>())
            .expect("TreeNodeHighlighted posted");
        assert_eq!(highlighted.node_id, leaf);
        // The message id round-trips into a lookup (the live-borrow-safe
        // handler idiom: capture the Copy key, defer the mutation).
        assert_eq!(tree.get_node_by_id(selected.node_id).unwrap().label(), "leaf");
    }

    #[test]
    fn select_node_by_id_with_stale_id_errors() {
        let mut tree = cursor_fixture();
        let leaf = tree.root().unwrap().child_ids()[0];
        tree.remove(leaf).expect("leaf removable");
        let mut ctx = EventCtx::default();
        let result = {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            tree.select_node_by_id(leaf, &mut __w)
        };
        assert_eq!(result, Err(super::TreeError::UnknownNode(leaf)));
        assert!(ctx.take_messages().is_empty());
    }

    #[test]
    fn move_cursor_moves_highlight_without_selecting() {
        // Python test_move_cursor: moving the cursor posts no Selected message.
        let mut tree = cursor_fixture();
        let leaf = tree.root().unwrap().child_ids()[0];
        tree.move_cursor(Some(leaf));
        assert_eq!(tree.cursor_node_id(), Some(leaf));
        assert_eq!(tree.selected(), 1);
        // Python test_move_cursor_reset: move_cursor(None) resets to the top.
        tree.move_cursor(None);
        assert_eq!(tree.cursor_node_id(), tree.root_id());
        assert_eq!(tree.selected(), 0);
    }

    #[test]
    fn toggle_posts_toggled_and_expanded_with_node_id() {
        let mut tree = Tree::new(vec![
            TreeNode::new("Root").with_child(TreeNode::new("Child")),
        ]);
        tree.on_layout(24, 4);
        let root_id = tree.root_id().expect("root");
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            tree.toggle_selected(&mut __w);
        }
        let messages = ctx.take_messages();
        let toggled = messages
            .iter()
            .find_map(|m| m.downcast_ref::<TreeNodeToggled>())
            .expect("TreeNodeToggled posted");
        assert!(toggled.expanded);
        assert_eq!(toggled.node_id, root_id);
        let expanded = messages
            .iter()
            .find_map(|m| m.downcast_ref::<TreeNodeExpanded>())
            .expect("TreeNodeExpanded posted");
        assert_eq!(expanded.node_id, root_id);

        // Collapsing posts TreeNodeCollapsed with the same id.
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            tree.toggle_selected(&mut __w);
        }
        let messages = ctx.take_messages();
        let collapsed = messages
            .iter()
            .find_map(|m| m.downcast_ref::<TreeNodeCollapsed>())
            .expect("TreeNodeCollapsed posted");
        assert_eq!(collapsed.node_id, root_id);
    }
}

impl crate::widgets::Components for Tree {
    fn component_classes(&self) -> &[&'static str] {
        &[
            "tree--cursor",
            "tree--guides",
            "tree--guides-hover",
            "tree--guides-selected",
            "tree--highlight",
            "tree--highlight-line",
            "tree--label",
        ]
    }
}
