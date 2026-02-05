use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};

use super::style_selectors;
use crate::debug::debug_input;
use crate::event::{Action, Event, EventCtx};
use crate::style::parse_color_like;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints, focused_classes},
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Default,
    Primary,
    Success,
    Warning,
    Error,
}

#[derive(Clone)]
pub struct Button {
    id: WidgetId,
    label: String,
    focused: bool,
    hovered: bool,
    pressed: PressedState,
    variant: ButtonVariant,
    disabled: bool,
    flat: bool,
    on_press: Option<Arc<dyn Fn(&Button) + Send + Sync>>,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PressedState {
    #[default]
    None,
    Mouse,
    KeyboardPending,
    KeyboardUntil(u64),
}

impl std::fmt::Debug for Button {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Button")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("focused", &self.focused)
            .field("hovered", &self.hovered)
            .field("pressed", &(self.pressed != PressedState::None))
            .field("variant", &self.variant)
            .field("disabled", &self.disabled)
            .field("flat", &self.flat)
            .field("classes", &self.classes)
            .finish()
    }
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            focused: false,
            hovered: false,
            pressed: PressedState::None,
            variant: ButtonVariant::Default,
            disabled: false,
            flat: false,
            on_press: None,
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
        self.pressed != PressedState::None
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

    pub fn on_press(mut self, handler: impl Fn(&Button) + Send + Sync + 'static) -> Self {
        self.on_press = Some(Arc::new(handler));
        self
    }

    pub fn describe(&self) -> String {
        let mut classes = self.classes.clone();
        let is_active = match self.pressed {
            PressedState::None => false,
            PressedState::Mouse => self.hovered,
            _ => true,
        };
        if is_active {
            classes.push("-active".to_string());
        }
        let class_str = classes.join(" ");
        let variant = match self.variant {
            ButtonVariant::Default => "default",
            ButtonVariant::Primary => "primary",
            ButtonVariant::Success => "success",
            ButtonVariant::Warning => "warning",
            ButtonVariant::Error => "error",
        };
        format!("Button(classes='{}', variant='{}')", class_str, variant)
    }

    fn rebuild_classes(mut self) -> Self {
        // Mirror Textual's class naming conventions where practical, but keep our legacy
        // class names around so existing demos keep working.
        let mut classes = vec!["button".to_string()];
        if self.flat {
            classes.push("flat".to_string());
            classes.push("-style-flat".to_string());
        } else {
            classes.push("-style-default".to_string());
        }
        match self.variant {
            ButtonVariant::Primary => {
                classes.push("primary".to_string());
                classes.push("-primary".to_string());
            }
            ButtonVariant::Success => {
                classes.push("success".to_string());
                classes.push("-success".to_string());
            }
            ButtonVariant::Warning => {
                classes.push("warning".to_string());
                classes.push("-warning".to_string());
            }
            ButtonVariant::Error => {
                classes.push("error".to_string());
                classes.push("-error".to_string());
            }
            ButtonVariant::Default => {}
        }
        if self.disabled {
            classes.push("disabled".to_string());
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

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn mouse_interactive(&self) -> bool {
        // Buttons should still get hover affordances even when disabled.
        true
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn is_active(&self) -> bool {
        match self.pressed {
            PressedState::None => false,
            PressedState::Mouse => self.hovered,
            _ => true,
        }
    }

    fn content_width(&self) -> Option<usize> {
        // Match Textual's default behavior: content width is label width + a small padding.
        Some(rich_rs::cell_len(&self.label).saturating_add(2).max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        match event {
            Event::MouseDown(target, _, _) if *target == self.id => {
                self.pressed = PressedState::Mouse;
                debug_input(&format!(
                    "[button] mouse id={} label=\"{}\"",
                    self.id.as_u64(),
                    self.label
                ));
                ctx.set_handled();
            }
            Event::MouseUp(target, _, _) => {
                if self.pressed == PressedState::Mouse {
                    self.pressed = PressedState::None;
                    // Activate only on click (mouse released while still over the button).
                    if *target == Some(self.id) {
                        if let Some(handler) = &self.on_press {
                            handler(self);
                        }
                        ctx.set_handled();
                    }
                }
            }
            Event::Action(Action::Toggle) if self.focused => {
                self.pressed = PressedState::KeyboardPending;
                if let Some(handler) = &self.on_press {
                    handler(self);
                }
                debug_input(&format!(
                    "[button] toggle id={} label=\"{}\"",
                    self.id.as_u64(),
                    self.label
                ));
                ctx.set_handled();
            }
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.pressed = PressedState::KeyboardPending;
                    if let Some(handler) = &self.on_press {
                        handler(self);
                    }
                    debug_input(&format!(
                        "[button] key id={} label=\"{}\"",
                        self.id.as_u64(),
                        self.label
                    ));
                    ctx.set_handled();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn on_tick(&mut self, tick: u64) {
        match self.pressed {
            PressedState::KeyboardPending => {
                self.pressed = PressedState::KeyboardUntil(tick + 2);
            }
            PressedState::KeyboardUntil(expire) if tick >= expire => {
                self.pressed = PressedState::None;
            }
            _ => {}
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let label = self.label.as_str();
        let label_width = rich_rs::cell_len(label).min(width);
        let left = width.saturating_sub(label_width) / 2;
        let right = width.saturating_sub(label_width) - left;
        let line = format!(
            "{}{}{}",
            " ".repeat(left),
            rich_rs::set_cell_size(label, label_width),
            " ".repeat(right)
        );
        let mut out = Segments::new();
        out.push(Segment::new(line));
        out
    }

    fn layout_height(&self) -> Option<usize> {
        let meta = style_selectors::selector_meta_generic(self);
        let base_style = style_selectors::resolve_style(self, &meta);
        let default_height = 1 + super::helpers::border_vertical_padding(&base_style);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorType {
    Cell,
    Row,
    Column,
    None,
}

#[derive(Debug, Clone)]
pub struct DataTable {
    id: WidgetId,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    selected: usize,
    offset: usize,
    cursor_column: usize,
    cursor_type: CursorType,
    focused: bool,
    hovered: bool,
    hover_coordinate: Option<(usize, usize)>,
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
            cursor_column: 0,
            cursor_type: CursorType::Cell,
            focused: false,
            hovered: false,
            hover_coordinate: None,
            styles: WidgetStyles::default(),
        }
    }

    /// Create an empty table (columns and rows added later).
    pub fn empty() -> Self {
        Self {
            id: WidgetId::new(),
            headers: Vec::new(),
            rows: Vec::new(),
            selected: 0,
            offset: 0,
            cursor_column: 0,
            cursor_type: CursorType::Cell,
            focused: false,
            hovered: false,
            hover_coordinate: None,
            styles: WidgetStyles::default(),
        }
    }

    pub fn add_columns(&mut self, columns: &[&str]) {
        for col in columns {
            self.headers.push((*col).to_string());
        }
    }

    pub fn add_rows(&mut self, rows: &[&[&str]]) {
        for row in rows {
            self.rows
                .push(row.iter().map(|s| (*s).to_string()).collect());
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

    pub fn cursor_type(mut self, ct: CursorType) -> Self {
        self.cursor_type = ct;
        self
    }

    /// Compute column widths (used by render and click-to-column).
    fn column_widths(&self) -> Vec<usize> {
        let mut widths: Vec<usize> = self
            .headers
            .iter()
            .map(|h| rich_rs::cell_len(h).max(3))
            .collect();
        for row in &self.rows {
            for (idx, value) in row.iter().enumerate() {
                if let Some(w) = widths.get_mut(idx) {
                    *w = (*w).max(rich_rs::cell_len(value).max(1));
                }
            }
        }
        widths
    }

    /// Map an x coordinate to a column index, given column widths with 2-space gap.
    fn column_at_x(&self, x: usize, column_widths: &[usize]) -> usize {
        let mut pos = 0;
        for (i, &w) in column_widths.iter().enumerate() {
            let end = pos + w;
            if x < end {
                return i;
            }
            pos = end + 2; // 2-space gap between columns
        }
        column_widths.len().saturating_sub(1)
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

    fn mouse_interactive(&self) -> bool {
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
            self.hover_coordinate = None;
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) {
        let widths = self.column_widths();
        let col_idx = self.column_at_x(x as usize, &widths);
        if y == 0 {
            // Header row — use usize::MAX as sentinel (mirrors Textual's row_index=-1).
            self.hover_coordinate = Some((usize::MAX, col_idx));
        } else {
            let row_idx = (y as usize - 1) + self.offset;
            if row_idx < self.rows.len() {
                self.hover_coordinate = Some((row_idx, col_idx));
            } else {
                self.hover_coordinate = None;
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        // Handle mouse events regardless of focus state.
        match event {
            Event::MouseDown(target, x, y) if *target == self.id => {
                let row_y = (*y as usize).saturating_sub(1);
                let clicked = row_y + self.offset;
                if clicked < self.rows.len() {
                    self.selected = clicked;
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                        let widths = self.column_widths();
                        self.cursor_column = self.column_at_x(*x as usize, &widths);
                    }
                }
                ctx.set_handled();
                return;
            }
            _ => {}
        }

        if !self.focused {
            return;
        }
        let mut handled = false;
        match event {
            Event::Action(Action::ScrollUp) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollDown) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                    if self.selected + 1 < self.rows.len() {
                        self.selected += 1;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollLeft) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                    if self.cursor_column > 0 {
                        self.cursor_column -= 1;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollRight) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                    if self.cursor_column + 1 < self.headers.len() {
                        self.cursor_column += 1;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollPageUp) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                    if self.selected > 0 {
                        let step = 5.min(self.selected);
                        self.selected -= step;
                    }
                    handled = true;
                }
            }
            Event::Action(Action::ScrollPageDown) => {
                if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                    if self.selected + 1 < self.rows.len() {
                        let step = 5.min(self.rows.len().saturating_sub(1) - self.selected);
                        self.selected += step;
                    }
                    handled = true;
                }
            }
            Event::Key(key) => match key.code {
                KeyCode::Up => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                        if self.selected > 0 {
                            self.selected -= 1;
                        }
                        handled = true;
                    }
                }
                KeyCode::Down => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                        if self.selected + 1 < self.rows.len() {
                            self.selected += 1;
                        }
                        handled = true;
                    }
                }
                KeyCode::Left => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                        if self.cursor_column > 0 {
                            self.cursor_column -= 1;
                        }
                        handled = true;
                    }
                }
                KeyCode::Right => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Column) {
                        if self.cursor_column + 1 < self.headers.len() {
                            self.cursor_column += 1;
                        }
                        handled = true;
                    }
                }
                KeyCode::PageUp => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                        if self.selected > 0 {
                            let step = 5.min(self.selected);
                            self.selected -= step;
                        }
                        handled = true;
                    }
                }
                KeyCode::PageDown => {
                    if matches!(self.cursor_type, CursorType::Cell | CursorType::Row) {
                        if self.selected + 1 < self.rows.len() {
                            let step = 5.min(self.rows.len().saturating_sub(1) - self.selected);
                            self.selected += step;
                        }
                        handled = true;
                    }
                }
                _ => {}
            },
            _ => {}
        }
        if handled {
            ctx.set_handled();
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut view = self.clone();
        view.ensure_visible(height.saturating_sub(1));

        let column_widths = self.column_widths();
        let cursor_type = self.cursor_type;
        let show_cursor = self.focused && cursor_type != CursorType::None;

        // Cursor and hover coordinates.
        let cursor_coord = (view.selected, self.cursor_column);
        let hover_coord = self.hover_coordinate;

        // Resolve theme colors.
        let header_bg = parse_color_like("$panel");
        let row_bg = parse_color_like("$surface");
        let cursor_bg = parse_color_like("$primary");
        let hover_bg = parse_color_like("$block-hover-background");
        let header_hover_bg = parse_color_like("$header-hover-background");

        let mut header_style = rich_rs::Style::new().with_bold(true);
        if let Some(bg) = header_bg {
            header_style = header_style.with_bgcolor(bg);
        }
        let mut normal_style = rich_rs::Style::new();
        if let Some(bg) = row_bg {
            normal_style = normal_style.with_bgcolor(bg);
        }
        let mut selected_style = rich_rs::Style::new().with_bold(true);
        if let Some(bg) = cursor_bg {
            selected_style = selected_style.with_bgcolor(bg);
        }
        let mut hover_style = rich_rs::Style::new();
        if let Some(bg) = hover_bg {
            hover_style = hover_style.with_bgcolor(bg);
        }
        let mut header_hover_style = rich_rs::Style::new().with_bold(true);
        if let Some(bg) = header_hover_bg {
            header_hover_style = header_hover_style.with_bgcolor(bg);
        }

        let mut out = Segments::new();

        // Helper: join column values with 2-space padding, pad to full width.
        let format_row_uniform =
            |values: &[String], widths: &[usize], total_width: usize| -> String {
                let parts: Vec<String> = values
                    .iter()
                    .enumerate()
                    .map(|(i, val)| {
                        let col_w = *widths.get(i).unwrap_or(&3);
                        rich_rs::set_cell_size(val, col_w)
                    })
                    .collect();
                let joined = parts.join("  ");
                rich_rs::set_cell_size(&joined, total_width)
            };

        // Mirrors Textual's _should_highlight: does `target` match `cursor` given the type?
        let should_highlight =
            |cursor: (usize, usize), target: (usize, usize), ct: CursorType| -> bool {
                match ct {
                    CursorType::Cell => cursor == target,
                    CursorType::Row => cursor.0 == target.0,
                    CursorType::Column => cursor.1 == target.1,
                    CursorType::None => false,
                }
            };

        // Header line.
        // Headers use usize::MAX as their row sentinel (mirroring Textual's row=-1).
        // Per-cell rendering is needed when cursor or hover could highlight individual
        // header cells (Column cursor always, Cell/Column hover when on the header row).
        let header_needs_per_cell = (show_cursor
            && matches!(cursor_type, CursorType::Column))
            || (hover_coord.is_some() && cursor_type != CursorType::None);

        if header_needs_per_cell {
            emit_row_per_cell(
                &self.headers,
                &column_widths,
                width,
                |col_idx| {
                    let target = (usize::MAX, col_idx);
                    if show_cursor && should_highlight(cursor_coord, target, cursor_type) {
                        return selected_style.with_bold(true);
                    }
                    if let Some(hc) = hover_coord {
                        if should_highlight(hc, target, cursor_type) {
                            return header_hover_style;
                        }
                    }
                    header_style
                },
                header_style,
                &mut out,
            );
        } else {
            let header_text = format_row_uniform(&self.headers, &column_widths, width);
            out.push(Segment::styled(header_text, header_style));
        }
        out.push(Segment::line());
        let mut lines_used = 1usize;

        // Data rows.
        for (idx, row) in view.rows.iter().enumerate() {
            if idx < view.offset {
                continue;
            }
            if lines_used >= height {
                break;
            }

            // Check if any cell in this row needs per-cell styling.
            let row_has_cursor = show_cursor
                && match cursor_type {
                    CursorType::Row => idx == cursor_coord.0,
                    CursorType::Cell => idx == cursor_coord.0,
                    CursorType::Column => true, // every row has the highlighted column
                    CursorType::None => false,
                };
            let row_has_hover = hover_coord.is_some()
                && match cursor_type {
                    CursorType::Row => hover_coord.map(|h| h.0) == Some(idx),
                    CursorType::Cell => hover_coord.map(|h| h.0) == Some(idx),
                    CursorType::Column => true,
                    CursorType::None => false,
                };

            let needs_per_cell = (row_has_cursor
                && matches!(cursor_type, CursorType::Cell | CursorType::Column))
                || (row_has_hover
                    && matches!(cursor_type, CursorType::Cell | CursorType::Column));

            if needs_per_cell {
                emit_row_per_cell(
                    row,
                    &column_widths,
                    width,
                    |col_idx| {
                        let target = (idx, col_idx);
                        if show_cursor && should_highlight(cursor_coord, target, cursor_type) {
                            return selected_style;
                        }
                        if let Some(hc) = hover_coord {
                            if should_highlight(hc, target, cursor_type) {
                                return hover_style;
                            }
                        }
                        normal_style
                    },
                    normal_style,
                    &mut out,
                );
            } else {
                // Uniform style for the whole row.
                let style = if show_cursor
                    && should_highlight(cursor_coord, (idx, 0), cursor_type)
                {
                    selected_style
                } else if let Some(hc) = hover_coord {
                    if should_highlight(hc, (idx, 0), cursor_type) {
                        hover_style
                    } else {
                        normal_style
                    }
                } else {
                    normal_style
                };
                let row_text = format_row_uniform(row, &column_widths, width);
                out.push(Segment::styled(row_text, style));
            }

            out.push(Segment::line());
            lines_used += 1;
        }

        out
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

/// Emit a row where each cell can have a different style determined by `style_for_col`.
fn emit_row_per_cell(
    values: &[String],
    column_widths: &[usize],
    total_width: usize,
    style_for_col: impl Fn(usize) -> rich_rs::Style,
    gap_style: rich_rs::Style,
    out: &mut Segments,
) {
    let mut used = 0usize;
    for (i, val) in values.iter().enumerate() {
        if i > 0 {
            out.push(Segment::styled("  ", gap_style));
            used += 2;
        }
        let col_w = *column_widths.get(i).unwrap_or(&3);
        let cell_text = rich_rs::set_cell_size(val, col_w);
        out.push(Segment::styled(cell_text, style_for_col(i)));
        used += col_w;
    }
    // Pad remainder to full width.
    if used < total_width {
        out.push(Segment::styled(" ".repeat(total_width - used), gap_style));
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

    fn has_focus(&self) -> bool {
        self.focused
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

    fn has_focus(&self) -> bool {
        self.focused
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

    fn has_focus(&self) -> bool {
        self.focused
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
