/// Port of Python Textual `docs/examples/styles/height.py`.
///
/// Demonstrates the `height: 50%` CSS property on a generic widget.
/// Python uses a bare `Widget()` (the base class directly), which renders the
/// literal text "Widget" top-left (no content-align centering), fills the full
/// width (an unset `width` resolves to `1fr` for the bare Widget), and gets its
/// green background + white foreground from the `Screen > Widget` rule. In
/// textual-rs the bare-widget equivalent is `Label::new("Widget")` — it renders
/// text top-left with no centering. Label's default `width: auto` only sizes to
/// its single line, so the CSS adds `width: 1fr` to reproduce Python's full-width
/// default; the selector targets `Label` (textual-rs's base-class match, since
/// `Widget` is a trait, not a concrete widget type).
use textual::prelude::*;

const CSS: &str = r##"
Screen > Label {
    background: green;
    width: 1fr;
    height: 50%;
    color: white;
}
"##;

struct HeightApp;

impl TextualApp for HeightApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Label::new("Widget"))
    }
}

fn main() -> Result<()> {
    run_sync(HeightApp)
}
