use rich_rs::{Segment, Segments};
use textual::compose;
use textual::message::{ButtonPressed, Message};
use textual::prelude::*;
use textual::style::Color;

struct ButtonsAdvancedApp;

impl TextualApp for ButtonsAdvancedApp {
    fn compose(&mut self) -> AppRoot {
        let buttons = Horizontal::new().with_compose(compose![
            VerticalScroll::new().with_compose(compose![
                Static::new("Standard Buttons").class("header"),
                Button::new("Default"),
                Button::primary("Primary!"),
                Button::success("Success!"),
                Button::warning("Warning!"),
                Button::error("Error!"),
            ]),
            VerticalScroll::new().with_compose(compose![
                Static::new("Disabled Buttons").class("header"),
                Button::new("Default").disabled(true),
                Button::primary("Primary!").disabled(true),
                Button::success("Success!").disabled(true),
                Button::warning("Warning!").disabled(true),
                Button::error("Error!").disabled(true),
            ]),
            VerticalScroll::new().with_compose(compose![
                Static::new("Flat Buttons").class("header"),
                Button::new("Default").flat(true),
                Button::primary("Primary!").flat(true),
                Button::success("Success!").flat(true),
                Button::warning("Warning!").flat(true),
                Button::error("Error!").flat(true),
            ]),
            VerticalScroll::new().with_compose(compose![
                Static::new("Disabled Flat Buttons").class("header"),
                Button::new("Default").disabled(true).flat(true),
                Button::primary("Primary!").disabled(true).flat(true),
                Button::success("Success!").disabled(true).flat(true),
                Button::warning("Warning!").disabled(true).flat(true),
                Button::error("Error!").disabled(true).flat(true),
            ]),
        ]);

        let status = Styled::new(
            StatusLine::new(),
            Style::new()
                .line_pad(1)
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

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Message::ButtonPressed(ButtonPressed { description }) = &message.message {
            let _ = app.with_query_one_mut_as::<StatusLine, _>("StatusLine", |status| {
                status.set_text(description.clone());
            });
            ctx.request_repaint();
            ctx.set_handled();
        }
    }
}

struct StatusLine {
    text: String,
}

impl StatusLine {
    fn new() -> Self {
        Self {
            text: String::new(),
        }
    }

    fn set_text(&mut self, text: String) {
        self.text = text;
    }
}

impl Widget for StatusLine {
    fn style_type(&self) -> &'static str {
        "StatusLine"
    }

    fn render(&self, _console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let line = rich_rs::set_cell_size(&format!("Events: {}", self.text), width);
        let mut out = Segments::new();
        out.push(Segment::new(line));
        out
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync_snapshot(ButtonsAdvancedApp)
}
