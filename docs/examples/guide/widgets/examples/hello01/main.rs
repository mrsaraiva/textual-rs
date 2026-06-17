/// Port of Python Textual `docs/examples/guide/widgets/hello01.py`.
///
/// Demonstrates the simplest possible custom widget: a `Hello` struct that
/// implements `Widget::render()` and returns rich-markup text.
///
/// Python source:
/// ```python
/// class Hello(Widget):
///     def render(self) -> RenderResult:
///         return "Hello, [b]World[/b]!"
/// ```
use rich_rs::{Console, ConsoleOptions, Renderable, Segments};
use textual::prelude::*;

struct Hello;

impl Widget for Hello {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        // Render rich-markup string: [b]...[/b] produces bold text,
        // matching Python Textual's default markup=True render result.
        let rendered = console.render_str("Hello, [b]World[/b]!", Some(true), None, None, None);
        rendered.render(console, options)
    }
}

struct CustomApp;

impl TextualApp for CustomApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Hello)
    }
}

fn main() -> Result<()> {
    run_sync(CustomApp)
}
