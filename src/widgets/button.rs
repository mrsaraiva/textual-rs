use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};
use std::sync::Arc;

use crate::debug::debug_input;
use crate::event::{Action, Event, EventCtx};
use crate::message::Message;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

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
    id: WidgetId,
    label: String,
    focused: bool,
    hovered: bool,
    pressed: PressedState,
    variant: ButtonVariant,
    disabled: bool,
    flat: bool,
    on_press: Option<Arc<dyn Fn(&Button) + Send + Sync>>,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
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
            .field("id", &self.id)
            .field("label", &self.label)
            .field("focused", &self.focused)
            .field("hovered", &self.hovered)
            .field("pressed", &(self.pressed != PressedState::None))
            .field("variant", &self.variant)
            .field("disabled", &self.disabled)
            .field("flat", &self.flat)
            .field("classes", &self.classes)
            .finish()
    }
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            focused: false,
            hovered: false,
            pressed: PressedState::None,
            variant: ButtonVariant::Default,
            disabled: false,
            flat: false,
            on_press: None,
            classes: Vec::new(),
            focused_classes: Vec::new(),
            styles: WidgetStyles::default(),
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

    pub fn on_press(mut self, handler: impl Fn(&Button) + Send + Sync + 'static) -> Self {
        self.on_press = Some(Arc::new(handler));
        self
    }

    pub fn describe(&self) -> String {
        let mut classes = self.classes.clone();
        let is_active = match self.pressed {
            PressedState::None => false,
            PressedState::Mouse => self.hovered,
            _ => true,
        };
        if is_active {
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

    fn rebuild_classes(mut self) -> Self {
        // Mirror Textual's class naming conventions where practical, but keep our legacy
        // class names around so existing demos keep working.
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
        if self.disabled {
            classes.push("disabled".to_string());
        }
        let mut focused_classes = classes.clone();
        focused_classes.push("focused".to_string());
        self.classes = classes;
        self.focused_classes = focused_classes;
        self
    }
}

impl Widget for Button {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        !self.disabled
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn mouse_interactive(&self) -> bool {
        // Buttons should still get hover affordances even when disabled.
        true
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn is_active(&self) -> bool {
        match self.pressed {
            PressedState::None => false,
            PressedState::Mouse => self.hovered,
            _ => true,
        }
    }

    fn content_width(&self) -> Option<usize> {
        // Match Textual's default behavior: content width is label width + a small padding.
        Some(rich_rs::cell_len(&self.label).saturating_add(2).max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == self.id => {
                self.pressed = PressedState::Mouse;
                debug_input(&format!(
                    "[button] mouse id={} label=\"{}\"",
                    self.id.as_u64(),
                    self.label
                ));
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(mouse) => {
                if self.pressed == PressedState::Mouse {
                    self.pressed = PressedState::None;
                    ctx.request_repaint();
                    // Activate only on click (mouse released while still over the button).
                    if mouse.target == Some(self.id) {
                        ctx.post_message(
                            self.id,
                            Message::ButtonPressed {
                                description: self.describe(),
                            },
                        );
                        if let Some(handler) = &self.on_press {
                            handler(self);
                        }
                        ctx.set_handled();
                    }
                }
            }
            Event::Action(Action::Toggle) if self.focused => {
                self.pressed = PressedState::KeyboardPending;
                ctx.post_message(
                    self.id,
                    Message::ButtonPressed {
                        description: self.describe(),
                    },
                );
                if let Some(handler) = &self.on_press {
                    handler(self);
                }
                debug_input(&format!(
                    "[button] toggle id={} label=\"{}\"",
                    self.id.as_u64(),
                    self.label
                ));
                ctx.set_handled();
            }
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.pressed = PressedState::KeyboardPending;
                    ctx.post_message(
                        self.id,
                        Message::ButtonPressed {
                            description: self.describe(),
                        },
                    );
                    if let Some(handler) = &self.on_press {
                        handler(self);
                    }
                    debug_input(&format!(
                        "[button] key id={} label=\"{}\"",
                        self.id.as_u64(),
                        self.label
                    ));
                    ctx.set_handled();
                }
                _ => {}
            },
            _ => {}
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

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let label = self.label.as_str();
        let label_width = rich_rs::cell_len(label).min(width);
        let left = width.saturating_sub(label_width) / 2;
        let right = width.saturating_sub(label_width) - left;
        let line = format!(
            "{}{}{}",
            " ".repeat(left),
            rich_rs::set_cell_size(label, label_width),
            " ".repeat(right)
        );
        let mut out = Segments::new();
        out.push(Segment::new(line));
        out
    }

    fn layout_height(&self) -> Option<usize> {
        let meta = crate::css::selector_meta_generic(self);
        let base_style = crate::css::resolve_style(self, &meta);
        let default_height = 1 + super::helpers::border_vertical_padding(&base_style);
        fixed_height_from_constraints(self.layout_constraints()).or(Some(default_height))
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Button {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
