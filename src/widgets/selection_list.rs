use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event};
use crate::message::*;

use super::option_list::{OptionItem, OptionList};
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
}

impl<T: Clone + PartialEq> Selection<T> {
    /// Create a new selection with default (unselected) state.
    pub fn new(prompt: impl Into<String>, value: T) -> Self {
        Self {
            prompt: prompt.into(),
            value,
            initially_selected: false,
            disabled: false,
        }
    }

    /// Create a new selection that starts selected.
    pub fn selected(prompt: impl Into<String>, value: T) -> Self {
        Self {
            prompt: prompt.into(),
            value,
            initially_selected: true,
            disabled: false,
        }
    }

    /// Create a new selection that is disabled.
    pub fn disabled(prompt: impl Into<String>, value: T) -> Self {
        Self {
            prompt: prompt.into(),
            value,
            initially_selected: false,
            disabled: true,
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
            hovered_index: None,
            border_title_text: None,
            seed,
        }
    }

    /// Create a `SelectionList` pre-populated with selections.
    pub fn with_selections(selections: Vec<Selection<T>>) -> Self {
        let mut list = Self::new();
        let items: Vec<OptionItem> = selections
            .iter()
            .map(|s| {
                if s.disabled {
                    OptionItem::disabled(&s.prompt)
                } else {
                    OptionItem::new(&s.prompt)
                }
            })
            .collect();
        let values: Vec<T> = selections.iter().map(|s| s.value.clone()).collect();
        let selected: Vec<bool> = selections
            .iter()
            .map(|s| s.initially_selected && !s.disabled)
            .collect();
        list.inner = OptionList::with_items(items);
        list.values = values;
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
        ctx.post_message(SelectionListToggled { index, selected });
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
            ctx.post_message(SelectionListSelectedChanged);
            ctx.request_repaint();
        }
    }

    /// Returns a `Vec` of indices that are currently selected.
    pub fn selected(&self) -> Vec<usize> {
        self.selected_set
            .iter()
            .enumerate()
            .filter_map(|(i, &sel)| if sel { Some(i) } else { None })
            .collect()
    }

    /// Whether the item at `index` is currently selected.
    pub fn is_selected(&self, index: usize) -> bool {
        self.selected_set.get(index).copied().unwrap_or(false)
    }

    /// Returns the values of all currently selected items.
    pub fn selected_values(&self) -> Vec<&T> {
        self.selected_set
            .iter()
            .enumerate()
            .filter_map(|(i, &sel)| if sel { self.values.get(i) } else { None })
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
            Event::Key(key) if self.node_state().focused => match key.code {
                KeyCode::Char(' ') | KeyCode::Enter => {
                    if let Some(index) = self.inner.highlighted() {
                        if self.item_is_selectable(index) {
                            self.toggle(index, ctx);
                            ctx.set_handled();
                        }
                    }
                }
                // Delegate navigation keys to the inner OptionList.
                KeyCode::Up
                | KeyCode::Down
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Home
                | KeyCode::End => {
                    self.inner.on_event(event, ctx);
                }
                _ => {}
            },
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

        let base_style = crate::css::resolve_component_style(self, &["option-list--option"])
            .to_rich()
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
                        let sep_style =
                            crate::css::resolve_component_style(self, &["option-list--separator"])
                                .to_rich()
                                .unwrap_or(base_style);
                        let text = "─".repeat(width);
                        let line =
                            adjust_line_length_no_bg(&[Segment::styled(text, sep_style)], width);
                        out.extend(line);
                    }
                    OptionItem::Option {
                        prompt, disabled, ..
                    } => {
                        // Resolve the option row style (same classes as OptionList).
                        let mut opt_classes = vec!["option-list--option"];
                        if highlighted {
                            opt_classes.push("-highlighted");
                        }
                        if hovered_row && !highlighted {
                            opt_classes.push("-hover");
                        }
                        if *disabled {
                            opt_classes.push("-disabled");
                        }
                        if highlighted && self.node_state().focused {
                            opt_classes.push("-focus");
                        }
                        let opt_style = crate::css::resolve_component_style(self, &opt_classes)
                            .to_rich()
                            .unwrap_or(base_style);

                        // Resolve button component style.
                        let mut btn_class = "selection-list--button".to_string();
                        if selected {
                            btn_class.push_str("-selected");
                        }
                        if highlighted {
                            btn_class.push_str("-highlighted");
                        }
                        let btn_style = crate::css::resolve_component_style(self, &[&btn_class])
                            .to_rich()
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

                        let prompt_width = width.saturating_sub(btn_width);
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
        // `layout_height()` reports the OUTER auto height (content + own
        // border/padding chrome); the layout side adds only margin on top
        // (see `extract_child_spec`). Resolve the cascaded style so an example
        // that adds `border`/`padding` (e.g. selection_list_selected.tcss) is
        // measured correctly instead of clipping its rows.
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (border_top, border_bottom, _, _) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_v =
            usize::from(padding.top.saturating_add(padding.bottom)) + border_top + border_bottom;
        Some(self.inner.option_count().max(1).saturating_add(chrome_v))
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

        list.toggle(0, &mut ctx);
        assert!(list.is_selected(0));

        list.toggle(0, &mut ctx);
        assert!(!list.is_selected(0));
    }

    #[test]
    fn selection_list_toggle_emits_ordered_messages() {
        let selections = vec![Selection::new("Alpha", "a".to_string())];
        let mut list = SelectionList::with_selections(selections);
        let mut ctx = EventCtx::default();

        list.toggle(0, &mut ctx);
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

        list.select_all(&mut ctx);
        assert_eq!(list.selected(), vec![0, 1, 2]);

        list.deselect_all(&mut ctx);
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

        list.select(1, &mut ctx);
        assert!(list.is_selected(1));

        // Selecting again is a no-op.
        list.select(1, &mut ctx);
        assert!(list.is_selected(1));

        list.deselect(1, &mut ctx);
        assert!(!list.is_selected(1));
    }

    #[test]
    fn selection_list_out_of_bounds() {
        let selections = vec![Selection::new("A", "a".to_string())];
        let mut list = SelectionList::with_selections(selections);
        let mut ctx = EventCtx::default();

        // Should not panic.
        list.toggle(99, &mut ctx);
        list.select(99, &mut ctx);
        list.deselect(99, &mut ctx);
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

        list.toggle(0, &mut ctx);
        list.select(0, &mut ctx);
        list.deselect(0, &mut ctx);
        assert!(!list.is_selected(0));

        list.select_all(&mut ctx);
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

        let key = crate::keys::KeyEventData::from_crossterm(crossterm::event::KeyEvent::new(
            KeyCode::Char(' '),
            crossterm::event::KeyModifiers::NONE,
        ));
        let mut ctx = EventCtx::default();
        list.on_event(&Event::Key(key), &mut ctx);

        assert_eq!(list.selected(), Vec::<usize>::new());
        assert!(!ctx.handled());
        assert!(!list.focusable());
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
        list.toggle_all(&mut ctx);
        // A=true, B=false, C=still false (disabled), D=true
        assert!(list.is_selected(0));
        assert!(!list.is_selected(1));
        assert!(!list.is_selected(2)); // disabled stays unchanged
        assert!(list.is_selected(3));

        list.toggle_all(&mut ctx);
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
        list.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: NodeId::default(),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );

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
        list.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
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
        list.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: other_id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(!ctx.handled());
        assert!(!list.is_selected(0));
    }
}
