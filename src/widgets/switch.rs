use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::Message;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
    option_list::toggle_option::BinaryToggleState,
};

/// The visual width of the switch slider track (in cells).
const SWITCH_WIDTH: usize = 8;

/// A boolean toggle switch widget.
///
/// Renders as a slider track with a knob that moves left/right.
/// Toggled via click, Enter, or Space.
#[derive(Debug, Clone)]
pub struct Switch {
    id: WidgetId,
    state: BinaryToggleState,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl Switch {
    pub fn new(value: bool) -> Self {
        Self {
            id: WidgetId::new(),
            state: BinaryToggleState::new(value),
            classes: Vec::new(),
            focused_classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
        .rebuild_classes()
    }

    pub fn value(&self) -> bool {
        self.state.value()
    }

    pub fn set_value(&mut self, value: bool) {
        if self.state.value() != value {
            self.state.set_value(value);
            self.rebuild_classes_in_place();
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.set_disabled(disabled);
        self.rebuild_classes()
    }

    fn emit_changed(&self, ctx: &mut EventCtx) {
        ctx.post_message(
            self.id,
            Message::SwitchChanged {
                value: self.state.value(),
            },
        );
    }

    fn on_toggled(&mut self) {
        self.rebuild_classes_in_place();
    }

    fn rebuild_classes(mut self) -> Self {
        self.rebuild_classes_in_place();
        self
    }

    fn rebuild_classes_in_place(&mut self) {
        let mut classes = vec!["switch".to_string()];
        if self.state.value() {
            classes.push("-on".to_string());
        } else {
            classes.push("-off".to_string());
        }
        if self.state.disabled() {
            classes.push("disabled".to_string());
        }
        let mut focused_classes = classes.clone();
        focused_classes.push("focused".to_string());
        self.classes = classes;
        self.focused_classes = focused_classes;
    }
}

impl Widget for Switch {
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
        Some(SWITCH_WIDTH)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        let outcome = self.state.handle_event(event, self.id);
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

        // Render a slider track with a knob.
        // The Python Textual Switch uses ScrollBarRender to draw a slider;
        // we approximate this with Unicode block characters.
        //
        // ON state:  ▐████████ ▌  (knob on right)
        // OFF state: ▐ ████████▌  (knob on left... well, space on right)
        //
        // Simplified: We draw a track where the "knob" (block chars) slides.
        let slider_style = crate::css::resolve_component_style(self, &["switch--slider"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        let track_inner = width.saturating_sub(2); // minus left/right border chars
        let knob_size = track_inner.saturating_sub(1).max(1); // knob takes most of the track

        let track = if self.state.value() {
            // ON: knob (filled) then space
            let knob = "█".repeat(knob_size);
            let space = " ".repeat(track_inner.saturating_sub(knob_size));
            format!("▐{knob}{space}▌")
        } else {
            // OFF: space then knob (filled)
            let space = " ".repeat(track_inner.saturating_sub(knob_size));
            let knob = "█".repeat(knob_size);
            format!("▐{space}{knob}▌")
        };

        let line = rich_rs::set_cell_size(&track, width);
        let mut out = Segments::new();
        out.push(Segment::styled(line, slider_style));
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
        "Switch"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Switch {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseDownEvent;
    use crate::keys::KeyEventData;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn switch_space_toggles_and_emits_message() {
        let mut widget = Switch::new(false);
        widget.set_focus(true);
        let mut ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        widget.on_event(&Event::Key(key), &mut ctx);
        assert!(widget.value());
        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m.message, Message::SwitchChanged { value: true }))
        );
    }

    #[test]
    fn switch_disabled_ignores_input() {
        let mut widget = Switch::new(false).disabled(true);
        widget.set_focus(true);
        let mut ctx = EventCtx::default();
        widget.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: widget.id(),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(!widget.value());
        assert!(!ctx.handled());
    }
}
