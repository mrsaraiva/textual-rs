use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, MetaValue, Renderable, Segment, Segments, StyleMeta, Text};

use crate::debug::{debug_input, debug_message};
use crate::event::{Action, Event, EventCtx};
use crate::message::*;
#[cfg(test)]
use crate::node_id::NodeId;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

use crate::action::ParsedAction;

#[cfg(test)]
use super::NodeState;
use super::{BindingDecl, NodeSeed, Widget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Default,
    Primary,
    Success,
    Warning,
    Error,
}

#[derive(Clone)]
pub struct Button {
    label: String,
    /// Rich text content for the button label. When set, takes precedence over
    /// the plain `label` string during rendering.
    content: Option<Text>,
    /// Optional action string dispatched on press instead of `ButtonPressed`.
    /// Mirrors Python Textual's `Button(action=...)` parameter.
    action: Option<String>,
    pressed: PressedState,
    variant: ButtonVariant,
    disabled: bool,
    flat: bool,
    compact: bool,
    seed: NodeSeed,
    /// CSS id cached from the seed at `take_node_seed` time so `ButtonPressed.button_id`
    /// can include it after the seed is consumed at mount.
    css_id: Option<String>,
    /// CSS classes mirrored from `seed.classes` but NOT consumed by `take_node_seed`.
    /// Used by `style_classes()` so that `layout_height()` and component-style resolution
    /// continue to work correctly after mount (when `seed.classes` is empty).
    layout_classes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PressedState {
    #[default]
    None,
    Mouse,
    KeyboardPending,
    KeyboardUntil(u64),
}

impl std::fmt::Debug for Button {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Button")
            .field("label", &self.label)
            .field("content", &self.content.is_some())
            .field("action", &self.action)
            .field("pressed", &(self.pressed != PressedState::None))
            .field("variant", &self.variant)
            .field("disabled", &self.disabled)
            .field("flat", &self.flat)
            .field("compact", &self.compact)
            .field("classes", &self.seed.classes)
            .finish()
    }
}

impl Button {
    fn no_style_space_segment(width: usize) -> Segment {
        let mut segment = Segment::new(" ".repeat(width));
        let mut map = std::collections::BTreeMap::new();
        map.insert("textual:no_text_style".to_string(), MetaValue::Bool(true));
        let mut meta = StyleMeta::new();
        meta.meta = Some(std::sync::Arc::new(map));
        segment.meta = Some(meta);
        segment
    }

    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            content: None,
            action: None,
            pressed: PressedState::None,
            variant: ButtonVariant::Default,
            disabled: false,
            flat: false,
            compact: false,
            seed: NodeSeed::default(),
            css_id: None,
            layout_classes: Vec::new(),
        }
        .rebuild_classes()
    }

    pub fn primary(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Primary)
    }

    pub fn success(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Success)
    }

    pub fn warning(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Warning)
    }

    pub fn error(label: impl Into<String>) -> Self {
        Self::new(label).variant(ButtonVariant::Error)
    }

    pub fn pressed(&self) -> bool {
        self.pressed != PressedState::None
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self.rebuild_classes()
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.rebuild_classes()
    }

    pub fn flat(mut self, flat: bool) -> Self {
        self.flat = flat;
        self.rebuild_classes()
    }

    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self.rebuild_classes()
    }

    /// Set this button's CSS id.
    ///
    /// The id is included in `ButtonPressed.button_id`, mirroring Python's
    /// `Button.Pressed.button.id` semantics.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        self.seed.css_id = Some(id.clone());
        self.css_id = Some(id);
        self
    }

    /// Set an action string to dispatch on press instead of `ButtonPressed`.
    ///
    /// When set, clicking or pressing Enter/Space parses the action string and
    /// dispatches it through the action system. The `ButtonPressed` message is
    /// suppressed, matching Python Textual's behavior.
    ///
    /// Accepted formats: `"toggle_dark"`, `"app.quit"`, `"push_screen('settings')"`.
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    /// Set a rich text label (markup content) for the button.
    ///
    /// When set, the rich `Text` is rendered instead of the plain label string.
    /// Use `Text::from_markup("[bold]Save[/]", true)` or similar to create
    /// styled button labels.
    pub fn with_content(mut self, content: Text) -> Self {
        self.content = Some(content);
        self
    }

    /// Access the button's action string, if set.
    pub fn action(&self) -> Option<&str> {
        self.action.as_deref()
    }

    /// Access the button's rich text content, if set.
    pub fn content(&self) -> Option<&Text> {
        self.content.as_ref()
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    /// Reactive getter for `label`.
    pub fn label(&self) -> &str {
        &self.label
    }

    // Note: getters for `variant`, `disabled`, `flat` are not generated because
    // they conflict with the existing builder methods of the same name.
    // Use `is_disabled()` (Widget trait) or direct field access within the crate.

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `label`. Records the change in the provided
    /// [`ReactiveCtx`] if the value actually changed.
    pub fn set_label(&mut self, value: String, ctx: &mut ReactiveCtx) {
        if self.label != value {
            let old = self.label.clone();
            self.label = value;
            let new = self.label.clone();
            ctx.record_change(
                "label",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(new),
            );
        }
    }

    /// Reactive setter for `variant`. Records the change and triggers
    /// watcher dispatch via [`ReactiveWidget::reactive_dispatch`].
    pub fn set_variant(&mut self, value: ButtonVariant, ctx: &mut ReactiveCtx) {
        if self.variant != value {
            let old = self.variant;
            self.variant = value;
            ctx.record_change(
                "variant",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    /// Reactive setter for `disabled`. Records the change and triggers
    /// watcher dispatch via [`ReactiveWidget::reactive_dispatch`].
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

    /// Reactive setter for `flat`. Records the change and triggers
    /// watcher dispatch via [`ReactiveWidget::reactive_dispatch`].
    pub fn set_flat(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.flat != value {
            let old = self.flat;
            self.flat = value;
            ctx.record_change(
                "flat",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    /// Reactive setter for `compact`. Toggles `"-textual-compact"` CSS class.
    /// Mirrors Python's `compact = reactive(False, toggle_class="-textual-compact")`.
    pub fn set_compact(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.compact != value {
            let old = self.compact;
            self.compact = value;
            ctx.record_change(
                "compact",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_variant(
        &mut self,
        _old: &ButtonVariant,
        _new: &ButtonVariant,
        _ctx: &mut ReactiveCtx,
    ) {
        self.rebuild_classes_in_place();
    }

    fn watch_disabled(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        self.rebuild_classes_in_place();
    }

    fn watch_flat(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        self.rebuild_classes_in_place();
    }

    fn watch_compact(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        self.rebuild_classes_in_place();
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    /// The plain-text width of the button label (content or plain label).
    fn label_cell_len(&self) -> usize {
        if let Some(ref content) = self.content {
            content.cell_len()
        } else {
            rich_rs::cell_len(&self.label)
        }
    }

    pub fn describe(&self) -> String {
        let mut classes = self.seed.classes.clone();
        // Include -active when the button is in a pressed state.
        if self.pressed != PressedState::None {
            classes.push("-active".to_string());
        }
        let class_str = classes.join(" ");
        let variant = match self.variant {
            ButtonVariant::Default => "default",
            ButtonVariant::Primary => "primary",
            ButtonVariant::Success => "success",
            ButtonVariant::Warning => "warning",
            ButtonVariant::Error => "error",
        };
        format!("Button(classes='{}', variant='{}')", class_str, variant)
    }

    /// Dispatch the press: either the stored action or a `ButtonPressed` message.
    ///
    /// When `self.action` is `Some`, an `ActionDispatchRequested` message is
    /// posted and `ButtonPressed` is suppressed, matching Python Textual's
    /// behavior where `action` takes precedence over `Pressed`.
    fn dispatch_press(&mut self, ctx: &mut EventCtx) {
        if let Some(ref action_str) = self.action {
            debug_message(&format!(
                "[button] dispatch action=\"{}\" label=\"{}\"",
                action_str, self.label
            ));
            ctx.post_message(crate::message::ActionDispatchRequested {
                action: action_str.clone(),
            });
        } else {
            ctx.post_message(ButtonPressed {
                description: self.describe(),
                button_id: self.css_id.clone(),
            });
        }
    }

    fn rebuild_classes(mut self) -> Self {
        self.rebuild_classes_in_place();
        self
    }

    fn rebuild_classes_in_place(&mut self) {
        // Mirror Textual's class naming conventions where practical, but keep our legacy
        // class names around so existing demos keep working.
        // Note: focus, hover, and active state classes are NOT baked in here.
        // `:focus` is matched via NodeState by the CSS resolver.
        // `-active` is added/removed dynamically via ctx.add_class/ctx.remove_class.
        let mut classes = vec!["button".to_string()];
        if self.flat {
            classes.push("flat".to_string());
            classes.push("-style-flat".to_string());
        } else {
            classes.push("-style-default".to_string());
        }
        match self.variant {
            ButtonVariant::Primary => {
                classes.push("primary".to_string());
                classes.push("-primary".to_string());
            }
            ButtonVariant::Success => {
                classes.push("success".to_string());
                classes.push("-success".to_string());
            }
            ButtonVariant::Warning => {
                classes.push("warning".to_string());
                classes.push("-warning".to_string());
            }
            ButtonVariant::Error => {
                classes.push("error".to_string());
                classes.push("-error".to_string());
            }
            ButtonVariant::Default => {}
        }
        if self.compact {
            classes.push("-textual-compact".to_string());
        }
        if self.disabled {
            classes.push("disabled".to_string());
        }
        self.layout_classes = classes.clone();
        self.seed.classes = classes;
    }
}

impl ReactiveWidget for Button {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            match change.field_name {
                "variant" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<ButtonVariant>(),
                        change.new_value.downcast_ref::<ButtonVariant>(),
                    ) {
                        self.watch_variant(old, new, ctx);
                    }
                }
                "disabled" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_disabled(old, new, ctx);
                    }
                }
                "flat" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_flat(old, new, ctx);
                    }
                }
                "compact" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_compact(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Widget for Button {
    fn focusable(&self) -> bool {
        !self.disabled
    }

    fn can_focus(&self) -> bool {
        true
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        if self.disabled != new.disabled {
            self.disabled = new.disabled;
            self.rebuild_classes_in_place();
        }
    }

    fn mouse_interactive(&self) -> bool {
        // Buttons should still get hover affordances even when disabled.
        true
    }

    fn is_active(&self) -> bool {
        match self.pressed {
            PressedState::None => false,
            PressedState::Mouse => self.node_state().hovered,
            _ => true,
        }
    }

    fn is_initially_disabled(&self) -> bool {
        self.disabled
    }

    fn style_classes(&self) -> &[String] {
        &self.layout_classes
    }

    fn content_width(&self) -> Option<usize> {
        // Match Textual's default behavior: content width is label width + a small padding.
        Some(self.label_cell_len().saturating_add(2).max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                // Enter active visual state immediately on targeted press even if
                // hover-move events haven't run yet in this frame.
                self.pressed = PressedState::Mouse;
                ctx.add_class("-active");
                debug_input(&format!(
                    "[button] mouse id={} label=\"{}\"",
                    0u64, self.label
                ));
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(mouse) => {
                if self.pressed == PressedState::Mouse {
                    // Activate only on click (mouse released while still over the button).
                    if mouse.target.is_some_and(|t| t == self.node_id()) {
                        debug_message(&format!(
                            "[button] emit mouse_up sender={} label=\"{}\"",
                            0u64, self.label
                        ));
                        self.dispatch_press(ctx);
                        ctx.set_handled();
                    } else {
                        debug_message(&format!(
                            "[button] cancel mouse_up sender={} label=\"{}\" up_target={:?}",
                            0u64, self.label, mouse.target
                        ));
                    }
                    self.pressed = PressedState::None;
                    ctx.remove_class("-active");
                    ctx.request_repaint();
                }
            }
            Event::Action(Action::Toggle) if self.node_state().focused => {
                self.pressed = PressedState::KeyboardPending;
                ctx.add_class("-active");
                debug_message(&format!(
                    "[button] emit action_toggle sender={} label=\"{}\"",
                    0u64, self.label
                ));
                self.dispatch_press(ctx);
                debug_input(&format!(
                    "[button] toggle id={} label=\"{}\"",
                    0u64, self.label
                ));
                ctx.set_handled();
            }
            Event::Key(key) if self.node_state().focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.pressed = PressedState::KeyboardPending;
                    ctx.add_class("-active");
                    debug_message(&format!(
                        "[button] emit key sender={} label=\"{}\" code={:?}",
                        0u64, self.label, key.code
                    ));
                    self.dispatch_press(ctx);
                    debug_input(&format!(
                        "[button] key id={} label=\"{}\"",
                        0u64, self.label
                    ));
                    ctx.set_handled();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn action_namespace(&self) -> &str {
        "button"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("enter,space", "press", "Press button")]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        if self.disabled {
            return false;
        }
        match action.name.as_str() {
            "press" => {
                self.pressed = PressedState::KeyboardPending;
                debug_message(&format!(
                    "[button] emit action sender={} label=\"{}\"",
                    0u64, self.label
                ));
                self.dispatch_press(ctx);
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }

    fn on_tick(&mut self, tick: u64) {
        match self.pressed {
            PressedState::KeyboardPending => {
                self.pressed = PressedState::KeyboardUntil(tick + 2);
            }
            PressedState::KeyboardUntil(expire) if tick >= expire => {
                self.pressed = PressedState::None;
            }
            _ => {}
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let state = self.node_state();
        if state.hovered || state.focused || self.is_active() {
            let meta = crate::css::selector_meta_generic(self);
            let resolved = crate::css::resolve_style(self, &meta);
            debug_input(&format!(
                "[hover][button-style] label=\"{}\" hovered={} focused={} active={} bg={:?} fg={:?} border_top={:?} border_bottom={:?} tint={:?}",
                self.label,
                state.hovered,
                state.focused,
                self.is_active(),
                resolved.bg,
                resolved.fg,
                resolved.border_top,
                resolved.border_bottom,
                resolved.background_tint
            ));
        }

        let width = options.size.0.max(1);

        // Rich text content takes precedence over plain label.
        if let Some(ref content) = self.content {
            let mut content_options = options.clone();
            content_options.size = (width, options.size.1);
            content_options.max_width = width;
            content_options.justify = Some(rich_rs::JustifyMethod::Center);
            return content.render(console, &content_options);
        }

        // `line-pad`: when the label fits, pad it with styled spaces that SHARE the
        // label's text-style, so the `:focus` reverse band covers them (matching
        // Python — content.py pads the styled content; only the content-align fill
        // is background-only). Plain text is unchanged: the line-pad spaces replace
        // centering spaces 1:1 (left shrinks by line_pad, the styled pad adds it
        // back), so glyph positions are identical. For buttons too narrow to fit
        // label + line-pad we keep the prior truncation — narrow-button line-pad is
        // a tracked edge case we can't yet verify against Python.
        let label = self.label.as_str();
        let line_pad = {
            let meta = crate::css::selector_meta_generic(self);
            crate::css::resolve_style(self, &meta).line_pad.unwrap_or(0) as usize
        };
        let label_cells = rich_rs::cell_len(label);
        let (content, content_width) = if line_pad > 0 && label_cells + line_pad * 2 <= width {
            let pad = " ".repeat(line_pad);
            (format!("{pad}{label}{pad}"), label_cells + line_pad * 2)
        } else {
            let w = label_cells.min(width);
            (rich_rs::set_cell_size(label, w), w)
        };
        let left = width.saturating_sub(content_width) / 2;
        let right = width.saturating_sub(content_width) - left;
        let mut out = Segments::new();
        if left > 0 {
            out.push(Self::no_style_space_segment(left));
        }
        out.push(Segment::new(rich_rs::set_cell_size(&content, content_width)));
        if right > 0 {
            out.push(Self::no_style_space_segment(right));
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        // Include the button's own classes (e.g. `-style-default`) so that
        // CSS rules nested under class selectors (border-top, border-bottom) match.
        //
        // `self.seed.classes` may be empty if `take_node_seed()` already consumed
        // it (tree-mount path), so we rebuild the class list from the button's
        // current fields which is the same deterministic logic as `rebuild_classes_in_place`.
        let mut classes = vec!["button"];
        if self.flat {
            classes.push("-style-flat");
        } else {
            classes.push("-style-default");
        }
        let meta = crate::css::selector_meta_component("Button", &classes);
        let base_style = crate::css::resolve_style(self, &meta);
        let default_height = 1 + super::helpers::border_vertical_padding(&base_style);
        Some(default_height)
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        let seed = std::mem::take(&mut self.seed);
        // Cache the CSS id so ButtonPressed.button_id can include it post-mount.
        self.css_id = seed.css_id.clone();
        seed
    }
}

impl Renderable for Button {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyEventData;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
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
    fn enter_posts_button_pressed_message() {
        let mut button = Button::new("Run");
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        let mut ctx = EventCtx::default();

        button.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );

        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| m.is::<ButtonPressed>()));
    }

    #[test]
    fn bindings_are_declared() {
        let button = Button::new("Test");
        let bindings = button.bindings();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().any(|b| b.action == "press"));
    }

    #[test]
    fn execute_action_handles_press() {
        let mut button = Button::new("Run");
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "press".to_string(),
            arguments: vec![],
        };
        assert!(button.execute_action(&action, &mut ctx));
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| m.is::<ButtonPressed>()));
    }

    // ── WP-18: Button action parameter ──────────────────────────────────

    #[test]
    fn with_action_stores_action_string() {
        let button = Button::new("Quit").with_action("app.quit");
        assert_eq!(button.action(), Some("app.quit"));
    }

    #[test]
    fn action_suppresses_button_pressed_on_enter() {
        let mut button = Button::new("Quit").with_action("app.quit");
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        let mut ctx = EventCtx::default();

        button.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );

        let messages = ctx.take_messages();
        // ButtonPressed should NOT be posted when action is set.
        assert!(
            !messages.iter().any(|m| m.is::<ButtonPressed>()),
            "ButtonPressed should be suppressed when action is set"
        );
        assert!(
            messages
                .iter()
                .any(|m| m.is::<crate::message::ActionDispatchRequested>()),
            "ActionDispatchRequested should be emitted when action is set"
        );
    }

    #[test]
    fn action_suppresses_button_pressed_on_execute_action() {
        let mut button = Button::new("Quit").with_action("app.quit");
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "press".to_string(),
            arguments: vec![],
        };
        assert!(button.execute_action(&action, &mut ctx));

        let messages = ctx.take_messages();
        assert!(
            !messages.iter().any(|m| m.is::<ButtonPressed>()),
            "ButtonPressed should be suppressed when action is set"
        );
        assert!(
            messages
                .iter()
                .any(|m| m.is::<crate::message::ActionDispatchRequested>()),
            "ActionDispatchRequested should be emitted when action is set"
        );
    }

    #[test]
    fn no_action_posts_button_pressed() {
        let mut button = Button::new("Run");
        let _guard = set_dispatch_recipient(make_node_id(), focused_state());
        let mut ctx = EventCtx::default();

        button.on_event(
            &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char(' '),
                KeyModifiers::NONE,
            ))),
            &mut ctx,
        );

        let messages = ctx.take_messages();
        assert!(
            messages.iter().any(|m| m.is::<ButtonPressed>()),
            "ButtonPressed should be posted when no action is set"
        );
    }

    #[test]
    fn action_builder_is_chainable() {
        let button = Button::primary("Save")
            .with_action("app.save")
            .disabled(false);
        assert_eq!(button.action(), Some("app.save"));
        assert!(!button.disabled);
    }

    #[test]
    fn mouse_down_enters_active_state_without_prior_hover() {
        use crate::event::ClassOp;
        let mut button = Button::new("Run");
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, NodeState::default());
        let mut ctx = EventCtx::default();

        button.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: id,
                screen_x: 1,
                screen_y: 1,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );

        assert!(button.pressed(), "mouse down should set pressed state");
        // After RA-2, active state is signalled via ctx.add_class rather than a widget field.
        let ops = ctx.take_class_ops();
        assert!(
            ops.iter()
                .any(|(_, op)| matches!(op, ClassOp::Add(c) if c == "-active")),
            "mouse down should queue -active class add"
        );
    }

    #[test]
    fn mouse_click_message_description_includes_active_class() {
        let mut button = Button::new("Run");
        let mut ctx = EventCtx::default();

        button.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: NodeId::default(),
                screen_x: 1,
                screen_y: 1,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
        button.on_event(
            &Event::MouseUp(crate::event::MouseUpEvent {
                target: Some(NodeId::default()),
                screen_x: 1,
                screen_y: 1,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );

        let description = ctx
            .take_messages()
            .into_iter()
            .find_map(|event| {
                event
                    .downcast_ref::<ButtonPressed>()
                    .map(|p| p.description.clone())
            })
            .expect("mouse click should emit ButtonPressed");

        assert!(
            description.contains("-active"),
            "mouse click ButtonPressed description should include -active; got: {description}"
        );
    }

    // ── WP-19: Button markup label ──────────────────────────────────────

    #[test]
    fn with_content_stores_rich_text() {
        let text = Text::plain("Bold Label");
        let button = Button::new("fallback").with_content(text);
        assert!(button.content().is_some());
    }

    #[test]
    fn content_width_uses_content_when_set() {
        let text = Text::plain("Short");
        let button = Button::new("much longer fallback label").with_content(text);
        // Content width should use the rich text length, not the plain label.
        let cw = button.content_width().unwrap();
        // "Short" = 5 chars + 2 padding = 7
        assert_eq!(cw, 7);
    }

    #[test]
    fn content_width_uses_label_when_no_content() {
        let button = Button::new("Hello");
        let cw = button.content_width().unwrap();
        // "Hello" = 5 chars + 2 padding = 7
        assert_eq!(cw, 7);
    }

    #[test]
    fn render_with_content_produces_segments() {
        let text = Text::plain("OK");
        let button = Button::new("fallback").with_content(text);
        let console = Console::new();
        let options = ConsoleOptions {
            size: (20, 1),
            max_width: 20,
            ..Default::default()
        };
        let segments = Widget::render(&button, &console, &options);
        assert!(!segments.is_empty());
    }

    #[test]
    fn render_plain_label_produces_centered_text() {
        let button = Button::new("OK");
        let console = Console::new();
        let options = ConsoleOptions {
            size: (10, 1),
            max_width: 10,
            ..Default::default()
        };
        let segments = Widget::render(&button, &console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text.len(), 10);
        assert!(text.contains("OK"));
    }

    #[test]
    fn markup_content_renders_correctly() {
        let text = Text::from_markup("[bold]Save[/]", true).unwrap();
        let button = Button::new("fallback").with_content(text);
        assert!(button.content().is_some());
        assert_eq!(button.content().unwrap().plain_text(), "Save");
    }

    // ── Reactive field tests ────────────────────────────────────────────

    #[test]
    fn reactive_label_getter() {
        let button = Button::new("Hello");
        assert_eq!(button.label(), "Hello");
    }

    #[test]
    fn reactive_set_label_records_change() {
        let mut button = Button::new("Old");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        button.set_label("New".to_string(), &mut ctx);
        assert_eq!(button.label(), "New");
        assert!(ctx.has_changes());
        assert!(ctx.needs_repaint());
        assert!(ctx.needs_layout());
        assert_eq!(ctx.changes()[0].field_name, "label");
    }

    #[test]
    fn reactive_set_label_noop_when_same() {
        let mut button = Button::new("Same");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        button.set_label("Same".to_string(), &mut ctx);
        assert!(!ctx.has_changes());
    }

    #[test]
    fn reactive_set_variant_records_change_and_dispatches() {
        let mut button = Button::new("Test");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        button.set_variant(ButtonVariant::Primary, &mut ctx);
        assert!(ctx.has_changes());
        let changes = ctx.take_changes();
        assert_eq!(changes[0].field_name, "variant");
        // Dispatch triggers watch_variant → rebuild_classes_in_place
        button.reactive_dispatch(&changes, &mut ctx);
        assert!(button.seed.classes.contains(&"primary".to_string()));
    }

    #[test]
    fn reactive_set_disabled_records_change_and_dispatches() {
        let mut button = Button::new("Test");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        button.set_disabled(true, &mut ctx);
        assert!(ctx.has_changes());
        let changes = ctx.take_changes();
        button.reactive_dispatch(&changes, &mut ctx);
        assert!(button.seed.classes.contains(&"disabled".to_string()));
    }

    #[test]
    fn compact_builder_adds_textual_compact_class() {
        let button = Button::new("X").compact(true);
        assert!(
            button
                .seed
                .classes
                .contains(&"-textual-compact".to_string()),
            "compact(true) should add -textual-compact class"
        );
    }

    #[test]
    fn compact_false_has_no_textual_compact_class() {
        let button = Button::new("X");
        assert!(
            !button
                .seed
                .classes
                .contains(&"-textual-compact".to_string()),
            "default button should not have -textual-compact class"
        );
    }

    #[test]
    fn reactive_set_compact_records_change_and_dispatches() {
        let mut button = Button::new("Test");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        button.set_compact(true, &mut ctx);
        assert!(ctx.has_changes());
        let changes = ctx.take_changes();
        assert_eq!(changes[0].field_name, "compact");
        button.reactive_dispatch(&changes, &mut ctx);
        assert!(
            button
                .seed
                .classes
                .contains(&"-textual-compact".to_string())
        );
    }

    #[test]
    fn reactive_set_flat_records_change_and_dispatches() {
        let mut button = Button::new("Test");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        button.set_flat(true, &mut ctx);
        assert!(ctx.has_changes());
        let changes = ctx.take_changes();
        button.reactive_dispatch(&changes, &mut ctx);
        assert!(button.seed.classes.contains(&"flat".to_string()));
        assert!(button.seed.classes.contains(&"-style-flat".to_string()));
    }
}
