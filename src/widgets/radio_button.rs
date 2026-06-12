use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::*;

use super::{
    helpers::{empty_classes, fixed_height_from_constraints},
    option_list::toggle_option::BinaryToggleState,
    Widget, WidgetStyles,
};

/// A radio button widget that represents a boolean on/off value.
///
/// RadioButton is very similar to Checkbox but uses radio semantics:
/// - Circle glyph (`●` / `○`) instead of checkbox marks
/// - Typically used inside a `RadioSet` for mutual exclusion
///
/// On its own a RadioButton can be toggled freely. When placed inside a
/// `RadioSet`, the set enforces that only one button is selected at a time.
#[derive(Debug, Clone)]
pub struct RadioButton {
    label: String,
    state: BinaryToggleState,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl RadioButton {
    pub fn new(label: impl Into<String>) -> Self {
        let label = label.into();
        Self {
            label,
            state: BinaryToggleState::new(false),
            classes: vec!["radio-button".to_string(), "-off".to_string()],
            focused_classes: vec![
                "radio-button".to_string(),
                "-off".to_string(),
                "focused".to_string(),
            ],
            styles: WidgetStyles::default(),
        }
    }

    /// Create a radio button with an initial value.
    pub fn with_value(mut self, value: bool) -> Self {
        self.state.set_value(value);
        self.rebuild_classes();
        self
    }

    /// Builder method to set the disabled state.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.set_disabled(disabled);
        self
    }

    /// Returns the current value (`true` = selected).
    pub fn value(&self) -> bool {
        self.state.value()
    }

    /// Returns the label text.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Set the value without emitting a message.
    ///
    /// This is used by `RadioSet` to programmatically deselect buttons
    /// without triggering further change events.
    pub fn set_value_silent(&mut self, value: bool) {
        self.state.set_value(value);
        self.rebuild_classes();
    }

    /// Toggle the value and emit a `RadioButtonChanged` message.
    pub fn toggle(&mut self, ctx: &mut EventCtx) {
        if self.state.disabled() {
            return;
        }
        self.state.toggle();
        self.on_toggled();
        self.emit_changed(ctx);
        ctx.request_repaint();
        ctx.set_handled();
    }

    fn emit_changed(&self, ctx: &mut EventCtx) {
        ctx.post_message(RadioButtonChanged {
            value: self.state.value(),
        });
    }

    fn on_toggled(&mut self) {
        self.rebuild_classes();
    }

    fn rebuild_classes(&mut self) {
        let on_off = if self.state.value() { "-on" } else { "-off" };
        self.classes = vec!["radio-button".to_string(), on_off.to_string()];
        self.focused_classes = vec![
            "radio-button".to_string(),
            on_off.to_string(),
            "focused".to_string(),
        ];
    }
}

impl Widget for RadioButton {
    fn focusable(&self) -> bool {
        self.state.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.state.set_focused(focused);
    }

    fn has_focus(&self) -> bool {
        self.state.focused()
    }

    fn is_disabled(&self) -> bool {
        self.state.disabled()
    }

    fn is_hovered(&self) -> bool {
        self.state.hovered()
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.state.set_hovered(hovered);
    }

    fn is_active(&self) -> bool {
        self.state.is_active()
    }

    fn content_width(&self) -> Option<usize> {
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        // Rendered content is "▐●▌ " + label.
        let content = rich_rs::cell_len(&self.label).saturating_add(4);
        Some(content.saturating_add(chrome_lr).max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        let outcome = self.state.handle_event(event, self.node_id());
        if outcome.toggled {
            self.on_toggled();
            self.emit_changed(ctx);
        }
        if outcome.repaint {
            ctx.request_repaint();
        }
        if outcome.handled {
            ctx.set_handled();
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        let glyph = if self.state.value() { "●" } else { "○" };

        // Resolve component styles for the button glyph and label separately.
        let mut glyph_classes = vec!["radio-button--button"];
        let mut label_classes = vec!["radio-button--label"];
        if self.state.value() {
            glyph_classes.push("-on");
            label_classes.push("-on");
        }
        if self.state.focused() {
            glyph_classes.push("-focus");
            label_classes.push("-focus");
        }
        if self.state.hovered() {
            glyph_classes.push("-hover");
            label_classes.push("-hover");
        }

        let glyph_style = crate::css::resolve_component_style(self, &glyph_classes)
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let label_style = crate::css::resolve_component_style(self, &label_classes)
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        // Build: "▐●▌ label"
        // Use the Textual-style half-block frame around the glyph.
        let glyph_bg = glyph_style.bgcolor;
        let parent_bg = crate::css::resolve_component_style(self, &["radio-button--button"])
            .bg
            .map(|c| c.to_simple_opaque());
        let outer_bg = parent_bg.unwrap_or_else(|| {
            crate::style::parse_color_like("$surface")
                .unwrap_or(crate::style::Color::rgb(0, 0, 0))
                .to_simple_opaque()
        });

        let left_style = rich_rs::Style::new()
            .with_color(glyph_bg.unwrap_or(outer_bg))
            .with_bgcolor(outer_bg);
        let right_style = left_style;

        let segments = vec![
            Segment::styled("▐".to_string(), left_style),
            Segment::styled(glyph.to_string(), glyph_style),
            Segment::styled("▌".to_string(), right_style),
            Segment::styled(format!(" {}", self.label), label_style),
        ];

        // Pad/crop to width.
        let line = super::helpers::adjust_line_length_no_bg(&segments, width);
        let mut out = Segments::new();
        out.extend(line);
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
    }

    fn style_classes(&self) -> &[String] {
        if self.state.focused() {
            &self.focused_classes
        } else if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn style_type(&self) -> &'static str {
        "RadioButton"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for RadioButton {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyEventData;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn radio_button_toggle_emits_message() {
        let mut button = RadioButton::new("A");
        button.set_focus(true);
        let mut ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        button.on_event(&Event::Key(key), &mut ctx);
        assert!(button.value());
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| m
            .downcast_ref::<RadioButtonChanged>()
            .is_some_and(|r| r.value)));
    }

    #[test]
    fn radio_button_disabled_is_not_focusable() {
        let button = RadioButton::new("A").disabled(true);
        assert!(!button.focusable());
    }
}
