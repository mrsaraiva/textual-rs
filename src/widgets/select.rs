use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx, MouseDownEvent};
use crate::message::*;
use crate::render::{Cell, FrameBuffer};
use crate::runtime::dispatch_ctx::set_dispatch_recipient;
use crate::widgets::NodeState;

use super::option_list::toggle_option::OptionCursorState;
use super::option_list::{OptionItem, OptionList};
use super::select_current::SelectCurrent;
use crate::action::ParsedAction;

use super::{BindingDecl, NodeSeed, Widget};
use crate::compose::ComposeResult;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

/// Number of ticks before the type-to-search buffer resets (~500ms at 60Hz).
const SEARCH_RESET_TICKS: u64 = 30;

/// A dropdown select control.
///
/// Shows the current selection (or a placeholder prompt) with a dropdown arrow.
/// On activation (Enter/Space/click), opens an [`OptionList`] overlay for choosing a value.
/// When open, typing characters performs type-to-search (case-insensitive prefix matching).
///
/// Generic over the value type `T`.
pub struct Select<T: Clone + PartialEq + Send + Sync + 'static> {
    options: Vec<(String, T)>,
    cursor: OptionCursorState,
    prompt: String,
    disabled: bool,
    /// When `true`, the selection can be blank (no value). When `false` (default),
    /// the first option is auto-selected and the user cannot clear the selection.
    allow_blank: bool,
    open: bool,
    list: OptionList,
    viewport_width: usize,
    viewport_height: usize,
    /// Current tick counter (updated via on_tick).
    current_tick: u64,
    /// Type-to-search buffer (accumulated while dropdown is open).
    search_buffer: String,
    /// Tick when last search character was typed (for timeout reset).
    search_last_tick: u64,
    seed: NodeSeed,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Select<T> {
    /// Create a new `Select` widget.
    ///
    /// `options` is a list of `(label, value)` pairs.
    /// `prompt` is shown when nothing is selected.
    pub fn new(options: Vec<(String, T)>, prompt: impl Into<String>) -> Self {
        let list_items: Vec<OptionItem> = options
            .iter()
            .map(|(label, _)| OptionItem::new(label.as_str()))
            .collect();
        let mut list = OptionList::with_items(list_items);

        // Default allow_blank=false: auto-select first option.
        let mut cursor = OptionCursorState::default();
        if !options.is_empty() {
            cursor.set_selected(Some(0));
            cursor.set_highlighted(Some(0));
            list.set_highlighted(0);
        }

        let mut seed = NodeSeed::default();
        seed.classes = vec!["select".to_string()];
        Self {
            options,
            cursor,
            prompt: prompt.into(),
            disabled: false,
            allow_blank: false,
            open: false,
            list,
            viewport_width: 20,
            viewport_height: 10,
            current_tick: 0,
            search_buffer: String::new(),
            search_last_tick: 0,
            seed,
        }
    }

    // ── Public API ──────────────────────────────────────────────────

    /// The currently selected value, or `None`.
    pub fn value(&self) -> Option<&T> {
        self.cursor
            .selected()
            .and_then(|i| self.options.get(i).map(|(_, v)| v))
    }

    /// Reactive setter for the selected value. If the value is not found,
    /// selection is cleared. Records the change in the provided [`ReactiveCtx`].
    pub fn set_value(&mut self, value: &T, ctx: &mut ReactiveCtx) {
        let selected = self.options.iter().position(|(_, v)| v == value);
        let old = self.cursor.selected();
        if old != selected {
            self.cursor.set_selected(selected);
            self.cursor.set_highlighted(selected);
            if let Some(index) = selected {
                self.list.set_highlighted(index);
            } else {
                self.list.clear_highlighted();
            }
            ctx.record_change(
                "value",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(selected),
            );
        } else {
            // Even if index matches, still sync UI state.
            self.cursor.set_selected(selected);
            self.cursor.set_highlighted(selected);
            if let Some(index) = selected {
                self.list.set_highlighted(index);
            } else {
                self.list.clear_highlighted();
            }
        }
    }

    /// Clear the current selection (revert to prompt state).
    ///
    /// This is a no-op when `allow_blank` is `false`.
    pub fn clear(&mut self) {
        if !self.allow_blank {
            return;
        }
        self.cursor.clear();
        self.list.clear_highlighted();
    }

    /// Whether the dropdown overlay is currently open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether blank (no selection) is allowed.
    pub fn allow_blank(&self) -> bool {
        self.allow_blank
    }

    /// Reactive setter for `allow_blank`. Records the change in the provided
    /// [`ReactiveCtx`].
    ///
    /// When switching from `allow_blank=true` to `false` and no option is
    /// currently selected, the first option is auto-selected.
    pub fn set_allow_blank(&mut self, allow: bool, ctx: &mut ReactiveCtx) {
        if self.allow_blank != allow {
            let old = self.allow_blank;
            self.allow_blank = allow;
            // Auto-select first option when switching to false (also done via watcher).
            if !allow && self.cursor.selected().is_none() && !self.options.is_empty() {
                self.cursor.set_selected(Some(0));
                self.cursor.set_highlighted(Some(0));
                self.list.set_highlighted(0);
            }
            ctx.record_change(
                "allow_blank",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(allow),
            );
        }
    }

    /// Reactive setter for `disabled`. Records the change in the provided
    /// [`ReactiveCtx`].
    pub fn set_disabled(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.disabled != value {
            let old = self.disabled;
            self.disabled = value;
            ctx.record_change(
                "disabled",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    /// Builder: set whether blank (no selection) is allowed.
    ///
    /// When `true`, the initial state is no selection (placeholder is shown)
    /// and the user can deselect. When `false` (default), the first option
    /// is auto-selected and the user cannot clear the selection.
    pub fn with_allow_blank(mut self, allow: bool) -> Self {
        if allow {
            // Undo the auto-selection from new() — start blank.
            self.allow_blank = true;
            self.cursor.clear();
            self.list.clear_highlighted();
        } else {
            self.allow_blank = false;
            if self.cursor.selected().is_none() && !self.options.is_empty() {
                self.cursor.set_selected(Some(0));
                self.cursor.set_highlighted(Some(0));
                self.list.set_highlighted(0);
            }
        }
        self
    }

    /// Builder: set disabled state for the entire select.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        if disabled {
            self.open = false;
        }
        self
    }

    /// Reactive setter for `options`. Clears the current selection.
    /// Records the change in the provided [`ReactiveCtx`].
    ///
    /// When `allow_blank` is `false` and new options are non-empty,
    /// the first option is auto-selected.
    pub fn set_options(&mut self, options: Vec<(String, T)>, ctx: &mut ReactiveCtx) {
        let list_items: Vec<OptionItem> = options
            .iter()
            .map(|(label, _)| OptionItem::new(label.as_str()))
            .collect();
        let old_len = self.options.len();
        self.cursor.clear();
        self.list.set_items(list_items);
        self.options = options;
        let new_len = self.options.len();
        if !self.allow_blank && !self.options.is_empty() {
            self.cursor.set_selected(Some(0));
            self.cursor.set_highlighted(Some(0));
            self.list.set_highlighted(0);
        }
        ctx.record_change(
            "options",
            ReactiveFlags::reactive_layout(),
            Box::new(old_len),
            Box::new(new_len),
        );
    }

    // ── Watchers ─────────────────────────────────────────────────────

    fn watch_allow_blank(&mut self, _old: &bool, new: &bool, _ctx: &mut ReactiveCtx) {
        // When switching to allow_blank=false, auto-select first option if nothing selected.
        if !new && self.cursor.selected().is_none() && !self.options.is_empty() {
            self.cursor.set_selected(Some(0));
            self.cursor.set_highlighted(Some(0));
            self.list.set_highlighted(0);
        }
    }

    // ── Internals ───────────────────────────────────────────────────

    fn set_open(&mut self, open: bool, ctx: &mut EventCtx) {
        if self.open == open {
            return;
        }
        self.open = open;
        if self.open {
            // Sync list highlight with current selection.
            if let Some(selected) = self.cursor.selected() {
                self.list.set_highlighted(selected);
                self.cursor.set_highlighted(Some(selected));
            } else if let Some(first) = self.list.first_selectable_index() {
                self.list.set_highlighted(first);
                self.cursor.set_highlighted(Some(first));
            } else {
                self.list.clear_highlighted();
                self.cursor.set_highlighted(None);
            }
            // Reset search state when opening.
            self.search_buffer.clear();
        } else {
            self.search_buffer.clear();
        }
        ctx.request_repaint();
    }

    fn apply_selection(&mut self, index: usize, ctx: &mut EventCtx) {
        if index >= self.options.len() {
            return;
        }
        let changed = self.cursor.selected() != Some(index);
        self.cursor.set_selected(Some(index));
        self.cursor.set_highlighted(Some(index));
        self.set_open(false, ctx);
        if changed {
            let label = self.options[index].0.clone();
            ctx.post_message(SelectChanged { index, label });
        }
    }

    /// Geometry for the dropdown overlay panel.
    fn dropdown_geometry(&self) -> (usize, usize, usize, usize) {
        let panel_x = 0usize;
        // Directly below the closed-state bar (which is now a 3-row tall-bordered
        // box owned by SelectCurrent, not a single line).
        let panel_y = self.make_current().layout_height().unwrap_or(3);
        let panel_width = self.viewport_width.max(1);
        let available_height = self.viewport_height.saturating_sub(panel_y).max(1);
        let desired = self.options.len().max(1);
        let panel_height = desired.min(available_height).min(12).max(1);
        (panel_x, panel_y, panel_width, panel_height)
    }

    /// The label of the current value, or `None` when nothing is selected
    /// (the placeholder/prompt is shown instead).
    fn current_label(&self) -> Option<String> {
        self.cursor
            .selected()
            .map(|index| self.options[index].0.clone())
    }

    /// Build the closed-state bar widget ([`SelectCurrent`]) configured from the
    /// current state. `SelectCurrent` owns the `tall` border + padding chrome via
    /// CSS (Python parity: the border lives on `SelectCurrent`, not `Select`), so
    /// the framework's `render_styled` pipeline draws the bordered box around it.
    fn make_current(&self) -> SelectCurrent {
        SelectCurrent::new(self.prompt.clone())
            .with_label(self.current_label())
            .with_focused(self.node_state().focused)
            .with_expanded(self.open)
    }

    /// Handle a character typed for type-to-search when the dropdown is open.
    /// Appends to the search buffer and highlights the first matching option.
    fn handle_search_char(&mut self, ch: char, tick: u64) {
        // Reset buffer if timeout expired.
        if tick.saturating_sub(self.search_last_tick) > SEARCH_RESET_TICKS {
            self.search_buffer.clear();
        }
        self.search_buffer.push(ch);
        self.search_last_tick = tick;

        // Find first option whose label starts with the search buffer (case-insensitive).
        let query = self.search_buffer.to_lowercase();
        if let Some(index) = self
            .options
            .iter()
            .position(|(label, _)| label.to_lowercase().starts_with(&query))
        {
            self.list.set_highlighted(index);
            self.cursor.set_highlighted(Some(index));
        }
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Widget for Select<T> {
    /// Declare children for tree-based mounting.
    ///
    /// Select's inner OptionList is managed internally (not a mountable child),
    /// so compose returns an empty list.
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        Vec::new()
    }

    fn focusable(&self) -> bool {
        !self.disabled
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        if !new.focused && self.open {
            // Close dropdown when focus is lost.
            self.open = false;
            self.search_buffer.clear();
        }
        if new.disabled && self.open {
            self.open = false;
            self.search_buffer.clear();
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.viewport_width = usize::from(width).max(1);
        self.viewport_height = usize::from(height).max(1);
        if self.open {
            let (_, _, pw, ph) = self.dropdown_geometry();
            self.list.on_layout(pw as u16, ph as u16);
        }
    }

    fn on_tick(&mut self, tick: u64) {
        self.current_tick = tick;
        // Reset search buffer after timeout.
        if self.open
            && !self.search_buffer.is_empty()
            && tick.saturating_sub(self.search_last_tick) > SEARCH_RESET_TICKS
        {
            self.search_buffer.clear();
        }
    }

    fn action_namespace(&self) -> &str {
        "select"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("enter,space,down,up", "show_overlay", "Show select options"),
            BindingDecl::new("escape", "dismiss_overlay", "Dismiss select options").hidden(),
        ]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        if self.disabled {
            return false;
        }
        match action.name.as_str() {
            "show_overlay" => {
                if !self.open {
                    self.set_open(true, ctx);
                    ctx.set_handled();
                    true
                } else {
                    false
                }
            }
            "dismiss_overlay" => {
                if self.open {
                    if self.allow_blank {
                        self.cursor.clear();
                        self.list.clear_highlighted();
                    }
                    self.set_open(false, ctx);
                    ctx.set_handled();
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        if self.open {
            // When the overlay is open, handle its events first.
            match event {
                Event::Key(key) => match key.code {
                    KeyCode::Esc => {
                        if self.allow_blank {
                            // Deselect: revert to blank/placeholder state.
                            self.cursor.clear();
                            self.list.clear_highlighted();
                        }
                        self.set_open(false, ctx);
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Enter => {
                        if self.list.highlighted().is_none() {
                            self.set_open(false, ctx);
                        } else {
                            // Route selection through OptionList message flow.
                            let _guard = set_dispatch_recipient(
                                crate::node_id::NodeId::default(),
                                NodeState {
                                    focused: true,
                                    ..Default::default()
                                },
                            );
                            self.list.on_event(event, ctx);
                            drop(_guard);
                            self.cursor.set_highlighted(self.list.highlighted());
                        }
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Char(ch) => {
                        // Type-to-search: printable chars that aren't space (space toggles).
                        if ch != ' ' {
                            self.handle_search_char(ch, self.current_tick);
                            ctx.request_repaint();
                            ctx.set_handled();
                            return;
                        }
                    }
                    _ => {}
                },
                Event::MouseDown(mouse) => {
                    if mouse.target != self.node_id() {
                        // Click outside the Select widget — close dropdown.
                        self.set_open(false, ctx);
                        ctx.set_handled();
                        return;
                    }
                    // Click within Select — check if it's in the dropdown area.
                    let (_, panel_y, _, panel_h) = self.dropdown_geometry();
                    let click_y = mouse.y as usize;
                    if click_y >= panel_y && click_y < panel_y + panel_h {
                        // Forward click to the inner OptionList, preserving the raw y
                        // coordinate so the list's offset-based index calculation works.
                        self.list.on_event(
                            &Event::MouseDown(MouseDownEvent {
                                target: self.node_id(),
                                screen_x: mouse.screen_x,
                                screen_y: mouse.screen_y,
                                x: mouse.x,
                                y: mouse.y,
                            }),
                            ctx,
                        );
                        self.cursor.set_highlighted(self.list.highlighted());
                    } else {
                        // Click on the closed-state bar area — toggle closed.
                        self.set_open(false, ctx);
                    }
                    ctx.set_handled();
                    return;
                }
                _ => {}
            }
            // Delegate navigation keys to the inner OptionList.
            {
                let _guard = set_dispatch_recipient(
                    crate::node_id::NodeId::default(),
                    NodeState {
                        focused: true,
                        ..Default::default()
                    },
                );
                self.list.on_event(event, ctx);
            }
            self.cursor.set_highlighted(self.list.highlighted());
            if !ctx.handled() {
                // Absorb all events when overlay is open.
                ctx.set_handled();
            }
        } else {
            // Closed state: open on Enter/Space/click.
            match event {
                Event::Key(key) if self.node_state().focused => match key.code {
                    KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Down | KeyCode::Up => {
                        self.set_open(true, ctx);
                        ctx.set_handled();
                    }
                    _ => {}
                },
                Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                    self.set_open(true, ctx);
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        // Handle OptionSelected from inner list.
        if message.sender == self.node_id() {
            if let Some(OptionSelected { index }) = message.downcast_ref::<OptionSelected>() {
                self.apply_selection(*index, ctx);
                ctx.set_handled();
                return;
            }
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if self.disabled {
            return false;
        }
        if self.open {
            // Forward to the list if the mouse is within the dropdown area.
            let (_, panel_y, _, panel_h) = self.dropdown_geometry();
            let y_usize = y as usize;
            if y_usize >= panel_y && y_usize < panel_y + panel_h {
                return self.list.on_mouse_move(x, (y_usize - panel_y) as u16);
            }
        }
        false
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        if self.open {
            self.list.on_mouse_scroll(delta_x, delta_y, ctx);
            if !ctx.handled() {
                ctx.set_handled();
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        // The closed-state bar is a `SelectCurrent` widget that owns the tall
        // border via CSS. Render it through the styled pipeline (which draws the
        // border) tagged with THIS Select's node id so the bar remains
        // hit-testable (click-to-open targets the Select).
        let current = self.make_current();
        let bar_height = current.layout_height().unwrap_or(3);

        if !self.open {
            return current.render_styled_dyn_obj(console, options, None, self.node_id());
        }

        // Open state: render the bordered bar at the top + dropdown overlay below.
        let (width, height) = options.size;
        let width = width.max(1);
        let height = height.max(1);

        let mut bar_options = options.clone();
        bar_options.size = (width, bar_height);
        bar_options.max_width = width;
        bar_options.max_height = bar_height;
        let bar_segments =
            current.render_styled_dyn_obj(console, &bar_options, None, self.node_id());
        let bar_lines = Segment::split_and_crop_lines(bar_segments, width, None, false, false);
        let bar_buf = FrameBuffer::from_lines(&bar_lines, width, bar_height, None);
        let mut merged = FrameBuffer::new(width, height, None);
        for y in 0..bar_height.min(height).min(bar_buf.height) {
            for x in 0..width.min(bar_buf.width) {
                merged.set_cell(x, y, bar_buf.get(x, y).clone());
            }
        }

        // Render the dropdown OptionList.
        let (panel_x, panel_y, panel_width, panel_height) = self.dropdown_geometry();
        let panel_width = panel_width.min(width);
        let panel_height = panel_height.min(height.saturating_sub(panel_y));
        if panel_height == 0 {
            return merged.to_segments();
        }

        let panel_style = crate::css::resolve_component_style(self, &["select--dropdown"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        // Clear the dropdown area.
        for y in panel_y..panel_y.saturating_add(panel_height).min(height) {
            for x in panel_x..panel_x.saturating_add(panel_width).min(width) {
                merged.set_cell(x, y, Cell::blank(Some(panel_style)));
            }
        }

        // Render the OptionList into a sub-buffer.
        let mut list_options = options.clone();
        list_options.size = (panel_width, panel_height);
        list_options.max_width = panel_width;
        list_options.max_height = panel_height;
        let list_buffer = FrameBuffer::from_renderable(console, &list_options, &self.list, None);

        for sy in 0..list_buffer.height.min(panel_height) {
            let ty = panel_y.saturating_add(sy);
            if ty >= height {
                break;
            }
            for sx in 0..list_buffer.width.min(panel_width) {
                let tx = panel_x.saturating_add(sx);
                if tx >= width {
                    break;
                }
                merged.set_cell(tx, ty, list_buffer.get(sx, sy).clone());
            }
        }

        merged.to_segments()
    }

    fn layout_height(&self) -> Option<usize> {
        // Closed: the SelectCurrent bar's outer height (border + 1 content row).
        // Open: bar height + dropdown height.
        let bar_height = self.make_current().layout_height().unwrap_or(3);
        if self.open {
            let (_, _, _, ph) = self.dropdown_geometry();
            Some(bar_height + ph)
        } else {
            Some(bar_height)
        }
    }

    fn content_width(&self) -> Option<usize> {
        let label_width = self
            .options
            .iter()
            .map(|(label, _)| rich_rs::cell_len(label))
            .max()
            .unwrap_or(0)
            .max(rich_rs::cell_len(&self.prompt));
        // label + space padding + arrow
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        Some(
            label_width
                .saturating_add(3)
                .saturating_add(chrome_lr)
                .max(1),
        )
    }

    fn style_type(&self) -> &'static str {
        "Select"
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    /// Stage a `SelectChanged` for the initially-selected value so the message
    /// is posted at mount time.
    ///
    /// Python parity: `Select.value` is a reactive set during init, and
    /// `_watch_value` posts `Select.Changed` whenever it changes — including the
    /// initial assignment on mount. With `allow_blank=False` the first option is
    /// auto-selected, so apps observe `Changed(first_value)` at startup (e.g. the
    /// `select_widget_no_blank` demo sets its title from the first option).
    ///
    /// The runtime drains this once right after the node is mounted and routes
    /// it through the normal message bus (see
    /// `Widget::take_pending_mount_messages`).
    fn take_pending_mount_messages(&mut self) -> Vec<Box<dyn crate::message::Message>> {
        if let Some(index) = self.cursor.selected()
            && let Some((label, _)) = self.options.get(index)
        {
            return vec![Box::new(SelectChanged {
                index,
                label: label.clone(),
            })];
        }
        Vec::new()
    }

    // NOTE: Select intentionally does NOT implement visit_children_mut.
    // The inner OptionList is a private implementation detail and should not
    // appear in the global focus traversal — Select manages it internally.
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Renderable for Select<T> {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> ReactiveWidget for Select<T> {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            match change.field_name {
                "allow_blank" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_allow_blank(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, EventCtx, MouseDownEvent};
    use crate::keys::KeyEventData;
    use crate::node_id::NodeId;
    use crate::node_id::node_id_from_ffi;
    use crate::reactive::ReactiveCtx;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
    use crate::widgets::NodeState;
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

    /// Derive a test-only NodeId from a widget's pointer address.
    fn widget_node_id(w: &dyn Widget) -> crate::node_id::NodeId {
        let ptr = (w as *const dyn Widget).cast::<()>() as u64;
        node_id_from_ffi(ptr)
    }
    use crate::message::MessageEvent;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn make_select() -> Select<i32> {
        Select::new(
            vec![
                ("Alpha".to_string(), 1),
                ("Beta".to_string(), 2),
                ("Gamma".to_string(), 3),
            ],
            "Pick one...",
        )
    }

    fn make_select_blank() -> Select<i32> {
        make_select().with_allow_blank(true)
    }

    fn dispatch_messages(sel: &mut Select<i32>, ctx: &mut EventCtx) -> Vec<MessageEvent> {
        let mut delivered = Vec::new();
        loop {
            let batch = ctx.take_messages();
            if batch.is_empty() {
                break;
            }
            delivered.extend(batch.clone());
            for message in batch {
                sel.on_message(&message, ctx);
            }
        }
        delivered
    }

    #[test]
    fn select_starts_closed_with_first_selected() {
        // Default allow_blank=false auto-selects the first option.
        let sel = make_select();
        assert!(!sel.is_open());
        assert_eq!(sel.value(), Some(&1)); // Alpha
    }

    #[test]
    fn select_opens_on_enter() {
        let mut sel = make_select();
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        sel.on_layout(30, 20);

        let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(key), &mut ctx);
        assert!(sel.is_open());
        assert!(ctx.handled());
    }

    #[test]
    fn select_closes_on_escape() {
        let mut sel = make_select();
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        sel.on_layout(30, 20);

        // Open
        let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(key), &mut ctx);
        assert!(sel.is_open());

        // Close
        let esc = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let mut ctx2 = EventCtx::default();
        sel.on_event(&Event::Key(esc), &mut ctx2);
        assert!(!sel.is_open());
    }

    #[test]
    fn select_enter_selects_highlighted_option() {
        let mut sel = make_select();
        // Use NodeId::default() so self.node_id() == ctx.node_id (message sender) == default.
        let _guard = set_dispatch_recipient(NodeId::default(), focused_state());
        sel.on_layout(30, 20);

        // Open
        let enter = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(enter.clone()), &mut ctx);
        assert!(sel.is_open());

        // Move down once
        let down = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let mut ctx2 = EventCtx::default();
        sel.on_event(&Event::Key(down), &mut ctx2);

        // Confirm with Enter
        let mut ctx3 = EventCtx::default();
        sel.on_event(&Event::Key(enter), &mut ctx3);
        let delivered = dispatch_messages(&mut sel, &mut ctx3);
        assert!(!sel.is_open());
        assert_eq!(sel.value(), Some(&2)); // Beta

        let option_selected_pos = delivered.iter().position(|m| {
            m.downcast_ref::<OptionSelected>()
                .is_some_and(|s| s.index == 1)
        });
        let select_changed_pos = delivered.iter().position(|m| {
            m.downcast_ref::<SelectChanged>()
                .is_some_and(|s| s.index == 1)
        });
        assert!(
            option_selected_pos.is_some()
                && select_changed_pos.is_some()
                && option_selected_pos < select_changed_pos
        );
    }

    #[test]
    fn select_mouse_click_inside_dropdown_selects_item() {
        let mut sel = make_select();
        // Use NodeId::default() so self.node_id() matches mouse.target (NodeId::default()).
        let _guard = set_dispatch_recipient(NodeId::default(), focused_state());
        sel.on_layout(30, 20);

        let open_key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut open_ctx = EventCtx::default();
        sel.on_event(&Event::Key(open_key), &mut open_ctx);
        assert!(sel.is_open());

        let mut click_ctx = EventCtx::default();
        sel.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 1,
                screen_y: 2,
                x: 1,
                y: 1,
            }),
            &mut click_ctx,
        );
        let delivered = dispatch_messages(&mut sel, &mut click_ctx);

        assert!(!sel.is_open());
        assert_eq!(sel.value(), Some(&2));
        assert!(click_ctx.handled());
        let option_selected_pos = delivered.iter().position(|m| {
            m.downcast_ref::<OptionSelected>()
                .is_some_and(|s| s.index == 1)
        });
        let select_changed_pos = delivered.iter().position(|m| {
            m.downcast_ref::<SelectChanged>()
                .is_some_and(|s| s.index == 1)
        });
        assert!(
            option_selected_pos.is_some()
                && select_changed_pos.is_some()
                && option_selected_pos < select_changed_pos
        );
    }

    #[test]
    fn select_set_value_programmatic() {
        let mut sel = make_select();
        let mut ctx = ReactiveCtx::new(make_node_id());
        sel.set_value(&3, &mut ctx);
        assert_eq!(sel.value(), Some(&3));
    }

    #[test]
    fn select_clear_resets_when_allow_blank() {
        let mut sel = make_select_blank();
        let mut ctx = ReactiveCtx::new(make_node_id());
        sel.set_value(&2, &mut ctx);
        sel.clear();
        assert!(sel.value().is_none());
    }

    #[test]
    fn select_clear_then_reopen_highlights_first_selectable() {
        let mut sel = make_select_blank();
        let mut ctx = ReactiveCtx::new(make_node_id());
        sel.set_value(&3, &mut ctx);
        sel.clear();
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        sel.on_layout(30, 20);

        let open = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(open), &mut ctx);

        assert!(sel.is_open());
        assert_eq!(sel.list.highlighted(), Some(0));
    }

    #[test]
    fn select_ignores_disabled_option_click() {
        let mut sel = Select::new(
            vec![("Alpha".to_string(), 1), ("Beta".to_string(), 2)],
            "Pick one...",
        );
        sel.list
            .set_items(vec![OptionItem::new("Alpha"), OptionItem::disabled("Beta")]);
        // Use NodeId::default() so self.node_id() matches mouse.target (NodeId::default()).
        let _guard = set_dispatch_recipient(NodeId::default(), focused_state());
        sel.on_layout(30, 20);

        let open = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut open_ctx = EventCtx::default();
        sel.on_event(&Event::Key(open), &mut open_ctx);
        assert!(sel.is_open());

        let mut click_ctx = EventCtx::default();
        sel.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 1,
                screen_y: 2,
                x: 1,
                y: 1,
            }),
            &mut click_ctx,
        );

        // Value unchanged — still Alpha (auto-selected, disabled Beta ignored).
        assert_eq!(sel.value(), Some(&1));
        assert!(sel.is_open());
    }

    #[test]
    fn select_disabled_ignores_open_input() {
        let mut sel = make_select().disabled(true);
        sel.on_layout(30, 20);

        let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut key_ctx = EventCtx::default();
        sel.on_event(&Event::Key(key), &mut key_ctx);
        assert!(!sel.is_open());
        assert!(!key_ctx.handled());

        let mut click_ctx = EventCtx::default();
        sel.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: widget_node_id(&sel),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut click_ctx,
        );
        assert!(!sel.is_open());
        assert!(!click_ctx.handled());
        assert!(!sel.focusable());
    }

    #[test]
    fn select_type_to_search_highlights_matching_option() {
        let mut sel = make_select();
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        sel.on_layout(30, 20);

        // Open
        let enter = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(enter), &mut ctx);
        assert!(sel.is_open());

        // Advance tick so type-to-search has a time reference.
        sel.on_tick(10);

        // Type 'g' — should highlight "Gamma" (index 2).
        let g_key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        let mut ctx2 = EventCtx::default();
        sel.on_event(&Event::Key(g_key), &mut ctx2);

        assert_eq!(sel.list.highlighted(), Some(2));
        assert!(ctx2.handled());
    }

    #[test]
    fn select_type_to_search_accumulates_chars() {
        let mut sel = Select::new(
            vec![
                ("Apple".to_string(), 1),
                ("Apricot".to_string(), 2),
                ("Banana".to_string(), 3),
            ],
            "Pick one...",
        );
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        sel.on_layout(30, 20);

        // Open
        let enter = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(enter), &mut ctx);
        assert!(sel.is_open());

        // Type 'a' at tick 10 — should match "Apple" (index 0).
        sel.on_tick(10);
        let a_key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let mut ctx2 = EventCtx::default();
        sel.on_event(&Event::Key(a_key), &mut ctx2);
        assert_eq!(sel.list.highlighted(), Some(0));

        // Type 'p' at tick 11 — buffer is "ap", matches "Apple" (index 0).
        sel.on_tick(11);
        let p_key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
        let mut ctx3 = EventCtx::default();
        sel.on_event(&Event::Key(p_key), &mut ctx3);
        assert_eq!(sel.list.highlighted(), Some(0));

        // Type 'r' at tick 12 — buffer is "apr", matches "Apricot" (index 1).
        sel.on_tick(12);
        let r_key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        let mut ctx4 = EventCtx::default();
        sel.on_event(&Event::Key(r_key), &mut ctx4);
        assert_eq!(sel.list.highlighted(), Some(1));
    }

    #[test]
    fn select_type_to_search_resets_on_timeout() {
        let mut sel = make_select();
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        sel.on_layout(30, 20);

        // Open
        let enter = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(enter), &mut ctx);

        // Type 'b' at tick 10 — highlights "Beta" (index 1).
        sel.on_tick(10);
        let b_key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        let mut ctx2 = EventCtx::default();
        sel.on_event(&Event::Key(b_key), &mut ctx2);
        assert_eq!(sel.list.highlighted(), Some(1));

        // Simulate timeout via on_tick.
        sel.on_tick(50);
        assert!(sel.search_buffer.is_empty());

        // Type 'a' after timeout — fresh search, highlights "Alpha" (index 0).
        let a_key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let mut ctx3 = EventCtx::default();
        sel.on_event(&Event::Key(a_key), &mut ctx3);
        assert_eq!(sel.list.highlighted(), Some(0));
    }

    // ── allow_blank tests ─────────────────────────────────────────────

    #[test]
    fn allow_blank_true_starts_with_no_selection() {
        let sel = make_select_blank();
        assert!(sel.allow_blank());
        assert!(sel.value().is_none());
    }

    #[test]
    fn allow_blank_false_auto_selects_first() {
        let sel = make_select();
        assert!(!sel.allow_blank());
        assert_eq!(sel.value(), Some(&1)); // Alpha
    }

    #[test]
    fn allow_blank_true_escape_clears_selection() {
        let mut sel = make_select_blank();
        let mut rctx = ReactiveCtx::new(make_node_id());
        sel.set_value(&2, &mut rctx); // Beta
        assert_eq!(sel.value(), Some(&2));
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        sel.on_layout(30, 20);

        // Open
        let enter = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(enter), &mut ctx);
        assert!(sel.is_open());

        // Escape — should clear selection (allow_blank=true)
        let esc = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let mut ctx2 = EventCtx::default();
        sel.on_event(&Event::Key(esc), &mut ctx2);
        assert!(!sel.is_open());
        assert!(sel.value().is_none());
    }

    #[test]
    fn allow_blank_false_escape_keeps_selection() {
        let mut sel = make_select();
        let mut rctx = ReactiveCtx::new(make_node_id());
        sel.set_value(&2, &mut rctx); // Beta
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        sel.on_layout(30, 20);

        // Open
        let enter = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(enter), &mut ctx);
        assert!(sel.is_open());

        // Escape — should NOT clear (allow_blank=false)
        let esc = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let mut ctx2 = EventCtx::default();
        sel.on_event(&Event::Key(esc), &mut ctx2);
        assert!(!sel.is_open());
        assert_eq!(sel.value(), Some(&2)); // Still Beta
    }

    #[test]
    fn allow_blank_false_clear_is_noop() {
        let mut sel = make_select();
        assert_eq!(sel.value(), Some(&1)); // Alpha auto-selected
        sel.clear();
        assert_eq!(sel.value(), Some(&1)); // Still Alpha — clear is a no-op
    }

    #[test]
    fn with_allow_blank_builder() {
        let sel = make_select().with_allow_blank(true);
        assert!(sel.allow_blank());
        assert!(sel.value().is_none());

        let sel2 = make_select().with_allow_blank(false);
        assert!(!sel2.allow_blank());
        assert_eq!(sel2.value(), Some(&1)); // Alpha
    }

    #[test]
    fn set_allow_blank_auto_selects_when_switching_to_false() {
        let mut sel = make_select_blank();
        let mut ctx = ReactiveCtx::new(make_node_id());
        assert!(sel.value().is_none());
        sel.set_allow_blank(false, &mut ctx);
        assert!(!sel.allow_blank());
        assert_eq!(sel.value(), Some(&1)); // Alpha auto-selected
    }

    #[test]
    fn set_options_auto_selects_when_not_allow_blank() {
        let mut sel = make_select();
        let mut ctx = ReactiveCtx::new(make_node_id());
        sel.set_options(
            vec![("Delta".to_string(), 10), ("Echo".to_string(), 20)],
            &mut ctx,
        );
        assert_eq!(sel.value(), Some(&10)); // Delta auto-selected
    }

    #[test]
    fn set_options_does_not_auto_select_when_allow_blank() {
        let mut sel = make_select_blank();
        let mut ctx = ReactiveCtx::new(make_node_id());
        sel.set_options(
            vec![("Delta".to_string(), 10), ("Echo".to_string(), 20)],
            &mut ctx,
        );
        assert!(sel.value().is_none());
    }

    #[test]
    fn bindings_are_declared() {
        let sel = make_select();
        let bindings = sel.bindings();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().any(|b| b.action == "show_overlay"));
        assert!(bindings.iter().any(|b| b.action == "dismiss_overlay"));
    }

    // ── compose() / take_composed_children() tests ────────────────

    #[test]
    fn compose_returns_empty() {
        let sel = make_select();
        let result = sel.compose();
        assert!(result.is_empty());
    }

    #[test]
    fn take_composed_children_returns_empty() {
        let mut sel = make_select();
        let children = sel.take_composed_children();
        assert!(children.is_empty());
    }

    #[test]
    fn compose_stable_across_state_changes() {
        let mut sel = make_select();
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        sel.on_layout(30, 20);

        // Open the dropdown
        let enter = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        sel.on_event(&Event::Key(enter), &mut ctx);
        assert!(sel.is_open());

        // compose() should still return empty even when open
        assert!(sel.compose().is_empty());
        assert!(sel.take_composed_children().is_empty());
    }

    #[test]
    fn execute_action_handles_show_overlay() {
        use crate::action::ParsedAction;
        let mut sel = make_select();
        sel.on_layout(20, 10);
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "show_overlay".to_string(),
            arguments: vec![],
        };
        assert!(!sel.is_open());
        assert!(sel.execute_action(&action, &mut ctx));
        assert!(sel.is_open());
    }

    // ── P1-14 dispatch-context regression tests ─────────────────────────

    #[test]
    fn mouse_click_with_dispatch_context_opens_select() {
        let mut sel = make_select();
        sel.on_layout(30, 20);

        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, NodeState::default());

        let mut ctx = EventCtx::default();
        sel.on_event(
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
        assert!(sel.is_open());
    }

    #[test]
    fn mouse_click_with_wrong_target_closes_open_select() {
        use slotmap::SlotMap;

        let mut sel = make_select();
        sel.on_layout(30, 20);

        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        let my_id = sm.insert(());
        let other_id = sm.insert(());
        let _guard = set_dispatch_recipient(my_id, focused_state());

        // Open via keyboard first.
        let open_key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut open_ctx = EventCtx::default();
        sel.on_event(&Event::Key(open_key), &mut open_ctx);
        assert!(sel.is_open());

        // Click with a different target (outside) — should close dropdown.
        let mut ctx = EventCtx::default();
        sel.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: other_id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(ctx.handled());
        assert!(!sel.is_open());
    }
}
