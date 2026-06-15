/// Mirrors Python Textual's `docs/examples/widgets/rich_log.py`.
use rich_rs::{Column, Segment, Style as RichStyle, Syntax, Table};
use textual::keys::KeyEventData;
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
    &["6", "László Cseh", "Hungary", "51.14"],
    &["3", "Li Zhuhao", "China", "51.26"],
    &["8", "Mehdy Metella", "France", "51.58"],
    &["7", "Tom Shields", "United States", "51.73"],
    &["1", "Aleksandr Sadovnikov", "Russia", "51.84"],
];

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
struct RichLogApp;

fn build_rich_log() -> RichLog {
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
    log
}

impl TextualApp for RichLogApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(build_rich_log())
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        let key_name = key.name().to_string();
        let _ = app.with_query_one_mut_as::<RichLog, _>("RichLog", |log| {
            write_key_line(log, &key_name, key.character, key.is_printable);
        });
        ctx.request_repaint();
        ctx.set_handled();
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(RichLogApp::default())
}
