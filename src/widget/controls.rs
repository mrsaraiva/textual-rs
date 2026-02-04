use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};

use crate::event::{Action, Event, EventCtx};
use super::style_selectors;

use super::{
    helpers::{empty_classes, fixed_height_from_constraints, focused_classes},
    Widget, WidgetId, WidgetStyles,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Default,
    Primary,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Button {
    id: WidgetId,
    label: String,
    focused: bool,
    pressed: bool,
    variant: ButtonVariant,
    disabled: bool,
    flat: bool,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            focused: false,
            pressed: false,
            variant: ButtonVariant::Default,
            disabled: false,
            flat: false,
            classes: Vec::new(),
            focused_classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
        .rebuild_classes()
    }

    pub fn primary(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Primary)
    }

    pub fn success(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Success)
    }

    pub fn warning(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Warning)
    }

    pub fn error(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Error)
    }

    pub fn pressed(&self) -> bool {
        self.pressed
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self.rebuild_classes()
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.rebuild_classes()
    }

    pub fn flat(mut self, flat: bool) -> Self {
        self.flat = flat;
        self.rebuild_classes()
    }

    fn rebuild_classes(mut self) -> Self {
        let mut classes = vec!["button".to_string()];
        match self.variant {
            ButtonVariant::Primary => classes.push("primary".to_string()),
            ButtonVariant::Success => classes.push("success".to_string()),
            ButtonVariant::Warning => classes.push("warning".to_string()),
            ButtonVariant::Error => classes.push("error".to_string()),
            ButtonVariant::Default => {}
        }
        if self.disabled {
            classes.push("disabled".to_string());
        }
        if self.flat {
            classes.push("flat".to_string());
        }
        let mut focused_classes = classes.clone();
        focused_classes.push("focused".to_string());
        self.classes = classes;
        self.focused_classes = focused_classes;
        self
    }
}

impl Widget for Button {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        !self.disabled
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.focused || self.disabled {
            return;
        }
        if let Event::Action(Action::Toggle) = event {
            self.pressed = !self.pressed;
            ctx.set_handled();
            return;
        }
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.pressed = !self.pressed;
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let padding_h = 2usize;
        let inner_width = width.saturating_sub(padding_h * 2).max(1);

        let label_row = height / 2;
        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..height {
            let line = if row == label_row {
                let label = self.label.as_str();
                let label_width = rich_rs::cell_len(label).min(inner_width);
                let left = inner_width.saturating_sub(label_width) / 2;
                let right = inner_width.saturating_sub(label_width) - left;
                let label_line = format!(
                    "{}{}{}",
                    " ".repeat(left),
                    rich_rs::set_cell_size(label, label_width),
                    " ".repeat(right)
                );
                label_line
            } else {
                " ".repeat(inner_width)
            };
            let mut row_segments: Vec<Segment> = Vec::new();
            if padding_h > 0 {
                row_segments.push(Segment::new(" ".repeat(padding_h)));
            }
            row_segments.push(Segment::new(line));
            if padding_h > 0 {
                row_segments.push(Segment::new(" ".repeat(padding_h)));
            }
            out_lines.push(row_segments);
        }
        let out_lines = Segment::set_shape(&out_lines, width, Some(height), None, false);
        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        let meta = style_selectors::selector_meta_generic(self);
        let base_style = style_selectors::resolve_style(self, &meta);
        let line_pad = base_style.line_pad.unwrap_or(1);
        let default_height = 1 + line_pad * 2 + super::helpers::border_vertical_padding(&base_style);
        fixed_height_from_constraints(self.layout_constraints()).or(Some(default_height))
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


impl Renderable for Button {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct ListView {
    id: WidgetId,
    items: Vec<String>,
    selected: usize,
    offset: usize,
    focused: bool,
    styles: WidgetStyles,
}

impl ListView {
    pub fn new(items: Vec<String>) -> Self {
        Self {
            id: WidgetId::new(),
            items,
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
        if self.items.is_empty() {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(self.items.len() - 1);
    }

    fn ensure_visible(&mut self, height: usize) {
        if self.items.is_empty() {
            self.offset = 0;
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }
}

impl Widget for ListView {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
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
                if self.selected + 1 < self.items.len() {
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
                if self.selected + 1 < self.items.len() {
                    let step = 5.min(self.items.len().saturating_sub(1) - self.selected);
                    self.selected += step;
                }
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
                    if self.selected + 1 < self.items.len() {
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
                    if self.selected + 1 < self.items.len() {
                        let step = 5.min(self.items.len().saturating_sub(1) - self.selected);
                        self.selected += step;
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
        for (idx, item) in view.items.iter().enumerate() {
            if idx < view.offset {
                continue;
            }
            if lines.len() >= height {
                break;
            }
            let marker = if self.focused && idx == view.selected {
                "> "
            } else if idx == view.selected {
                "* "
            } else {
                "  "
            };
            lines.push(format!("{marker}{item}"));
        }
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

impl Renderable for ListView {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct DataTable {
    id: WidgetId,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    selected: usize,
    offset: usize,
    focused: bool,
    styles: WidgetStyles,
}

impl DataTable {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self {
            id: WidgetId::new(),
            headers,
            rows,
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
        if self.rows.is_empty() {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(self.rows.len() - 1);
    }

    fn ensure_visible(&mut self, height: usize) {
        if self.rows.is_empty() || height == 0 {
            self.offset = 0;
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }
}

impl Widget for DataTable {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
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
                if self.selected + 1 < self.rows.len() {
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
                if self.selected + 1 < self.rows.len() {
                    let step = 5.min(self.rows.len().saturating_sub(1) - self.selected);
                    self.selected += step;
                }
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
                    if self.selected + 1 < self.rows.len() {
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
                    if self.selected + 1 < self.rows.len() {
                        let step = 5.min(self.rows.len().saturating_sub(1) - self.selected);
                        self.selected += step;
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
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut view = self.clone();
        view.ensure_visible(height.saturating_sub(1));

        let col_count = self.headers.len().max(1);
        let sep_width = 3usize.saturating_mul(col_count.saturating_sub(1));

        // Natural widths based on headers + row values.
        let mut natural_widths: Vec<usize> = self
            .headers
            .iter()
            .map(|h| rich_rs::cell_len(h).max(3))
            .collect();
        for row in &self.rows {
            for (idx, value) in row.iter().enumerate() {
                if let Some(w) = natural_widths.get_mut(idx) {
                    *w = (*w).max(rich_rs::cell_len(value).max(1));
                }
            }
        }

        // Distribute remaining space (if any) to the first columns so the table fills the width.
        let natural_total: usize = natural_widths.iter().sum::<usize>().saturating_add(sep_width);
        let mut column_widths = natural_widths;
        if natural_total < width {
            let mut remaining = width - natural_total;
            let mut idx = 0usize;
            while remaining > 0 && idx < column_widths.len() {
                column_widths[idx] += remaining;
                remaining = 0;
                idx += 1;
            }
        }

        let header = self
            .headers
            .iter()
            .enumerate()
            .map(|(idx, col)| {
                let col_width = *column_widths.get(idx).unwrap_or(&3);
                rich_rs::set_cell_size(col, col_width)
            })
            .collect::<Vec<_>>()
            .join(" | ");

        let header_line = rich_rs::set_cell_size(&header, width);
        let mut lines = vec![header_line];
        if col_count >= 2 {
            let left = (*column_widths.get(0).unwrap_or(&3)).min(width);
            lines.push(format!(
                "{}+{}",
                "-".repeat(left),
                "-".repeat(width.saturating_sub(left + 1))
            ));
        } else {
            lines.push("-".repeat(width));
        }

        for (idx, row) in view.rows.iter().enumerate() {
            if idx < view.offset {
                continue;
            }
            if lines.len() >= height {
                break;
            }
            let mut parts = Vec::new();
            for (col_idx, value) in row.iter().enumerate() {
                let col_width = *column_widths.get(col_idx).unwrap_or(&3);
                let value = if col_idx == 0 && self.focused && idx == view.selected {
                    format!("> {value}")
                } else {
                    value.to_string()
                };
                parts.push(rich_rs::set_cell_size(&value, col_width));
            }
            let line = parts.join(" | ");
            lines.push(rich_rs::set_cell_size(&line, width));
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

impl Renderable for DataTable {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

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

pub struct Tabs {
    id: WidgetId,
    tabs: Vec<Tab>,
    active: usize,
    focused: bool,
    styles: WidgetStyles,
}

pub struct Tab {
    title: String,
    child: Box<dyn Widget>,
}

impl Tabs {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            tabs: Vec::new(),
            active: 0,
            focused: false,
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_tab(mut self, title: impl Into<String>, child: impl Widget + 'static) -> Self {
        self.tabs.push(Tab {
            title: title.into(),
            child: Box::new(child),
        });
        self
    }

    pub fn add_tab(&mut self, title: impl Into<String>, child: impl Widget + 'static) {
        self.tabs.push(Tab {
            title: title.into(),
            child: Box::new(child),
        });
    }

    pub fn active(&self) -> usize {
        self.active
    }

    pub fn set_active(&mut self, index: usize) {
        if self.tabs.is_empty() {
            self.active = 0;
            return;
        }
        let next = index.min(self.tabs.len() - 1);
        if next != self.active {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.child.set_focus(false);
            }
            self.active = next;
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.child.set_focus(true);
            }
        }
    }

    pub fn activate_prev(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let prev = if self.active == 0 {
            self.tabs.len() - 1
        } else {
            self.active - 1
        };
        self.set_active(prev);
    }

    pub fn activate_next(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let next = (self.active + 1) % self.tabs.len();
        self.set_active(next);
    }
}

impl Widget for Tabs {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.set_focus(focused);
        }
    }

    fn on_mount(&mut self) {
        for tab in &mut self.tabs {
            tab.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        for tab in &mut self.tabs {
            tab.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.focused {
            if let Event::Key(key) = event {
                match key.code {
                    KeyCode::Left => {
                        self.activate_prev();
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Right => {
                        self.activate_next();
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Char('h') => {
                        self.activate_prev();
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Char('l') => {
                        self.activate_next();
                        ctx.set_handled();
                        return;
                    }
                    _ => {}
                }
            }
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_event(event, ctx);
        }
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        for tab in &mut self.tabs {
            f(tab.child.as_mut());
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let header = if self.tabs.is_empty() {
            "no tabs".to_string()
        } else {
            let mut parts = Vec::new();
            for (idx, tab) in self.tabs.iter().enumerate() {
                if idx == self.active {
                    parts.push(format!("[{}]", tab.title));
                } else {
                    parts.push(format!(" {} ", tab.title));
                }
            }
            parts.join(" ")
        };
        let header_line = rich_rs::set_cell_size(&header, width);
        let header_segments = Text::plain(header_line).render(console, options);
        let mut lines = Segment::split_and_crop_lines(header_segments, width, None, true, false);
        lines = Segment::set_shape(&lines, width, Some(1), None, false);

        if height > 1 {
            if let Some(tab) = self.tabs.get(self.active) {
                let mut child_options = options.clone();
                child_options.size = (width, height - 1);
                child_options.max_width = width;
                child_options.max_height = height - 1;
                let child_segments = tab.child.render_styled(console, &child_options);
                let mut child_lines =
                    Segment::split_and_crop_lines(child_segments, width, None, true, false);
                child_lines =
                    Segment::set_shape(&child_lines, width, Some(height - 1), None, false);
                lines.extend(child_lines);
            }
        }

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        let child_height = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.child.layout_height());
        child_height.map(|height| height + 1)
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

impl Renderable for Tabs {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Checkbox {
    id: WidgetId,
    label: String,
    checked: bool,
    focused: bool,
    styles: WidgetStyles,
}

impl Checkbox {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            checked: false,
            focused: false,
            styles: WidgetStyles::default(),
        }
    }

    pub fn checked(&self) -> bool {
        self.checked
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }
}

impl Widget for Checkbox {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.focused {
            return;
        }
        if let Event::Action(Action::Toggle) = event {
            self.checked = !self.checked;
            ctx.set_handled();
            return;
        }
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.checked = !self.checked;
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let marker = if self.focused { "> " } else { "  " };
        let state = if self.checked { "[x]" } else { "[ ]" };
        let text = Text::plain(format!("{marker}{state} {}", self.label));
        text.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
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

impl Renderable for Checkbox {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Spacer {
    id: WidgetId,
    height: usize,
    styles: WidgetStyles,
}

impl Spacer {
    pub fn new(height: usize) -> Self {
        Self {
            id: WidgetId::new(),
            height: height.max(1),
            styles: WidgetStyles::default(),
        }
    }
}

impl Widget for Spacer {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let line = " ".repeat(width);
        let mut out = Segments::new();
        for idx in 0..self.height {
            out.push(Segment::new(line.clone()));
            if idx + 1 < self.height {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(self.height))
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Spacer {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Input {
    id: WidgetId,
    text: String,
    cursor: usize,
    focused: bool,
    placeholder: Option<String>,
    styles: WidgetStyles,
}

impl Input {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            text: String::new(),
            cursor: 0,
            focused: false,
            placeholder: None,
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_placeholder(mut self, value: impl Into<String>) -> Self {
        self.placeholder = Some(value.into());
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, value: impl Into<String>) {
        self.text = value.into();
        if self.cursor > self.text.len() {
            self.cursor = self.text.len();
        }
    }
}

impl Widget for Input {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.focused {
            return;
        }
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char(ch) => {
                    self.text.insert(self.cursor, ch);
                    self.cursor += 1;
                    ctx.set_handled();
                }
                KeyCode::Backspace => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        self.text.remove(self.cursor);
                        ctx.set_handled();
                    }
                }
                KeyCode::Delete => {
                    if self.cursor < self.text.len() {
                        self.text.remove(self.cursor);
                        ctx.set_handled();
                    }
                }
                KeyCode::Left => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        ctx.set_handled();
                    }
                }
                KeyCode::Right => {
                    if self.cursor < self.text.len() {
                        self.cursor += 1;
                        ctx.set_handled();
                    }
                }
                KeyCode::Home => {
                    self.cursor = 0;
                    ctx.set_handled();
                }
                KeyCode::End => {
                    self.cursor = self.text.len();
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let marker = if self.focused { "> " } else { "  " };
        let content = if self.text.is_empty() {
            self.placeholder.clone().unwrap_or_default()
        } else {
            self.text.clone()
        };
        let text = Text::plain(format!("{marker}{content}"));
        text.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
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

impl Renderable for Input {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
