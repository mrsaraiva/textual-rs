use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::event::{Action, Event, EventCtx};
use crate::message::*;

use crate::node_id::NodeId;

use crate::action::ParsedAction;

use super::{
    BindingDecl, ScrollView, Widget, WidgetStyles,
    helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints},
};

#[derive(Debug, Clone)]
pub struct ListView {
    items: Vec<String>,
    disabled: Vec<bool>,
    selected: usize,
    offset: usize,
    focused: bool,
    hovered: bool,
    hovered_index: Option<usize>,
    pressed_index: Option<usize>,
    viewport_height: usize,
    scroll_step: usize,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl ListView {
    pub fn new(items: Vec<String>) -> Self {
        let len = items.len();
        Self {
            items,
            disabled: vec![false; len],
            selected: 0,
            offset: 0,
            focused: false,
            hovered: false,
            hovered_index: None,
            pressed_index: None,
            viewport_height: 1,
            scroll_step: 1,
            classes: vec!["list-view".to_string()],
            focused_classes: vec!["list-view".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_item(&self) -> Option<&str> {
        self.items.get(self.selected).map(String::as_str)
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn set_selected(&mut self, index: usize) {
        if self.selectable_count() == 0 {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        if let Some(next) = self.closest_selectable(index, 1) {
            self.selected = next;
        }
        self.ensure_visible();
    }

    pub fn set_items(&mut self, items: Vec<String>) {
        self.disabled = vec![false; items.len()];
        self.items = items;
        self.clamp_offsets();
        self.ensure_visible();
    }

    pub fn set_item_disabled(&mut self, index: usize, disabled: bool) {
        if index >= self.items.len() {
            return;
        }
        if index >= self.disabled.len() {
            self.disabled.resize(self.items.len(), false);
        }
        self.disabled[index] = disabled;
        self.clamp_offsets();
        self.ensure_visible();
    }

    pub fn is_item_disabled(&self, index: usize) -> bool {
        self.disabled.get(index).copied().unwrap_or(false)
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    /// Append an item to the end of the list.
    pub fn append(&mut self, item: String) {
        self.items.push(item);
        self.disabled.push(false);
    }

    /// Remove all items, resetting selection and scroll offset to 0.
    pub fn clear(&mut self) {
        self.items.clear();
        self.disabled.clear();
        self.selected = 0;
        self.offset = 0;
        self.hovered_index = None;
        self.pressed_index = None;
    }

    /// Remove the item at `index`, returning it if valid.
    ///
    /// Adjusts selected index and scroll offset so they remain valid.
    pub fn remove(&mut self, index: usize) -> Option<String> {
        if index >= self.items.len() {
            return None;
        }
        let item = self.items.remove(index);
        if index < self.disabled.len() {
            self.disabled.remove(index);
        }
        self.clamp_offsets();
        self.ensure_visible();
        Some(item)
    }

    /// Insert an item at `index`. Panics if `index > items.len()`.
    ///
    /// If the current selection is at or after `index`, it shifts forward.
    pub fn insert(&mut self, index: usize, item: String) {
        self.items.insert(index, item);
        self.disabled.insert(index, false);
        if self.selected >= index && !self.items.is_empty() && self.selected + 1 < self.items.len()
        {
            self.selected += 1;
        }
        self.ensure_visible();
    }

    /// Remove and return the last item, if any.
    ///
    /// Adjusts selected index and scroll offset so they remain valid.
    pub fn pop(&mut self) -> Option<String> {
        let item = self.items.pop()?;
        self.disabled.pop();
        self.clamp_offsets();
        self.ensure_visible();
        Some(item)
    }

    /// Compose stub — ListView items are strings, not widgets.
    pub(crate) fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        Vec::new()
    }

    fn max_offset(&self) -> usize {
        ScrollView::line_max_offset(self.items.len(), self.viewport_height.max(1))
    }

    fn is_selectable(&self, index: usize) -> bool {
        index < self.items.len() && !self.is_item_disabled(index)
    }

    fn selectable_count(&self) -> usize {
        self.items
            .iter()
            .enumerate()
            .filter(|(idx, _)| self.is_selectable(*idx))
            .count()
    }

    fn first_selectable(&self) -> Option<usize> {
        (0..self.items.len()).find(|idx| self.is_selectable(*idx))
    }

    fn last_selectable(&self) -> Option<usize> {
        (0..self.items.len())
            .rev()
            .find(|idx| self.is_selectable(*idx))
    }

    fn closest_selectable(&self, from: usize, direction: isize) -> Option<usize> {
        if self.selectable_count() == 0 {
            return None;
        }
        let max = self.items.len().saturating_sub(1) as isize;
        let mut idx = (from as isize).clamp(0, max) as usize;
        if self.is_selectable(idx) {
            return Some(idx);
        }
        let step = if direction >= 0 { 1 } else { -1 };
        loop {
            let next = idx as isize + step;
            if next < 0 || next > max {
                return None;
            }
            idx = next as usize;
            if self.is_selectable(idx) {
                return Some(idx);
            }
        }
    }

    fn clamp_offsets(&mut self) {
        if self.items.is_empty() {
            self.selected = 0;
            self.offset = 0;
            self.hovered_index = None;
            self.pressed_index = None;
            return;
        }
        self.selected = self.selected.min(self.items.len() - 1);
        if !self.is_selectable(self.selected) {
            self.selected = self
                .closest_selectable(self.selected, 1)
                .or_else(|| self.closest_selectable(self.selected, -1))
                .or_else(|| self.first_selectable())
                .unwrap_or(0);
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
        if self.items.is_empty() {
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

    fn emit_selection_changed(&self, ctx: &mut EventCtx) {
        if self.is_selectable(self.selected)
            && let Some(item) = self.items.get(self.selected)
        {
            ctx.post_message(Message::ListViewSelectionChanged(
                ListViewSelectionChanged {
                    index: self.selected,
                    item: item.clone(),
                },
            ));
        }
    }

    fn emit_item_activated(&self, index: usize, ctx: &mut EventCtx) {
        if self.is_selectable(index)
            && let Some(item) = self.items.get(index)
        {
            ctx.post_message(Message::ListViewItemActivated(ListViewItemActivated {
                index,
                item: item.clone(),
            }));
        }
    }

    fn select_index(&mut self, index: usize, ctx: &mut EventCtx) {
        if self.selectable_count() == 0 {
            return;
        }
        let next = self
            .closest_selectable(index, 1)
            .or_else(|| self.closest_selectable(index, -1))
            .unwrap_or(self.selected);
        if next != self.selected {
            self.selected = next;
            self.ensure_visible();
            self.emit_selection_changed(ctx);
            ctx.request_repaint();
        }
    }

    fn move_selection(&mut self, delta: isize, ctx: &mut EventCtx) {
        if self.selectable_count() == 0 {
            return;
        }
        let current = self.selected as isize;
        let max = (self.items.len() - 1) as isize;
        let mut next = (current + delta).clamp(0, max) as usize;
        let step = if delta >= 0 { 1 } else { -1 };
        while next < self.items.len() && !self.is_selectable(next) {
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

    fn scroll_offset(&mut self, delta_rows: isize, ctx: &mut EventCtx) {
        let before = self.offset;
        self.offset = ScrollView::line_scroll_by(
            self.offset,
            delta_rows as i32,
            self.items.len(),
            self.viewport_height.max(1),
        );
        if self.offset != before {
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn item_classes(
        highlighted: bool,
        hovered: bool,
        focused: bool,
        disabled: bool,
    ) -> Vec<&'static str> {
        let mut classes = vec!["list-view--item"];
        if highlighted {
            classes.push("-highlighted");
        }
        if hovered && !highlighted {
            classes.push("-hover");
        }
        if highlighted && focused {
            classes.push("-focus");
        }
        if disabled {
            classes.push("-disabled");
        }
        classes
    }
}

impl Widget for ListView {
    fn compose(&self) -> ComposeResult {
        Vec::new()
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

    fn action_namespace(&self) -> &str {
        "list-view"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("up", "cursor_up", "Move cursor up"),
            BindingDecl::new("down", "cursor_down", "Move cursor down"),
            BindingDecl::new("pageup", "scroll_up", "Page up").hidden(),
            BindingDecl::new("pagedown", "scroll_down", "Page down").hidden(),
            BindingDecl::new("home", "first", "Move to first item").hidden(),
            BindingDecl::new("end", "last", "Move to last item").hidden(),
            BindingDecl::new("enter", "select_cursor", "Select item"),
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
            "first" => {
                if let Some(first) = self.first_selectable() {
                    self.select_index(first, ctx);
                }
                ctx.set_handled();
                true
            }
            "last" => {
                if let Some(last) = self.last_selectable() {
                    self.select_index(last, ctx);
                }
                ctx.set_handled();
                true
            }
            "select_cursor" => {
                self.emit_item_activated(self.selected, ctx);
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            // TODO(P1-14 integration): wire tree-based NodeId comparison
            Event::MouseDown(mouse) if mouse.target == NodeId::default() => {
                let index = self.offset.saturating_add(mouse.y as usize);
                if self.is_selectable(index) {
                    self.select_index(index, ctx);
                    self.pressed_index = Some(index);
                    if self.hovered_index != Some(index) {
                        self.hovered_index = Some(index);
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                }
            }
            // TODO(P1-14 integration): wire tree-based NodeId comparison
            Event::MouseUp(mouse) if mouse.target == Some(NodeId::default()) => {
                let index = self.offset.saturating_add(mouse.y as usize);
                if self.pressed_index == Some(index) && self.is_selectable(index) {
                    self.emit_item_activated(index, ctx);
                    ctx.set_handled();
                }
                self.pressed_index = None;
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
                    if let Some(first) = self.first_selectable() {
                        self.select_index(first, ctx);
                    }
                    ctx.set_handled();
                }
                KeyCode::End => {
                    if let Some(last) = self.last_selectable() {
                        self.select_index(last, ctx);
                    }
                    ctx.set_handled();
                }
                KeyCode::Enter => {
                    self.emit_item_activated(self.selected, ctx);
                    ctx.set_handled();
                }
                _ => {}
            },
            Event::AppFocus(false) => {
                self.pressed_index = None;
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
        if self.items.is_empty() {
            return false;
        }
        let index = self.offset.saturating_add(y as usize);
        let hovered = self.is_selectable(index).then_some(index);
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
        self.pressed_index = None;
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        let base_style = crate::css::resolve_component_style(self, &["list-view--item"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        for row in 0..height {
            let index = self.offset + row;
            let mut text = String::new();
            let mut style = base_style;
            if let Some(item) = self.items.get(index) {
                let highlighted = index == self.selected && self.is_selectable(index);
                let hovered = self.hovered_index == Some(index);
                let classes = Self::item_classes(
                    highlighted,
                    hovered,
                    self.focused,
                    self.is_item_disabled(index),
                );
                style = crate::css::resolve_component_style(self, &classes)
                    .to_rich()
                    .unwrap_or(style);
                let marker = if highlighted { "› " } else { "  " };
                text = format!("{marker}{item}");
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
            .map(|item| rich_rs::cell_len(item).saturating_add(2))
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

#[cfg(test)]
mod tests {
    use super::ListView;
    use crate::event::{Event, EventCtx, MouseDownEvent, MouseUpEvent};
    use crate::keys::KeyEventData;
    use crate::message::*;
    use crate::node_id::NodeId;
    use crate::widgets::Widget;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn highlighted_item_uses_highlight_class_not_hover() {
        let classes = ListView::item_classes(true, true, true, false);
        assert!(classes.contains(&"-highlighted"));
        assert!(classes.contains(&"-focus"));
        assert!(!classes.contains(&"-hover"));
    }

    #[test]
    fn enter_activates_selected_item() {
        let mut list = ListView::new(vec!["one".to_string(), "two".to_string()]);
        list.set_focus(true);
        list.set_selected(1);

        let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        list.on_event(&Event::Key(key), &mut ctx);

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].message,
            Message::ListViewItemActivated(ListViewItemActivated {
                index: 1,
                ref item
            }) if item == "two"
        ));
    }

    #[test]
    fn mouse_click_activates_even_when_selection_unchanged() {
        let mut list = ListView::new(vec!["one".to_string(), "two".to_string()]);
        list.on_layout(20, 2);
        let id = NodeId::default();

        let mut ctx = EventCtx::default();
        list.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(ctx.handled());

        let mut up_ctx = EventCtx::default();
        list.on_event(
            &Event::MouseUp(MouseUpEvent {
                target: Some(id),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut up_ctx,
        );

        let messages = up_ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].message,
            Message::ListViewItemActivated(ListViewItemActivated {
                index: 0,
                ref item
            }) if item == "one"
        ));
    }

    #[test]
    fn app_focus_loss_clears_hover_state() {
        let mut list = ListView::new(vec!["one".to_string(), "two".to_string()]);
        list.set_hovered(true);
        assert!(list.on_mouse_move(0, 0));
        assert_eq!(list.hovered_index, Some(0));

        let mut ctx = EventCtx::default();
        list.on_event(&Event::AppFocus(false), &mut ctx);

        assert!(!list.is_hovered());
        assert_eq!(list.hovered_index, None);
        assert!(ctx.repaint_requested());
    }

    #[test]
    fn mouse_click_updates_hovered_index() {
        let mut list = ListView::new(vec!["one".to_string(), "two".to_string()]);
        list.on_layout(20, 2);
        let id = NodeId::default();

        let mut down_ctx = EventCtx::default();
        list.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 1,
                x: 0,
                y: 1,
            }),
            &mut down_ctx,
        );

        assert_eq!(list.hovered_index, Some(1));
        assert!(down_ctx.repaint_requested());
    }

    #[test]
    fn bindings_are_declared() {
        let list = ListView::new(vec!["A".into(), "B".into()]);
        let bindings = list.bindings();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().any(|b| b.action == "cursor_up"));
        assert!(bindings.iter().any(|b| b.action == "cursor_down"));
        assert!(bindings.iter().any(|b| b.action == "select_cursor"));
    }

    #[test]
    fn execute_action_handles_cursor_down() {
        use crate::action::ParsedAction;
        let mut list = ListView::new(vec!["A".into(), "B".into(), "C".into()]);
        list.set_focus(true);
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "cursor_down".to_string(),
            arguments: vec![],
        };
        assert!(list.execute_action(&action, &mut ctx));
        assert_eq!(list.selected(), 1);
    }

    // ── Mutation API tests ──────────────────────────────────────────────

    #[test]
    fn append_adds_item_and_extends_disabled() {
        let mut list = ListView::new(vec!["A".into()]);
        list.append("B".into());
        assert_eq!(list.items(), &["A", "B"]);
        assert!(!list.is_item_disabled(1));
    }

    #[test]
    fn clear_resets_everything() {
        let mut list = ListView::new(vec!["A".into(), "B".into(), "C".into()]);
        list.set_selected(2);
        list.clear();
        assert!(list.items().is_empty());
        assert_eq!(list.selected(), 0);
        assert_eq!(list.offset(), 0);
    }

    #[test]
    fn remove_valid_index() {
        let mut list = ListView::new(vec!["A".into(), "B".into(), "C".into()]);
        let removed = list.remove(1);
        assert_eq!(removed.as_deref(), Some("B"));
        assert_eq!(list.items(), &["A", "C"]);
    }

    #[test]
    fn remove_out_of_bounds_returns_none() {
        let mut list = ListView::new(vec!["A".into()]);
        assert!(list.remove(5).is_none());
        assert_eq!(list.items().len(), 1);
    }

    #[test]
    fn remove_from_empty_returns_none() {
        let mut list = ListView::new(vec![]);
        assert!(list.remove(0).is_none());
    }

    #[test]
    fn remove_adjusts_selected_when_at_end() {
        let mut list = ListView::new(vec!["A".into(), "B".into()]);
        list.set_selected(1);
        list.remove(1);
        assert_eq!(list.selected(), 0);
    }

    #[test]
    fn insert_at_beginning() {
        let mut list = ListView::new(vec!["B".into(), "C".into()]);
        list.set_selected(0);
        list.insert(0, "A".into());
        assert_eq!(list.items(), &["A", "B", "C"]);
        // selected was at 0, insert at 0 shifts it to 1
        assert_eq!(list.selected(), 1);
    }

    #[test]
    fn insert_at_end() {
        let mut list = ListView::new(vec!["A".into(), "B".into()]);
        list.insert(2, "C".into());
        assert_eq!(list.items(), &["A", "B", "C"]);
    }

    #[test]
    fn insert_disabled_entry_is_false() {
        let mut list = ListView::new(vec!["A".into()]);
        list.insert(0, "Z".into());
        assert!(!list.is_item_disabled(0));
    }

    #[test]
    fn pop_removes_last_item() {
        let mut list = ListView::new(vec!["A".into(), "B".into(), "C".into()]);
        let popped = list.pop();
        assert_eq!(popped.as_deref(), Some("C"));
        assert_eq!(list.items(), &["A", "B"]);
    }

    #[test]
    fn pop_empty_returns_none() {
        let mut list = ListView::new(vec![]);
        assert!(list.pop().is_none());
    }

    #[test]
    fn pop_adjusts_selected_when_pointing_past_end() {
        let mut list = ListView::new(vec!["A".into(), "B".into()]);
        list.set_selected(1);
        list.pop();
        assert_eq!(list.selected(), 0);
    }

    #[test]
    fn compose_returns_empty() {
        let list = ListView::new(vec!["A".into()]);
        assert!(list.compose().is_empty());
    }

    #[test]
    fn take_composed_children_returns_empty() {
        let mut list = ListView::new(vec!["A".into()]);
        assert!(list.take_composed_children().is_empty());
    }
}
