/// Port of Python Textual `docs/examples/guide/widgets/hello02.py`.
///
/// Demonstrates a custom widget with a `render()` method that returns markup text.
/// The `Hello` widget renders "Hello, **World**!" centered in a styled box,
/// matching the Python example's layout and CSS styling.
use rich_rs::{Console, ConsoleOptions, Renderable, Segments};
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    align: center middle;
}

Hello {
    width: 40;
    height: 9;
    padding: 1 2;
    background: $panel;
    color: $text;
    border: $secondary tall;
    content-align: center middle;
}
"##;

/// A custom widget that displays a greeting with rich markup.
///
/// Mirrors Python `Hello(Widget)` with `render() -> "Hello, [b]World[/b]!"`.
struct Hello {
    seed: NodeSeed,
}

impl Hello {
    fn new() -> Self {
        Self {
            seed: NodeSeed::default(),
        }
    }
}

impl Widget for Hello {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let rendered = console.render_str("Hello, [b]World[/b]!", Some(true), None, None, None);
        rendered.render(console, options)
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

struct CustomApp;

impl TextualApp for CustomApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Hello::new())
    }
}

fn main() -> Result<()> {
    run_sync(CustomApp)
}
