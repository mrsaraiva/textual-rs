use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments, Text};

use crate::event::{Action, Event, EventCtx};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, focused_classes},
};

#[derive(Debug, Clone)]
pub struct TreeNode {
    label: String,
    expanded: bool,
    children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            expanded: true,
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
}

#[derive(Debug, Clone)]
pub struct Tree {
    id: WidgetId,
    roots: Vec<TreeNode>,
    selected: usize,
    offset: usize,
    focused: bool,
    styles: WidgetStyles,
}

impl Tree {
    pub fn new(roots: Vec<TreeNode>) -> Self {
        Self {
            id: WidgetId::new(),
            roots,
            selected: 0,
            offset: 0,
            focused: false,
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
        self.selected = index.min(total.saturating_sub(1));
    }

    fn visible_count(&self) -> usize {
        fn count(node: &TreeNode) -> usize {
            let mut total = 1;
            if node.expanded {
                for child in &node.children {
                    total += count(child);
                }
            }
            total
        }
        let mut total = 0;
        for root in &self.roots {
            total += count(root);
        }
        total
    }

    fn ensure_visible(&mut self, height: usize) {
        if height == 0 {
            self.offset = 0;
            return;
        }
        let total = self.visible_count();
        if total == 0 {
            self.offset = 0;
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }

    fn toggle_selected(&mut self) {
        let mut index = 0usize;
        if let Some(node) = node_mut_by_visible_index(&mut self.roots, self.selected, &mut index) {
            if !node.children.is_empty() {
                node.expanded = !node.expanded;
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

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.focused {
            return;
        }
        let mut handled = false;
        match event {
            Event::Action(Action::ScrollUp) => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                handled = true;
            }
            Event::Action(Action::ScrollDown) => {
                let total = self.visible_count();
                if self.selected + 1 < total {
                    self.selected += 1;
                }
                handled = true;
            }
            Event::Action(Action::ScrollPageUp) => {
                if self.selected > 0 {
                    let step = 5.min(self.selected);
                    self.selected -= step;
                }
                handled = true;
            }
            Event::Action(Action::ScrollPageDown) => {
                let total = self.visible_count();
                if self.selected + 1 < total {
                    let step = 5.min(total.saturating_sub(1) - self.selected);
                    self.selected += step;
                }
                handled = true;
            }
            Event::Action(Action::Toggle) => {
                self.toggle_selected();
                handled = true;
            }
            Event::Key(key) => match key.code {
                KeyCode::Up => {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                    handled = true;
                }
                KeyCode::Down => {
                    let total = self.visible_count();
                    if self.selected + 1 < total {
                        self.selected += 1;
                    }
                    handled = true;
                }
                KeyCode::PageUp => {
                    if self.selected > 0 {
                        let step = 5.min(self.selected);
                        self.selected -= step;
                    }
                    handled = true;
                }
                KeyCode::PageDown => {
                    let total = self.visible_count();
                    if self.selected + 1 < total {
                        let step = 5.min(total.saturating_sub(1) - self.selected);
                        self.selected += step;
                    }
                    handled = true;
                }
                KeyCode::Left => {
                    let mut index = 0usize;
                    if let Some(node) =
                        node_mut_by_visible_index(&mut self.roots, self.selected, &mut index)
                    {
                        if node.expanded {
                            node.expanded = false;
                        }
                    }
                    handled = true;
                }
                KeyCode::Right => {
                    let mut index = 0usize;
                    if let Some(node) =
                        node_mut_by_visible_index(&mut self.roots, self.selected, &mut index)
                    {
                        if !node.children.is_empty() {
                            node.expanded = true;
                        }
                    }
                    handled = true;
                }
                _ => {}
            },
            _ => {}
        }
        if handled {
            ctx.set_handled();
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let height = options.size.1.max(1);
        let mut view = self.clone();
        view.ensure_visible(height);

        let mut lines: Vec<String> = Vec::new();
        let mut index = 0usize;
        render_tree_lines(
            &view.roots,
            0,
            &mut index,
            view.selected,
            view.offset,
            height,
            view.focused,
            &mut lines,
        );

        if lines.is_empty() {
            lines.push(String::new());
        }
        let text = Text::plain(lines.join("\n"));
        text.render(console, options)
    }

    fn style_classes(&self) -> &[String] {
        if self.focused {
            focused_classes()
        } else {
            empty_classes()
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

fn node_mut_by_visible_index<'a>(
    nodes: &'a mut [TreeNode],
    target: usize,
    index: &mut usize,
) -> Option<&'a mut TreeNode> {
    for node in nodes {
        if *index == target {
            return Some(node);
        }
        *index += 1;
        if node.expanded {
            if let Some(found) = node_mut_by_visible_index(&mut node.children, target, index) {
                return Some(found);
            }
        }
    }
    None
}

fn render_tree_lines(
    nodes: &[TreeNode],
    depth: usize,
    index: &mut usize,
    selected: usize,
    offset: usize,
    height: usize,
    focused: bool,
    lines: &mut Vec<String>,
) {
    for node in nodes {
        if lines.len() >= height {
            return;
        }
        if *index >= offset && lines.len() < height {
            let marker = if *index == selected {
                if focused { "> " } else { "* " }
            } else {
                "  "
            };
            let twist = if node.children.is_empty() {
                " "
            } else if node.expanded {
                "v"
            } else {
                ">"
            };
            let indent = "  ".repeat(depth);
            lines.push(format!("{marker}{indent}{twist} {}", node.label));
        }
        *index += 1;
        if node.expanded {
            render_tree_lines(
                &node.children,
                depth + 1,
                index,
                selected,
                offset,
                height,
                focused,
                lines,
            );
        }
    }
}
