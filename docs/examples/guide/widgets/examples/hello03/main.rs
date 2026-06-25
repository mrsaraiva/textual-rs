/// Port of Python Textual `docs/examples/guide/widgets/hello03.py`.
///
/// Demonstrates a custom widget that subclasses `Static` in Python.  In Rust,
/// `Hello` wraps an inner `Static` and delegates rendering.  Clicking anywhere
/// on the widget cycles through a list of greetings in multiple languages,
/// updating the displayed text — exactly as in the Python original.
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
    border: $secondary tall;
    content-align: center middle;
}
"##;

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

struct Hello {
    inner: Static,
    index: usize,
}

impl Hello {
    fn new() -> Self {
        Self {
            inner: Static::new(""),
            index: 0,
        }
    }

    fn next_word(&mut self) {
        let hello = HELLOS[self.index % HELLOS.len()];
        self.inner.update(format!("{hello}, [b]World[/b]!"));
        self.index += 1;
    }
}

impl Widget for Hello {
    fn style_type(&self) -> &'static str {
        "Hello"
    }

    fn render(&self, console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    fn on_mount(&mut self) {
        self.next_word();
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::Click(_) = event {
            self.next_word();
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }
}

struct HelloApp;

impl TextualApp for HelloApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Hello::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(HelloApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_cycles_through_greetings() {
        let mut h = Hello::new();
        // Before mount, index is 0
        assert_eq!(h.index, 0);
        h.next_word();
        assert_eq!(h.index, 1);
        h.next_word();
        assert_eq!(h.index, 2);
    }

    #[test]
    fn hello_wraps_around() {
        let mut h = Hello::new();
        h.index = HELLOS.len() - 1;
        h.next_word(); // last item
        assert_eq!(h.index, HELLOS.len());
        h.next_word(); // wraps back to index 0
        assert_eq!(h.index, HELLOS.len() + 1);
    }

    #[test]
    fn hello_app_composes_without_panic() {
        let mut app = HelloApp;
        let _root = app.compose();
    }

    // -- LIVENESS PROBE (Pilot run_test) --------------------------------------
    // The whole point of hello03: clicking the Hello widget cycles to the next
    // greeting and updates the displayed text. Clicking must change the frame.
    #[test]
    fn liveness_click_cycles_greeting() {
        textual::run_test(HelloApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.click("Hello")?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "clicking the Hello widget must cycle the greeting and change \
                 the rendered frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
