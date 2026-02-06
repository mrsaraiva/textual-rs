//! Key / mouse diagnostics harness.
//!
//! Python parity target for this demo is the Textual "keys" preview layout:
//! top title bar, instruction panel, scrolling log body, and bottom action bar.

use std::path::Path;

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
        }
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

struct ActionBar {
    id: WidgetId,
    clear_id: WidgetId,
    quit_id: WidgetId,
    child: Box<dyn Widget>,
}

impl ActionBar {
    fn new() -> Self {
        let clear = Button::warning("Clear").flat(true);
        let clear_id = clear.id();
        let quit = Button::error("Quit").flat(true);
        let quit_id = quit.id();
        let row = Row::new()
            .with_child(Constrained::new(clear))
            .with_child(Constrained::new(quit));
        Self {
            id: WidgetId::new(),
            clear_id,
            quit_id,
            child: Box::new(Constrained::new(row).min_height(3).max_height(3)),
        }
    }
}

impl Widget for ActionBar {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.child.render_styled(console, options)
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.child.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Message::ButtonPressed { .. } = &message.message {
            if message.sender == self.clear_id {
                ctx.post_message(self.id, Message::ClearRequested);
                ctx.set_handled();
                return;
            }
            if message.sender == self.quit_id {
                ctx.request_stop();
                ctx.set_handled();
                return;
            }
        }
        self.child.on_message(message, ctx);
    }

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(self.child.as_mut());
    }
}

fn help_panel() -> impl Widget {
    let title = Styled::new(
        Label::new("Press some keys!"),
        Style::new().bold(true).underline(true),
    );
    let content = Container::new().with_child(title).with_child(Label::new(
        "To quit the app press Ctrl+Q twice or press the Quit button below.",
    ));
    let boxed = Styled::new(
        content,
        Style::new()
            .line_pad(1)
            .border_top(Color::parse("#7f868d").unwrap())
            .border_right(Color::parse("#7f868d").unwrap())
            .border_bottom(Color::parse("#7f868d").unwrap())
            .border_left(Color::parse("#7f868d").unwrap()),
    );
    Constrained::new(boxed).min_height(4).max_height(4)
}

fn build_keys_widget() -> AppRoot {
    let body = Dock::new()
        .push_top(Some(4), help_panel())
        .push_fill(KeyLog::new())
        .push_bottom(Some(3), ActionBar::new());

    let layout = Dock::new()
        .push_top(None, Header::new().title("Textual Keys"))
        .push_fill(body);
    AppRoot::new().with_child(layout)
}

#[tokio::main]
async fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }

    let mut app = App::new()?;
    app.set_quit_keys(vec![KeyBind::new(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL,
    )]);
    if Path::new("examples/keys.tcss").exists() {
        app.watch_stylesheet("examples/keys.tcss", std::time::Duration::from_millis(500))?;
    }
    let mut root = build_keys_widget();
    app.run_widget_tree(&mut root).await
}
