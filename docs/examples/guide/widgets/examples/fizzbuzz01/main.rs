/// Port of Python Textual `docs/examples/guide/widgets/fizzbuzz01.py`.
///
/// Demonstrates a custom widget that renders a `rich_rs::Table` — mirroring
/// the Python example's `FizzBuzz(Static)` widget that builds a `rich.table.Table`
/// in `on_mount` and calls `self.update(table)`.
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

/// Mirrors Python's `FizzBuzz(Static)` widget.
///
/// Builds a `rich_rs::Table` with columns "Number", "Fizz?", "Buzz?" and
/// rows for numbers 1–15, then renders it directly.
struct FizzBuzz {
    table: Table,
    seed: NodeSeed,
}

impl FizzBuzz {
    fn new() -> Self {
        let mut table = Table::new();
        table.add_column_str("Number");
        table.add_column_str("Fizz?");
        table.add_column_str("Buzz?");
        for n in 1u32..=15 {
            let fizz = n % 3 == 0;
            let buzz = n % 5 == 0;
            table.add_row_strs(&[
                &n.to_string(),
                if fizz { "fizz" } else { "" },
                if buzz { "buzz" } else { "" },
            ]);
        }
        Self {
            table,
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

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

struct FizzBuzzApp;

impl TextualApp for FizzBuzzApp {
    fn title(&self) -> &'static str {
        "FizzBuzzApp"
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(FizzBuzz::new())
    }
}

fn main() -> Result<()> {
    run_sync(FizzBuzzApp)
}
