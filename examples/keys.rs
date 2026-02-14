//! Key / mouse diagnostics harness.
//!
//! Python parity target: Textual "keys" preview layout with title bar,
//! instruction panel, scrolling log body, and bottom action bar.

use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyModifiers};
use rich_rs::{Segment, Style as RichStyle};
use textual::keys::KeyEventData;
use textual::prelude::*;

struct SharedKeyLog {
    log: Arc<Mutex<RichLog>>,
}

impl SharedKeyLog {
    fn new(log: Arc<Mutex<RichLog>>) -> Self {
        Self { log }
    }
}

impl Widget for SharedKeyLog {
    fn style_type(&self) -> &'static str {
        "KeyLog"
    }

    fn focusable(&self) -> bool {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .has_focus()
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_event_capture(event, ctx);
    }

    fn on_mouse_scroll(&mut self, dx: i32, dy: i32, ctx: &mut EventCtx) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_mouse_scroll(dx, dy, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_mouse_move(x, y)
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .render(console, options)
    }
}

fn write_key_line(log: &mut RichLog, key_name: &str, character: Option<char>, is_printable: bool) {
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

    log.write_segments(vec![
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

#[derive(Clone)]
struct KeysApp {
    log: Arc<Mutex<RichLog>>,
}

impl Default for KeysApp {
    fn default() -> Self {
        Self {
            log: Arc::new(Mutex::new(RichLog::new().max_lines(400).scroll_step(2))),
        }
    }
}

impl TextualApp for KeysApp {
    fn compose(&mut self) -> AppRoot {
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
            SharedKeyLog::new(self.log.clone()),
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

    fn on_key(&mut self, key: &KeyEventData, ctx: &mut EventCtx) {
        let mut log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        let key_name = key.name();
        write_key_line(&mut log, key_name, key.character, key.is_printable);
        if !matches!(key.modifiers, KeyModifiers::NONE) {
            log.write(format!("  modifiers={:?}", key.modifiers));
        }
        if !matches!(key.kind, crossterm::event::KeyEventKind::Press) {
            log.write(format!("  kind={:?}", key.kind));
        }
        if key.aliases().len() > 1 {
            log.write(format!(
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
            log.write(String::new());
        }
        ctx.set_handled();
        ctx.request_repaint();
    }

    fn on_button_pressed(&mut self, description: &str, ctx: &mut EventCtx) {
        match description {
            "Clear" => {
                self.log.lock().unwrap_or_else(|e| e.into_inner()).clear();
                ctx.request_repaint();
                ctx.set_handled();
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
    run_sync(KeysApp::default())
}
