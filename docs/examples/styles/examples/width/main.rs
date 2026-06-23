/// Port of Python Textual `docs/examples/styles/width.py`.
///
/// Demonstrates the `width` CSS property: a single widget taking 50% of the
/// screen width with a green background and white text.
///
/// Note: Python uses a bare `Widget()` (the base class directly), which renders
/// the literal text "Widget" top-left (no content-align centering), fills the
/// remaining height (an unset `height` resolves to `1fr` for the bare Widget),
/// and gets its green background + white foreground from the `Screen > Widget`
/// rule. In textual-rs the bare-widget equivalent is `Label::new("Widget")` — it
/// renders text top-left with no centering. Label's default `height: auto` only
/// sizes to its single line, so the CSS adds `height: 1fr` to reproduce Python's
/// fill-the-screen default; the selector targets `Label` (textual-rs's
/// base-class match, since `Widget` is a trait, not a concrete widget type).
use textual::prelude::*;

const CSS: &str = r##"
Screen > Label {
    background: green;
    width: 50%;
    height: 1fr;
    color: white;
}
"##;

struct WidthApp;

impl TextualApp for WidthApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Label::new("Widget"))
    }
}

fn main() -> Result<()> {
    run_sync(WidthApp)
}
