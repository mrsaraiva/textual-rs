use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event};
use crate::message::*;

use super::option_list::{OptionId, OptionItem, OptionList, OptionListError};
use super::{NodeSeed, Widget, helpers::adjust_line_length_no_bg};

// ── Toggle-button characters (matching Python Textual's ToggleButton) ───

const BUTTON_LEFT: &str = "▐";
const BUTTON_RIGHT: &str = "▌";
// Python's `ToggleButton.BUTTON_INNER = "X"` is rendered for BOTH states; the
// selected/unselected distinction is conveyed purely by the button foreground
// color (invisible-ish when unselected because fg ≈ bg), not by swapping the
// glyph for a space.
const BUTTON_INNER: &str = "X";

/// A single selection entry for a [`SelectionList`].
///
/// Generic over the value type `T`.
#[derive(Debug, Clone)]
pub struct Selection<T: Clone + PartialEq> {
    /// The display text shown to the user.
    pub prompt: String,
    /// A value associated with this selection.
    pub value: T,
    /// Whether this selection starts in the selected state.
    pub initially_selected: bool,
    /// Whether this selection is disabled.
    pub disabled: bool,
    /// Optional stable id, passed through to the underlying [`OptionItem`]
    /// (Python parity: `Selection(..., id=...)`, distinct from `value`).
    pub id: Option<OptionId>,
}

impl<T: Clone + PartialEq> Selection<T> {
    /// Create a new selection with default (unselected) state.
    pub fn new(prompt: impl Into<String>, value: T) -> Self {
        Self {
            prompt: prompt.into(),
            value,
            initially_selected: false,
            disabled: false,
            id: None,
        }
    }

    /// Create a new selection that starts selected.
    pub fn selected(prompt: impl Into<String>, value: T) -> Self {
        Self {
            prompt: prompt.into(),
            value,
            initially_selected: true,
            disabled: false,
            id: None,
        }
    }

    /// Create a new selection that is disabled.
    pub fn disabled(prompt: impl Into<String>, value: T) -> Self {
        Self {
            prompt: prompt.into(),
            value,
            initially_selected: false,
            disabled: true,
            id: None,
        }
    }

    /// Builder: attach a stable id to this selection.
    pub fn with_id(mut self, id: impl Into<OptionId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// The underlying option row for this selection.
    fn to_option_item(&self) -> OptionItem {
        OptionItem::Option {
            prompt: self.prompt.clone(),
            content: None,
            id: self.id.clone(),
            disabled: self.disabled,
        }
    }
}

/// Backwards-compatible type alias for `SelectionList<String>`.
pub type SelectionListString = SelectionList<String>;

/// A vertical selection list that allows making multiple selections.
///
/// Generic over the value type `T`. Use [`SelectionListString`] for string-valued lists.
///
/// Wraps an inner [`OptionList`] for navigation, adding per-item toggle checkboxes
/// rendered as `▐X▌` before each option's prompt. The `X` glyph is always present
/// (matching Python's `ToggleButton`); selected vs. deselected is conveyed by the
/// button foreground color.
///
/// # Messages
///
/// - [`SelectionListToggled`] — posted when an individual item is toggled.
/// - [`SelectionListSelectedChanged`] — posted when the overall selected set changes.
pub struct SelectionList<T: Clone + PartialEq + Send + Sync + 'static> {
    inner: OptionList,
    disabled: bool,
    /// The values associated with each selection.
    values: Vec<T>,
    /// Per-index selection state.
    selected_set: Vec<bool>,
    /// Selection INSERTION order (Python `_selected` is an ordered dict:
    /// `selected` reports values in the order they were selected, not in
    /// option order).
    selected_order: Vec<usize>,
    hovered_index: Option<usize>,
    border_title_text: Option<String>,
    seed: NodeSeed,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Default for SelectionList<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> SelectionList<T> {
    crate::seed_ident_methods!();

    /// Create an empty `SelectionList`.
    pub fn new() -> Self {
        let seed = NodeSeed {
            classes: vec!["selection-list".to_string()],
            ..NodeSeed::default()
        };
        Self {
            inner: OptionList::new(),
            disabled: false,
            values: Vec::new(),
            selected_set: Vec::new(),
            selected_order: Vec::new(),
            hovered_index: None,
            border_title_text: None,
            seed,
        }
    }

    /// Create a `SelectionList` pre-populated with selections.
    ///
    /// # Panics
    ///
    /// Panics if two selections carry the same id (same constructor policy as
    /// [`OptionList::with_items`]).
    pub fn with_selections(selections: Vec<Selection<T>>) -> Self {
        let mut list = Self::new();
        let items: Vec<OptionItem> = selections.iter().map(Selection::to_option_item).collect();
        let values: Vec<T> = selections.iter().map(|s| s.value.clone()).collect();
        let selected: Vec<bool> = selections
            .iter()
            .map(|s| s.initially_selected && !s.disabled)
            .collect();
        list.inner = OptionList::with_items(items);
        list.values = values;
        list.selected_order = selected
            .iter()
            .enumerate()
            .filter_map(|(i, &sel)| sel.then_some(i))
            .collect();
        list.selected_set = selected;
        list
    }

    /// Builder: set a border title (rendered on the top border).
    pub fn with_border_title(mut self, title: impl Into<String>) -> Self {
        self.border_title_text = Some(title.into());
        self
    }

    /// Builder: set disabled state for the entire list.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    // ── Public API ──────────────────────────────────────────────────

    /// Toggle the selection state of the item at `index`.
    pub fn toggle(&mut self, index: usize, ctx: &mut crate::event::WidgetCtx) {
        if index >= self.selected_set.len() || !self.item_is_selectable(index) {
            return;
        }
        self.selected_set[index] = !self.selected_set[index];
        let selected = self.selected_set[index];
        if selected {
            self.selected_order.push(index);
        } else {
            self.selected_order.retain(|&i| i != index);
        }
        ctx.post_message(SelectionListToggled {
            index,
            selected,
            option_id: self
                .inner
                .get_option(index)
                .and_then(|item| item.id().cloned()),
        });
        ctx.post_message(SelectionListSelectedChanged);
        ctx.request_repaint();
    }

    /// Mark the item at `index` as selected (no-op if already selected).
    pub fn select(&mut self, index: usize, ctx: &mut crate::event::WidgetCtx) {
        if index >= self.selected_set.len()
            || self.selected_set[index]
            || !self.item_is_selectable(index)
        {
            return;
        }
        self.selected_set[index] = true;
        self.selected_order.push(index);
        ctx.post_message(SelectionListSelectedChanged);
        ctx.request_repaint();
    }

    /// Mark the item at `index` as deselected (no-op if already deselected).
    pub fn deselect(&mut self, index: usize, ctx: &mut crate::event::WidgetCtx) {
        if index >= self.selected_set.len()
            || !self.selected_set[index]
            || !self.item_is_selectable(index)
        {
            return;
        }
        self.selected_set[index] = false;
        self.selected_order.retain(|&i| i != index);
        ctx.post_message(SelectionListSelectedChanged);
        ctx.request_repaint();
    }

    /// Select all items.
    pub fn select_all(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let selectable: Vec<bool> = (0..self.selected_set.len())
            .map(|index| self.item_is_selectable(index))
            .collect();
        let mut changed = false;
        for (index, sel) in self.selected_set.iter_mut().enumerate() {
            if selectable[index] && !*sel {
                *sel = true;
                self.selected_order.push(index);
                changed = true;
            }
        }
        if changed {
            ctx.post_message(SelectionListSelectedChanged);
            ctx.request_repaint();
        }
    }

    /// Toggle all items (selected become deselected and vice versa).
    pub fn toggle_all(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let selectable: Vec<bool> = (0..self.selected_set.len())
            .map(|index| self.item_is_selectable(index))
            .collect();
        let mut changed = false;
        for (index, sel) in self.selected_set.iter_mut().enumerate() {
            if selectable[index] {
                *sel = !*sel;
                if *sel {
                    self.selected_order.push(index);
                } else {
                    self.selected_order.retain(|&i| i != index);
                }
                changed = true;
            }
        }
        if changed {
            ctx.post_message(SelectionListSelectedChanged);
            ctx.request_repaint();
        }
    }

    /// Deselect all items.
    pub fn deselect_all(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let selectable: Vec<bool> = (0..self.selected_set.len())
            .map(|index| self.item_is_selectable(index))
            .collect();
        let mut changed = false;
        for (index, sel) in self.selected_set.iter_mut().enumerate() {
            if selectable[index] && *sel {
                *sel = false;
                changed = true;
            }
        }
        if changed {
            self.selected_order.clear();
        }
        if changed {
            ctx.post_message(SelectionListSelectedChanged);
            ctx.request_repaint();
        }
    }

    /// Returns a `Vec` of indices that are currently selected, in selection
    /// (insertion) order — Python `SelectionList.selected` parity.
    pub fn selected(&self) -> Vec<usize> {
        self.selected_order.clone()
    }

    /// Whether the item at `index` is currently selected.
    pub fn is_selected(&self, index: usize) -> bool {
        self.selected_set.get(index).copied().unwrap_or(false)
    }

    /// Returns the values of all currently selected items, in selection
    /// (insertion) order — Python `SelectionList.selected` parity.
    pub fn selected_values(&self) -> Vec<&T> {
        self.selected_order
            .iter()
            .filter_map(|&i| self.values.get(i))
            .collect()
    }

    /// Returns the value associated with the item at `index`.
    pub fn value_at(&self, index: usize) -> Option<&T> {
        self.values.get(index)
    }

    /// The currently highlighted index in the inner list.
    pub fn highlighted(&self) -> Option<usize> {
        self.inner.highlighted()
    }

    /// Number of items.
    pub fn item_count(&self) -> usize {
        self.inner.option_count()
    }

    // ── Identity CRUD (rides the inner OptionList registry) ───────────

    /// Add a selection to the end of the list (Python `add_option`).
    ///
    /// Returns `Err(OptionListError::DuplicateId)` on id collision; the list
    /// is not modified.
    pub fn add_selection(&mut self, selection: Selection<T>) -> Result<(), OptionListError> {
        self.inner.add_item(selection.to_option_item())?;
        self.values.push(selection.value);
        let selected = selection.initially_selected && !selection.disabled;
        self.selected_set.push(selected);
        if selected {
            self.selected_order.push(self.selected_set.len() - 1);
        }
        Ok(())
    }

    /// Add a batch of selections (Python `add_options`): the whole batch is
    /// validated first; a failing batch adds NOTHING.
    pub fn add_selections(
        &mut self,
        selections: Vec<Selection<T>>,
    ) -> Result<(), OptionListError> {
        let items: Vec<OptionItem> = selections.iter().map(Selection::to_option_item).collect();
        self.inner.add_options(items)?;
        for selection in selections {
            self.values.push(selection.value);
            let selected = selection.initially_selected && !selection.disabled;
            self.selected_set.push(selected);
            if selected {
                self.selected_order.push(self.selected_set.len() - 1);
            }
        }
        Ok(())
    }

    /// Get a selection's option row by stable id.
    pub fn get_option_by_id(&self, id: &str) -> Result<&OptionItem, OptionListError> {
        self.inner.get_option_by_id(id)
    }

    /// Get the current index of the selection with the given id.
    pub fn get_option_index(&self, id: &str) -> Result<usize, OptionListError> {
        self.inner.get_option_index(id)
    }

    /// Get a selection's option row by index, with a typed error.
    pub fn get_option_at_index(&self, index: usize) -> Result<&OptionItem, OptionListError> {
        self.inner.get_option_at_index(index)
    }

    /// Remove the selection with the given id, repairing the parallel
    /// value/selected bookkeeping in lockstep (the Rust wrapper owns `inner`,
    /// so it IS Python's `_pre_remove_option` hook).
    pub fn remove_option(&mut self, id: &str) -> Result<(), OptionListError> {
        let index = self.inner.get_option_index(id)?;
        self.remove_option_at_index(index)
    }

    /// Remove the selection at the given index, repairing the parallel
    /// value/selected bookkeeping in lockstep.
    pub fn remove_option_at_index(&mut self, index: usize) -> Result<(), OptionListError> {
        self.inner.remove_option_at_index(index)?;
        if index < self.values.len() {
            self.values.remove(index);
        }
        if index < self.selected_set.len() {
            self.selected_set.remove(index);
        }
        self.selected_order.retain(|&i| i != index);
        for stored in self.selected_order.iter_mut() {
            if *stored > index {
                *stored -= 1;
            }
        }
        if self.hovered_index == Some(index) {
            self.hovered_index = None;
        } else if let Some(hovered) = self.hovered_index {
            if hovered > index {
                self.hovered_index = Some(hovered - 1);
            }
        }
        Ok(())
    }

    /// Remove all selections, clearing values and selected state too
    /// (Python: clearing the options clears the selections).
    pub fn clear_options(&mut self) {
        self.inner.clear_options();
        self.values.clear();
        self.selected_set.clear();
        self.selected_order.clear();
        self.hovered_index = None;
    }

    // ── Internals ───────────────────────────────────────────────────

    fn item_is_selectable(&self, index: usize) -> bool {
        self.inner
            .get_option(index)
            .is_some_and(OptionItem::is_selectable)
    }

    /// Width of the toggle button prefix: `▐X▌ ` = 4 cells.
    fn button_width() -> usize {
        4
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Widget for SelectionList<T> {
    fn border_title(&self) -> Option<&str> {
        self.border_title_text.as_deref()
    }

    fn focusable(&self) -> bool {
        !self.disabled
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

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    /// Python `SelectionList.BINDINGS` adds `space → select` (show=False) on
    /// top of the `OptionList.BINDINGS` it inherits, so both enter (inherited)
    /// and space route to `select`. Declarative bindings are resolved
    /// focused→root, so a focused SelectionList's `down → cursor_down` wins
    /// over an ancestor scroll container's `down → scroll_down` — exactly like
    /// Python's binding chain. Raw `on_event` key handling would LOSE to the
    /// ancestor binding (bindings dispatch first), so the keyboard behavior
    /// lives here, not in `on_event`.
    fn bindings(&self) -> Vec<crate::widgets::BindingDecl> {
        let mut bindings = Widget::bindings(&self.inner);
        bindings.push(crate::widgets::BindingDecl::new("space", "select", "Toggle option").hidden());
        bindings
    }

    fn execute_action(
        &mut self,
        action: &crate::action::ParsedAction,
        ctx: &mut crate::event::WidgetCtx,
    ) -> bool {
        if self.disabled {
            return false;
        }
        match action.name.as_str() {
            // Python: enter/space run OptionList's `action_select`, which posts
            // `OptionSelected`; SelectionList intercepts that event
            // (`_on_option_list_option_selected` → `event.stop()`) and toggles
            // the highlighted selection instead of re-emitting it.
            "select" => {
                if let Some(index) = self.inner.highlighted() {
                    if self.item_is_selectable(index) {
                        self.toggle(index, ctx);
                        ctx.set_handled();
                    }
                }
                true
            }
            // Navigation actions (cursor_up/cursor_down/first/last/
            // page_up/page_down) are inherited from the inner OptionList.
            _ => Widget::execute_action(&mut self.inner, action, ctx),
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        if self.disabled {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                // Compute the item index from the click position.
                let index = self
                    .inner
                    .offset_for_click()
                    .saturating_add(mouse.y as usize);
                if index < self.inner.option_count() && self.item_is_selectable(index) {
                    // First, highlight the clicked item via the inner list.
                    self.inner.set_highlighted(index);
                    // Then toggle it.
                    self.toggle(index, ctx);
                    ctx.set_handled();
                }
            }
            Event::Action(Action::Toggle) if self.node_state().focused => {
                if let Some(index) = self.inner.highlighted() {
                    if self.item_is_selectable(index) {
                        self.toggle(index, ctx);
                        ctx.set_handled();
                    }
                }
            }
            // Delegate action-based scroll to inner list.
            Event::Action(
                Action::ScrollUp
                | Action::ScrollDown
                | Action::ScrollPageUp
                | Action::ScrollPageDown,
            ) if self.node_state().focused => {
                self.inner.on_event(event, ctx);
            }
            _ => {}
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if self.disabled {
            return false;
        }
        let index = self.inner.offset_for_click().saturating_add(y as usize);
        let hovered = if index < self.inner.option_count() {
            Some(index)
        } else {
            None
        };
        let changed = hovered != self.hovered_index;
        self.hovered_index = hovered;
        // Also delegate to inner so its internal state stays consistent.
        let inner_changed = self.inner.on_mouse_move(x, y);
        changed || inner_changed
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut crate::event::WidgetCtx) {
        if self.disabled {
            return;
        }
        self.inner.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        // The SelectionList's own composited surface (its `$surface` bg plus
        // the `:focus` `background-tint`). Mirrors OptionList: all component
        // colours (highlight backgrounds, auto foregrounds) compose over this
        // surface, and component styles resolve with an EMPTY-type leaf meta so
        // the `OptionList { background: $surface }` base rule does not stamp an
        // opaque surface onto every row. The SelectionList node meta (with its
        // OptionList alias + :focus state) is already on the selector stack.
        let surface_flat = crate::css::current_composited_background().unwrap_or_else(|| {
            crate::style::parse_color_like("$surface").unwrap_or(crate::style::Color::rgb(0, 0, 0))
        });
        let resolve_comp = |classes: &[&str]| -> crate::style::Style {
            crate::css::resolve_style_for_meta(&crate::css::selector_meta_component("", classes))
        };
        let base_style = resolve_comp(&["option-list--option"])
            .to_rich_over(surface_flat)
            .unwrap_or_default();

        let btn_width = Self::button_width();

        for row in 0..height {
            let index = self.inner.offset_for_click() + row;
            let highlighted = self.inner.highlighted() == Some(index);
            let hovered_row = self.hovered_index == Some(index);
            let selected = self.is_selected(index);

            if let Some(item) = self.inner.get_option(index) {
                match item {
                    OptionItem::Separator => {
                        let sep_style = resolve_comp(&["option-list--separator"])
                            .to_rich_over(surface_flat)
                            .unwrap_or(base_style);
                        let text = "─".repeat(width);
                        let line =
                            adjust_line_length_no_bg(&[Segment::styled(text, sep_style)], width);
                        out.extend(line);
                    }
                    OptionItem::Option {
                        prompt, disabled, ..
                    } => {
                        // Resolve the option row style. Mirror OptionList's
                        // `_get_option_style`: base + ONE state-specific
                        // component class (disabled > highlighted > hover); the
                        // `:focus` variant of the highlighted colours comes from
                        // the `OptionList:focus > ...` rule matched via the
                        // focused SelectionList meta on the selector stack.
                        let mut opt_classes = vec!["option-list--option"];
                        if *disabled {
                            opt_classes.push("option-list--option-disabled");
                        } else if highlighted {
                            opt_classes.push("option-list--option-highlighted");
                        } else if hovered_row {
                            opt_classes.push("option-list--option-hover");
                        }
                        let mut opt_crate = resolve_comp(&opt_classes);
                        // Resolve an auto-contrast foreground against the widget
                        // surface (same rationale as OptionList).
                        if opt_crate.fg.is_none() {
                            if let Some(auto) = opt_crate.fg_auto {
                                let contrast = crate::style::contrast_text(surface_flat);
                                opt_crate.fg =
                                    Some(contrast.blend_over_float(surface_flat, auto.alpha()));
                                opt_crate.fg_auto = None;
                            }
                        }
                        // Compose the highlighted (possibly semi-transparent)
                        // background over the widget surface.
                        if highlighted {
                            if let Some(bg) = opt_crate.bg {
                                opt_crate.bg = Some(bg.flatten_over(surface_flat));
                            }
                        }
                        let opt_style = opt_crate
                            .to_rich_over(surface_flat)
                            .unwrap_or(base_style);

                        // Resolve button component style.
                        let mut btn_class = "selection-list--button".to_string();
                        if selected {
                            btn_class.push_str("-selected");
                        }
                        if highlighted {
                            btn_class.push_str("-highlighted");
                        }
                        let btn_style = resolve_comp(&[&btn_class])
                            .to_rich_over(surface_flat)
                            .unwrap_or(opt_style);

                        // Button prefix is always `▐X▌`; `btn_style` color (from
                        // the resolved component class) conveys selected state.
                        let inner_char = BUTTON_INNER;

                        // Side style: button fg on option bg (for the half-block chars).
                        let side_style = {
                            let mut s = rich_rs::Style::new();
                            s.color = btn_style.bgcolor;
                            s.bgcolor = opt_style.bgcolor;
                            s
                        };

                        // The button glyphs (`▐X▌`) occupy 3 cells; the 4th
                        // `button_width` cell is the leading space of the prompt
                        // text, so the prompt run must span the REMAINING width
                        // (otherwise the row is one cell short and the highlight
                        // stops one column early).
                        let prompt_width = width.saturating_sub(btn_width.saturating_sub(1));
                        let prompt_text =
                            rich_rs::set_cell_size(&format!(" {prompt}"), prompt_width);

                        let segments = [
                            Segment::styled(BUTTON_LEFT.to_string(), side_style),
                            Segment::styled(inner_char.to_string(), btn_style),
                            Segment::styled(BUTTON_RIGHT.to_string(), side_style),
                            Segment::styled(prompt_text, opt_style),
                        ];

                        let line = adjust_line_length_no_bg(&segments, width);
                        out.extend(line);
                    }
                }
            } else {
                // Empty row below the items.
                let line =
                    adjust_line_length_no_bg(&[Segment::styled(String::new(), base_style)], width);
                out.extend(line);
            }

            if row + 1 < height {
                out.push(Segment::line());
            }
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        // PURE content height (one row per option). The flow layout adds the
        // CSS-resolved vertical chrome (e.g. selection_list_selected.tcss's
        // border/padding) with ancestor context, symmetric with the width axis.
        Some(self.inner.option_count().max(1))
    }

    fn content_width(&self) -> Option<usize> {
        // OptionList's content_width includes a 2-cell indent prefix. We replace that
        // with our 4-cell button prefix (▐X▌ + space), so add 2 to OptionList width.
        let content_width = self.inner.content_width().unwrap_or(1).saturating_add(2);
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        Some(content_width.saturating_add(chrome_lr).max(1))
    }

    fn style_type(&self) -> &'static str {
        "SelectionList"
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        // Python MRO: SelectionList(OptionList) — the `OptionList { ... }`
        // DEFAULT_CSS block (surface bg, tall border, option component styles)
        // must match a SelectionList node too.
        &["OptionList"]
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Renderable for SelectionList<T> {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::node_id::NodeId;

    #[test]
    fn selection_list_initial_state() {
        let selections = vec![
            Selection::new("Alpha", "a".to_string()),
            Selection::selected("Beta", "b".to_string()),
            Selection::new("Gamma", "g".to_string()),
        ];
        let list = SelectionList::with_selections(selections);
        assert_eq!(list.item_count(), 3);
        assert!(!list.is_selected(0));
        assert!(list.is_selected(1));
        assert!(!list.is_selected(2));
        assert_eq!(list.selected(), vec![1]);
    }

    #[test]
    fn selection_list_toggle() {
        let selections = vec![
            Selection::new("Alpha", "a".to_string()),
            Selection::new("Beta", "b".to_string()),
        ];
        let mut list = SelectionList::with_selections(selections);
        let mut ctx = EventCtx::default();

        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.toggle(0, &mut __w) };
        assert!(list.is_selected(0));

        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.toggle(0, &mut __w) };
        assert!(!list.is_selected(0));
    }

    /// `SelectionListToggled` carries the toggled selection's stable id.
    #[test]
    fn selection_list_toggled_message_carries_option_id() {
        let selections = vec![
            Selection::new("Alpha", "a".to_string()).with_id("alpha"),
            Selection::new("Beta", "b".to_string()),
        ];
        let mut list = SelectionList::with_selections(selections);
        let mut ctx = EventCtx::default();

        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.toggle(0, &mut __w) };
        let messages = ctx.take_messages();
        let toggled = messages
            .iter()
            .find_map(|m| m.downcast_ref::<crate::message::SelectionListToggled>())
            .expect("SelectionListToggled posted");
        assert_eq!(toggled.index, 0);
        assert!(toggled.selected);
        assert_eq!(
            toggled.option_id,
            Some(crate::widgets::OptionId::new("alpha"))
        );

        // Anonymous selections carry no id.
        let mut ctx = EventCtx::default();
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.toggle(1, &mut __w) };
        let messages = ctx.take_messages();
        let toggled = messages
            .iter()
            .find_map(|m| m.downcast_ref::<crate::message::SelectionListToggled>())
            .expect("SelectionListToggled posted");
        assert_eq!(toggled.option_id, None);
    }

    #[test]
    fn selection_list_toggle_emits_ordered_messages() {
        let selections = vec![Selection::new("Alpha", "a".to_string())];
        let mut list = SelectionList::with_selections(selections);
        let mut ctx = EventCtx::default();

        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.toggle(0, &mut __w) };
        let messages = ctx.take_messages();
        let toggled_pos = messages
            .iter()
            .position(|m| m.is::<crate::message::SelectionListToggled>());
        let changed_pos = messages
            .iter()
            .position(|m| m.is::<crate::message::SelectionListSelectedChanged>());
        assert!(toggled_pos.is_some() && changed_pos.is_some() && toggled_pos < changed_pos);
    }

    #[test]
    fn selection_list_select_all_deselect_all() {
        let selections = vec![
            Selection::new("A", "a".to_string()),
            Selection::new("B", "b".to_string()),
            Selection::selected("C", "c".to_string()),
        ];
        let mut list = SelectionList::with_selections(selections);
        let mut ctx = EventCtx::default();

        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.select_all(&mut __w) };
        // Python parity: `selected` reports SELECTION (insertion) order — "C"
        // was selected at construction, so select_all appends the others after.
        assert_eq!(list.selected(), vec![2, 0, 1]);

        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.deselect_all(&mut __w) };
        assert!(list.selected().is_empty());
    }

    #[test]
    fn selection_list_select_deselect_individual() {
        let selections = vec![
            Selection::new("A", "a".to_string()),
            Selection::new("B", "b".to_string()),
        ];
        let mut list = SelectionList::with_selections(selections);
        let mut ctx = EventCtx::default();

        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.select(1, &mut __w) };
        assert!(list.is_selected(1));

        // Selecting again is a no-op.
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.select(1, &mut __w) };
        assert!(list.is_selected(1));

        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.deselect(1, &mut __w) };
        assert!(!list.is_selected(1));
    }

    #[test]
    fn selection_list_out_of_bounds() {
        let selections = vec![Selection::new("A", "a".to_string())];
        let mut list = SelectionList::with_selections(selections);
        let mut ctx = EventCtx::default();

        // Should not panic.
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.toggle(99, &mut __w) };
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.select(99, &mut __w) };
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.deselect(99, &mut __w) };
        assert!(!list.is_selected(99));
    }

    #[test]
    fn selection_list_disabled_items_are_not_toggled() {
        let selections = vec![
            Selection::disabled("A", "a".to_string()),
            Selection::selected("B", "b".to_string()),
            Selection::new("C", "c".to_string()),
        ];
        let mut list = SelectionList::with_selections(selections);
        let mut ctx = EventCtx::default();

        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.toggle(0, &mut __w) };
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.select(0, &mut __w) };
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.deselect(0, &mut __w) };
        assert!(!list.is_selected(0));

        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.select_all(&mut __w) };
        assert!(!list.is_selected(0));
        assert!(list.is_selected(1));
        assert!(list.is_selected(2));
    }

    #[test]
    fn selection_list_disabled_widget_ignores_keyboard_toggle() {
        let mut list = SelectionList::with_selections(vec![
            Selection::new("A", "a".to_string()),
            Selection::new("B", "b".to_string()),
        ])
        .disabled(true);

        let mut ctx = EventCtx::default();
        assert!(!run_action(&mut list, "select", &mut ctx));

        assert_eq!(list.selected(), Vec::<usize>::new());
        assert!(!ctx.handled());
        assert!(!list.focusable());
    }

    /// Run a SelectionList binding action (the canonical keyboard path — keys
    /// reach the list through its declarative `bindings()`, not raw `on_event`).
    fn run_action(list: &mut SelectionList<String>, name: &str, ctx: &mut EventCtx) -> bool {
        let parsed = crate::action::parse_action(name).expect("parse action");
        let mut __w =
            crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), ctx);
        Widget::execute_action(list, &parsed, &mut __w)
    }

    #[test]
    fn bindings_mirror_python_selection_list() {
        // Python `SelectionList.BINDINGS` = inherited `OptionList.BINDINGS`
        // (down/end/enter/home/pagedown/pageup/up) + its own space → select
        // (all show=False).
        let list: SelectionList<String> = SelectionList::new();
        let bindings = Widget::bindings(&list);
        let pairs: Vec<(&str, &str)> = bindings
            .iter()
            .map(|b| (b.key.as_str(), b.action.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("down", "cursor_down"),
                ("end", "last"),
                ("enter", "select"),
                ("home", "first"),
                ("pagedown", "page_down"),
                ("pageup", "page_up"),
                ("up", "cursor_up"),
                ("space", "select"),
            ]
        );
        assert!(bindings.iter().all(|b| !b.show), "Python declares show=False");
    }

    #[test]
    fn select_action_toggles_highlighted_and_nav_actions_delegate() {
        let mut list = SelectionList::with_selections(vec![
            Selection::new("A", "a".to_string()),
            Selection::new("B", "b".to_string()),
        ]);
        list.on_layout(40, 5);

        // cursor_down delegates to the inner OptionList's highlight cursor.
        let mut ctx = EventCtx::default();
        assert!(run_action(&mut list, "cursor_down", &mut ctx));
        assert_eq!(list.highlighted(), Some(1));

        // `select` toggles the highlighted entry (Python intercepts
        // OptionSelected and toggles; no OptionSelected escapes).
        let mut ctx = EventCtx::default();
        assert!(run_action(&mut list, "select", &mut ctx));
        assert!(list.is_selected(1));
        assert!(ctx.handled());

        let mut ctx = EventCtx::default();
        assert!(run_action(&mut list, "select", &mut ctx));
        assert!(!list.is_selected(1));
    }

    #[test]
    fn selection_list_toggle_all() {
        let selections = vec![
            Selection::new("A", "a".to_string()),
            Selection::selected("B", "b".to_string()),
            Selection::disabled("C", "c".to_string()),
            Selection::new("D", "d".to_string()),
        ];
        let mut list = SelectionList::with_selections(selections);
        let mut ctx = EventCtx::default();

        // A=false, B=true, C=disabled(false), D=false
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.toggle_all(&mut __w) };
        // A=true, B=false, C=still false (disabled), D=true
        assert!(list.is_selected(0));
        assert!(!list.is_selected(1));
        assert!(!list.is_selected(2)); // disabled stays unchanged
        assert!(list.is_selected(3));

        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); list.toggle_all(&mut __w) };
        // Back to: A=false, B=true, C=false, D=false
        assert!(!list.is_selected(0));
        assert!(list.is_selected(1));
        assert!(!list.is_selected(2));
        assert!(!list.is_selected(3));
    }

    #[test]
    fn selection_list_click_on_disabled_item_is_not_handled() {
        let mut list = SelectionList::with_selections(vec![
            Selection::disabled("A", "a".to_string()),
            Selection::new("B", "b".to_string()),
        ]);
        list.on_layout(40, 5);

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: NodeId::default(),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut __w);
        }

        assert!(!ctx.handled());
        assert!(!list.is_selected(0));
    }

    #[test]
    fn selection_list_with_integer_values() {
        let selections = vec![
            Selection::new("One", 1i32),
            Selection::selected("Two", 2),
            Selection::new("Three", 3),
        ];
        let list = SelectionList::with_selections(selections);
        assert_eq!(list.item_count(), 3);
        assert!(!list.is_selected(0));
        assert!(list.is_selected(1));
        assert_eq!(list.value_at(0), Some(&1));
        assert_eq!(list.value_at(1), Some(&2));
        assert_eq!(list.selected_values(), vec![&2]);
    }

    // ── P1-14 dispatch-context regression tests ─────────────────────────

    fn make_node_id() -> NodeId {
        use slotmap::SlotMap;
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    #[test]
    fn mouse_click_with_dispatch_context_is_handled() {
        use crate::runtime::dispatch_ctx::set_dispatch_recipient;

        let mut list = SelectionList::with_selections(vec![
            Selection::new("A", "a".to_string()),
            Selection::new("B", "b".to_string()),
        ]);
        list.on_layout(40, 5);

        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, crate::widgets::NodeState::default());

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut __w);
        }
        assert!(ctx.handled());
        assert!(list.is_selected(0));
    }

    #[test]
    fn mouse_click_with_wrong_target_is_ignored() {
        use crate::runtime::dispatch_ctx::set_dispatch_recipient;
        use slotmap::SlotMap;

        let mut list = SelectionList::with_selections(vec![
            Selection::new("A", "a".to_string()),
            Selection::new("B", "b".to_string()),
        ]);
        list.on_layout(40, 5);

        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        let my_id = sm.insert(());
        let other_id = sm.insert(());
        let _guard = set_dispatch_recipient(my_id, crate::widgets::NodeState::default());

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            list.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: other_id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut __w);
        }
        assert!(!ctx.handled());
        assert!(!list.is_selected(0));
    }
}
