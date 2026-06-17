/// Port of Python Textual `docs/examples/guide/widgets/fizzbuzz02.py`.
///
/// Demonstrates a `Static` widget that renders a `rich_rs::Table` at mount
/// time. The Python original creates a custom `FizzBuzz(Static)` subclass and
/// builds the table in `on_mount`, calling `self.update(table)`. In Rust,
/// `Static::update_rich()` accepts a pre-rendered `rich_rs::Text`, so we
/// build and render the table at compose time using the same technique as the
/// `option_list_tables` example.
///
/// The Python `get_content_width` override returns 50 to force the widget
/// width. We mirror this with `width: 50` in the CSS `Static` rule.
///
/// CSS ported from `fizzbuzz02.tcss`:
///
/// ```css
/// Screen {
///     align: center middle;
/// }
///
/// FizzBuzz {
///     width: auto;
///     height: auto;
///     background: $primary;
///     color: $text;
/// }
/// ```
use rich_rs::{Console, ConsoleOptions, Renderable, Table, Text};
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    align: center middle;
}

Static {
    width: 50;
    height: auto;
    background: $primary;
    color: $text;
}
"##;

/// Build the FizzBuzz rich table (mirrors Python `FizzBuzz.on_mount`):
///
/// ```python
/// table = Table("Number", "Fizz?", "Buzz?", expand=True)
/// for n in range(1, 16):
///     fizz = not n % 3
///     buzz = not n % 5
///     table.add_row(str(n), "fizz" if fizz else "", "buzz" if buzz else "")
/// self.update(table)
/// ```
fn build_fizzbuzz_table() -> Table {
    let mut table = Table::new().with_expand(true);
    table.add_column_str("Number");
    table.add_column_str("Fizz?");
    table.add_column_str("Buzz?");
    for n in 1u32..=15 {
        let fizz = if n % 3 == 0 { "fizz" } else { "" };
        let buzz = if n % 5 == 0 { "buzz" } else { "" };
        table.add_row_strs(&[&n.to_string(), fizz, buzz]);
    }
    table
}

/// Render a `rich_rs::Table` to a `rich_rs::Text` at the given width.
/// This is the same technique used in `option_list_tables`.
fn table_to_text(table: &Table, width: usize) -> Text {
    let console = Console::new();
    let options = ConsoleOptions {
        size: (width, 40),
        max_width: width,
        max_height: 40,
        ..Default::default()
    };
    let segments: Vec<_> = table.render(&console, &options).into_iter().collect();

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for seg in &segments {
        if seg.is_control() || seg.text == "\n" {
            lines.push(std::mem::take(&mut current));
        } else {
            current.push_str(&seg.text);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    Text::plain(lines.join("\n"))
}

struct FizzBuzzApp;

impl TextualApp for FizzBuzzApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let table = build_fizzbuzz_table();
        // Render at width=50 to match the forced content width in the Python
        // `get_content_width` override.
        let text = table_to_text(&table, 50);
        let mut widget = Static::new("");
        widget.update_rich(text);
        AppRoot::new().with_child(widget)
    }
}

fn main() -> textual::Result<()> {
    run_sync(FizzBuzzApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fizzbuzz_table_has_15_rows() {
        let table = build_fizzbuzz_table();
        let text = table_to_text(&table, 50);
        let plain = text.plain_text();
        // rows 1-15 should appear
        assert!(plain.contains('1'), "missing row 1");
        assert!(plain.contains("15"), "missing row 15");
    }

    #[test]
    fn fizzbuzz_table_has_fizz_and_buzz() {
        let table = build_fizzbuzz_table();
        let text = table_to_text(&table, 50);
        let plain = text.plain_text();
        assert!(plain.contains("fizz"), "missing fizz");
        assert!(plain.contains("buzz"), "missing buzz");
    }

    #[test]
    fn fizzbuzz_table_column_headers() {
        let table = build_fizzbuzz_table();
        let text = table_to_text(&table, 50);
        let plain = text.plain_text();
        assert!(plain.contains("Number"), "missing Number header");
        assert!(plain.contains("Fizz?"), "missing Fizz? header");
        assert!(plain.contains("Buzz?"), "missing Buzz? header");
    }

    #[test]
    fn fizzbuzz_app_composes_without_panic() {
        let mut app = FizzBuzzApp;
        let _root = app.compose();
    }
}
