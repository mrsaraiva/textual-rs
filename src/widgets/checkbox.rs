use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::Message;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
    option_list::toggle_option::BinaryToggleState,
};

#[derive(Debug, Clone)]
pub struct Checkbox {
    id: WidgetId,
    label: String,
    state: BinaryToggleState,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl Checkbox {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            state: BinaryToggleState::new(false),
            classes: vec!["checkbox".to_string()],
            focused_classes: vec!["checkbox".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn checked(&self) -> bool {
        self.state.value()
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.state.set_value(checked);
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.set_disabled(disabled);
        self
    }

    fn emit_changed(&self, ctx: &mut EventCtx) {
        ctx.post_message(
            self.id,
            Message::CheckboxChanged {
                checked: self.state.value(),
            },
        );
    }
}

impl Widget for Checkbox {
    fn id(&self) -> WidgetId {
        self.id
    }

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
        Some(rich_rs::cell_len(&self.label).saturating_add(4).max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        let outcome = self.state.handle_event(event, self.id);
        if outcome.toggled {
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
        let state = if self.state.value() { "☑" } else { "☐" };
        let line = rich_rs::set_cell_size(&format!("{state} {}", self.label), width);
        let mut out = Segments::new();
        out.push(Segment::new(line));
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
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
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn checkbox_emits_message_on_toggle() {
        let mut checkbox = Checkbox::new("Remember");
        checkbox.set_focus(true);
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        checkbox.on_event(&Event::Key(key), &mut ctx);
        let messages = ctx.take_messages();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m.message, Message::CheckboxChanged { checked: true }))
        );
    }
}
