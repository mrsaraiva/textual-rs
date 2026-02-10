use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::Message;

use super::{
    ScrollView, Widget, WidgetId, WidgetStyles,
    helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints},
};

#[derive(Debug, Clone)]
pub struct TreeNode {
    label: String,
    expanded: bool,
    allow_expand: bool,
    disabled: bool,
    children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            expanded: true,
            allow_expand: false,
            disabled: false,
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
}

#[derive(Debug, Clone)]
pub struct Tree {
    id: WidgetId,
    roots: Vec<TreeNode>,
    selected: usize,
    offset: usize,
    focused: bool,
    hovered: bool,
    hovered_index: Option<usize>,
    viewport_height: usize,
    scroll_step: usize,
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
}

impl Tree {
    pub fn new(roots: Vec<TreeNode>) -> Self {
        Self {
            id: WidgetId::new(),
            roots,
            selected: 0,
            offset: 0,
            focused: false,
            hovered: false,
            hovered_index: None,
            viewport_height: 1,
            scroll_step: 1,
            classes: vec!["tree".to_string()],
            focused_classes: vec!["tree".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, index: usize) {
        let total = self.visible_count();
        if total == 0 {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(total - 1);
        self.ensure_visible();
    }

    fn visible_nodes(&self) -> Vec<VisibleNode> {
        fn walk(
            nodes: &[TreeNode],
            depth: usize,
            path: &mut Vec<usize>,
            out: &mut Vec<VisibleNode>,
        ) {
            for (idx, node) in nodes.iter().enumerate() {
                path.push(idx);
                out.push(VisibleNode {
                    path: path.clone(),
                    depth,
                    label: node.label.clone(),
                    expanded: node.expanded,
                    disabled: node.disabled,
                    expandable: node.allow_expand || !node.children.is_empty(),
                });
                if node.expanded {
                    walk(&node.children, depth + 1, path, out);
                }
                path.pop();
            }
        }

        let mut out = Vec::new();
        let mut path = Vec::new();
        walk(&self.roots, 0, &mut path, &mut out);
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
            ctx.post_message(
                self.id,
                Message::TreeNodeSelected {
                    index: self.selected,
                    label: node.label.clone(),
                },
            );
        }
    }

    fn emit_toggled(&self, ctx: &mut EventCtx, index: usize, label: String, expanded: bool) {
        ctx.post_message(
            self.id,
            Message::TreeNodeToggled {
                index,
                label,
                expanded,
            },
        );
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
            let width = 2usize
                .saturating_add(node.depth.saturating_mul(2))
                .saturating_add(2)
                .saturating_add(rich_rs::cell_len(&node.label));
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
}

impl Widget for Tree {
    fn id(&self) -> WidgetId {
        self.id
    }

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

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == self.id => {
                let nodes = self.visible_nodes();
                let index = self.offset.saturating_add(mouse.y as usize);
                if let Some(node) = nodes.get(index) {
                    if node.disabled {
                        return;
                    }
                    self.select_index(index, ctx);
                    let twist_col = node.depth.saturating_mul(2) + 2;
                    if node.expandable && (mouse.x as usize) <= twist_col {
                        self.toggle_selected(ctx);
                    }
                    ctx.set_handled();
                }
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
            Event::Key(key) if self.focused => match key.code {
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
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.toggle_selected(ctx);
                    ctx.set_handled();
                }
                _ => {}
            },
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
                let selected = index == self.selected;
                let hovered = self.hovered_index == Some(index);
                let twist = if !node.expandable {
                    " "
                } else if node.expanded {
                    "▾"
                } else {
                    "▸"
                };
                let mut classes = vec!["tree--node"];
                if selected {
                    classes.push("-selected");
                }
                if hovered {
                    classes.push("-hover");
                }
                if selected && self.focused {
                    classes.push("-focus");
                }
                if node.expandable {
                    classes.push("-branch");
                } else {
                    classes.push("-leaf");
                }
                if node.expanded {
                    classes.push("-expanded");
                } else {
                    classes.push("-collapsed");
                }
                if node.disabled {
                    classes.push("-disabled");
                }
                style = crate::css::resolve_component_style(self, &classes)
                    .to_rich()
                    .unwrap_or(style);
                let marker = if selected { "› " } else { "  " };
                text = format!(
                    "{}{}{} {}",
                    marker,
                    "  ".repeat(node.depth),
                    twist,
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

impl Renderable for Tree {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
