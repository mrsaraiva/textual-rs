use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::Message;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints},
};

/// A single item within an [`OptionList`].
///
/// Can be a selectable option (with text, optional string ID, and optional disabled flag)
/// or a visual separator rendered as a horizontal rule.
#[derive(Debug, Clone)]
pub enum OptionItem {
    /// A selectable option.
    Option {
        prompt: String,
        id: Option<String>,
        disabled: bool,
    },
    /// A visual separator (horizontal rule) between option groups.
    Separator,
}

impl OptionItem {
    /// Create a new enabled option with the given prompt text.
    pub fn new(prompt: impl Into<String>) -> Self {
        Self::Option {
            prompt: prompt.into(),
            id: None,
            disabled: false,
        }
    }

    /// Create a new option with prompt and string ID.
    pub fn with_id(prompt: impl Into<String>, id: impl Into<String>) -> Self {
        Self::Option {
            prompt: prompt.into(),
            id: Some(id.into()),
            disabled: false,
        }
    }

    /// Create a disabled option.
    pub fn disabled(prompt: impl Into<String>) -> Self {
        Self::Option {
            prompt: prompt.into(),
            id: None,
            disabled: true,
        }
    }

    /// Create a disabled option with an ID.
    pub fn disabled_with_id(prompt: impl Into<String>, id: impl Into<String>) -> Self {
        Self::Option {
            prompt: prompt.into(),
            id: Some(id.into()),
            disabled: true,
        }
    }

    /// Returns `true` if this item is a separator.
    pub fn is_separator(&self) -> bool {
        matches!(self, Self::Separator)
    }

    /// Returns `true` if this item is a disabled option.
    pub fn is_disabled(&self) -> bool {
        matches!(self, Self::Option { disabled: true, .. })
    }

    /// Returns the prompt text, or `None` for separators.
    pub fn prompt(&self) -> Option<&str> {
        match self {
            Self::Option { prompt, .. } => Some(prompt),
            Self::Separator => None,
        }
    }

    /// Returns the string ID, or `None` for separators or options without an ID.
    pub fn string_id(&self) -> Option<&str> {
        match self {
            Self::Option { id, .. } => id.as_deref(),
            Self::Separator => None,
        }
    }
}

/// A scrollable, navigable list of selectable options.
///
/// Supports separators between groups, disabled items, keyboard and mouse navigation,
/// and emits [`Message::OptionHighlighted`] / [`Message::OptionSelected`] messages.
#[derive(Debug, Clone)]
pub struct OptionList {
    id: WidgetId,
    items: Vec<OptionItem>,
    /// Index of the highlighted (cursor) item, or `None` if no items.
    highlighted: Option<usize>,
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

impl OptionList {
    /// Create an empty `OptionList`.
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            items: Vec::new(),
            highlighted: None,
            offset: 0,
            focused: false,
            hovered: false,
            hovered_index: None,
            viewport_height: 1,
            scroll_step: 1,
            classes: vec!["option-list".to_string()],
            focused_classes: vec!["option-list".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    /// Create an `OptionList` pre-populated with items.
    pub fn with_items(items: Vec<OptionItem>) -> Self {
        let mut list = Self::new();
        list.items = items;
        list.highlighted = list.first_selectable();
        list
    }

    /// Builder: set the scroll step (number of rows per scroll tick).
    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    // ── Public API ──────────────────────────────────────────────────

    /// Add a selectable option.
    pub fn add_option(&mut self, prompt: impl Into<String>, id: Option<String>, disabled: bool) {
        let was_empty = self.highlighted.is_none();
        self.items.push(OptionItem::Option {
            prompt: prompt.into(),
            id,
            disabled,
        });
        if was_empty && !disabled {
            self.highlighted = Some(self.items.len() - 1);
        }
    }

    /// Add a visual separator.
    pub fn add_separator(&mut self) {
        self.items.push(OptionItem::Separator);
    }

    /// Remove all items from the list.
    pub fn clear_options(&mut self) {
        self.items.clear();
        self.highlighted = None;
        self.offset = 0;
        self.hovered_index = None;
    }

    /// Number of items (including separators).
    pub fn option_count(&self) -> usize {
        self.items.len()
    }

    /// Get a reference to an item by index.
    pub fn get_option(&self, index: usize) -> Option<&OptionItem> {
        self.items.get(index)
    }

    /// The currently highlighted index, or `None`.
    pub fn highlighted(&self) -> Option<usize> {
        self.highlighted
    }

    /// The current scroll offset (first visible item index).
    pub fn offset_for_click(&self) -> usize {
        self.offset
    }

    /// Programmatically move the highlight to `index`.
    /// Ignores separators and disabled items.
    pub fn set_highlighted(&mut self, index: usize) {
        if index < self.items.len()
            && !self.items[index].is_separator()
            && !self.items[index].is_disabled()
        {
            self.highlighted = Some(index);
            self.ensure_visible();
        }
    }

    /// Replace all items at once.
    pub fn set_items(&mut self, items: Vec<OptionItem>) {
        self.items = items;
        self.highlighted = self.first_selectable();
        self.offset = 0;
        self.hovered_index = None;
        self.ensure_visible();
    }

    // ── Internals ───────────────────────────────────────────────────

    fn first_selectable(&self) -> Option<usize> {
        self.items
            .iter()
            .position(|item| !item.is_separator() && !item.is_disabled())
    }

    fn last_selectable(&self) -> Option<usize> {
        self.items
            .iter()
            .enumerate()
            .rev()
            .find(|(_, item)| !item.is_separator() && !item.is_disabled())
            .map(|(i, _)| i)
    }

    /// Total number of *selectable* items (excludes separators and disabled items).
    fn selectable_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| !item.is_separator() && !item.is_disabled())
            .count()
    }

    fn max_offset(&self) -> usize {
        self.items.len().saturating_sub(self.viewport_height.max(1))
    }

    fn clamp_offsets(&mut self) {
        if self.items.is_empty() {
            self.highlighted = None;
            self.offset = 0;
            self.hovered_index = None;
            return;
        }
        self.offset = self.offset.min(self.max_offset());
        if let Some(index) = self.hovered_index {
            if index >= self.items.len() {
                self.hovered_index = None;
            }
        }
    }

    fn ensure_visible(&mut self) {
        self.clamp_offsets();
        let Some(highlighted) = self.highlighted else {
            return;
        };
        let viewport = self.viewport_height.max(1);
        if highlighted < self.offset {
            self.offset = highlighted;
        } else if highlighted >= self.offset + viewport {
            self.offset = highlighted + 1 - viewport;
        }
        self.offset = self.offset.min(self.max_offset());
    }

    fn emit_highlighted(&self, ctx: &mut EventCtx) {
        if let Some(index) = self.highlighted {
            ctx.post_message(self.id, Message::OptionHighlighted { index });
        }
    }

    fn emit_selected(&self, ctx: &mut EventCtx) {
        if let Some(index) = self.highlighted {
            ctx.post_message(self.id, Message::OptionSelected { index });
        }
    }

    /// Move highlight to a specific index. Skips separators and disabled items.
    fn highlight_index(&mut self, index: usize, ctx: &mut EventCtx) {
        if index >= self.items.len() {
            return;
        }
        if self.items[index].is_separator() || self.items[index].is_disabled() {
            return;
        }
        let changed = self.highlighted != Some(index);
        self.highlighted = Some(index);
        self.ensure_visible();
        if changed {
            self.emit_highlighted(ctx);
            ctx.request_repaint();
        }
    }

    /// Move highlight by `delta`, skipping separators and disabled items.
    fn move_highlight(&mut self, delta: isize, ctx: &mut EventCtx) {
        if self.selectable_count() == 0 {
            return;
        }
        let current = self.highlighted.unwrap_or(0) as isize;
        let max = (self.items.len() - 1) as isize;
        let mut target = (current + delta).clamp(0, max) as usize;

        // Walk in the direction of delta to find the next selectable item.
        let step: isize = if delta >= 0 { 1 } else { -1 };
        while target < self.items.len()
            && (self.items[target].is_separator() || self.items[target].is_disabled())
        {
            let next = target as isize + step;
            if next < 0 || next > max {
                // Can't move further; stay at current position.
                return;
            }
            target = next as usize;
        }
        self.highlight_index(target, ctx);
    }

    fn page_step(&self) -> usize {
        self.viewport_height.saturating_sub(1).max(1)
    }

    fn scroll_offset(&mut self, delta_rows: isize, ctx: &mut EventCtx) {
        let before = self.offset;
        if delta_rows.is_negative() {
            self.offset = self.offset.saturating_sub(delta_rows.unsigned_abs());
        } else {
            self.offset = self.offset.saturating_add(delta_rows as usize);
        }
        self.offset = self.offset.min(self.max_offset());
        if self.offset != before {
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    /// Confirm the currently highlighted item (Enter or click).
    fn confirm_selection(&mut self, ctx: &mut EventCtx) {
        let Some(index) = self.highlighted else {
            return;
        };
        if self.items[index].is_separator() || self.items[index].is_disabled() {
            return;
        }
        self.emit_selected(ctx);
    }
}

impl Widget for OptionList {
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
                let index = self.offset.saturating_add(mouse.y as usize);
                if index < self.items.len()
                    && !self.items[index].is_separator()
                    && !self.items[index].is_disabled()
                {
                    self.highlight_index(index, ctx);
                    self.confirm_selection(ctx);
                    ctx.set_handled();
                }
            }
            Event::Action(action) if self.focused => match action {
                Action::ScrollUp => {
                    self.move_highlight(-1, ctx);
                    ctx.set_handled();
                }
                Action::ScrollDown => {
                    self.move_highlight(1, ctx);
                    ctx.set_handled();
                }
                Action::ScrollPageUp => {
                    self.move_highlight(-(self.page_step() as isize), ctx);
                    ctx.set_handled();
                }
                Action::ScrollPageDown => {
                    self.move_highlight(self.page_step() as isize, ctx);
                    ctx.set_handled();
                }
                _ => {}
            },
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Up => {
                    self.move_highlight(-1, ctx);
                    ctx.set_handled();
                }
                KeyCode::Down => {
                    self.move_highlight(1, ctx);
                    ctx.set_handled();
                }
                KeyCode::PageUp => {
                    self.move_highlight(-(self.page_step() as isize), ctx);
                    ctx.set_handled();
                }
                KeyCode::PageDown => {
                    self.move_highlight(self.page_step() as isize, ctx);
                    ctx.set_handled();
                }
                KeyCode::Home => {
                    if let Some(first) = self.first_selectable() {
                        self.highlight_index(first, ctx);
                    }
                    ctx.set_handled();
                }
                KeyCode::End => {
                    if let Some(last) = self.last_selectable() {
                        self.highlight_index(last, ctx);
                    }
                    ctx.set_handled();
                }
                KeyCode::Enter => {
                    self.confirm_selection(ctx);
                    ctx.set_handled();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn on_mouse_move(&mut self, _x: u16, y: u16) -> bool {
        if self.items.is_empty() {
            return false;
        }
        let index = self.offset.saturating_add(y as usize);
        let hovered = if index < self.items.len() && !self.items[index].is_separator() {
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
        let mut out = Segments::new();

        let base_style = crate::css::resolve_component_style(self, &["option-list--option"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        for row in 0..height {
            let index = self.offset + row;
            let mut text = String::new();
            let mut style = base_style;

            if let Some(item) = self.items.get(index) {
                match item {
                    OptionItem::Separator => {
                        let sep_style =
                            crate::css::resolve_component_style(self, &["option-list--separator"])
                                .to_rich()
                                .unwrap_or(base_style);
                        text = "─".repeat(width);
                        style = sep_style;
                    }
                    OptionItem::Option {
                        prompt, disabled, ..
                    } => {
                        let highlighted = self.highlighted == Some(index);
                        let hovered = self.hovered_index == Some(index);
                        let mut classes = vec!["option-list--option"];
                        if highlighted {
                            classes.push("-highlighted");
                        }
                        if hovered && !highlighted {
                            classes.push("-hover");
                        }
                        if *disabled {
                            classes.push("-disabled");
                        }
                        if highlighted && self.focused {
                            classes.push("-focus");
                        }
                        style = crate::css::resolve_component_style(self, &classes)
                            .to_rich()
                            .unwrap_or(style);
                        text = format!("  {prompt}");
                    }
                }
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
        fixed_height_from_constraints(self.layout_constraints()).or(Some(self.items.len().max(1)))
    }

    fn content_width(&self) -> Option<usize> {
        let width = self
            .items
            .iter()
            .map(|item| match item {
                OptionItem::Option { prompt, .. } => rich_rs::cell_len(prompt).saturating_add(2),
                OptionItem::Separator => 3,
            })
            .max()
            .unwrap_or(2)
            .max(1);
        Some(width)
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

    fn style_type(&self) -> &'static str {
        "OptionList"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for OptionList {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_list_navigation_skips_separators() {
        let items = vec![
            OptionItem::new("Alpha"),
            OptionItem::Separator,
            OptionItem::new("Beta"),
        ];
        let mut list = OptionList::with_items(items);
        list.set_focus(true);
        list.on_layout(40, 10);

        assert_eq!(list.highlighted(), Some(0));

        let mut ctx = EventCtx::default();
        list.move_highlight(1, &mut ctx);
        // Should skip the separator and land on Beta (index 2).
        assert_eq!(list.highlighted(), Some(2));
    }

    #[test]
    fn option_list_navigation_skips_disabled() {
        let items = vec![
            OptionItem::new("Alpha"),
            OptionItem::disabled("Bravo"),
            OptionItem::new("Charlie"),
        ];
        let mut list = OptionList::with_items(items);
        list.set_focus(true);
        list.on_layout(40, 10);

        assert_eq!(list.highlighted(), Some(0));

        let mut ctx = EventCtx::default();
        list.move_highlight(1, &mut ctx);
        assert_eq!(list.highlighted(), Some(2));
    }

    #[test]
    fn option_list_home_end() {
        let items = vec![
            OptionItem::new("First"),
            OptionItem::new("Middle"),
            OptionItem::Separator,
            OptionItem::new("Last"),
        ];
        let mut list = OptionList::with_items(items);
        list.set_focus(true);
        list.on_layout(40, 10);

        // End goes to last selectable
        let mut ctx = EventCtx::default();
        if let Some(last) = list.last_selectable() {
            list.highlight_index(last, &mut ctx);
        }
        assert_eq!(list.highlighted(), Some(3));

        // Home goes to first selectable
        if let Some(first) = list.first_selectable() {
            list.highlight_index(first, &mut ctx);
        }
        assert_eq!(list.highlighted(), Some(0));
    }

    #[test]
    fn option_list_confirm_emits_selected() {
        let items = vec![OptionItem::new("Only")];
        let mut list = OptionList::with_items(items);
        list.set_focus(true);
        list.on_layout(40, 10);

        let mut ctx = EventCtx::default();
        list.confirm_selection(&mut ctx);
        let messages = ctx.take_messages();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m.message, Message::OptionSelected { index: 0 }))
        );
    }

    #[test]
    fn option_list_clear_resets_state() {
        let items = vec![OptionItem::new("A"), OptionItem::new("B")];
        let mut list = OptionList::with_items(items);
        list.set_highlighted(1);
        list.clear_options();

        assert_eq!(list.highlighted(), None);
        assert_eq!(list.option_count(), 0);
    }
}
