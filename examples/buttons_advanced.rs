use std::sync::{Arc, Mutex};

use rich_rs::{Segment, Segments};
use textual::prelude::*;
use textual::style::{Color, parse_color_like};

struct ButtonsAdvancedApp {
    status: Arc<Mutex<String>>,
}

impl ButtonsAdvancedApp {
    fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(String::new())),
        }
    }
}

impl TextualApp for ButtonsAdvancedApp {
    fn compose(&mut self) -> AppRoot {
        let buttons = Horizontal::new()
            .with_child(
                VerticalScroll::new()
                    .with_child(Static::new("Standard Buttons").class("header"))
                    .with_child(Button::new("Default"))
                    .with_child(Button::primary("Primary!"))
                    .with_child(Button::success("Success!"))
                    .with_child(Button::warning("Warning!"))
                    .with_child(Button::error("Error!")),
            )
            .with_child(
                VerticalScroll::new()
                    .with_child(Static::new("Disabled Buttons").class("header"))
                    .with_child(Button::new("Default").disabled(true))
                    .with_child(Button::primary("Primary!").disabled(true))
                    .with_child(Button::success("Success!").disabled(true))
                    .with_child(Button::warning("Warning!").disabled(true))
                    .with_child(Button::error("Error!").disabled(true)),
            )
            .with_child(
                VerticalScroll::new()
                    .with_child(Static::new("Flat Buttons").class("header"))
                    .with_child(Button::new("Default").flat(true))
                    .with_child(Button::primary("Primary!").flat(true))
                    .with_child(Button::success("Success!").flat(true))
                    .with_child(Button::warning("Warning!").flat(true))
                    .with_child(Button::error("Error!").flat(true)),
            )
            .with_child(
                VerticalScroll::new()
                    .with_child(Static::new("Disabled Flat Buttons").class("header"))
                    .with_child(Button::new("Default").disabled(true).flat(true))
                    .with_child(Button::primary("Primary!").disabled(true).flat(true))
                    .with_child(Button::success("Success!").disabled(true).flat(true))
                    .with_child(Button::warning("Warning!").disabled(true).flat(true))
                    .with_child(Button::error("Error!").disabled(true).flat(true)),
            );

        let status_bg = parse_color_like("$panel").or_else(|| parse_color_like("$surface"));
        let status = Styled::new(
            StatusLine::new(self.status.clone()),
            Style::new()
                .line_pad(1)
                .bg(status_bg.unwrap_or(Color::parse("#303a43").unwrap()))
                .border_top(Color::parse("#44cc44").unwrap())
                .border_right(Color::parse("#44cc44").unwrap())
                .border_bottom(Color::parse("#44cc44").unwrap())
                .border_left(Color::parse("#44cc44").unwrap()),
        );
        AppRoot::new().with_child(
            Dock::new()
                .push_fill(ScrollView::new(buttons).scroll_step(2))
                .push_bottom(Some(3), status),
        )
    }

    fn css_path(&self) -> Option<&'static str> {
        Some("examples/button.tcss")
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Message::ButtonPressed { description } = &message.message {
            *self.status.lock().unwrap_or_else(|e| e.into_inner()) = description.clone();
            ctx.request_repaint();
            ctx.set_handled();
        }
    }
}

struct StatusLine {
    id: WidgetId,
    text: Arc<Mutex<String>>,
}

impl StatusLine {
    fn new(text: Arc<Mutex<String>>) -> Self {
        Self {
            id: WidgetId::new(),
            text,
        }
    }
}

impl Widget for StatusLine {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, _console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let text = self.text.lock().unwrap_or_else(|e| e.into_inner());
        let line = rich_rs::set_cell_size(&format!("Events: {text}"), width);
        let mut out = Segments::new();
        out.push(Segment::new(line));
        out
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync_snapshot(ButtonsAdvancedApp::new())
}
