use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual_macros::widget;

use crate::compose::ComposeResult;
use crate::css;
use crate::event::{Action, Event};
use crate::message::*;

use crate::action::ParsedAction;

use super::{BindingDecl, ListItem, NodeSeed, ScrollView, Widget};

/// A vertical list view widget.
///
/// Displays a vertical list of [`ListItem`]s which can be highlighted and
/// selected using the mouse or keyboard. This mirrors Python Textual's
/// `ListView` (`textual/widgets/_list_view.py`): it composes `ListItem`
/// children through the normal widget tree, is focusable but does **not** let
/// its children take focus (`can_focus_children=False`), and drives the
/// highlight on the highlighted child by setting the `-highlight` class on that
/// child's node — the highlight is conveyed **by background only**, there is no
/// text marker.
///
/// # Construction
///
/// ```rust
/// use textual::prelude::*;
///
/// // Python: ListView(ListItem(Label("One")), ListItem(Label("Two")))
/// let list = ListView::from_list_items(vec![
///     ListItem::new(Label::new("One")),
///     ListItem::new(Label::new("Two")),
/// ]);
/// assert_eq!(list.len(), 2);
///
/// // Convenience: build items from plain strings.
/// let list = ListView::new(vec!["One".to_string(), "Two".to_string()]);
/// assert_eq!(list.items(), &["One", "Two"]);
/// ```
///
/// # Headless use
///
/// `ListView` also serves as a headless selection/scroll state model (used by
/// the command palette): the index/offset/hover state machine works
/// independently of arena composition, so embedding widgets can read
/// [`selected`](Self::selected) / [`offset`](Self::offset) /
/// [`hovered_index`](Self::hovered_index) without mounting the list.
#[derive(Debug)]
#[widget(Focus, Interactive, Layout, Scrollable)]
pub struct ListView {
    /// Authoritative text of each item (the headless state model and message
    /// payloads use this; it survives arena extraction). For text-based items it
    /// is also the source from which `ListItem` children are (re)built.
    item_text: Vec<String>,
    /// Authoritative per-item disabled state, in lockstep with `item_text`.
    disabled: Vec<bool>,
    /// Compose buffer for [`from_list_items`](Self::from_list_items) /
    /// [`set_list_items`](Self::set_list_items): the user-supplied `ListItem`s
    /// awaiting their first arena mount. Drained by `compose`;
    /// `None`/empty thereafter (recomposes rebuild from `item_text`).
    pending_items: Vec<ListItem>,
    selected: usize,
    offset: usize,
    hovered_index: Option<usize>,
    pressed_index: Option<usize>,
    viewport_height: usize,
    scroll_step: usize,
    children_extracted: bool,
    /// `true` once an initial `Highlighted` should be posted at mount (Python's
    /// `_on_mount` sets `self.index`, which fires the `Highlighted` watcher).
    pending_initial_highlight: bool,
    seed: NodeSeed,
}

impl ListView {
    crate::seed_ident_methods!();

    /// Create a `ListView` from plain strings; each becomes a `ListItem(Label)`.
    pub fn new(items: Vec<String>) -> Self {
        let pending: Vec<ListItem> = items.iter().cloned().map(ListItem::from_text).collect();
        let disabled = vec![false; items.len()];
        Self::build(pending, items, disabled)
    }

    /// Create a `ListView` from [`ListItem`] children (the Python composition API).
    ///
    /// Python: `ListView(ListItem(Label("One")), ListItem(Label("Two")), ...)`.
    pub fn from_list_items(items: Vec<ListItem>) -> Self {
        let text: Vec<String> = items.iter().map(|i| i.text().to_string()).collect();
        let disabled: Vec<bool> = items.iter().map(ListItem::is_disabled).collect();
        Self::build(items, text, disabled)
    }

    fn build(pending_items: Vec<ListItem>, item_text: Vec<String>, disabled: Vec<bool>) -> Self {
        let mut view = Self {
            item_text,
            disabled,
            pending_items,
            selected: 0,
            offset: 0,
            hovered_index: None,
            pressed_index: None,
            viewport_height: 1,
            scroll_step: 1,
            children_extracted: false,
            pending_initial_highlight: false,
            seed: NodeSeed::default(),
        };
        // Python `_on_mount` highlights `initial_index` (0), skipping disabled.
        if let Some(first) = view.first_selectable() {
            view.selected = first;
            view.pending_initial_highlight = true;
        }
        view
    }

    // ── State accessors (also used by the headless command-palette model) ────

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_item(&self) -> Option<&str> {
        self.item_text.get(self.selected).map(String::as_str)
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn hovered_index(&self) -> Option<usize> {
        self.hovered_index
    }

    /// The number of items in the list.
    pub fn len(&self) -> usize {
        self.item_text.len()
    }

    /// Whether the list has no items.
    pub fn is_empty(&self) -> bool {
        self.item_text.is_empty()
    }

    /// The item texts, in order.
    pub fn items(&self) -> &[String] {
        &self.item_text
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

    /// Mark the items as changed so the next mount/recompose rebuilds the arena
    /// children from the current `item_text`/`disabled` state.
    fn invalidate_children(&mut self) {
        self.pending_items.clear();
        self.children_extracted = false;
    }

    /// Replace all items with new ones built from plain strings.
    pub fn set_items(&mut self, items: Vec<String>) {
        self.disabled = vec![false; items.len()];
        self.item_text = items;
        self.invalidate_children();
        self.clamp_offsets();
        self.ensure_visible();
    }

    /// Replace all items with new [`ListItem`] children.
    pub fn set_list_items(&mut self, items: Vec<ListItem>) {
        self.item_text = items.iter().map(|i| i.text().to_string()).collect();
        self.disabled = items.iter().map(ListItem::is_disabled).collect();
        self.pending_items = items;
        self.children_extracted = false;
        self.clamp_offsets();
        self.ensure_visible();
    }

    pub fn set_item_disabled(&mut self, index: usize, disabled: bool) {
        if index >= self.item_text.len() {
            return;
        }
        if index >= self.disabled.len() {
            self.disabled.resize(self.item_text.len(), false);
        }
        self.disabled[index] = disabled;
        // Keep any pending (not-yet-mounted) item in sync.
        if let Some(item) = self.pending_items.get_mut(index) {
            *item = std::mem::replace(item, ListItem::empty()).disabled(disabled);
        }
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

    /// Append a `ListItem` to the end of the list.
    pub fn append_item(&mut self, item: ListItem) {
        self.item_text.push(item.text().to_string());
        self.disabled.push(item.is_disabled());
        // Once mounted, items are rebuilt from text on recompose; before mount we
        // keep the user-supplied item in the pending buffer.
        if !self.children_extracted {
            self.pending_items.push(item);
        }
        self.children_extracted = false;
    }

    /// Append an item from a plain string.
    pub fn append(&mut self, item: String) {
        self.append_item(ListItem::from_text(item));
    }

    /// Remove all items, resetting selection and scroll offset to 0.
    pub fn clear(&mut self) {
        self.item_text.clear();
        self.disabled.clear();
        self.pending_items.clear();
        self.selected = 0;
        self.offset = 0;
        self.hovered_index = None;
        self.pressed_index = None;
        self.children_extracted = false;
    }

    /// Remove the item at `index`, returning its text if valid.
    pub fn remove(&mut self, index: usize) -> Option<String> {
        if index >= self.item_text.len() {
            return None;
        }
        let text = self.item_text.remove(index);
        if index < self.disabled.len() {
            self.disabled.remove(index);
        }
        self.invalidate_children();
        self.clamp_offsets();
        self.ensure_visible();
        Some(text)
    }

    /// Insert an item from a plain string at `index`. Panics if `index > len()`.
    pub fn insert(&mut self, index: usize, item: String) {
        self.item_text.insert(index, item);
        self.disabled.insert(index, false);
        self.invalidate_children();
        if self.selected >= index
            && !self.item_text.is_empty()
            && self.selected + 1 < self.item_text.len()
        {
            self.selected += 1;
        }
        self.ensure_visible();
    }

    /// Remove and return the last item's text, if any.
    pub fn pop(&mut self) -> Option<String> {
        let text = self.item_text.pop()?;
        self.disabled.pop();
        self.invalidate_children();
        self.clamp_offsets();
        self.ensure_visible();
        Some(text)
    }

    // ── Selection / navigation internals ────────────────────────────────────

    fn max_offset(&self) -> usize {
        ScrollView::line_max_offset(self.item_text.len(), self.viewport_height.max(1))
    }

    fn is_selectable(&self, index: usize) -> bool {
        index < self.item_text.len() && !self.is_item_disabled(index)
    }

    fn selectable_count(&self) -> usize {
        (0..self.item_text.len())
            .filter(|idx| self.is_selectable(*idx))
            .count()
    }

    fn first_selectable(&self) -> Option<usize> {
        (0..self.item_text.len()).find(|idx| self.is_selectable(*idx))
    }

    fn last_selectable(&self) -> Option<usize> {
        (0..self.item_text.len())
            .rev()
            .find(|idx| self.is_selectable(*idx))
    }

    fn closest_selectable(&self, from: usize, direction: isize) -> Option<usize> {
        if self.selectable_count() == 0 {
            return None;
        }
        let max = self.item_text.len().saturating_sub(1) as isize;
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
        if self.item_text.is_empty() {
            self.selected = 0;
            self.offset = 0;
            self.hovered_index = None;
            self.pressed_index = None;
            return;
        }
        self.selected = self.selected.min(self.item_text.len() - 1);
        if !self.is_selectable(self.selected) {
            self.selected = self
                .closest_selectable(self.selected, 1)
                .or_else(|| self.closest_selectable(self.selected, -1))
                .or_else(|| self.first_selectable())
                .unwrap_or(0);
        }
        self.offset = self.offset.min(self.max_offset());
        if let Some(index) = self.hovered_index {
            if index >= self.item_text.len() {
                self.hovered_index = None;
            }
        }
    }

    fn ensure_visible(&mut self) {
        self.clamp_offsets();
        if self.item_text.is_empty() {
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

    /// Post the Python `ListView.Highlighted` message for the current selection.
    fn emit_highlighted(&self, ctx: &mut crate::event::WidgetCtx) {
        if self.is_selectable(self.selected)
            && let Some(item) = self.item_text.get(self.selected)
        {
            ctx.post_message(ListViewSelectionChanged {
                index: self.selected,
                item: item.clone(),
            });
        }
    }

    /// Post the Python `ListView.Selected` message for `index`.
    fn emit_selected(&self, index: usize, ctx: &mut crate::event::WidgetCtx) {
        if self.is_selectable(index)
            && let Some(item) = self.item_text.get(index)
        {
            ctx.post_message(ListViewItemActivated {
                index,
                item: item.clone(),
            });
        }
    }

    fn select_index(&mut self, index: usize, ctx: &mut crate::event::WidgetCtx) {
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
            self.emit_highlighted(ctx);
            ctx.request_repaint();
        }
    }

    fn move_selection(&mut self, delta: isize, ctx: &mut crate::event::WidgetCtx) {
        if self.selectable_count() == 0 {
            return;
        }
        let current = self.selected as isize;
        let max = (self.item_text.len() - 1) as isize;
        let mut next = (current + delta).clamp(0, max) as usize;
        let step = if delta >= 0 { 1 } else { -1 };
        while next < self.item_text.len() && !self.is_selectable(next) {
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

    /// Widest item text, in cells — the intrinsic content width used for
    /// `width: auto` sizing (the layout adds the box chrome on top).
    fn intrinsic_text_width(&self) -> Option<usize> {
        self.item_text
            .iter()
            .map(|t| rich_rs::cell_len(t).max(1))
            .max()
            .map(|w| w.max(1))
    }

    fn scroll_offset(&mut self, delta_rows: isize, ctx: &mut crate::event::WidgetCtx) {
        let before = self.offset;
        self.offset = ScrollView::line_scroll_by(
            self.offset,
            delta_rows as i32,
            self.item_text.len(),
            self.viewport_height.max(1),
        );
        if self.offset != before {
            ctx.request_repaint();
            ctx.set_handled();
        }
    }
}

impl crate::widgets::Focus for ListView {
    fn focusable(&self) -> bool {
        true
    }

    fn can_focus(&self) -> bool {
        true
    }

    fn can_focus_children(&self) -> bool {
        // Python: `ListView(..., can_focus_children=False)`.
        false
    }

    fn action_namespace(&self) -> &str {
        "list-view"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        // Python `ListView.BINDINGS` declares all of these `show=False`, so a
        // focused ListView must not flood the Footer with its nav hints.
        vec![
            BindingDecl::new("up", "cursor_up", "Move cursor up").hidden(),
            BindingDecl::new("down", "cursor_down", "Move cursor down").hidden(),
            BindingDecl::new("pageup", "scroll_up", "Page up").hidden(),
            BindingDecl::new("pagedown", "scroll_down", "Page down").hidden(),
            BindingDecl::new("home", "first", "Move to first item").hidden(),
            BindingDecl::new("end", "last", "Move to last item").hidden(),
            BindingDecl::new("enter", "select_cursor", "Select item").hidden(),
        ]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut crate::event::WidgetCtx) -> bool {
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
                self.emit_selected(self.selected, ctx);
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }
}

impl crate::widgets::Interactive for ListView {
    /// Post the initial `ListViewSelectionChanged` at mount, mirroring Python's
    /// `watch_index` firing for the initial highlight. RA2.3: this replaces the
    /// former mount-message staging hook — `on_mount` runs with a `WidgetCtx` in
    /// every mount path, so the message posts (and bubbles) through the normal bus.
    fn on_mount(&mut self, ctx: &mut crate::event::WidgetCtx) {
        if self.pending_initial_highlight
            && self.is_selectable(self.selected)
            && let Some(item) = self.item_text.get(self.selected)
        {
            ctx.post_message(ListViewSelectionChanged {
                index: self.selected,
                item: item.clone(),
            });
        }
        self.pending_initial_highlight = false;
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

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        // A child ListItem was clicked (Python: `_on_list_item__child_clicked`):
        // focus the list, highlight the item, and post `Selected`.
        if let Some(clicked) = message.downcast_ref::<ListItemChildClicked>() {
            let index = clicked.ordinal;
            if self.is_selectable(index) {
                if index != self.selected {
                    self.selected = index;
                    self.ensure_visible();
                    self.emit_highlighted(ctx);
                }
                self.emit_selected(index, ctx);
                ctx.request_repaint();
            }
            ctx.set_handled();
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        match event {
            // Headless mouse path (row-based): used when ListView is embedded as
            // a state model (the command palette) rather than mounted with real
            // ListItem children. Composed lists are driven by ListItemChildClicked.
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
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
            Event::MouseUp(mouse) if mouse.target.is_some_and(|t| t == self.node_id()) => {
                let index = self.offset.saturating_add(mouse.y as usize);
                if self.pressed_index == Some(index) && self.is_selectable(index) {
                    self.emit_selected(index, ctx);
                    ctx.set_handled();
                }
                self.pressed_index = None;
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
                _ => {}
            },
            Event::Key(key) if self.node_state().focused => match key.code {
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
                    self.emit_selected(self.selected, ctx);
                    ctx.set_handled();
                }
                _ => {}
            },
            Event::AppFocus(false) => {
                self.pressed_index = None;
                if self.hovered_index.is_some() {
                    self.hovered_index = None;
                    ctx.request_repaint();
                }
            }
            _ => {}
        }
    }

    fn on_mouse_move(&mut self, _x: u16, y: u16) -> bool {
        if self.item_text.is_empty() {
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

    fn on_unmount(&mut self) {
        self.hovered_index = None;
        self.pressed_index = None;
    }
}

impl crate::widgets::Layout for ListView {
    /// Drive the highlight (and hover) on child `ListItem` nodes by index. This
    /// is the canonical arena mechanism (mirrors Python's `watch_index` setting
    /// `-highlight` on the highlighted child). The highlight is background-only.
    fn child_classes_for_tree(&self, child_index: usize) -> Vec<(&'static str, bool)> {
        // The highlight is independent of focus — the `:focus` CSS variant
        // restyles the highlighted item when the list is focused.
        let highlighted = child_index == self.selected && self.is_selectable(child_index);
        let hovered = self.hovered_index == Some(child_index) && !highlighted;
        vec![("-highlight", highlighted), ("-hovered", hovered)]
    }

    fn layout_height(&self) -> Option<usize> {
        // height: auto — pre-mount estimate from the pending items. After
        // extraction the arena owns layout, so return None.
        if self.children_extracted {
            return None;
        }
        if !self.pending_items.is_empty() {
            let mut total = 0usize;
            for item in &self.pending_items {
                match crate::widgets::Widget::layout_height(item) {
                    Some(h) => total = total.saturating_add(h.max(1)),
                    None => return None,
                }
            }
            return Some(total.max(1));
        }
        // Text-only estimate: one row per item (real heights come from the
        // arena once the `ListItem`/`Label` children are mounted).
        Some(self.item_text.len().max(1))
    }

    fn auto_content_width(&self) -> Option<usize> {
        // For `width: auto`, report the widest item text regardless of arena
        // extraction (the authoritative `item_text` survives a drain).
        self.intrinsic_text_width()
    }

    fn content_width(&self) -> Option<usize> {
        if self.children_extracted {
            return None;
        }
        self.intrinsic_text_width()
    }
}

impl crate::widgets::Scrollable for ListView {
    fn on_mouse_scroll(&mut self, _delta_x: i32, delta_y: i32, ctx: &mut crate::event::WidgetCtx) {
        if delta_y == 0 {
            return;
        }
        self.scroll_offset(
            delta_y.saturating_mul(self.scroll_step as i32) as isize,
            ctx,
        );
    }
}

impl crate::widgets::Render for ListView {
    fn compose(&mut self) -> ComposeResult {
        if self.children_extracted {
            return Vec::new();
        }
        self.children_extracted = true;
        // Use the user-supplied items on first mount; on a recompose (after a
        // text-based mutation such as `append`/`set_items`) the pending buffer is
        // empty, so rebuild every item from the retained `item_text`/`disabled`
        // state — this keeps previously-mounted items and avoids dropping any.
        let mut items = std::mem::take(&mut self.pending_items);
        if items.is_empty() && !self.item_text.is_empty() {
            items = self
                .item_text
                .iter()
                .enumerate()
                .map(|(idx, text)| {
                    ListItem::from_text(text.clone()).disabled(self.is_item_disabled(idx))
                })
                .collect();
        }
        let mut out: ComposeResult = Vec::with_capacity(items.len());
        for (ordinal, mut item) in items.into_iter().enumerate() {
            item.set_ordinal(ordinal);
            out.push(crate::compose::ChildDecl::new(Box::new(item)));
        }
        out
    }

    fn style_type(&self) -> &'static str {
        "ListView"
    }

    /// Chrome-only render. The `ListItem` children render through the arena
    /// tree; `ListView` only paints its own resolved surface (background/tint).
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let resolved = css::resolve_style(self, &css::selector_meta_generic(self));
        let paints_surface = resolved.bg.is_some()
            || resolved.hatch.is_some()
            || resolved.border_top.is_set()
            || resolved.border_right.is_set()
            || resolved.border_bottom.is_set()
            || resolved.border_left.is_set();
        if !paints_surface {
            return Segments::new();
        }
        let height = options.size.1.max(1);
        let mut out = Segments::new();
        for idx in 0..height {
            out.push(Segment::new(" ".repeat(width)));
            if idx + 1 < height {
                out.push(Segment::line());
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::{ListItem, ListView};
    use crate::event::{Event, EventCtx, MouseDownEvent, MouseUpEvent};
    use crate::keys::KeyEventData;
    use crate::message::*;
    use crate::node_id::NodeId;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
    use crate::widgets::{Label, NodeState, Widget};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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

    // ── Composition / arena tests ───────────────────────────────────────

    #[test]
    fn from_list_items_keeps_text() {
        let list = ListView::from_list_items(vec![
            ListItem::new(Label::new("One")),
            ListItem::new(Label::new("Two")),
        ]);
        assert_eq!(list.items(), &["One", "Two"]);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn compose_drains_items_with_ordinals() {
        let mut list = ListView::from_list_items(vec![
            ListItem::new(Label::new("a")),
            ListItem::new(Label::new("b")),
            ListItem::new(Label::new("c")),
        ]);
        let children = list.compose();
        assert_eq!(children.len(), 3);
        // After extraction the call is idempotent.
        assert!(list.compose().is_empty());
        // Item text survives extraction (headless state machine still works).
        assert_eq!(list.items(), &["a", "b", "c"]);
    }

    #[test]
    fn can_focus_children_is_false() {
        let list = ListView::new(vec!["A".into()]);
        assert!(list.focusable());
        assert!(!list.can_focus_children());
    }

    #[test]
    fn child_classes_highlight_selected_only() {
        let mut list = ListView::new(vec!["A".into(), "B".into(), "C".into()]);
        list.set_selected(1);
        let c0 = list.child_classes_for_tree(0);
        let c1 = list.child_classes_for_tree(1);
        assert!(c0.contains(&("-highlight", false)));
        assert!(c1.contains(&("-highlight", true)));
    }

    #[test]
    fn child_classes_mark_hover_when_not_highlighted() {
        let mut list = ListView::new(vec!["A".into(), "B".into()]);
        list.set_selected(0);
        assert!(list.on_mouse_move(0, 1));
        let c1 = list.child_classes_for_tree(1);
        assert!(c1.contains(&("-hovered", true)));
        // The highlighted row never shows hover.
        list.set_selected(1);
        let c1 = list.child_classes_for_tree(1);
        assert!(c1.contains(&("-hovered", false)));
    }

    /// Run `on_mount` with a synthesized `WidgetCtx` and return the messages it
    /// posted (RA2.3 replaced the former mount-message staging hook).
    fn on_mount_messages(list: &mut ListView) -> Vec<crate::message::MessageEvent> {
        let mut ctx = crate::event::EventCtx::default();
        {
            let mut wctx = crate::event::WidgetCtx::__from_dispatch(
                crate::node_id::NodeId::default(),
                &mut ctx,
            );
            list.on_mount(&mut wctx);
        }
        ctx.take_messages()
    }

    #[test]
    fn initial_highlight_message_posted_at_mount() {
        let mut list = ListView::new(vec!["A".into(), "B".into()]);
        let msgs = on_mount_messages(&mut list);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].is::<ListViewSelectionChanged>());
        // Only fires once — the pending flag is cleared at mount.
        assert!(on_mount_messages(&mut list).is_empty());
    }

    #[test]
    fn empty_list_posts_no_initial_highlight() {
        let mut list = ListView::new(vec![]);
        assert!(on_mount_messages(&mut list).is_empty());
    }

    // ── Selection / message tests ───────────────────────────────────────

    #[test]
    fn enter_selects_current_item() {
        let mut list = ListView::new(vec!["one".to_string(), "two".to_string()]);
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        list.set_selected(1);

        let key = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(&Event::Key(key), &mut __w);
        }

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        let activated = messages[0]
            .downcast_ref::<ListViewItemActivated>()
            .expect("Selected");
        assert_eq!(activated.index, 1);
        assert_eq!(activated.item, "two");
    }

    #[test]
    fn child_clicked_message_highlights_and_selects() {
        let mut list = ListView::new(vec!["one".to_string(), "two".to_string()]);
        let _guard = set_dispatch_recipient(make_node_id(), NodeState::default());
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_message(
            &MessageEvent::new(
                NodeId::default(),
                ListItemChildClicked {
                    ordinal: 1,
                    item: "two".to_string(),
                },
            ),
            &mut __w);
        }
        assert_eq!(list.selected(), 1);
        let messages = ctx.take_messages();
        // Highlighted (selection changed) + Selected.
        assert!(
            messages
                .iter()
                .any(|m| m.downcast_ref::<ListViewSelectionChanged>().is_some())
        );
        assert!(
            messages
                .iter()
                .any(|m| m.downcast_ref::<ListViewItemActivated>().is_some())
        );
        assert!(ctx.handled());
    }

    #[test]
    fn headless_mouse_click_selects_row() {
        let mut list = ListView::new(vec!["one".to_string(), "two".to_string()]);
        list.on_layout(20, 2);
        let id = NodeId::default();

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 1,
                x: 0,
                y: 1,
            }),
            &mut __w);
        }
        assert!(ctx.handled());
        assert_eq!(list.selected(), 1);

        let mut up_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut up_ctx);
            list.on_event(
            &Event::MouseUp(MouseUpEvent {
                target: Some(id),
                screen_x: 0,
                screen_y: 1,
                x: 0,
                y: 1,
            }),
            &mut __w);
        }
        let messages = up_ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is::<ListViewItemActivated>());
    }

    #[test]
    fn app_focus_loss_clears_hover_state() {
        let mut list = ListView::new(vec!["one".to_string(), "two".to_string()]);
        assert!(list.on_mouse_move(0, 0));
        assert_eq!(list.hovered_index(), Some(0));

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(&Event::AppFocus(false), &mut __w);
        }

        assert_eq!(list.hovered_index(), None);
        assert!(ctx.repaint_requested());
    }

    #[test]
    fn bindings_are_declared() {
        let list = ListView::new(vec!["A".into(), "B".into()]);
        let bindings = list.bindings();
        assert!(bindings.iter().any(|b| b.action == "cursor_up"));
        assert!(bindings.iter().any(|b| b.action == "cursor_down"));
        assert!(bindings.iter().any(|b| b.action == "select_cursor"));
    }

    #[test]
    fn execute_action_handles_cursor_down() {
        use crate::action::ParsedAction;
        let mut list = ListView::new(vec!["A".into(), "B".into(), "C".into()]);
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "cursor_down".to_string(),
            arguments: vec![],
        };
        assert!({ let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.execute_action(&action, &mut __w) });
        assert_eq!(list.selected(), 1);
    }

    // ── Mutation API tests ──────────────────────────────────────────────

    #[test]
    fn append_adds_item() {
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
        assert_eq!(list.remove(1).as_deref(), Some("B"));
        assert_eq!(list.items(), &["A", "C"]);
    }

    #[test]
    fn remove_out_of_bounds_returns_none() {
        let mut list = ListView::new(vec!["A".into()]);
        assert!(list.remove(5).is_none());
        assert_eq!(list.items().len(), 1);
    }

    #[test]
    fn remove_adjusts_selected_when_at_end() {
        let mut list = ListView::new(vec!["A".into(), "B".into()]);
        list.set_selected(1);
        list.remove(1);
        assert_eq!(list.selected(), 0);
    }

    #[test]
    fn insert_at_beginning_shifts_selection() {
        let mut list = ListView::new(vec!["B".into(), "C".into()]);
        list.set_selected(0);
        list.insert(0, "A".into());
        assert_eq!(list.items(), &["A", "B", "C"]);
        assert_eq!(list.selected(), 1);
    }

    #[test]
    fn pop_removes_last_item() {
        let mut list = ListView::new(vec!["A".into(), "B".into(), "C".into()]);
        assert_eq!(list.pop().as_deref(), Some("C"));
        assert_eq!(list.items(), &["A", "B"]);
    }

    #[test]
    fn navigation_skips_disabled_items() {
        let mut list = ListView::new(vec!["one".into(), "two".into(), "three".into()]);
        list.set_item_disabled(1, true);
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        list.on_layout(20, 3);
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(&Event::Action(crate::event::Action::ScrollDown), &mut __w);
        }
        assert_eq!(list.selected(), 2);
    }

    #[test]
    fn mouse_scroll_clamps_to_bounds() {
        let mut list = ListView::new((0..10).map(|i| format!("item-{i}")).collect());
        list.on_layout(20, 3);
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_mouse_scroll(0, 100, &mut __w);
        }
        assert!(ctx.handled());
        assert_eq!(list.offset(), 7);
    }

    #[test]
    fn compose_drains_declared_items() {
        // compose() is now the single child path — it yields the declared items.
        let mut list = ListView::new(vec!["A".into()]);
        assert_eq!(list.compose().len(), 1);
        // Idempotent: re-composing a drained list yields nothing.
        assert!(list.compose().is_empty());
    }
}
