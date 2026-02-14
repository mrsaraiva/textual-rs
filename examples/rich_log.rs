/// Mirrors Python Textual's `docs/examples/widgets/rich_log.py`.
///
/// Uses a thin wrapper around `RichLog` to intercept key events (the framework
/// does not yet expose an app-level `on_key` hook, so widget-level interception
/// is the minimum viable approach).
use rich_rs::{Column, Segment, Style as RichStyle, Syntax, Table};
use textual::prelude::*;

const CODE: &str = r#"def loop_first_last(values: Iterable[T]) -> Iterable[tuple[bool, bool, T]]:
    """Iterate and generate a tuple with a flag for first and last value."""
    iter_values = iter(values)
    try:
        previous_value = next(iter_values)
    except StopIteration:
        return
    first = True
    for value in iter_values:
        yield first, False, previous_value
        first = False
        previous_value = value
    yield first, True, previous_value"#;

const SWIM_ROWS: &[&[&str]] = &[
    &["4", "Joseph Schooling", "Singapore", "50.39"],
    &["2", "Michael Phelps", "United States", "51.14"],
    &["5", "Chad le Clos", "South Africa", "51.14"],
    &["6", "Laszlo Cseh", "Hungary", "51.14"],
    &["3", "Li Zhuhao", "China", "51.26"],
    &["8", "Mehdy Metella", "France", "51.58"],
    &["7", "Tom Shields", "United States", "51.73"],
    &["1", "Aleksandr Sadovnikov", "Russia", "51.84"],
];

/// Thin wrapper that adds key-event logging to a `RichLog`.
struct KeyLoggingRichLog {
    log: RichLog,
}

impl KeyLoggingRichLog {
    fn new() -> Self {
        let mut log = RichLog::new().highlight(true).markup(true).scroll_step(2);

        log.write_renderable(
            Syntax::new(CODE, "python")
                .with_theme("ansi_dark")
                .with_indent_guides(true),
        );

        let mut table = Table::new();
        table.add_column(Column::with_header_str("lane"));
        table.add_column(Column::with_header_str("swimmer"));
        table.add_column(Column::with_header_str("country"));
        table.add_column(Column::with_header_str("time"));
        for row in SWIM_ROWS {
            table.add_row_strs(row);
        }
        log.write_renderable(table);

        log.write("[bold red]Write text or any Rich renderable!");

        Self { log }
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
}

impl Widget for KeyLoggingRichLog {
    fn style_type(&self) -> &'static str {
        "RichLog"
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
    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::Key(key) = event {
            self.write_key_line(key.name(), key.character, key.is_printable);
            ctx.request_repaint();
        }
        self.log.on_event(event, ctx);
    }
    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.log.on_event_capture(event, ctx);
    }
    fn on_mouse_scroll(&mut self, dx: i32, dy: i32, ctx: &mut EventCtx) {
        self.log.on_mouse_scroll(dx, dy, ctx);
    }
    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.log.on_mouse_move(x, y)
    }
    fn render(&self, console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> rich_rs::Segments {
        self.log.render(console, options)
    }
    fn styles(&self) -> Option<&WidgetStyles> {
        self.log.styles()
    }
    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.log.styles_mut()
    }
}

struct RichLogApp;

impl TextualApp for RichLogApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(KeyLoggingRichLog::new())
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(RichLogApp)
}
