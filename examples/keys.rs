//! Key / mouse diagnostics harness.
//!
//! Python parity target for this demo is the Textual "keys" preview layout:
//! top title bar, instruction panel, scrolling log body, and bottom action bar.

use crossterm::event::{KeyCode, KeyModifiers};
use rich_rs::{Segment, Style as RichStyle};
use textual::prelude::*;

struct KeyLog {
    log: RichLog,
}

impl KeyLog {
    fn new() -> Self {
        Self {
            log: RichLog::new().max_lines(400).scroll_step(2),
        }
    }

    fn write_line(&mut self, line: impl Into<String>) {
        self.log.write(line);
    }

    fn write_key_line(&mut self, key_name: &str, character: Option<char>, is_printable: bool) {
        let key_style =
            RichStyle::new().with_color(Color::parse("#b73763").unwrap().to_simple_opaque());
        let field_style =
            RichStyle::new().with_color(Color::parse("#f5a623").unwrap().to_simple_opaque());
        let value_style =
            RichStyle::new().with_color(Color::parse("#98d168").unwrap().to_simple_opaque());
        let bool_style = RichStyle::new()
            .with_color(Color::parse("#b73763").unwrap().to_simple_opaque())
            .with_italic(true);

        let character = character
            .map(|ch| format!("'{ch}'"))
            .unwrap_or_else(|| "None".to_string());
        let printable = if is_printable { "True" } else { "False" };

        self.log.write_segments(vec![
            Segment::styled("Key".to_string(), key_style),
            Segment::new("(".to_string()),
            Segment::styled("key".to_string(), field_style),
            Segment::new("=".to_string()),
            Segment::styled(format!("'{key_name}'"), value_style),
            Segment::new(", ".to_string()),
            Segment::styled("character".to_string(), field_style),
            Segment::new("=".to_string()),
            Segment::styled(character, value_style),
            Segment::new(", ".to_string()),
            Segment::styled("name".to_string(), field_style),
            Segment::new("=".to_string()),
            Segment::styled(format!("'{key_name}'"), value_style),
            Segment::new(", ".to_string()),
            Segment::styled("is_printable".to_string(), field_style),
            Segment::new("=".to_string()),
            Segment::styled(printable.to_string(), bool_style),
            Segment::new(")".to_string()),
        ]);
    }

    fn clear(&mut self) {
        self.log.clear();
    }
}

impl Widget for KeyLog {
    fn id(&self) -> WidgetId {
        self.log.id()
    }

    fn focusable(&self) -> bool {
        self.log.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.log.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.log.has_focus()
    }

    fn style_type(&self) -> &'static str {
        "KeyLog"
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::Key(key) => {
                let key_name = key.name();
                self.write_key_line(&key_name, key.character, key.is_printable);
                if !matches!(key.modifiers, KeyModifiers::NONE) {
                    self.write_line(format!("  modifiers={:?}", key.modifiers));
                }
                if !matches!(key.kind, crossterm::event::KeyEventKind::Press) {
                    self.write_line(format!("  kind={:?}", key.kind));
                }
                if key.aliases().len() > 1 {
                    self.write_line(format!(
                        "  aliases={:?} display={:?} id={:?}",
                        key.aliases(),
                        key.display(),
                        key.identifier()
                    ));
                }
                if key.modifiers != KeyModifiers::NONE
                    || !matches!(key.kind, crossterm::event::KeyEventKind::Press)
                    || key.aliases().len() > 1
                {
                    self.write_line(String::new());
                }

                ctx.set_handled();
                ctx.request_repaint();
            }
            Event::AppFocus(focused) => {
                self.write_line(format!("AppFocus: {}", focused));
                self.write_line("");
                ctx.request_repaint();
            }
            Event::Resize(w, h) => {
                self.write_line(format!("Resize: {}x{}", w, h));
                self.write_line("");
                ctx.request_repaint();
            }
            _ => self.log.on_event(event, ctx),
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Message::ClearRequested = &message.message {
            self.clear();
            ctx.request_repaint();
            ctx.set_handled();
            return;
        }
        self.log.on_message(message, ctx);
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.log.render(console, options)
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.log.on_event_capture(event, ctx);
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.log.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.log.on_mouse_move(x, y)
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.log.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.log.styles_mut()
    }

    fn style_classes(&self) -> &[String] {
        self.log.style_classes()
    }
}

struct KeysApp {
    key_log_id: Option<WidgetId>,
}

impl KeysApp {
    fn new() -> Self {
        Self { key_log_id: None }
    }
}

impl TextualApp for KeysApp {
    fn compose(&mut self) -> AppRoot {
        let key_log = KeyLog::new();
        self.key_log_id = Some(key_log.id());
        preview_root_with_top_bottom(
            Some("Textual Keys"),
            Some(4),
            Constrained::new(Styled::new(
                Container::new()
                    .with_child(Styled::new(
                        Label::new("Press some keys!"),
                        Style::new().bold(true).underline(true),
                    ))
                    .with_child(Label::new(
                        "To quit the app press Ctrl+Q twice or press the Quit button below.",
                    )),
                Style::new()
                    .line_pad(1)
                    .border_top(Color::parse("#7f868d").unwrap())
                    .border_right(Color::parse("#7f868d").unwrap())
                    .border_bottom(Color::parse("#7f868d").unwrap())
                    .border_left(Color::parse("#7f868d").unwrap()),
            ))
            .min_height(4)
            .max_height(4),
            key_log,
            Some(3),
            Constrained::new(
                Row::new()
                    .with_child(Constrained::new(Button::warning("Clear").flat(true)))
                    .with_child(Constrained::new(Button::error("Quit").flat(true))),
            )
            .min_height(3)
            .max_height(3),
        )
    }

    fn css_path(&self) -> Option<&'static str> {
        Some("examples/keys.tcss")
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.set_quit_keys(vec![KeyBind::new(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL,
        )]);
        Ok(())
    }

    fn on_button_pressed(&mut self, description: &str, ctx: &mut EventCtx) {
        match description {
            "Clear" => {
                if let Some(key_log_id) = self.key_log_id {
                    ctx.post_message(key_log_id, Message::ClearRequested);
                    ctx.set_handled();
                }
            }
            "Quit" => {
                ctx.request_stop();
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(KeysApp::new())
}
