/// Mirrors Python Textual's `docs/examples/widgets/rich_log.py`.
use rich_rs::{Column, Syntax, Table};
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
    let character = character
        .map(|ch| format!("'{ch}'"))
        .unwrap_or_else(|| "None".to_string());
    let printable = if is_printable { "True" } else { "False" };
    // Python: `RichLog.write(event)` wraps the Key event in `Pretty`, coloured
    // by rich's `ReprHighlighter` (ANSI-standard colours mapped to the terminal
    // theme at paint time). Mirror that path — no hardcoded colours.
    log.write_pretty(format!(
        "Key(key='{key_name}', character={character}, name='{key_name}', \
         is_printable={printable})"
    ));
}

#[derive(Clone, Default)]
struct RichLogApp;

fn build_rich_log() -> RichLog {
    let mut log = RichLog::new().highlight(true).markup(true).scroll_step(2);

    log.write_renderable(
        Syntax::new(CODE, "python")
            .with_theme("monokai")
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

    log.write("[bold magenta]Write text or any Rich renderable!");
    log
}

impl TextualApp for RichLogApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(build_rich_log())
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut textual::event::WidgetCtx) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS: pressing a key runs `on_key`, which writes a formatted
    /// `Key(...)` line into the RichLog. The new line scrolls into view, so the
    /// rendered frame must change. A dead key handler leaves the log identical.
    #[test]
    fn liveness_keypress_writes_log_line() {
        RichLogApp::default()
            .run_test(|pilot| {
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["z"])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(
                    before, after,
                    "a key press must append a log line (frame changes)"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
