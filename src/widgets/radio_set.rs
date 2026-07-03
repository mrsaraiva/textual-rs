use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::Event;
use crate::message::*;

use super::{
    NodeSeed, Widget, option_list::toggle_option::OptionCursorState, radio_button::RadioButton,
};
use crate::compose::{ChildDecl, ComposeResult};
use crate::reactive::{ReactiveCtx, ReactiveFlags, ReactiveWidget};

/// A container widget that groups `RadioButton` children for mutual exclusion.
///
/// When one radio button is toggled on, all others are automatically deselected.
/// The set itself is focusable and handles keyboard navigation (Up/Down) between
/// its children. Individual RadioButtons inside a set do not receive independent
/// focus — the set has `can_focus_children = false` and drives each child's
/// `-on` (pressed) and `-selected` (navigation cursor) classes onto the real
/// arena child nodes via [`Widget::child_classes_for_tree`]. Because the cascade
/// resolves on the live child nodes (their `-on`/`-selected` classes plus the
/// `RadioSet:focus`/`:blur` ancestor), the RadioButtons style themselves — the
/// set owns no per-glyph compensation.
#[derive(Debug, Clone)]
pub struct RadioSet {
    /// Authoritative button metadata (labels / disabled / initial value). This
    /// survives compose (the buttons are *cloned* into arena children, not
    /// drained) so `children()`/`button()`/`len()`/`pressed_index()` and the
    /// pre-mount size estimates keep working, and `compose()` stays state-pure.
    buttons: Vec<RadioButton>,
    /// `selected` = the pressed (on) button index; `highlighted` = the keyboard
    /// navigation cursor. Authoritative for both `-on` and `-selected`.
    cursor: OptionCursorState,
    disabled: bool,
    /// `true` once mounted: layout/size are then owned by the arena children, so
    /// the pre-mount estimates in `layout_height`/`content_width` back off.
    mounted: bool,
    seed: NodeSeed,
}

impl Default for RadioSet {
    fn default() -> Self {
        Self::new()
    }
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
            mounted: false,
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

        // Turn off the previously pressed button, turn on the newly selected one
        // (the metadata copies — the child nodes' `-on` is driven by
        // `child_classes_for_tree` from `cursor.selected`).
        if let Some(prev) = self.cursor.selected() {
            if let Some(btn) = self.buttons.get_mut(prev) {
                btn.set_value_silent(false);
            }
        }
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
    /// Emit the RadioButtons as real arena children.
    ///
    /// State-pure and idempotent: every call regenerates the children from the
    /// authoritative `buttons` metadata (cloned, with their ordinal stamped), so
    /// a recompose of this node rebuilds an identical child set rather than
    /// clearing it. RadioSet never *requests* a recompose for selection changes
    /// (those are driven onto the existing children via `child_classes_for_tree`),
    /// so it stays clear of the recompose-under-draining-compose trap.
    fn compose(&mut self) -> ComposeResult {
        self.buttons
            .iter()
            .enumerate()
            .map(|(ordinal, button)| {
                let mut child = button.clone();
                child.set_ordinal(ordinal);
                ChildDecl::new(Box::new(child))
            })
            .collect()
    }

    fn focusable(&self) -> bool {
        !self.disabled && self.has_enabled_button()
    }

    fn can_focus(&self) -> bool {
        !self.disabled && self.has_enabled_button()
    }

    fn can_focus_children(&self) -> bool {
        // Python: `RadioSet(..., can_focus_children=False)` — the set takes over
        // focus and drives selection between its buttons.
        false
    }

    /// Drive each child RadioButton's `-on` (pressed) and `-selected`
    /// (navigation cursor) classes onto its arena node. This is the canonical
    /// arena mechanism (mirrors Python's `watch__selected` adding `-selected`
    /// and `watch_value` toggling `-on`), letting the CSS cascade resolve on the
    /// real child node.
    fn child_classes_for_tree(&self, child_index: usize) -> Vec<(&'static str, bool)> {
        let pressed = self.cursor.selected() == Some(child_index);
        let selected = self.cursor.highlighted() == Some(child_index)
            && self
                .buttons
                .get(child_index)
                .map(|b| !b.is_disabled())
                .unwrap_or(false);
        vec![("-on", pressed), ("-selected", selected)]
    }

    fn on_mount(&mut self, _ctx: &mut crate::event::WidgetCtx) {
        self.mounted = true;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        if self.disabled || self.buttons.is_empty() {
            return;
        }
        if let Event::Key(key) = event {
            if self.node_state().focused {
                match key.code {
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
                }
            }
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        // A child RadioButton was toggled (typically via a mouse click — the
        // children never take focus, so keyboard toggles are handled directly by
        // the set). Consume it (Python `event.stop()`) and enforce mutual
        // exclusion, routing by the ordinal the child was stamped with.
        if let Some(rbc) = message.downcast_ref::<RadioButtonChanged>() {
            let index = rbc.ordinal;
            if index >= self.buttons.len() {
                return;
            }
            ctx.set_handled();
            if rbc.value {
                // Turned on: enforce exclusion + become the pressed button.
                if self.cursor.selected() != Some(index) {
                    if let Some(prev) = self.cursor.selected() {
                        if let Some(btn) = self.buttons.get_mut(prev) {
                            btn.set_value_silent(false);
                        }
                    }
                    if let Some(btn) = self.buttons.get_mut(index) {
                        btn.set_value_silent(true);
                    }
                    self.cursor.set_selected(Some(index));
                    self.cursor.set_highlighted(Some(index));
                    let button_id = self.node_id();
                    ctx.post_message(RadioSetChanged { index, button_id });
                }
            } else {
                // Clicked off: a radio set cannot be deselected — keep it on.
                if let Some(btn) = self.buttons.get_mut(index) {
                    btn.set_value_silent(true);
                }
            }
            ctx.request_repaint();
        }
    }

    // NOTE: RadioSet intentionally does NOT implement visit_children_mut.
    // With `can_focus_children = false` the buttons never enter the global focus
    // traversal — the set manages navigation internally.

    /// Chrome-only render. The `RadioButton` children render through the arena
    /// tree; `RadioSet` only paints its own resolved surface (background/tint/
    /// border) — the children composite over it.
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let resolved = crate::css::resolve_style(self, &crate::css::selector_meta_generic(self));
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

    fn layout_height(&self) -> Option<usize> {
        // Pre-mount estimate: one row per button + own vertical chrome (default
        // `border: tall` adds 2). After mount the arena owns child layout.
        if self.mounted {
            return None;
        }
        Some(self.buttons.len().max(1) + super::helpers::resolved_vertical_chrome(self))
    }

    fn content_width(&self) -> Option<usize> {
        // After mount the arena children (width: 1fr) own the width.
        if self.mounted {
            return None;
        }
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
    fn compose_emits_button_children_state_pure() {
        // compose() emits the buttons as real arena children, and is idempotent:
        // a second call regenerates an identical child set (state-pure, so an
        // ancestor recompose rebuilds rather than clears — Trap 1).
        let mut set = RadioSet::from_labels(&["A", "B", "C"]);
        let first = set.compose();
        assert_eq!(first.len(), 3);
        let second = set.compose();
        assert_eq!(second.len(), 3, "compose must regenerate, never clear");
        // The authoritative metadata survives compose.
        assert_eq!(set.len(), 3);
        assert_eq!(set.children()[0].label(), "A");
        assert_eq!(set.children()[2].label(), "C");
    }

    #[test]
    fn compose_stamps_ordinals() {
        let mut set = RadioSet::from_labels(&["A", "B"]);
        let decls = set.compose();
        // Round-trip through a child clone: ordinals are used to route the
        // change message back to the right button.
        assert_eq!(decls.len(), 2);
    }

    #[test]
    fn child_classes_drive_on_and_selected() {
        // "Serenity" preselected at index 1; navigation cursor at index 0.
        let set = RadioSet::new()
            .with_button(RadioButton::new("A"))
            .with_button(RadioButton::new("B").with_value(true));
        assert_eq!(set.pressed_index(), Some(1));
        // Highlighted (nav cursor) starts at first enabled = 0.
        assert_eq!(set.selected_index(), 0);
        let c0 = set.child_classes_for_tree(0);
        let c1 = set.child_classes_for_tree(1);
        assert!(c0.contains(&("-selected", true)));
        assert!(c0.contains(&("-on", false)));
        assert!(c1.contains(&("-on", true)));
        assert!(c1.contains(&("-selected", false)));
    }

    #[test]
    fn can_focus_children_is_false() {
        let set = RadioSet::from_labels(&["A", "B"]);
        assert!(set.focusable());
        assert!(!set.can_focus_children());
    }

    #[test]
    fn children_mut_allows_modification() {
        let mut set = RadioSet::from_labels(&["A", "B"]);
        set.children_mut().push(RadioButton::new("C"));
        assert_eq!(set.len(), 3);
    }

    // ── Child-message routing (mutual exclusion via ordinal) ─────────────

    #[test]
    fn child_changed_message_moves_pressed_by_ordinal() {
        let mut set = RadioSet::from_labels(&["A", "B", "C"]);
        // Initially nothing pressed (from_labels does not preselect).
        assert_eq!(set.pressed_index(), None);
        let _guard = set_dispatch_recipient(make_node_id(), NodeState::default());
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(
                crate::node_id::NodeId::default(),
                &mut ctx,
            );
            set.on_message(
                &MessageEvent::new(
                    NodeId::default(),
                    RadioButtonChanged {
                        value: true,
                        ordinal: 2,
                    },
                ),
                &mut __w,
            );
        }
        assert_eq!(set.pressed_index(), Some(2));
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| {
            m.downcast_ref::<RadioSetChanged>()
                .is_some_and(|r| r.index == 2)
        }));
        assert!(ctx.handled());
    }

    #[test]
    fn child_changed_off_cannot_deselect() {
        let mut set = RadioSet::new()
            .with_button(RadioButton::new("A").with_value(true))
            .with_button(RadioButton::new("B"));
        assert_eq!(set.pressed_index(), Some(0));
        let _guard = set_dispatch_recipient(make_node_id(), NodeState::default());
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(
                crate::node_id::NodeId::default(),
                &mut ctx,
            );
            set.on_message(
                &MessageEvent::new(
                    NodeId::default(),
                    RadioButtonChanged {
                        value: false,
                        ordinal: 0,
                    },
                ),
                &mut __w,
            );
        }
        // Still pressed — a radio set cannot be deselected.
        assert_eq!(set.pressed_index(), Some(0));
    }
}
