use crossterm::event::{KeyCode, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::*;

use crate::action::ParsedAction;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

use super::{BindingDecl, NodeSeed, ScrollView, Widget, helpers::adjust_line_length_no_bg};

#[derive(Debug, Clone)]
pub struct TreeNode {
    label: String,
    expanded: bool,
    allow_expand: bool,
    disabled: bool,
    component_classes: Vec<String>,
    children: Vec<TreeNode>,
    /// Optional user data associated with this node (e.g. block_id for TOC headings).
    data: Option<String>,
}

impl TreeNode {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            expanded: false,
            allow_expand: false,
            disabled: false,
            component_classes: Vec::new(),
            children: Vec::new(),
            data: None,
        }
    }

    pub fn expanded(mut self, value: bool) -> Self {
        self.expanded = value;
        self
    }

    pub fn with_child(mut self, child: TreeNode) -> Self {
        self.children.push(child);
        self
    }

    pub fn allow_expand(mut self, value: bool) -> Self {
        self.allow_expand = value;
        self
    }

    pub fn disabled(mut self, value: bool) -> Self {
        self.disabled = value;
        self
    }

    pub fn with_component_class(mut self, class: impl Into<String>) -> Self {
        self.component_classes.push(class.into());
        self
    }

    /// Set optional user data on this node (builder pattern).
    pub fn with_data(mut self, data: impl Into<String>) -> Self {
        self.data = Some(data.into());
        self
    }

    /// Read-only access to the node's data.
    pub fn data(&self) -> Option<&str> {
        self.data.as_deref()
    }

    /// Read-only access to this node's children.
    pub fn children_slice(&self) -> &[TreeNode] {
        &self.children
    }

    /// Add a child node, returning a mutable reference to the newly added child.
    ///
    /// This enables the Python pattern of incremental tree construction:
    /// ```ignore
    /// let child = parent.add_child(TreeNode::new("child"));
    /// child.add_child(TreeNode::new("grandchild"));
    /// ```
    pub fn add_child(&mut self, child: TreeNode) -> &mut TreeNode {
        self.children.push(child);
        self.children.last_mut().expect("just pushed")
    }

    /// Add a leaf node (convenience for `add_child(TreeNode::new(label))`).
    pub fn add_leaf(&mut self, label: impl Into<String>) -> &mut TreeNode {
        self.add_child(TreeNode::new(label))
    }

    /// Mutate the node's label after construction.
    ///
    /// Mirrors Python's `node.set_label(text)`.
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    /// Read-only access to the node's label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Expand this node (make children visible).
    ///
    /// Mirrors Python's `node.expand()`.
    pub fn expand(&mut self) {
        self.expanded = true;
    }

    /// Collapse this node (hide children).
    ///
    /// Mirrors Python's `node.collapse()`.
    pub fn collapse(&mut self) {
        self.expanded = false;
    }

    /// Whether this node is currently expanded.
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Set whether this node can be expanded by the user.
    ///
    /// Mirrors Python's `node.allow_expand = value`.
    pub fn set_allow_expand(&mut self, value: bool) {
        self.allow_expand = value;
    }
}

#[derive(Debug, Clone)]
pub struct Tree {
    roots: Vec<TreeNode>,
    selected: usize,
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
    seed: NodeSeed,
}

#[derive(Debug, Clone)]
struct VisibleNode {
    path: Vec<usize>,
    depth: usize,
    label: String,
    expanded: bool,
    disabled: bool,
    expandable: bool,
    component_classes: Vec<String>,
    /// Optional user data associated with the underlying TreeNode.
    data: Option<String>,
    /// For each visual depth level, whether the ancestor at that level is the last sibling.
    /// Used for rendering tree guide lines (│, ├, └).
    is_last_at_depth: Vec<bool>,
}

impl Tree {
    pub fn new(roots: Vec<TreeNode>) -> Self {
        Self {
            roots,
            selected: 0,
            offset: 0,
            hovered_index: None,
            pressed_activation_index: None,
            viewport_height: 1,
            scroll_step: 1,
            show_root: true,
            show_guides: true,
            guide_depth: 4,
            seed: NodeSeed::default(),
        }
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    pub fn selected(&self) -> usize {
        self.selected
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
    pub fn set_selected(&mut self, index: usize, ctx: &mut ReactiveCtx) {
        let total = self.visible_count();
        if total == 0 {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        let old = self.selected;
        let new_selected = index.min(total - 1);
        self.selected = new_selected;
        self.ensure_visible();
        ctx.record_change(
            "selected",
            ReactiveFlags::reactive_always_update(),
            Box::new(old),
            Box::new(self.selected),
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

    /// Access the root node (first root) immutably.
    ///
    /// Mirrors Python's `tree.root` property. Python's Tree always has exactly
    /// one root; Rust's multi-root Vec is an implementation detail.
    pub fn root(&self) -> Option<&TreeNode> {
        self.roots.first()
    }

    /// Access the root node (first root) mutably.
    ///
    /// Mirrors Python's `tree.root` property.
    pub fn root_mut(&mut self) -> Option<&mut TreeNode> {
        self.roots.first_mut()
    }

    // ── API methods (QW-19) ──────────────────────────────────────────────

    /// Clear all children under the root node.
    ///
    /// Mirrors Python's `tree.clear()` which preserves the root node (label,
    /// expanded state) and only removes its children.
    pub fn clear(&mut self) {
        if let Some(root) = self.roots.first_mut() {
            root.children.clear();
        }
        self.selected = 0;
        self.offset = 0;
        self.hovered_index = None;
        self.pressed_activation_index = None;
    }

    /// Clear the tree and reset the root node with a new label.
    ///
    /// Mirrors Python's `tree.reset(label, data)`.
    pub fn reset(&mut self, label: impl Into<String>) {
        self.roots = vec![TreeNode::new(label)];
        self.selected = 0;
        self.offset = 0;
        self.hovered_index = None;
        self.pressed_activation_index = None;
    }

    /// Append a root node without clearing existing ones.
    ///
    /// Resets cursor/selection since the tree structure changed.
    pub fn add_root(&mut self, node: TreeNode) {
        self.roots.push(node);
        self.selected = 0;
        self.offset = 0;
    }

    /// Toggle `show_root` without reactive dispatch.
    ///
    /// For use from app-level hooks (`on_key_with_app`) where no `ReactiveCtx`
    /// is available.  Repaint must be requested separately via `ctx.request_repaint()`.
    pub fn toggle_show_root(&mut self) {
        self.show_root = !self.show_root;
        self.selected = 0;
        self.offset = 0;
    }

    /// Non-reactive setter for `show_root`.
    ///
    /// For use in construction contexts where no `ReactiveCtx` is available
    /// (e.g. building a Tree inside MarkdownTableOfContents).
    pub fn set_show_root_plain(&mut self, value: bool) {
        self.show_root = value;
    }

    /// Programmatically select a node: moves cursor and emits `TreeNodeSelected`.
    pub fn select_node(&mut self, node_index: usize, ctx: &mut EventCtx) {
        let nodes = self.visible_nodes();
        let total = nodes.len();
        if total == 0 || node_index >= total {
            return;
        }
        self.selected = node_index;
        self.ensure_visible();
        self.emit_selected(ctx, &nodes);
        self.emit_highlighted(ctx, &nodes);
    }

    /// Expand or collapse all nodes. If any expandable node is expanded, collapse all;
    /// otherwise expand all.
    pub fn toggle_all(&mut self) {
        let any_expanded = has_any_expanded(&self.roots);
        let target = !any_expanded;
        for root in &mut self.roots {
            set_all_expanded(root, target);
        }
        self.ensure_visible();
    }

    /// Enable or disable auto-expand. When enabled, all nodes start expanded.
    ///
    /// Mirrors Python's `Tree.auto_expand = True`. Expands all existing nodes
    /// and marks them so future additions also start expanded.
    pub fn set_auto_expand(&mut self, expand: bool) {
        if expand {
            for root in &mut self.roots {
                set_all_expanded(root, true);
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
        nodes.get(self.selected).map(|n| n.label.clone())
    }

    fn visible_nodes(&self) -> Vec<VisibleNode> {
        let depth_offset: usize = if self.show_root { 0 } else { 1 };

        fn walk(
            nodes: &[TreeNode],
            tree_depth: usize,
            depth_offset: usize,
            path: &mut Vec<usize>,
            is_last: &mut Vec<bool>,
            out: &mut Vec<VisibleNode>,
        ) {
            let count = nodes.len();
            for (idx, node) in nodes.iter().enumerate() {
                let last = idx == count - 1;
                path.push(idx);
                is_last.push(last);

                if tree_depth >= depth_offset {
                    let visual_depth = tree_depth - depth_offset;
                    let visual_is_last = is_last[depth_offset..].to_vec();
                    out.push(VisibleNode {
                        path: path.clone(),
                        depth: visual_depth,
                        label: node.label.clone(),
                        expanded: node.expanded,
                        disabled: node.disabled,
                        expandable: node.allow_expand || !node.children.is_empty(),
                        component_classes: node.component_classes.clone(),
                        data: node.data.clone(),
                        is_last_at_depth: visual_is_last,
                    });
                }
                if node.expanded {
                    walk(
                        &node.children,
                        tree_depth + 1,
                        depth_offset,
                        path,
                        is_last,
                        out,
                    );
                }
                path.pop();
                is_last.pop();
            }
        }

        let mut out = Vec::new();
        let mut path = Vec::new();
        let mut is_last = Vec::new();
        walk(
            &self.roots,
            0,
            depth_offset,
            &mut path,
            &mut is_last,
            &mut out,
        );
        out
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
            self.selected = 0;
            self.offset = 0;
            self.hovered_index = None;
            self.pressed_activation_index = None;
            return;
        }
        self.selected = self.selected.min(total - 1);
        if nodes.get(self.selected).is_some_and(|node| node.disabled) {
            if let Some(next) = self.closest_selectable(self.selected, 1, &nodes) {
                self.selected = next;
            } else if let Some(prev) = self.closest_selectable(self.selected, -1, &nodes) {
                self.selected = prev;
            }
        }
        self.offset = self.offset.min(self.max_offset());
        if let Some(index) = self.hovered_index {
            if index >= total {
                self.hovered_index = None;
            }
        }
    }

    fn ensure_visible(&mut self) {
        self.clamp_offsets();
        let total = self.visible_count();
        if total == 0 {
            return;
        }
        let viewport = self.viewport_height.max(1);
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + viewport {
            self.offset = self.selected + 1 - viewport;
        }
        self.offset = self.offset.min(self.max_offset());
    }

    fn node_mut_by_path<'a>(nodes: &'a mut [TreeNode], path: &[usize]) -> Option<&'a mut TreeNode> {
        if path.is_empty() {
            return None;
        }
        let idx = path[0];
        let node = nodes.get_mut(idx)?;
        if path.len() == 1 {
            Some(node)
        } else {
            Self::node_mut_by_path(&mut node.children, &path[1..])
        }
    }

    fn emit_selected(&self, ctx: &mut EventCtx, nodes: &[VisibleNode]) {
        if let Some(node) = nodes.get(self.selected) {
            if node.disabled {
                return;
            }
            ctx.post_message(TreeNodeSelected {
                index: self.selected,
                label: node.label.clone(),
                data: node.data.clone(),
            });
        }
    }

    fn emit_activated(&self, ctx: &mut EventCtx, index: usize, nodes: &[VisibleNode]) {
        if let Some(node) = nodes.get(index) {
            if node.disabled {
                return;
            }
            ctx.post_message(TreeNodeActivated {
                index,
                label: node.label.clone(),
                data: node.data.clone(),
            });
        }
    }

    fn emit_highlighted(&self, ctx: &mut EventCtx, nodes: &[VisibleNode]) {
        if let Some(node) = nodes.get(self.selected) {
            ctx.post_message(TreeNodeHighlighted {
                index: self.selected,
                label: node.label.clone(),
            });
        }
    }

    fn emit_toggled(&self, ctx: &mut EventCtx, index: usize, label: String, expanded: bool) {
        ctx.post_message(TreeNodeToggled {
            index,
            label: label.clone(),
            expanded,
        });
        if expanded {
            ctx.post_message(TreeNodeExpanded { index, label });
        } else {
            ctx.post_message(TreeNodeCollapsed { index, label });
        }
    }

    fn select_index(&mut self, index: usize, ctx: &mut EventCtx) {
        let nodes = self.visible_nodes();
        let total = nodes.len();
        if total == 0 {
            return;
        }
        let next = self
            .closest_selectable(index, 1, &nodes)
            .or_else(|| self.closest_selectable(index, -1, &nodes))
            .unwrap_or(self.selected.min(total - 1));
        if next != self.selected {
            self.selected = next;
            self.ensure_visible();
            self.emit_selected(ctx, &nodes);
            self.emit_highlighted(ctx, &nodes);
            ctx.request_repaint();
        }
    }

    fn move_selection(&mut self, delta: isize, ctx: &mut EventCtx) {
        let nodes = self.visible_nodes();
        let total = nodes.len();
        if total == 0 || self.selectable_count() == 0 {
            return;
        }
        let current = self.selected as isize;
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

    fn toggle_selected(&mut self, ctx: &mut EventCtx) {
        let nodes = self.visible_nodes();
        let Some(info) = nodes.get(self.selected).cloned() else {
            return;
        };
        if info.disabled || !info.expandable {
            return;
        }
        let mut expanded = info.expanded;
        if let Some(node) = Self::node_mut_by_path(&mut self.roots, &info.path) {
            node.expanded = !node.expanded;
            expanded = node.expanded;
        }
        self.ensure_visible();
        self.emit_toggled(ctx, self.selected, info.label, expanded);
        ctx.request_repaint();
    }

    fn toggle_index(&mut self, index: usize, ctx: &mut EventCtx) {
        let nodes = self.visible_nodes();
        let Some(info) = nodes.get(index).cloned() else {
            return;
        };
        if info.disabled || !info.expandable {
            return;
        }
        let mut expanded = info.expanded;
        if let Some(node) = Self::node_mut_by_path(&mut self.roots, &info.path) {
            node.expanded = !node.expanded;
            expanded = node.expanded;
        }
        self.ensure_visible();
        self.emit_toggled(ctx, index, info.label, expanded);
        ctx.request_repaint();
    }

    fn collapse_or_parent(&mut self, ctx: &mut EventCtx) {
        let nodes = self.visible_nodes();
        let Some(info) = nodes.get(self.selected).cloned() else {
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

    fn expand_or_child(&mut self, ctx: &mut EventCtx) {
        let nodes = self.visible_nodes();
        let Some(info) = nodes.get(self.selected).cloned() else {
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
            (self.selected + 1..nodes.len()).find(|idx| nodes[*idx].depth == current_depth + 1)
        {
            self.select_index(child_index, ctx);
        }
    }

    // ── Shift-key navigation (QW-22) ───────────────────────────────────

    /// Move cursor to the previous sibling; if none, move to parent.
    fn cursor_previous_sibling(&mut self, ctx: &mut EventCtx) {
        let nodes = self.visible_nodes();
        let Some(info) = nodes.get(self.selected) else {
            return;
        };
        let target_depth = info.depth;
        let parent_path: Vec<usize> = if info.path.len() > 1 {
            info.path[..info.path.len() - 1].to_vec()
        } else {
            Vec::new()
        };

        for i in (0..self.selected).rev() {
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
    fn cursor_next_sibling(&mut self, ctx: &mut EventCtx) {
        let nodes = self.visible_nodes();
        let Some(info) = nodes.get(self.selected) else {
            return;
        };
        let target_depth = info.depth;
        let parent_path: Vec<usize> = if info.path.len() > 1 {
            info.path[..info.path.len() - 1].to_vec()
        } else {
            Vec::new()
        };

        for (i, n) in nodes.iter().enumerate().skip(self.selected + 1) {
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
    fn cursor_parent(&mut self, ctx: &mut EventCtx) {
        let nodes = self.visible_nodes();
        let Some(info) = nodes.get(self.selected) else {
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
    fn toggle_expand_all_selected(&mut self, ctx: &mut EventCtx) {
        let nodes = self.visible_nodes();
        let Some(info) = nodes.get(self.selected).cloned() else {
            return;
        };
        // Need a parent to determine siblings (root-level excluded per Python).
        if info.path.len() <= 1 {
            return;
        }
        let parent_path = info.path[..info.path.len() - 1].to_vec();
        let Some(parent) = Self::node_mut_by_path(&mut self.roots, &parent_path) else {
            return;
        };

        // Check if all expandable siblings are collapsed.
        let all_collapsed = parent.children.iter().all(|child| {
            let expandable = child.allow_expand || !child.children.is_empty();
            !expandable || !child.expanded
        });
        let new_state = all_collapsed; // expand all if all collapsed, else collapse all

        for child in &mut parent.children {
            if child.allow_expand || !child.children.is_empty() {
                set_all_expanded(child, new_state);
            }
        }

        self.ensure_visible();
        self.emit_toggled(ctx, self.selected, info.label, new_state);
        ctx.request_repaint();
    }

    fn scroll_offset(&mut self, delta_rows: isize, ctx: &mut EventCtx) {
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

    fn max_line_width(&self) -> usize {
        let mut max_width = 1usize;
        for node in self.visible_nodes() {
            let prefix = Self::row_prefix(&node, false, self.show_guides, self.guide_depth);
            let width = rich_rs::cell_len(&prefix).saturating_add(rich_rs::cell_len(&node.label));
            max_width = max_width.max(width);
        }
        max_width
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

    #[allow(dead_code)] // Used by tests; render now resolves per-component styles directly.
    fn node_classes(
        node: &VisibleNode,
        highlighted: bool,
        hovered: bool,
        focused: bool,
    ) -> Vec<String> {
        let mut classes = vec!["tree--node".to_string()];
        if highlighted {
            classes.push("-highlighted".to_string());
        }
        if hovered && !highlighted {
            classes.push("-hover".to_string());
        }
        if highlighted && focused {
            classes.push("-focus".to_string());
        }
        if node.expandable {
            classes.push("-branch".to_string());
        } else {
            classes.push("-leaf".to_string());
        }
        if node.expanded {
            classes.push("-expanded".to_string());
        } else {
            classes.push("-collapsed".to_string());
        }
        if node.disabled {
            classes.push("-disabled".to_string());
        }
        classes.extend(node.component_classes.iter().cloned());
        classes
    }

    /// Render a tree node label, parsing Rich markup if present.
    ///
    /// Falls back to plain styled text if the label contains no markup or
    /// if parsing fails. Mirrors Python's `rich.text.Text` label storage
    /// where per-character styling is preserved.
    fn render_label_markup(
        label: &str,
        base_style: rich_rs::Style,
        console: &Console,
    ) -> Vec<Segment> {
        // Avoid parsing arbitrary bracketed labels as markup.
        // Parse only when the label has an explicit closing tag pattern.
        if !(label.contains('[') && label.contains("[/")) {
            return vec![Segment::styled(label.to_string(), base_style)];
        }
        match rich_rs::markup::render(label, false) {
            Ok(text) => {
                // Merge the base label style (cursor/highlight/component) with
                // any inline markup styles. The base style applies to unstyled
                // portions; markup styles layer on top.
                let opts = ConsoleOptions {
                    size: (label.len().max(1) + 20, 1),
                    max_width: label.len().max(1) + 20,
                    no_wrap: true,
                    ..console.options().clone()
                };
                let rendered: Vec<Segment> = text.render(console, &opts).into_iter().collect();
                // Apply base style to segments that have no explicit style.
                rendered
                    .into_iter()
                    .map(|seg| match seg.style {
                        Some(s) => Segment::styled(seg.text, base_style + s),
                        None => Segment::styled(seg.text, base_style),
                    })
                    .collect()
            }
            Err(_) => vec![Segment::styled(label.to_string(), base_style)],
        }
    }

    fn twisty(node: &VisibleNode) -> &'static str {
        if !node.expandable {
            ""
        } else if node.expanded {
            "▼ "
        } else {
            "▶ "
        }
    }

    fn guide_prefix(node: &VisibleNode, show_guides: bool, guide_depth: usize) -> String {
        if node.depth == 0 {
            return String::new();
        }
        let gd = guide_depth.clamp(2, 10);
        let mut prefix = String::new();

        // Ancestor continuation lines for visual depths 1..depth-1
        for level in 1..node.depth {
            if show_guides && !node.is_last_at_depth[level] {
                prefix.push('│');
                for _ in 0..gd - 1 {
                    prefix.push(' ');
                }
            } else {
                for _ in 0..gd {
                    prefix.push(' ');
                }
            }
        }

        // Branch connector for this node
        if show_guides {
            if node.is_last_at_depth[node.depth] {
                prefix.push('└');
            } else {
                prefix.push('├');
            }
            for _ in 0..gd.saturating_sub(2) {
                prefix.push('─');
            }
            prefix.push(' ');
        } else {
            for _ in 0..gd {
                prefix.push(' ');
            }
        }

        prefix
    }

    fn row_prefix(
        node: &VisibleNode,
        _highlighted: bool,
        show_guides: bool,
        guide_depth: usize,
    ) -> String {
        format!(
            "{}{}",
            Self::guide_prefix(node, show_guides, guide_depth),
            Self::twisty(node)
        )
    }

    fn twisty_hit_max_x(node: &VisibleNode, show_guides: bool, guide_depth: usize) -> usize {
        let guide = Self::guide_prefix(node, show_guides, guide_depth);
        let prefix = format!("{}{}", guide, Self::twisty(node));
        rich_rs::cell_len(&prefix).saturating_sub(1)
    }
}

impl ReactiveWidget for Tree {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            match change.field_name {
                "show_root" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_show_root(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Widget for Tree {
    fn focusable(&self) -> bool {
        true
    }

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

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
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
                self.emit_activated(ctx, self.selected, &nodes);
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

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                let nodes = self.visible_nodes();
                let index = self.offset.saturating_add(mouse.y as usize);
                if let Some(node) = nodes.get(index) {
                    if node.disabled {
                        return;
                    }
                    let twist_col =
                        Self::twisty_hit_max_x(node, self.show_guides, self.guide_depth);
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
                            self.emit_activated(ctx, self.selected, &nodes);
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

    fn on_mouse_scroll(&mut self, _delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        if delta_y == 0 {
            return;
        }
        self.scroll_offset(
            delta_y.saturating_mul(self.scroll_step as i32) as isize,
            ctx,
        );
    }

    fn on_unmount(&mut self) {
        self.hovered_index = None;
        self.pressed_activation_index = None;
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let nodes = self.visible_nodes();
        let mut out = Segments::new();

        // Resolve component styles once per render.
        let parent_meta = crate::css::selector_meta_generic(self);
        let parent_resolved = crate::css::resolve_style(self, &parent_meta);
        let resolve_component = |classes: &[&str]| {
            let meta = crate::css::selector_meta_component("", classes);
            crate::css::with_style_stack(parent_meta.clone(), parent_resolved.clone(), || {
                crate::css::resolve_style_for_meta(&meta)
            })
        };
        let default_bg = crate::style::parse_color_like("$background")
            .unwrap_or(crate::style::Color::rgb(0, 0, 0));
        let component_bg_base = crate::css::current_composited_background()
            .or(parent_resolved.bg)
            .unwrap_or(default_bg);
        let base_style = parent_resolved
            .to_rich_over(component_bg_base)
            .unwrap_or_else(rich_rs::Style::new);
        let guide_style = resolve_component(&["tree--guides"])
            .to_rich_over(component_bg_base)
            .unwrap_or(base_style);
        let guide_hover_style = resolve_component(&["tree--guides-hover"])
            .to_rich_over(component_bg_base)
            .unwrap_or(guide_style);
        let guide_selected_style = resolve_component(&["tree--guides-selected"])
            .to_rich_over(component_bg_base)
            .unwrap_or(guide_style);
        let label_style = resolve_component(&["tree--label"])
            .to_rich_over(component_bg_base)
            .unwrap_or(base_style);
        let cursor_style = resolve_component(&["tree--cursor"])
            .to_rich_over(component_bg_base)
            .unwrap_or(base_style);
        let highlight_style = resolve_component(&["tree--highlight"])
            .to_rich_over(component_bg_base)
            .unwrap_or(base_style);
        let highlight_line_style = resolve_component(&["tree--highlight-line"])
            .to_rich_over(component_bg_base)
            .unwrap_or(base_style);

        let selected_path: Option<&[usize]> = if self.node_state().focused {
            nodes.get(self.selected).map(|node| node.path.as_slice())
        } else {
            None
        };
        let hovered_path: Option<&[usize]> = self
            .hovered_index
            .and_then(|index| nodes.get(index))
            .map(|node| node.path.as_slice());

        for row in 0..height {
            let index = self.offset + row;
            if let Some(node) = nodes.get(index) {
                let highlighted = index == self.selected && !node.disabled;
                let hovered = self.hovered_index == Some(index);
                let hover_in_path = hovered_path.is_some_and(|path| node.path.starts_with(path));
                let selected_in_path =
                    selected_path.is_some_and(|path| node.path.starts_with(path));
                let row_line_style = if hover_in_path {
                    highlight_line_style
                } else {
                    rich_rs::Style::default()
                };

                // Pick guide style for this row.
                let row_guide_style = if selected_in_path {
                    guide_selected_style
                } else if hover_in_path {
                    guide_hover_style
                } else {
                    guide_style
                };
                let row_guide_style = row_guide_style + row_line_style;

                // Build label style: base label + component classes + highlight + cursor.
                let mut row_label_style = label_style + row_line_style;
                // Apply node-specific component classes (e.g. directory-tree--file).
                if !node.component_classes.is_empty() {
                    let cc_refs: Vec<&str> =
                        node.component_classes.iter().map(String::as_str).collect();
                    if let Some(cc_style) =
                        resolve_component(&cc_refs).to_rich_over(component_bg_base)
                    {
                        row_label_style = row_label_style + cc_style;
                    }
                }
                if hovered {
                    row_label_style = row_label_style + highlight_style;
                }
                if highlighted {
                    row_label_style = row_label_style + cursor_style;
                }

                // Build segments for this row.
                let mut row_segments: Vec<Segment> = Vec::new();

                // 1. Guide prefix segments (per-depth styled).
                if node.depth > 0 {
                    let gd = self.guide_depth.clamp(2, 10);
                    // Ancestor continuation lines.
                    for level in 1..node.depth {
                        let guide_text = if self.show_guides && !node.is_last_at_depth[level] {
                            let mut s = String::with_capacity(gd);
                            s.push('│');
                            for _ in 0..gd - 1 {
                                s.push(' ');
                            }
                            s
                        } else {
                            " ".repeat(gd)
                        };
                        row_segments.push(Segment::styled(guide_text, row_guide_style));
                    }
                    // Branch connector for this node.
                    let connector = if self.show_guides {
                        let ch = if node.is_last_at_depth[node.depth] {
                            '└'
                        } else {
                            '├'
                        };
                        let mut s = String::with_capacity(gd);
                        s.push(ch);
                        for _ in 0..gd.saturating_sub(2) {
                            s.push('─');
                        }
                        s.push(' ');
                        s
                    } else {
                        " ".repeat(gd)
                    };
                    row_segments.push(Segment::styled(connector, row_guide_style));
                }

                // 2. Twisty (expand/collapse indicator).
                let twisty = Self::twisty(node);
                if !twisty.is_empty() {
                    row_segments.push(Segment::styled(twisty.to_string(), row_label_style));
                }

                // 3. Label text (with Rich markup support).
                //
                // Mirrors Python's TreeNode which stores `rich.text.Text` objects
                // with per-character styling. Parse Rich markup (e.g. `[b]name[/b]`)
                // so json_tree can render bold keys like Python does.
                let label_segs = Self::render_label_markup(&node.label, row_label_style, console);
                row_segments.extend(label_segs);

                // Pad/crop to width.
                // For hover-line rows, fill the entire row width with hover background.
                // Otherwise keep trailing cells transparent so parent surface composes naturally.
                let line = if hover_in_path {
                    Segment::adjust_line_length(&row_segments, width, Some(row_line_style), true)
                } else {
                    adjust_line_length_no_bg(&row_segments, width)
                };
                out.extend(line);
            } else {
                // Empty row beyond visible nodes.
                let line =
                    adjust_line_length_no_bg(&[Segment::styled(String::new(), base_style)], width);
                out.extend(line);
            }
            if row + 1 < height {
                out.push(Segment::line());
            }
        }

        out
    }

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

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

// ── Free helpers for recursive tree operations ──────────────────────────

fn has_any_expanded(nodes: &[TreeNode]) -> bool {
    for node in nodes {
        if (node.allow_expand || !node.children.is_empty()) && node.expanded {
            return true;
        }
        if has_any_expanded(&node.children) {
            return true;
        }
    }
    false
}

fn set_all_expanded(node: &mut TreeNode, value: bool) {
    if node.allow_expand || !node.children.is_empty() {
        node.expanded = value;
    }
    for child in &mut node.children {
        set_all_expanded(child, value);
    }
}

impl Renderable for Tree {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::{Tree, TreeNode, VisibleNode};
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
        tree.on_event(&Event::Key(key), &mut ctx);

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
        tree.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
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
        tree.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 1,
                screen_y: 1,
                x: 1,
                y: 1,
            }),
            &mut down_ctx,
        );
        assert!(down_ctx.handled());
        // select_index emits TreeNodeSelected + TreeNodeHighlighted (2 messages)
        assert_eq!(down_ctx.take_messages().len(), 2);

        let mut up_ctx = EventCtx::default();
        tree.on_event(
            &Event::MouseUp(MouseUpEvent {
                target: Some(id),
                screen_x: 1,
                screen_y: 1,
                x: 1,
                y: 1,
            }),
            &mut up_ctx,
        );
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
        tree.on_event(&Event::AppFocus(false), &mut ctx);

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
        assert!(tree.execute_action(&action, &mut ctx));
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
    fn tree_root_mut_returns_first_root() {
        let mut tree = Tree::new(vec![TreeNode::new("Root")]);
        let root = tree.root_mut().expect("should have a root");
        assert_eq!(root.label(), "Root");
        root.set_label("NewRoot");
        assert_eq!(tree.root().unwrap().label(), "NewRoot");
    }

    #[test]
    fn tree_clear_preserves_root() {
        let mut tree = Tree::new(vec![
            TreeNode::new("Root")
                .with_child(TreeNode::new("A"))
                .with_child(TreeNode::new("B")),
        ]);
        assert_eq!(tree.root().unwrap().children.len(), 2);
        tree.clear();
        // Root preserved, children cleared.
        let root = tree.root().expect("root should survive clear()");
        assert_eq!(root.label(), "Root");
        assert!(root.children.is_empty());
    }

    #[test]
    fn tree_reset_replaces_root() {
        let mut tree = Tree::new(vec![
            TreeNode::new("Old").with_child(TreeNode::new("Child")),
        ]);
        tree.reset("Fresh");
        let root = tree.root().expect("reset should create a root");
        assert_eq!(root.label(), "Fresh");
        assert!(root.children.is_empty());
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
}
