use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::event::{Action, Event, EventCtx};
use crate::message::*;
#[cfg(test)]
use crate::node_id::NodeId;
use crate::reactive::{
    ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget, RuntimeReactiveEntry,
    enqueue_runtime_reactive_entry,
};

use crate::action::ParsedAction;

use super::{BindingDecl, NodeSeed, Widget};

#[derive(Debug, Clone)]
pub struct Checkbox {
    label: String,
    checked: bool,
    pressed: bool,
    disabled: bool,
    seed: NodeSeed,
}

impl Checkbox {
    pub fn new(label: impl Into<String>) -> Self {
        let mut seed = NodeSeed::default();
        seed.classes.push("checkbox".to_string());
        Self {
            label: label.into(),
            checked: false,
            pressed: false,
            disabled: false,
            seed,
        }
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    pub fn checked(&self) -> bool {
        self.checked
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `checked`. Records the change in the provided
    /// [`ReactiveCtx`] if the value actually changed.
    pub fn set_checked(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.checked != value {
            let old = self.checked;
            self.checked = value;
            ctx.record_change(
                "checked",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_checked(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        self.rebuild_classes_in_place();
    }

    // ── Builder methods ──────────────────────────────────────────────────

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.rebuild_classes_in_place();
        self
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    fn emit_changed(&self, ctx: &mut EventCtx) {
        ctx.post_message(CheckboxChanged {
            checked: self.checked,
        });
    }

    fn rebuild_classes_in_place(&mut self) {
        let mut classes = vec!["checkbox".to_string()];
        if self.checked {
            classes.push("-on".to_string());
        }
        if self.disabled {
            classes.push("disabled".to_string());
        }
        self.seed.classes = classes;
    }

    fn toggle_reactive(&mut self, ctx: &mut EventCtx) {
        let node_id = self.node_id();
        let mut reactive = ReactiveCtx::new(node_id);
        self.set_checked(!self.checked, &mut reactive);
        if reactive.has_changes() {
            enqueue_runtime_reactive_entry(RuntimeReactiveEntry::new(node_id, reactive));
            self.emit_changed(ctx);
        }
    }
}

impl ReactiveWidget for Checkbox {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            match change.field_name {
                "checked" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_checked(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Widget for Checkbox {
    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        Vec::new()
    }

    fn focusable(&self) -> bool {
        !self.disabled
    }

    fn is_active(&self) -> bool {
        self.pressed && self.node_state().hovered
    }

    fn content_width(&self) -> Option<usize> {
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        // Content is "☐ " + label.
        let content = rich_rs::cell_len(&self.label).saturating_add(2);
        Some(content.saturating_add(chrome_lr).max(1))
    }

    fn action_namespace(&self) -> &str {
        "checkbox"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("enter,space", "toggle", "Toggle checkbox")]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        match action.name.as_str() {
            "toggle" => {
                if self.disabled {
                    return false;
                }
                self.toggle_reactive(ctx);
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }

    fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                self.pressed = true;
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(mouse) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                    if mouse.target.is_some_and(|t| t == self.node_id()) {
                        self.toggle_reactive(ctx);
                        ctx.set_handled();
                    }
                }
            }
            Event::AppFocus(false) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                }
            }
            Event::Action(Action::Toggle) if self.node_state().focused => {
                self.toggle_reactive(ctx);
                ctx.set_handled();
            }
            Event::Key(key) if self.node_state().focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.toggle_reactive(ctx);
                    ctx.set_handled();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        // Python's `ToggleButton` renders `▐X▌` — the `X` is ALWAYS present; the
        // checked state is conveyed by the button color (`.toggle--button`, which
        // brightens via `&.-on > .toggle--button` since `self` carries `-on` when
        // checked), not by swapping the glyph. `self` exposes its `-on` class to
        // off-tree resolution via its seed classes.
        let button_style = crate::css::resolve_component_style(self, &["toggle--button"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let label_style = crate::css::resolve_component_style(self, &["toggle--label"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        // Side half-blocks use the button background as their foreground.
        let mut side_style = rich_rs::Style::new();
        side_style.color = button_style.bgcolor;

        let segs = [
            Segment::styled("▐".to_string(), side_style),
            Segment::styled("X".to_string(), button_style),
            Segment::styled("▌".to_string(), side_style),
            Segment::styled(format!(" {}", self.label), label_style),
        ];
        let mut out = Segments::new();
        out.extend(super::helpers::adjust_line_length_no_bg(&segs, width));
        out
    }

    fn layout_height(&self) -> Option<usize> {
        // 1 content row + own border/padding chrome (the default `border: tall`
        // adds 2 rows). The layout side adds only margin (extract_child_spec).
        Some(1 + super::helpers::resolved_vertical_chrome(self))
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for Checkbox {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyEventData;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
    use crate::widgets::NodeState;
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

    #[test]
    fn checkbox_emits_message_on_toggle() {
        let mut checkbox = Checkbox::new("Remember");
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        checkbox.on_event(&Event::Key(key), &mut ctx);
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| {
            m.downcast_ref::<CheckboxChanged>()
                .is_some_and(|c| c.checked)
        }));
    }

    #[test]
    fn bindings_are_declared() {
        let checkbox = Checkbox::new("Test");
        let bindings = checkbox.bindings();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().any(|b| b.action == "toggle"));
    }

    #[test]
    fn execute_action_handles_toggle() {
        let mut checkbox = Checkbox::new("Remember");
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "toggle".to_string(),
            arguments: vec![],
        };
        assert!(!checkbox.checked());
        assert!(checkbox.execute_action(&action, &mut ctx));
        assert!(checkbox.checked());
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| {
            m.downcast_ref::<CheckboxChanged>()
                .is_some_and(|c| c.checked)
        }));
    }

    // ── Reactive field tests ────────────────────────────────────────────

    #[test]
    fn reactive_set_checked_records_change() {
        let mut checkbox = Checkbox::new("Test");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        checkbox.set_checked(true, &mut ctx);
        assert!(checkbox.checked());
        assert!(ctx.has_changes());
        assert!(ctx.needs_repaint());
        assert_eq!(ctx.changes()[0].field_name, "checked");
    }

    #[test]
    fn reactive_set_checked_noop_when_same() {
        let mut checkbox = Checkbox::new("Test");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        checkbox.set_checked(false, &mut ctx);
        assert!(!ctx.has_changes());
    }

    #[test]
    fn reactive_dispatch_calls_watch_checked() {
        let mut checkbox = Checkbox::new("Test");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        checkbox.set_checked(true, &mut ctx);
        let changes = ctx.take_changes();
        checkbox.reactive_dispatch(&changes, &mut ctx);
        // watch_checked rebuilds classes — verify -on class
        assert!(checkbox.seed.classes.contains(&"-on".to_string()));
    }

    // ── compose / take_composed_children tests ──────────────────────────

    #[test]
    fn checkbox_compose_returns_empty() {
        let checkbox = Checkbox::new("Test");
        assert!(checkbox.compose().is_empty());
    }

    #[test]
    fn checkbox_take_composed_children_returns_empty() {
        let mut checkbox = Checkbox::new("Test");
        let taken = checkbox.take_composed_children();
        assert!(taken.is_empty());
    }
}
