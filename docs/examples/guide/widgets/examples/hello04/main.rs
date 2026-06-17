/// Port of Python Textual `docs/examples/guide/widgets/hello04.py`.
///
/// Displays a `Hello` widget (custom `Static` subclass) that cycles through
/// multilingual greetings on each click. The screen is centered via CSS.
///
/// Python uses a `cycle()` iterator; Rust tracks an index into a `&[&str]`
/// slice instead.
use textual::prelude::*;

// ---------------------------------------------------------------------------
// CSS — mirrors Hello.DEFAULT_CSS + hello04.tcss
// ---------------------------------------------------------------------------

const CSS: &str = r##"
Screen {
    align: center middle;
}

Hello {
    width: 40;
    height: 9;
    padding: 1 2;
    background: $panel;
    border: tall $secondary;
    content-align: center middle;
}
"##;

// ---------------------------------------------------------------------------
// Greeting list — mirrors Python's `hellos` cycle.
// ---------------------------------------------------------------------------

const HELLOS: &[&str] = &[
    "Hola",
    "Bonjour",
    "Guten tag",
    "Salve",
    "Nǐn hǎo",
    "Olá",
    "Asalaam alaikum",
    "Konnichiwa",
    "Anyoung haseyo",
    "Zdravstvuyte",
    "Hello",
];

// ---------------------------------------------------------------------------
// Hello widget — custom Static subclass
// ---------------------------------------------------------------------------

struct Hello {
    inner: Static,
    /// Index of the *next* greeting to show (wraps around).
    next_index: usize,
}

impl Hello {
    fn new() -> Self {
        Self {
            inner: Static::new(""),
            next_index: 0,
        }
    }

    /// Advance to the next greeting and update the displayed content.
    fn next_word(&mut self) {
        let hello = HELLOS[self.next_index % HELLOS.len()];
        self.next_index = self.next_index.wrapping_add(1);
        self.inner.update(format!("{hello}, [b]World[/b]!"));
    }
}

impl Widget for Hello {
    fn style_type(&self) -> &'static str {
        "Hello"
    }

    /// Inherit Static's default CSS rules (e.g. transparent background behaviour).
    fn style_type_aliases(&self) -> &[&'static str] {
        &["Static"]
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    fn auto_content_width(&self) -> Option<usize> {
        self.inner.auto_content_width()
    }

    fn border_title(&self) -> Option<&str> {
        self.inner.border_title()
    }

    fn border_subtitle(&self) -> Option<&str> {
        self.inner.border_subtitle()
    }

    fn set_inline_style(&mut self, style: Style) {
        self.inner.set_inline_style(style);
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    /// Python `on_mount` — show the first greeting immediately after mount.
    fn on_mount(&mut self) {
        self.next_word();
    }

    /// Python `on_click` — advance to the next greeting on each click.
    ///
    /// Python Textual fires `on_click` on `MouseUp` for the target widget,
    /// so we mirror that here.
    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::MouseUp(mouse) = event {
            if mouse.target == Some(self.node_id()) {
                self.next_word();
                ctx.request_repaint();
                ctx.set_handled();
            }
        }
        self.inner.on_event(event, ctx);
    }

    /// Enable hit-testing so mouse events are delivered to this widget.
    fn mouse_interactive(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct CustomApp;

impl TextualApp for CustomApp {
    fn configure(&mut self, app: &mut App) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello04_composes_without_panic() {
        let mut app = CustomApp;
        let _root = app.compose();
    }

    #[test]
    fn hello_cycles_greetings() {
        let mut widget = Hello::new();
        // Before any call next_index is 0.
        assert_eq!(widget.next_index, 0);
        widget.next_word();
        assert_eq!(widget.next_index, 1);
        widget.next_word();
        assert_eq!(widget.next_index, 2);
    }

    #[test]
    fn hello_wraps_around() {
        let mut widget = Hello::new();
        widget.next_index = HELLOS.len() - 1;
        widget.next_word();
        // Should wrap: next_index == HELLOS.len() (% len == 0 next call)
        assert_eq!(widget.next_index, HELLOS.len());
        widget.next_word();
        assert_eq!(widget.next_index, HELLOS.len() + 1);
    }
}
