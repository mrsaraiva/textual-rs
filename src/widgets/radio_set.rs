use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, MetaValue, Renderable, Segment, Segments};

use crate::event::Event;
use crate::message::*;

use super::{
    NodeSeed, Widget, helpers::adjust_line_length_no_bg,
    option_list::toggle_option::OptionCursorState, radio_button::RadioButton,
};
use crate::compose::ComposeResult;
use crate::reactive::{ReactiveCtx, ReactiveFlags, ReactiveWidget};

/// A container widget that groups `RadioButton` children for mutual exclusion.
///
/// When one radio button is toggled on, all others are automatically deselected.
/// The set itself is focusable and handles keyboard navigation (Up/Down) between
/// its children. Individual RadioButtons inside a set do not receive independent
/// focus — the set manages focus delegation visually via the selected index.
#[derive(Debug, Clone)]
pub struct RadioSet {
    buttons: Vec<RadioButton>,
    cursor: OptionCursorState,
    disabled: bool,
    hovered_index: Option<usize>,
    seed: NodeSeed,
}

impl Default for RadioSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Tag a segment with `textual:no_style = true` so the widget-level style pass
/// (`apply_style_to_segments`) leaves it untouched. Used for the inner glyph and
/// the selected label, whose backgrounds are fully composed here; without this,
/// the `RadioSet:focus` `background-tint: $foreground 5%` would be re-applied to
/// their opaque backgrounds (Python does not tint component-painted surfaces).
fn tag_segment_no_style(seg: &mut Segment) {
    let mut meta = seg.meta.take().unwrap_or_default();
    let mut map: std::collections::BTreeMap<String, MetaValue> = meta
        .meta
        .as_ref()
        .map(|m| (**m).clone())
        .unwrap_or_default();
    map.insert("textual:no_style".to_string(), MetaValue::Bool(true));
    meta.meta = Some(std::sync::Arc::new(map));
    seg.meta = Some(meta);
}

impl RadioSet {
    crate::seed_ident_methods!();

    /// Create a new empty RadioSet.
    pub fn new() -> Self {
        let seed = NodeSeed {
            classes: vec!["radio-set".to_string()],
            ..NodeSeed::default()
        };
        Self {
            buttons: Vec::new(),
            cursor: OptionCursorState::default(),
            disabled: false,
            hovered_index: None,
            seed,
        }
    }

    /// Create a RadioSet from string labels. Each label becomes a RadioButton.
    pub fn from_labels(labels: &[&str]) -> Self {
        let mut set = Self::new();
        for label in labels {
            set.buttons.push(RadioButton::new(*label));
        }
        set.cursor.set_highlighted(set.first_enabled_index());
        set
    }

    /// Builder: set disabled state for the entire set.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    // ── Reactive setters ─────────────────────────────────────────────────

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

    /// Builder: add a RadioButton to the set.
    /// If the button is pre-selected (value=true), it becomes the pressed button
    /// and any previously pressed button is deselected.
    pub fn with_button(mut self, button: RadioButton) -> Self {
        self.add_button(button);
        self
    }

    /// Add a RadioButton after construction.
    /// If the button is pre-selected (value=true), it becomes the pressed button
    /// and any previously pressed button is deselected.
    pub fn add_button(&mut self, button: RadioButton) {
        let index = self.buttons.len();
        if button.value() {
            // Enforce mutual exclusion: deselect any previously pressed button.
            if let Some(prev) = self.cursor.selected() {
                if let Some(btn) = self.buttons.get_mut(prev) {
                    btn.set_value_silent(false);
                }
            }
            self.cursor.set_selected(Some(index));
        }
        self.buttons.push(button);
        if self.cursor.highlighted().is_none() {
            self.cursor.set_highlighted(self.first_enabled_index());
        }
    }

    /// Returns the index of the currently pressed (on) button, or `None`.
    pub fn pressed_index(&self) -> Option<usize> {
        self.cursor.selected()
    }

    /// Returns the currently selected (highlighted) index.
    pub fn selected_index(&self) -> usize {
        self.cursor.highlighted().unwrap_or(0)
    }

    /// Returns a reference to the button at `index`, if it exists.
    pub fn button(&self, index: usize) -> Option<&RadioButton> {
        self.buttons.get(index)
    }

    /// Returns a mutable reference to the button at `index`.
    pub fn button_mut(&mut self, index: usize) -> Option<&mut RadioButton> {
        self.buttons.get_mut(index)
    }

    /// Returns the number of buttons in the set.
    pub fn len(&self) -> usize {
        self.buttons.len()
    }

    /// Returns `true` if the set contains no buttons.
    pub fn is_empty(&self) -> bool {
        self.buttons.is_empty()
    }

    /// Move the selection cursor by `delta` (-1 for up, +1 for down), wrapping.
    fn move_selection(&mut self, delta: isize) {
        if self.buttons.is_empty() {
            return;
        }
        let enabled_indices: Vec<usize> = self
            .buttons
            .iter()
            .enumerate()
            .filter_map(|(idx, button)| (!button.is_disabled()).then_some(idx))
            .collect();
        if enabled_indices.is_empty() {
            self.cursor.set_highlighted(None);
            return;
        }
        let current_pos = self
            .cursor
            .highlighted()
            .and_then(|idx| enabled_indices.iter().position(|&enabled| enabled == idx));
        let next_pos = if let Some(pos) = current_pos {
            let len = enabled_indices.len() as isize;
            ((pos as isize + if delta.is_negative() { -1 } else { 1 }) % len + len) % len
        } else if delta.is_negative() {
            enabled_indices.len() as isize - 1
        } else {
            0
        };
        self.cursor
            .set_highlighted(Some(enabled_indices[next_pos as usize]));
    }

    /// Toggle the currently selected button. Enforces mutual exclusion:
    /// if the selected button is being turned on, turn off the previously pressed one.
    fn toggle_selected(&mut self, ctx: &mut crate::event::WidgetCtx) {
        if self.disabled || self.buttons.is_empty() {
            return;
        }
        let index = self.cursor.highlighted().unwrap_or(0);
        if self
            .buttons
            .get(index)
            .map(|b| b.is_disabled())
            .unwrap_or(true)
        {
            return;
        }
        let already_pressed = self.cursor.selected() == Some(index);

        if already_pressed {
            // In a radio set, clicking the already-on button should keep it on
            // (same as Python Textual: prevents deselecting).
            ctx.set_handled();
            return;
        }

        // Turn off the previously pressed button.
        if let Some(prev) = self.cursor.selected() {
            if let Some(btn) = self.buttons.get_mut(prev) {
                btn.set_value_silent(false);
            }
        }

        // Turn on the newly selected button.
        if let Some(btn) = self.buttons.get_mut(index) {
            btn.set_value_silent(true);
        }
        self.cursor.set_selected(Some(index));

        let button_id = self.node_id();
        ctx.post_message(RadioSetChanged { index, button_id });
        ctx.request_repaint();
        ctx.set_handled();
    }

    fn first_enabled_index(&self) -> Option<usize> {
        self.buttons.iter().position(|button| !button.is_disabled())
    }

    fn has_enabled_button(&self) -> bool {
        self.buttons.iter().any(|button| !button.is_disabled())
    }

    /// Read-only access to the radio buttons.
    pub fn children(&self) -> &[RadioButton] {
        &self.buttons
    }

    /// Mutable access to the radio buttons.
    pub fn children_mut(&mut self) -> &mut Vec<RadioButton> {
        &mut self.buttons
    }
}

impl Widget for RadioSet {
    fn compose(&mut self) -> ComposeResult {
        // RadioSet renders its buttons INLINE (see `render`) and handles
        // navigation/selection itself, so the buttons must stay in `self`
        // (monolithic widget): it declares no arena children. Draining them
        // would leave `render`/`layout_height`/`content_width` with no buttons
        // (blank box, height 1).
        Vec::new()
    }

    fn focusable(&self) -> bool {
        !self.disabled && self.has_enabled_button()
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

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        if self.disabled || self.buttons.is_empty() {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                // Determine which button was clicked by y coordinate.
                let index = mouse.y as usize;
                if index < self.buttons.len() && !self.buttons[index].is_disabled() {
                    self.cursor.set_highlighted(Some(index));
                    self.toggle_selected(ctx);
                }
            }
            Event::Key(key) if self.node_state().focused => match key.code {
                KeyCode::Up | KeyCode::Left => {
                    self.move_selection(-1);
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::Down | KeyCode::Right => {
                    self.move_selection(1);
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.toggle_selected(ctx);
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        // Intercept RadioButtonChanged messages from child buttons.
        // This handles the case where a child button is toggled directly
        // (e.g. via its own event handler if it ever receives one).
        if let Some(rbc) = message.downcast_ref::<RadioButtonChanged>() {
            let value = rbc.value;
            // Find which button sent this message.
            if let Some(index) = self
                .buttons
                .iter()
                .position(|_b| message.sender == self.node_id())
            {
                if value {
                    // A button was turned on — enforce mutual exclusion.
                    if let Some(prev) = self.cursor.selected() {
                        if prev != index {
                            if let Some(btn) = self.buttons.get_mut(prev) {
                                btn.set_value_silent(false);
                            }
                        }
                    }
                    self.cursor.set_selected(Some(index));
                    self.cursor.set_highlighted(Some(index));

                    let button_id = self.node_id();
                    ctx.post_message(RadioSetChanged { index, button_id });
                    ctx.request_repaint();
                } else {
                    // A button was turned off — in a radio set, prevent deselection.
                    // Re-enable the button silently.
                    if let Some(btn) = self.buttons.get_mut(index) {
                        btn.set_value_silent(true);
                    }
                }
                ctx.set_handled();
            }
        }
    }

    fn on_mouse_move(&mut self, _x: u16, y: u16) -> bool {
        if self.disabled || self.buttons.is_empty() {
            return false;
        }
        let index = y as usize;
        let hovered =
            (index < self.buttons.len() && !self.buttons[index].is_disabled()).then_some(index);
        if hovered != self.hovered_index {
            self.hovered_index = hovered;
            return true;
        }
        false
    }

    // NOTE: RadioSet intentionally does NOT implement visit_children_mut.
    // The set manages focus delegation internally — individual RadioButtons
    // should not appear in the global focus traversal.

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        // Python renders each `RadioButton` child via `ToggleButton._button` /
        // `ToggleButton.render`, resolving the `toggle--button` / `toggle--label`
        // component styles in the button's own DOM context. `RadioSet` here is a
        // monolithic inline widget (its buttons are not arena children), so we
        // reproduce the same CSS cascade manually: the RadioSet's own selector
        // meta + resolved style are already on the selector/style stacks (pushed
        // by `render_widget_with_meta`), and per row we push a synthetic
        // `RadioButton` context carrying the live `-on` / `-selected` classes so
        // that `RadioSet:focus/:blur > RadioButton.-on/.-selected > .toggle--*`
        // rules resolve exactly as in Python.
        let panel_bg = crate::style::parse_color_like("$panel")
            .unwrap_or(crate::style::Color::rgb(0, 0, 0))
            .to_simple_opaque();

        // The RadioSet's own composited surface (its `$surface` bg plus the
        // `:focus` `background-tint`, if focused). Python composes the
        // `$block-cursor-blurred-background` (`$primary 30%`) selected-label
        // background over this surface; resolving it standalone would flatten the
        // alpha over black instead. Computed once (constant for this widget).
        let surface_bg = crate::css::current_composited_background();

        for row in 0..height {
            if row >= self.buttons.len() {
                // Padding rows below the buttons: transparent so the widget's
                // (tinted) surface composites through.
                out.push(Segment::styled(" ".repeat(width), rich_rs::Style::new()));
            } else {
                let button = &self.buttons[row];
                let is_selected = self.cursor.highlighted() == Some(row);
                let is_pressed = self.cursor.selected() == Some(row) || button.value();

                // Synthetic RadioButton context for this row. `-on` == pressed
                // (value), `-selected` == navigation cursor (highlighted).
                let mut rb_classes: Vec<&str> = Vec::new();
                if is_pressed {
                    rb_classes.push("-on");
                }
                if is_selected {
                    rb_classes.push("-selected");
                }
                let rb_meta = crate::css::selector_meta_component("RadioButton", &rb_classes);
                let rb_resolved = crate::css::resolve_style_for_meta(&rb_meta);
                crate::css::push_style_context(rb_meta, rb_resolved);

                // Leaf metas use an empty type so only the component-class-scoped
                // rules match (not the `RadioButton { border; background }` base
                // rule, which would otherwise pollute the glyph/label surface).
                let button_rich = crate::css::resolve_style_for_meta(
                    &crate::css::selector_meta_component("", &["toggle--button"]),
                )
                .to_rich()
                .unwrap_or_else(rich_rs::Style::new);
                let label_style = crate::css::resolve_style_for_meta(
                    &crate::css::selector_meta_component("", &["toggle--label"]),
                );

                crate::css::pop_style_context();

                let mut label_rich = label_style.to_rich().unwrap_or_else(rich_rs::Style::new);
                // Flatten the (possibly semi-transparent) selected-label
                // background over the widget's composited surface, matching
                // Python's `background_colors` compositing.
                if let (Some(bg), Some(surf)) = (label_style.bg, surface_bg) {
                    label_rich.bgcolor = Some(bg.flatten_over(surf).to_simple_opaque());
                }

                // Python's ToggleButton always renders the inner glyph (`●`);
                // the on/off state is conveyed by the glyph colour (`-on`), not by
                // swapping to an empty `○`.
                let glyph = "●";

                // The side half-blocks take the button's *background* as their
                // foreground (Python `side_style.foreground = button_style.background`)
                // over the widget surface, so they blend into the rounded button.
                // Their background is left transparent so the RadioSet surface —
                // and its `:focus` `background-tint` — composites through, exactly
                // like Python's `background_colors[1]`.
                let side_fg = button_rich.bgcolor.unwrap_or(panel_bg);
                let side_style = rich_rs::Style::new().with_color(side_fg);

                // Inner glyph carries the fully-resolved `toggle--button` style
                // (opaque `$panel` background). Tag `no_style` so the RadioSet's
                // `:focus` `background-tint` is NOT re-applied to it — Python does
                // not tint component-painted backgrounds.
                let mut inner_seg = Segment::styled(glyph.to_string(), button_rich);
                tag_segment_no_style(&mut inner_seg);

                // Label is padded (1, 1) and stylised like Python's
                // `self._label.pad(1, 1).stylize_before(label_style)`.
                let mut label_seg =
                    Segment::styled(format!(" {} ", button.label()), label_rich);
                // The selected row's label has an opaque `$block-cursor(-blurred)`
                // background; tag it `no_style` for the same reason as the glyph.
                // Non-selected labels have no explicit background and must stay
                // transparent so the surface (tint) shows through.
                if label_rich.bgcolor.is_some() {
                    tag_segment_no_style(&mut label_seg);
                }

                let segments = vec![
                    Segment::styled("▐".to_string(), side_style),
                    inner_seg,
                    Segment::styled("▌".to_string(), side_style),
                    label_seg,
                ];

                let line = adjust_line_length_no_bg(&segments, width);
                out.extend(line);
            }

            if row + 1 < height {
                out.push(Segment::line());
            }
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        // One row per button + own border/padding chrome (default `border: tall`
        // adds 2). The layout side adds only margin (extract_child_spec).
        Some(self.buttons.len().max(1) + super::helpers::resolved_vertical_chrome(self))
    }

    fn content_width(&self) -> Option<usize> {
        let content_width = self
            .buttons
            .iter()
            .map(|b| {
                // "▐●▌ " + label = 4 + label width
                rich_rs::cell_len(b.label()).saturating_add(4)
            })
            .max()
            .unwrap_or(4)
            .max(1);
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
        "RadioSet"
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for RadioSet {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl ReactiveWidget for RadioSet {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::keys::KeyEventData;
    use crate::node_id::NodeId;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
    use crate::widgets::NodeState;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn make_node_id() -> NodeId {
        use slotmap::SlotMap;
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    fn focused_state() -> NodeState {
        NodeState {
            focused: true,
            ..Default::default()
        }
    }

    #[test]
    fn radio_set_space_changes_selection_and_emits_message() {
        let mut set = RadioSet::from_labels(&["A", "B", "C"]);
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        let down = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let mut ctx1 = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx1);
            set.on_event(&Event::Key(down), &mut __w);
        }
        assert_eq!(set.selected_index(), 1);

        let space =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let mut ctx2 = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx2);
            set.on_event(&Event::Key(space), &mut __w);
        }
        assert_eq!(set.pressed_index(), Some(1));
        let messages = ctx2.take_messages();
        assert!(messages.iter().any(|m| {
            m.downcast_ref::<RadioSetChanged>()
                .is_some_and(|r| r.index == 1)
        }));
    }

    #[test]
    fn radio_set_cannot_deselect_active_button() {
        let mut set = RadioSet::new().with_button(RadioButton::new("A").with_value(true));
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        let space =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            set.on_event(&Event::Key(space), &mut __w);
        }
        assert_eq!(set.pressed_index(), Some(0));
    }

    #[test]
    fn radio_set_navigation_skips_disabled_buttons() {
        let mut set = RadioSet::new()
            .with_button(RadioButton::new("A").disabled(true))
            .with_button(RadioButton::new("B"))
            .with_button(RadioButton::new("C").disabled(true))
            .with_button(RadioButton::new("D"));
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        assert_eq!(set.selected_index(), 1);

        let down = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            set.on_event(&Event::Key(down), &mut __w);
        }
        assert_eq!(set.selected_index(), 3);
    }

    #[test]
    fn radio_set_click_on_disabled_button_is_ignored() {
        let mut set = RadioSet::new()
            .with_button(RadioButton::new("A"))
            .with_button(RadioButton::new("B").disabled(true));

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            set.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: NodeId::default(),
                screen_x: 0,
                screen_y: 1,
                x: 0,
                y: 1,
            }),
            &mut __w);
        }

        assert!(!ctx.handled());
        assert_eq!(set.pressed_index(), None);
    }

    #[test]
    fn radio_set_disabled_ignores_input() {
        let mut set = RadioSet::from_labels(&["A", "B"]).disabled(true);
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        let down = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            set.on_event(&Event::Key(down), &mut __w);
        }
        assert!(!ctx.handled());
        assert!(!set.focusable());
    }

    // ── Compose / children accessor tests ───────────────────────────────

    #[test]
    fn compose_returns_empty() {
        let mut set = RadioSet::from_labels(&["A", "B"]);
        assert!(set.compose().is_empty());
    }

    #[test]
    fn children_accessor_returns_buttons() {
        let set = RadioSet::from_labels(&["A", "B", "C"]);
        assert_eq!(set.children().len(), 3);
        assert_eq!(set.children()[0].label(), "A");
        assert_eq!(set.children()[2].label(), "C");
    }

    #[test]
    fn children_mut_allows_modification() {
        let mut set = RadioSet::from_labels(&["A", "B"]);
        set.children_mut().push(RadioButton::new("C"));
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn compose_keeps_buttons_internal() {
        // RadioSet renders its buttons inline and handles its own navigation,
        // so it does NOT drain them into the arena (draining left it blank).
        let mut set = RadioSet::from_labels(&["A", "B", "C"]);
        let children = set.compose();
        assert!(
            children.is_empty(),
            "buttons must stay internal, not drained"
        );
        assert_eq!(set.len(), 3, "buttons remain available for inline render");
    }

    // ── P1-14 dispatch-context regression tests ─────────────────────────

    #[test]
    fn mouse_click_with_dispatch_context_is_handled() {
        let mut set = RadioSet::from_labels(&["A", "B"]);

        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, NodeState::default());

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            set.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut __w);
        }
        // First button (index 0) should be selected.
        assert_eq!(set.pressed_index(), Some(0));
    }

    #[test]
    fn mouse_click_with_wrong_target_is_ignored() {
        use slotmap::SlotMap;

        let mut set = RadioSet::from_labels(&["A", "B"]);

        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        let my_id = sm.insert(());
        let other_id = sm.insert(());
        let _guard = set_dispatch_recipient(my_id, NodeState::default());

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            set.on_event(
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
        assert_eq!(set.pressed_index(), None);
    }
}
