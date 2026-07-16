use std::sync::atomic::{AtomicU64, Ordering};

use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::action::ParsedAction;
use crate::compose::{ChildDecl, ComposeResult};
use crate::event::WidgetCtx;
use crate::message::*;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

use super::option_list::toggle_option::OptionCursorState;
use super::option_list::OptionItem;
use super::select_current::SelectCurrent;
use super::select_overlay::SelectOverlay;
use super::{BindingDecl, NodeSeed, Widget};

/// Monotonic source of per-`Select` scope ids (mirrors the `Tabs` scope-id
/// pattern), so each `Select` node and its `SelectOverlay` child get stable,
/// unique CSS ids for focus routing across recomposes.
static NEXT_SELECT_ID: AtomicU64 = AtomicU64::new(1);

/// A dropdown select control.
///
/// Port of Python Textual's `Select` (`textual/widgets/_select.py`). It is a
/// composed-children ARENA widget: `compose()` emits a [`SelectCurrent`] bar and
/// a [`SelectOverlay`] pop-up as real child nodes (state-pure, so a recompose —
/// used to reflect a value/options change — rebuilds an identical subtree). The
/// overlay resolves `overlay: screen; display: block` when the Select carries
/// `-expanded`, so it floats UNCLIPPED at the top z via the Mechanism-A deferred
/// paint. Selection changes are driven by messages: `SelectCurrent` clicks post
/// [`SelectCurrentToggle`], option clicks/Enter surface as [`OptionSelected`]
/// from the overlay, and the overlay posts [`SelectOverlayDismiss`].
///
/// Generic over the value type `T`.
pub struct Select<T: Clone + PartialEq + Send + Sync + 'static> {
    options: Vec<(String, T)>,
    /// `cursor.selected()` is the index of the current value into `options`
    /// (`None` = blank / no selection).
    cursor: OptionCursorState,
    prompt: String,
    disabled: bool,
    /// When `true` (default, Python parity) the selection can be blank (a
    /// leading dim prompt row is added to the overlay). When `false` the first
    /// option is auto-selected and cannot be cleared.
    allow_blank: bool,
    /// Whether the overlay is currently shown.
    expanded: bool,
    /// This `Select` node's CSS id (auto-generated, or the caller's via `id()`),
    /// used to re-focus itself after a dismiss/selection.
    focus_id: String,
    /// The `SelectOverlay` child's stable CSS id (for `display` targeting +
    /// focus routing), assigned in `compose()` and reused across recomposes.
    overlay_id: String,
    seed: NodeSeed,
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Select<T> {
    /// Create a new `Select` widget.
    ///
    /// `options` is a list of `(label, value)` pairs. `prompt` is shown when
    /// nothing is selected. Defaults to `allow_blank = true` (Python parity):
    /// the widget starts blank, showing the prompt. Use
    /// [`with_allow_blank(false)`](Self::with_allow_blank) to forbid the blank
    /// state, which auto-selects the first option.
    pub fn new(options: Vec<(String, T)>, prompt: impl Into<String>) -> Self {
        let n = NEXT_SELECT_ID.fetch_add(1, Ordering::Relaxed);
        let focus_id = format!("select-{n}");
        let overlay_id = format!("select-overlay-{n}");

        // Default allow_blank = true (Python parity): start with no selection.
        let cursor = OptionCursorState::default();

        let seed = NodeSeed {
            css_id: Some(focus_id.clone()),
            classes: vec!["select".to_string()],
            ..NodeSeed::default()
        };
        Self {
            options,
            cursor,
            prompt: prompt.into(),
            disabled: false,
            allow_blank: true,
            expanded: false,
            focus_id,
            overlay_id,
            seed,
        }
    }

    /// Set this widget's CSS id (Python `id=`). Also becomes the id it re-focuses
    /// itself by after a dismiss.
    pub fn id(mut self, value: impl Into<String>) -> Self {
        let v = value.into();
        self.focus_id = v.clone();
        self.seed.css_id = Some(v);
        self
    }

    /// Add a CSS class (Python `classes=`). Idempotent.
    pub fn class(mut self, value: impl Into<String>) -> Self {
        let v = value.into();
        if !self.seed.classes.iter().any(|c| c == &v) {
            self.seed.classes.push(v);
        }
        self
    }

    // ── Public API ──────────────────────────────────────────────────

    /// The currently selected value, or `None`.
    pub fn value(&self) -> Option<&T> {
        self.cursor
            .selected()
            .and_then(|i| self.options.get(i).map(|(_, v)| v))
    }

    /// Reactive setter for the selected value. If the value is not found,
    /// selection is cleared. Records the change (with recompose) in the provided
    /// [`ReactiveCtx`], so the closed-state label + overlay highlight update.
    pub fn set_value(&mut self, value: &T, ctx: &mut ReactiveCtx) {
        let selected = self.options.iter().position(|(_, v)| v == value);
        let old = self.cursor.selected();
        self.cursor.set_selected(selected);
        if old != selected {
            ctx.record_change(
                "value",
                ReactiveFlags::reactive_recompose(),
                Box::new(old),
                Box::new(selected),
            );
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
    }

    /// Whether the dropdown overlay is currently open.
    pub fn is_open(&self) -> bool {
        self.expanded
    }

    /// Whether blank (no selection) is allowed.
    pub fn allow_blank(&self) -> bool {
        self.allow_blank
    }

    /// Reactive setter for `allow_blank`. Records the change in the provided
    /// [`ReactiveCtx`].
    ///
    /// When switching from `allow_blank = true` to `false` and no option is
    /// currently selected, the first option is auto-selected.
    pub fn set_allow_blank(&mut self, allow: bool, ctx: &mut ReactiveCtx) {
        if self.allow_blank != allow {
            let old = self.allow_blank;
            self.allow_blank = allow;
            if !allow && self.cursor.selected().is_none() && !self.options.is_empty() {
                self.cursor.set_selected(Some(0));
            }
            ctx.record_change(
                "allow_blank",
                ReactiveFlags::reactive_recompose(),
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
            if value {
                self.expanded = false;
            }
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
    /// When `true` (default, Python parity), the initial state is no selection
    /// (placeholder shown) and the user can deselect. When `false` the first
    /// option is auto-selected and the user cannot clear the selection.
    pub fn with_allow_blank(mut self, allow: bool) -> Self {
        self.allow_blank = allow;
        if allow {
            self.cursor.clear();
        } else if self.cursor.selected().is_none() && !self.options.is_empty() {
            self.cursor.set_selected(Some(0));
        }
        self
    }

    /// Builder: set disabled state for the entire select.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        if disabled {
            self.expanded = false;
        }
        self
    }

    /// Reactive setter for `options`. Clears the current selection. Records the
    /// change (with recompose) in the provided [`ReactiveCtx`] so the overlay
    /// rows + closed-state label rebuild.
    ///
    /// When `allow_blank` is `false` and new options are non-empty, the first
    /// option is auto-selected.
    pub fn set_options(&mut self, options: Vec<(String, T)>, ctx: &mut ReactiveCtx) {
        let old_len = self.options.len();
        self.cursor.clear();
        self.options = options;
        if !self.allow_blank && !self.options.is_empty() {
            self.cursor.set_selected(Some(0));
        }
        ctx.record_change(
            "options",
            ReactiveFlags::reactive_recompose(),
            Box::new(old_len),
            Box::new(self.options.len()),
        );
    }

    // ── Watchers ─────────────────────────────────────────────────────

    fn watch_allow_blank(&mut self, _old: &bool, new: &bool) {
        if !new && self.cursor.selected().is_none() && !self.options.is_empty() {
            self.cursor.set_selected(Some(0));
        }
    }

    // ── Internals ───────────────────────────────────────────────────

    /// The label of the current value, or `None` when blank (the prompt shows).
    fn current_label(&self) -> Option<String> {
        self.cursor
            .selected()
            .map(|index| self.options[index].0.clone())
    }

    /// Whether a real value is selected (drives SelectCurrent's `-has-value`).
    fn has_value(&self) -> bool {
        self.cursor.selected().is_some()
    }

    /// The overlay's option rows: a leading dim prompt row when `allow_blank`,
    /// then one row per option.
    fn build_overlay_items(&self) -> Vec<OptionItem> {
        let mut items = Vec::with_capacity(self.options.len() + 1);
        if self.allow_blank {
            items.push(SelectOverlay::blank_option(&self.prompt));
        }
        for (label, _) in &self.options {
            items.push(OptionItem::new(label.as_str()));
        }
        items
    }

    /// The overlay ROW to highlight for the current value (accounting for the
    /// leading blank row): the value's row, or the blank row (0) when blank.
    fn overlay_highlight_row(&self) -> Option<usize> {
        match self.cursor.selected() {
            Some(i) => Some(if self.allow_blank { i + 1 } else { i }),
            None => {
                if self.allow_blank {
                    Some(0)
                } else {
                    None
                }
            }
        }
    }

    /// Map an overlay ROW index (with the blank offset) back to an option index,
    /// or `None` for the blank row.
    fn row_to_option(&self, row: usize) -> Option<usize> {
        if self.allow_blank {
            row.checked_sub(1)
        } else {
            Some(row)
        }
    }

    fn expand(&mut self, ctx: &mut WidgetCtx) {
        if self.disabled || self.expanded {
            return;
        }
        self.expanded = true;
        // Revealing the overlay: add the class the CSS `Select.-expanded >
        // SelectOverlay { display: block }` keys on, then focus the overlay. The
        // focus request is deferred by the runtime until the same-frame display
        // resolution lands (see `App::retry_pending_focus`).
        ctx.add_class("-expanded");
        ctx.post_message(AppFocus {
            widget_id: self.overlay_id.clone(),
        });
    }

    fn collapse(&mut self, ctx: &mut WidgetCtx, refocus: bool) {
        if !self.expanded {
            return;
        }
        self.expanded = false;
        ctx.remove_class("-expanded");
        if refocus {
            ctx.post_message(AppFocus {
                widget_id: self.focus_id.clone(),
            });
        }
    }

    fn toggle_overlay(&mut self, ctx: &mut WidgetCtx) {
        if self.expanded {
            self.collapse(ctx, true);
        } else {
            self.expand(ctx);
        }
    }

    /// Apply an overlay selection (a click/Enter on an option row): update the
    /// value, collapse, and recompose so the closed-state label + overlay
    /// highlight reflect the new value.
    fn apply_overlay_selection(&mut self, row: usize, ctx: &mut WidgetCtx) {
        let new_selected = self.row_to_option(row);
        if let Some(i) = new_selected {
            if i >= self.options.len() {
                return;
            }
        } else if !self.allow_blank {
            return;
        }
        let old = self.cursor.selected();
        self.cursor.set_selected(new_selected);
        // Collapse FIRST (expanded = false) so the recompose below is safe: the
        // overlay is display:none / unfocused when the subtree remounts (Trap 1).
        self.collapse(ctx, true);
        if old != new_selected {
            if let Some(i) = new_selected {
                ctx.post_message(SelectChanged {
                    index: i,
                    label: self.options[i].0.clone(),
                });
            }
        }
        ctx.request_recompose();
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Widget for Select<T> {
    /// Emit the closed-state bar + overlay as real arena children (Python
    /// `Select.compose`). State-pure: rebuilt identically from `options` /
    /// `cursor` / `prompt` / `allow_blank`, so a recompose (value/options change)
    /// regenerates rather than clears.
    fn compose(&mut self) -> ComposeResult {
        let current = SelectCurrent::new(self.prompt.clone(), self.current_label());
        let overlay = SelectOverlay::new(self.build_overlay_items(), self.overlay_highlight_row());
        vec![
            ChildDecl::new(Box::new(current)),
            ChildDecl::new(Box::new(overlay)).with_id(&self.overlay_id),
        ]
    }

    fn focusable(&self) -> bool {
        !self.disabled
    }

    /// Drive the `-has-value` class onto the `SelectCurrent` child (index 0),
    /// which colours the label at full strength (Python
    /// `SelectCurrent._watch_has_value`).
    fn child_classes_for_tree(&self, child_index: usize) -> Vec<(&'static str, bool)> {
        if child_index == 0 {
            vec![("-has-value", self.has_value())]
        } else {
            Vec::new()
        }
    }

    fn action_namespace(&self) -> &str {
        "select"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new(
            "enter,down,space,up",
            "show_overlay",
            "Show menu",
        )
        .hidden()]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut WidgetCtx) -> bool {
        if self.disabled {
            return false;
        }
        if action.name == "show_overlay" && !self.expanded {
            self.expand(ctx);
            ctx.set_handled();
            return true;
        }
        false
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut WidgetCtx) {
        if message.downcast_ref::<SelectCurrentToggle>().is_some() {
            if !self.disabled {
                self.toggle_overlay(ctx);
                ctx.set_handled();
            }
            return;
        }
        if let Some(dismiss) = message.downcast_ref::<SelectOverlayDismiss>() {
            self.collapse(ctx, !dismiss.lost_focus);
            ctx.set_handled();
            return;
        }
        if let Some(selected) = message.downcast_ref::<OptionSelected>() {
            self.apply_overlay_selection(selected.index, ctx);
            ctx.set_handled();
        }
    }

    /// Chrome-only: `Select` has no border/background of its own (Python
    /// `Select { height: auto; color: $foreground }`); the `SelectCurrent` bar
    /// and floating `SelectOverlay` render as arena children.
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
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

    /// Post a `SelectChanged` for the initially-selected value at mount.
    ///
    /// Python parity: `Select.value` is reactive-set during init and
    /// `_watch_value` posts `Select.Changed`, including the initial assignment.
    /// With `allow_blank = false` the first option is auto-selected, so apps
    /// observe `Changed(first_value)` at startup. With the default
    /// `allow_blank = true` the widget starts blank and nothing is posted.
    fn on_mount(&mut self, ctx: &mut WidgetCtx) {
        if let Some(index) = self.cursor.selected() {
            if let Some((label, _)) = self.options.get(index) {
                ctx.post_message(SelectChanged {
                    index,
                    label: label.clone(),
                });
            }
        }
    }

    fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    // NOTE: like Python's `Select` (a `Vertical` whose overlay is focused
    // programmatically), the arena children are managed by the tree; there is no
    // `visit_children_mut` side path.
}

impl<T: Clone + PartialEq + Send + Sync + 'static> Renderable for Select<T> {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl<T: Clone + PartialEq + Send + Sync + 'static> ReactiveWidget for Select<T> {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], _ctx: &mut ReactiveCtx) {
        for change in changes {
            if change.field_name == "allow_blank" {
                if let (Some(old), Some(new)) = (
                    change.old_value.downcast_ref::<bool>(),
                    change.new_value.downcast_ref::<bool>(),
                ) {
                    self.watch_allow_blank(old, new);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::node_id::NodeId;
    use crate::reactive::ReactiveCtx;
    use slotmap::SlotMap;

    fn make_node_id() -> NodeId {
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

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

    fn make_select_no_blank() -> Select<i32> {
        make_select().with_allow_blank(false)
    }

    #[test]
    fn select_starts_closed_and_blank_by_default() {
        // Default allow_blank = true (Python parity): no initial selection.
        let sel = make_select();
        assert!(!sel.is_open());
        assert!(sel.allow_blank());
        assert!(sel.value().is_none());
    }

    #[test]
    fn allow_blank_false_auto_selects_first() {
        let sel = make_select_no_blank();
        assert!(!sel.allow_blank());
        assert_eq!(sel.value(), Some(&1));
    }

    #[test]
    fn set_value_programmatic() {
        let mut sel = make_select();
        let mut ctx = ReactiveCtx::new(make_node_id());
        sel.set_value(&3, &mut ctx);
        assert_eq!(sel.value(), Some(&3));
    }

    #[test]
    fn clear_resets_when_allow_blank() {
        let mut sel = make_select();
        let mut ctx = ReactiveCtx::new(make_node_id());
        sel.set_value(&2, &mut ctx);
        sel.clear();
        assert!(sel.value().is_none());
    }

    #[test]
    fn clear_is_noop_when_not_allow_blank() {
        let mut sel = make_select_no_blank();
        assert_eq!(sel.value(), Some(&1));
        sel.clear();
        assert_eq!(sel.value(), Some(&1));
    }

    #[test]
    fn set_allow_blank_auto_selects_when_switching_to_false() {
        let mut sel = make_select();
        let mut ctx = ReactiveCtx::new(make_node_id());
        assert!(sel.value().is_none());
        sel.set_allow_blank(false, &mut ctx);
        assert!(!sel.allow_blank());
        assert_eq!(sel.value(), Some(&1));
    }

    #[test]
    fn set_options_auto_selects_when_not_allow_blank() {
        let mut sel = make_select_no_blank();
        let mut ctx = ReactiveCtx::new(make_node_id());
        sel.set_options(
            vec![("Delta".to_string(), 10), ("Echo".to_string(), 20)],
            &mut ctx,
        );
        assert_eq!(sel.value(), Some(&10));
    }

    #[test]
    fn set_options_does_not_auto_select_when_allow_blank() {
        let mut sel = make_select();
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
        assert!(sel.bindings().iter().any(|b| b.action == "show_overlay"));
    }

    // ── compose (state-pure) ──────────────────────────────────────────

    #[test]
    fn compose_emits_current_and_overlay_state_pure() {
        let mut sel = make_select();
        let first = sel.compose();
        assert_eq!(first.len(), 2, "SelectCurrent + SelectOverlay");
        let second = sel.compose();
        assert_eq!(second.len(), 2, "compose must regenerate, never clear");
        // Overlay carries the stable scope id.
        assert_eq!(second[1].id(), Some(sel.overlay_id.as_str()));
    }

    #[test]
    fn compose_blank_adds_leading_prompt_row() {
        let sel = make_select();
        // 3 options + 1 blank row.
        assert_eq!(sel.build_overlay_items().len(), 4);
        // No blank row without allow_blank.
        assert_eq!(make_select_no_blank().build_overlay_items().len(), 3);
    }

    #[test]
    fn overlay_highlight_row_accounts_for_blank_offset() {
        // allow_blank=false, first selected -> row 0.
        assert_eq!(make_select_no_blank().overlay_highlight_row(), Some(0));
        // allow_blank=true (default), nothing selected -> blank row 0.
        assert_eq!(make_select().overlay_highlight_row(), Some(0));
        // allow_blank=true, value at option index 1 -> row 2.
        let mut sel = make_select();
        let mut ctx = ReactiveCtx::new(make_node_id());
        sel.set_value(&2, &mut ctx);
        assert_eq!(sel.overlay_highlight_row(), Some(2));
    }

    // ── message handling ──────────────────────────────────────────────

    #[test]
    fn toggle_message_expands_and_dismiss_collapses() {
        let mut sel = make_select();
        let mut ctx = EventCtx::default();
        {
            let mut w = WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
            sel.on_message(&MessageEvent::new(NodeId::default(), SelectCurrentToggle), &mut w);
        }
        assert!(sel.is_open());
        let mut ctx2 = EventCtx::default();
        {
            let mut w = WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx2);
            sel.on_message(
                &MessageEvent::new(NodeId::default(), SelectOverlayDismiss { lost_focus: false }),
                &mut w,
            );
        }
        assert!(!sel.is_open());
    }

    #[test]
    fn option_selected_updates_value_and_collapses() {
        let mut sel = make_select();
        // Open first.
        let mut open = EventCtx::default();
        {
            let mut w = WidgetCtx::__from_dispatch(NodeId::default(), &mut open);
            sel.on_message(&MessageEvent::new(NodeId::default(), SelectCurrentToggle), &mut w);
        }
        assert!(sel.is_open());
        // Overlay row 2 (blank row 0, options 1..) => option index 1 (Beta=2).
        let mut selctx = EventCtx::default();
        {
            let mut w = WidgetCtx::__from_dispatch(NodeId::default(), &mut selctx);
            sel.on_message(&MessageEvent::new(NodeId::default(), OptionSelected { index: 2, option_id: None }), &mut w);
        }
        assert!(!sel.is_open());
        assert_eq!(sel.value(), Some(&2));
        let msgs = selctx.take_messages();
        assert!(msgs.iter().any(|m| {
            m.downcast_ref::<SelectChanged>()
                .is_some_and(|c| c.index == 1)
        }));
    }

    #[test]
    fn blank_row_selection_clears_value() {
        let mut sel = make_select();
        let mut rctx = ReactiveCtx::new(make_node_id());
        sel.set_value(&2, &mut rctx);
        assert_eq!(sel.value(), Some(&2));
        let mut ctx = EventCtx::default();
        {
            let mut w = WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
            // Row 0 = blank.
            sel.on_message(&MessageEvent::new(NodeId::default(), OptionSelected { index: 0, option_id: None }), &mut w);
        }
        assert!(sel.value().is_none());
    }

    #[test]
    fn execute_action_show_overlay_expands() {
        let mut sel = make_select();
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "show_overlay".to_string(),
            arguments: vec![],
        };
        let handled = {
            let mut w = WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
            sel.execute_action(&action, &mut w)
        };
        assert!(handled);
        assert!(sel.is_open());
    }

    #[test]
    fn disabled_ignores_show_overlay() {
        let mut sel = make_select().disabled(true);
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "show_overlay".to_string(),
            arguments: vec![],
        };
        let handled = {
            let mut w = WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
            sel.execute_action(&action, &mut w)
        };
        assert!(!handled);
        assert!(!sel.is_open());
        assert!(!sel.focusable());
    }

    #[test]
    fn child_classes_drive_has_value() {
        let with = make_select_no_blank();
        assert!(with.child_classes_for_tree(0).contains(&("-has-value", true)));
        let without = make_select();
        assert!(without.child_classes_for_tree(0).contains(&("-has-value", false)));
        // Overlay child carries no driven classes.
        assert!(with.child_classes_for_tree(1).is_empty());
    }
}
