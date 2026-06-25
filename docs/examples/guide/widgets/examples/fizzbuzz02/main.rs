/// Port of Python Textual `docs/examples/guide/widgets/fizzbuzz02.py`.
///
/// Demonstrates a custom `FizzBuzz(Static)` widget that renders a
/// `rich_rs::Table`. Python's `FizzBuzz` overrides `get_content_width` to
/// return 50, forcing the widget width independently of the container.
///
/// This Rust port creates a custom `FizzBuzz` widget struct that implements
/// `Widget::content_width()` returning `Some(50)` — the Rust equivalent of
/// Python's `get_content_width` hook. The CSS uses `width: auto` (faithful
/// to the Python `FizzBuzz { width: auto; }` CSS) and the widget's
/// `content_width()` drives the sizing.
///
/// Python:
/// ```python
/// class FizzBuzz(Static):
///     def on_mount(self) -> None:
///         table = Table("Number", "Fizz?", "Buzz?", expand=True)
///         for n in range(1, 16):
///             ...
///         self.update(table)
///
///     def get_content_width(self, container: Size, viewport: Size) -> int:
///         """Force content width size."""
///         return 50
/// ```
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
use rich_rs::{Console, ConsoleOptions, Renderable, Segments, Table};
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    align: center middle;
}

FizzBuzz {
    width: auto;
    height: auto;
    background: $primary;
    color: $text;
}
"##;

/// The fixed content width mirrors Python `get_content_width` returning 50.
const FIZZBUZZ_CONTENT_WIDTH: usize = 50;

/// Build the FizzBuzz table (mirrors Python `on_mount`).
fn build_table() -> Table {
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

/// Count rendered line count for the table at the given width.
fn table_line_count(table: &Table, width: usize) -> usize {
    let console = Console::new();
    let options = ConsoleOptions {
        size: (width, 40),
        max_width: width,
        max_height: 40,
        ..Default::default()
    };
    let segments: Vec<_> = Renderable::render(table, &console, &options)
        .into_iter()
        .collect();
    // Reconstruct the rendered text and count lines. `str::lines()` drops the
    // single trailing newline rich-rs emits after the last row, so this yields
    // the true rendered line count (19), matching Python's rich table — not the
    // newline-count+1 that over-counted by one.
    let mut text = String::new();
    for s in &segments {
        text.push_str(&s.text);
    }
    text.lines().count().max(1)
}

/// Mirrors Python `FizzBuzz(Static)` with `get_content_width` returning 50.
///
/// The table is built at construction time (mirrors Python `on_mount` building
/// the table and calling `self.update(table)`). `content_width()` returns
/// `Some(50)` — the Rust equivalent of Python `get_content_width`.
/// `layout_height()` returns the pre-computed line count so `height: auto`
/// resolves to the correct height for centering.
struct FizzBuzz {
    table: Table,
    line_count: usize,
    seed: NodeSeed,
}

impl FizzBuzz {
    fn new() -> Self {
        let table = build_table();
        let line_count = table_line_count(&table, FIZZBUZZ_CONTENT_WIDTH);
        Self {
            table,
            line_count,
            seed: NodeSeed::default(),
        }
    }
}

impl Widget for FizzBuzz {
    fn style_type(&self) -> &'static str {
        "FizzBuzz"
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Renderable::render(&self.table, console, options)
    }

    /// Mirrors Python `FizzBuzz.get_content_width` which returns 50.
    ///
    /// This forces the widget's content width to 50 cells regardless of the
    /// container width, so `width: auto` in CSS resolves to exactly 50.
    fn content_width(&self) -> Option<usize> {
        Some(FIZZBUZZ_CONTENT_WIDTH)
    }

    /// Return the pre-computed line count so `height: auto` resolves correctly.
    fn layout_height(&self) -> Option<usize> {
        Some(self.line_count)
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

struct FizzBuzzApp;

impl TextualApp for FizzBuzzApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(FizzBuzz::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(FizzBuzzApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fizzbuzz_widget_content_width_is_50() {
        let fb = FizzBuzz::new();
        assert_eq!(fb.content_width(), Some(50));
    }

    #[test]
    fn fizzbuzz_widget_layout_height_is_nonzero() {
        let fb = FizzBuzz::new();
        let h = fb.layout_height().unwrap_or(0);
        // The table header (3 lines) + 15 data rows + 1 bottom border = 19.
        assert!(h > 0, "layout_height should be positive, got {h}");
        assert_eq!(h, 19, "expected 19 lines: header box + header text + separator + 15 rows + bottom = 19");
    }

    #[test]
    fn fizzbuzz_app_composes_without_panic() {
        let mut app = FizzBuzzApp;
        let _root = app.compose();
    }
}
