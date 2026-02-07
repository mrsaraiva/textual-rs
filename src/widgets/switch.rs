use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::Message;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
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
    value: bool,
    focused: bool,
    hovered: bool,
    pressed: bool,
    disabled: bool,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl Switch {
    pub fn new(value: bool) -> Self {
        Self {
            id: WidgetId::new(),
            value,
            focused: false,
            hovered: false,
            pressed: false,
            disabled: false,
            classes: Vec::new(),
            focused_classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
        .rebuild_classes()
    }

    pub fn value(&self) -> bool {
        self.value
    }

    pub fn set_value(&mut self, value: bool) {
        if self.value != value {
            self.value = value;
            self.rebuild_classes_in_place();
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.rebuild_classes()
    }

    fn toggle(&mut self, ctx: &mut EventCtx) {
        self.value = !self.value;
        self.rebuild_classes_in_place();
        ctx.post_message(self.id, Message::SwitchChanged { value: self.value });
        ctx.request_repaint();
        ctx.set_handled();
    }

    fn rebuild_classes(mut self) -> Self {
        self.rebuild_classes_in_place();
        self
    }

    fn rebuild_classes_in_place(&mut self) {
        let mut classes = vec!["switch".to_string()];
        if self.value {
            classes.push("-on".to_string());
        } else {
            classes.push("-off".to_string());
        }
        if self.disabled {
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
        !self.disabled
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn is_active(&self) -> bool {
        self.pressed && self.hovered
    }

    fn content_width(&self) -> Option<usize> {
        Some(SWITCH_WIDTH)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == self.id => {
                self.pressed = true;
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(mouse) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                    if mouse.target == Some(self.id) {
                        self.toggle(ctx);
                        return;
                    }
                }
            }
            Event::AppFocus(false) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                }
            }
            Event::Action(Action::Toggle) if self.focused => {
                self.toggle(ctx);
                return;
            }
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.toggle(ctx);
                    return;
                }
                _ => {}
            },
            _ => {}
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

        let track = if self.value {
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
        if self.focused {
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
