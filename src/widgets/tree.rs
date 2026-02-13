use crossterm::event::{KeyCode, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::*;

use crate::action::ParsedAction;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

use super::{
    BindingDecl, ScrollView, Widget, WidgetStyles,
    helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints},
};

#[derive(Debug, Clone)]
pub struct TreeNode {
    label: String,
    expanded: bool,
    allow_expand: bool,
    disabled: bool,
    component_classes: Vec<String>,
    children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            expanded: true,
            allow_expand: false,
            disabled: false,
            component_classes: Vec::new(),
            children: Vec::new(),
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
}

#[derive(Debug, Clone)]
pub struct Tree {
    roots: Vec<TreeNode>,
    selected: usize,
    offset: usize,
    focused: bool,
    hovered: bool,
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
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
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
            focused: false,
            hovered: false,
            hovered_index: None,
            pressed_activation_index: None,
            viewport_height: 1,
            scroll_step: 1,
            show_root: true,
            show_guides: true,
            guide_depth: 4,
            classes: vec!["tree".to_string()],
            focused_classes: vec!["tree".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
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

    /// Reactive setter for `selected`.
    pub fn set_selected(&mut self, index: usize, ctx: &mut ReactiveCtx) {
        let total = self.visible_count();
        if total == 0 {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        let new_selected = index.min(total - 1);
        if self.selected != new_selected {
            let old = self.selected;
            self.selected = new_selected;
            self.ensure_visible();
            ctx.record_change(
                "selected",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(self.selected),
            );
        }
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

    // ── API methods (QW-19) ──────────────────────────────────────────────

    /// Remove all nodes from the tree.
    pub fn clear(&mut self) {
        self.roots.clear();
        self.selected = 0;
        self.offset = 0;
        self.hovered_index = None;
        self.pressed_activation_index = None;
    }

    /// Move the cursor/highlight to a specific visible-node index, or reset if `None`.
    pub fn move_cursor(&mut self, node_index: Option<usize>) {
        match node_index {
            None => {
                self.selected = 0;
                self.offset = 0;
            }
            Some(index) => {
                // Direct field assignment (not using reactive setter).
                let total = self.visible_count();
                if total == 0 {
                    self.selected = 0;
                    self.offset = 0;
                } else {
                    self.selected = index.min(total - 1);
                    self.ensure_visible();
                }
            }
        }
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
            ctx.post_message(Message::TreeNodeSelected(TreeNodeSelected {
                index: self.selected,
                label: node.label.clone(),
            }));
        }
    }

    fn emit_activated(&self, ctx: &mut EventCtx, index: usize, nodes: &[VisibleNode]) {
        if let Some(node) = nodes.get(index) {
            if node.disabled {
                return;
            }
            ctx.post_message(Message::TreeNodeActivated(TreeNodeActivated {
                index,
                label: node.label.clone(),
            }));
        }
    }

    fn emit_highlighted(&self, ctx: &mut EventCtx, nodes: &[VisibleNode]) {
        if let Some(node) = nodes.get(self.selected) {
            ctx.post_message(Message::TreeNodeHighlighted(TreeNodeHighlighted {
                index: self.selected,
                label: node.label.clone(),
            }));
        }
    }

    fn emit_toggled(&self, ctx: &mut EventCtx, index: usize, label: String, expanded: bool) {
        ctx.post_message(Message::TreeNodeToggled(TreeNodeToggled {
            index,
            label: label.clone(),
            expanded,
        }));
        if expanded {
            ctx.post_message(Message::TreeNodeExpanded(TreeNodeExpanded { index, label }));
        } else {
            ctx.post_message(Message::TreeNodeCollapsed(TreeNodeCollapsed {
                index,
                label,
            }));
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

    /// Move cursor to the next sibling; stays put if none.
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
            // Past this subtree — no more siblings
            if n.depth < target_depth {
                return;
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

    /// Toggle expand/collapse on the selected node and all its descendants.
    fn toggle_expand_all_selected(&mut self, ctx: &mut EventCtx) {
        let nodes = self.visible_nodes();
        let Some(info) = nodes.get(self.selected).cloned() else {
            return;
        };
        if info.disabled || !info.expandable {
            return;
        }
        let new_state = !info.expanded;
        if let Some(node) = Self::node_mut_by_path(&mut self.roots, &info.path) {
            set_all_expanded(node, new_state);
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

    fn twisty(node: &VisibleNode) -> &'static str {
        if !node.expandable {
            " "
        } else if node.expanded {
            "▾"
        } else {
            "▸"
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
        highlighted: bool,
        show_guides: bool,
        guide_depth: usize,
    ) -> String {
        let marker = if highlighted { "› " } else { "  " };
        format!(
            "{}{}{} ",
            marker,
            Self::guide_prefix(node, show_guides, guide_depth),
            Self::twisty(node)
        )
    }

    fn twisty_hit_max_x(node: &VisibleNode, show_guides: bool, guide_depth: usize) -> usize {
        let guide = Self::guide_prefix(node, show_guides, guide_depth);
        let prefix = format!("  {}{}", guide, Self::twisty(node));
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

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        if !hovered {
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
            BindingDecl::new("up", "cursor_up", "Move cursor up"),
            BindingDecl::new("down", "cursor_down", "Move cursor down"),
            BindingDecl::new("pageup", "scroll_up", "Page up").hidden(),
            BindingDecl::new("pagedown", "scroll_down", "Page down").hidden(),
            BindingDecl::new("home", "scroll_home", "Move to first node").hidden(),
            BindingDecl::new("end", "scroll_end", "Move to last node").hidden(),
            BindingDecl::new("left", "collapse_or_parent", "Collapse or move to parent"),
            BindingDecl::new("right", "expand_or_child", "Expand or move to child"),
            BindingDecl::new("enter", "select_cursor", "Activate node"),
            BindingDecl::new("space", "toggle_node", "Toggle node"),
            BindingDecl::new("shift+up", "cursor_previous_sibling", "Previous sibling").hidden(),
            BindingDecl::new("shift+down", "cursor_next_sibling", "Next sibling").hidden(),
            BindingDecl::new("shift+left", "cursor_parent", "Go to parent").hidden(),
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
            Event::Action(action) if self.focused => match action {
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
            Event::Key(key) if self.focused => {
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
                if self.hovered || self.hovered_index.is_some() {
                    self.hovered = false;
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
        self.hovered = false;
        self.hovered_index = None;
        self.pressed_activation_index = None;
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let nodes = self.visible_nodes();
        let mut out = Segments::new();
        let base_style = crate::css::resolve_component_style(self, &["tree--node"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        for row in 0..height {
            let index = self.offset + row;
            let mut text = String::new();
            let mut style = base_style;
            if let Some(node) = nodes.get(index) {
                let highlighted = index == self.selected && !node.disabled;
                let hovered = self.hovered_index == Some(index);
                let classes = Self::node_classes(node, highlighted, hovered, self.focused);
                let class_refs: Vec<&str> = classes.iter().map(String::as_str).collect();
                style = crate::css::resolve_component_style(self, &class_refs)
                    .to_rich()
                    .unwrap_or(style);
                text = format!(
                    "{}{}",
                    Self::row_prefix(node, highlighted, self.show_guides, self.guide_depth),
                    node.label
                );
            }
            let line = adjust_line_length_no_bg(&[Segment::styled(text, style)], width);
            out.extend(line);
            if row + 1 < height {
                out.push(Segment::line());
            }
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints())
            .or(Some(self.visible_count().max(1)))
    }

    fn content_width(&self) -> Option<usize> {
        Some(self.max_line_width())
    }

    fn style_classes(&self) -> &[String] {
        if self.focused {
            &self.focused_classes
        } else if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
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
    use crate::widgets::Widget;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rich_rs::Console;

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
        tree.set_focus(true);
        tree.on_layout(24, 4);

        let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        tree.on_event(&Event::Key(key), &mut ctx);

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].message,
            Message::TreeNodeActivated(TreeNodeActivated {
                index: 0,
                ref label
            }) if label == "Root"
        ));

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
        assert!(matches!(
            messages[0].message,
            Message::TreeNodeToggled(TreeNodeToggled {
                index: 0,
                expanded: false,
                ..
            })
        ));
        assert!(matches!(
            messages[1].message,
            Message::TreeNodeCollapsed(TreeNodeCollapsed { index: 0, .. })
        ));
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
        assert!(matches!(
            messages[0].message,
            Message::TreeNodeActivated(TreeNodeActivated {
                index: 1,
                ref label
            }) if label == "Second"
        ));
    }

    #[test]
    fn app_focus_loss_clears_hover_state() {
        let mut tree = Tree::new(vec![TreeNode::new("Root")]);
        tree.set_hovered(true);
        assert!(tree.on_mouse_move(0, 0));
        assert_eq!(tree.hovered_index, Some(0));

        let mut ctx = EventCtx::default();
        tree.on_event(&Event::AppFocus(false), &mut ctx);

        assert!(!tree.is_hovered());
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
            .map(|label| rich_rs::cell_len("  ▸ ").saturating_add(rich_rs::cell_len(label)))
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
        tree.set_focus(true);
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "cursor_down".to_string(),
            arguments: vec![],
        };
        assert!(tree.execute_action(&action, &mut ctx));
    }
}
