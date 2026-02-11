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
    id: WidgetId,
    roots: Vec<TreeNode>,
    selected: usize,
    offset: usize,
    focused: bool,
    hovered: bool,
    hovered_index: Option<usize>,
    pressed_activation_index: Option<usize>,
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
    component_classes: Vec<String>,
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
            pressed_activation_index: None,
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
                    component_classes: node.component_classes.clone(),
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
            ctx.post_message(
                self.id,
                Message::TreeNodeSelected {
                    index: self.selected,
                    label: node.label.clone(),
                },
            );
        }
    }

    fn emit_activated(&self, ctx: &mut EventCtx, index: usize, nodes: &[VisibleNode]) {
        if let Some(node) = nodes.get(index) {
            if node.disabled {
                return;
            }
            ctx.post_message(
                self.id,
                Message::TreeNodeActivated {
                    index,
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
                    let twist_col = node.depth.saturating_mul(2) + 2;
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
            Event::MouseUp(mouse) if mouse.target == Some(self.id) => {
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
            },
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
                let twist = if !node.expandable {
                    " "
                } else if node.expanded {
                    "▾"
                } else {
                    "▸"
                };
                let classes = Self::node_classes(node, highlighted, hovered, self.focused);
                let class_refs: Vec<&str> = classes.iter().map(String::as_str).collect();
                style = crate::css::resolve_component_style(self, &class_refs)
                    .to_rich()
                    .unwrap_or(style);
                let marker = if highlighted { "› " } else { "  " };
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

#[cfg(test)]
mod tests {
    use super::{Tree, TreeNode, VisibleNode};
    use crate::event::{Event, EventCtx, MouseDownEvent, MouseUpEvent};
    use crate::keys::KeyEventData;
    use crate::message::Message;
    use crate::widgets::Widget;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
            Message::TreeNodeActivated {
                index: 0,
                ref label
            } if label == "Root"
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
        let id = tree.id();

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
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].message,
            Message::TreeNodeToggled {
                index: 0,
                expanded: false,
                ..
            }
        ));
    }

    #[test]
    fn mouse_row_click_activates_on_mouse_up() {
        let mut tree = Tree::new(vec![TreeNode::new("Root"), TreeNode::new("Second")]);
        tree.on_layout(24, 4);
        let id = tree.id();

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
        assert_eq!(down_ctx.take_messages().len(), 1); // selection changed only

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
            Message::TreeNodeActivated {
                index: 1,
                ref label
            } if label == "Second"
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
}
