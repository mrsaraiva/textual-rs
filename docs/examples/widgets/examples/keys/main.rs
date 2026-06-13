//! Key / mouse diagnostics harness.
//!
//! Python parity target: Textual "keys" preview layout with title bar,
//! instruction panel, scrolling log body, and bottom action bar.

use crossterm::event::{KeyCode, KeyModifiers};
use rich_rs::{Segment, Style as RichStyle};
use textual::keys::KeyEventData;
use textual::prelude::*;

#[derive(Debug, Clone)]
struct ClearKeyLogMessage;

textual::impl_message!(ClearKeyLogMessage);

struct KeyLog {
    log: RichLog,
}

impl KeyLog {
    fn new() -> Self {
        Self {
            log: RichLog::new().max_lines(400).scroll_step(2),
        }
    }
}

impl Widget for KeyLog {
    fn style_type(&self) -> &'static str {
        "KeyLog"
    }

    fn focusable(&self) -> bool {
        self.log.focusable()
    }

    fn on_node_state_changed(&mut self, old: NodeState, new: NodeState) {
        self.log.on_node_state_changed(old, new);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.log.on_event(event, ctx);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.log.on_event_capture(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if message.downcast_ref::<ClearKeyLogMessage>().is_some() {
            self.log.clear();
            ctx.request_repaint();
            ctx.set_handled();
            return;
        }
        self.log.on_message(message, ctx);
    }

    fn on_mouse_scroll(&mut self, dx: i32, dy: i32, ctx: &mut EventCtx) {
        self.log.on_mouse_scroll(dx, dy, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.log.on_mouse_move(x, y)
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.log.render(console, options)
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

#[derive(Clone, Default)]
struct KeysApp;

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
            KeyLog::new(),
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
        Some(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/keys/keys.tcss"
        ))
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.set_quit_keys(vec![KeyBind::new(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL,
        )]);
        Ok(())
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        let key_name = key.name();
        let _ = app.with_query_one_mut_as::<KeyLog, _>("KeyLog", |key_log| {
            let log = &mut key_log.log;
            write_key_line(log, key_name, key.character, key.is_printable);
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
        });
        ctx.set_handled();
        ctx.request_repaint();
    }

    fn on_button_pressed(&mut self, description: &str, ctx: &mut EventCtx) {
        match description {
            "Clear" => {
                ctx.post_message(ClearKeyLogMessage);
                ctx.set_handled();
                ctx.request_repaint();
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
