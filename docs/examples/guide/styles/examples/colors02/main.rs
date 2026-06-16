/// Port of Python Textual `docs/examples/guide/styles/colors02.py`.
///
/// Demonstrates background color with varying alpha levels using Color(191, 78, 96, a=N).
/// Python sets these via `on_mount` with dynamic style mutation (widget.styles.background).
/// Ported using CSS with equivalent rgba() color values (framework gap: no runtime
/// inline-style mutation API exposed to TextualApp).
///
/// Framework gap: on_mount dynamic style mutation not supported; styles baked into CSS.
use textual::prelude::*;

const CSS: &str = r##"
#w1  { background: rgba(191, 78, 96, 0.1); }
#w2  { background: rgba(191, 78, 96, 0.2); }
#w3  { background: rgba(191, 78, 96, 0.3); }
#w4  { background: rgba(191, 78, 96, 0.4); }
#w5  { background: rgba(191, 78, 96, 0.5); }
#w6  { background: rgba(191, 78, 96, 0.6); }
#w7  { background: rgba(191, 78, 96, 0.7); }
#w8  { background: rgba(191, 78, 96, 0.8); }
#w9  { background: rgba(191, 78, 96, 0.9); }
#w10 { background: rgba(191, 78, 96, 1.0); }
"##;

struct ColorApp;

impl TextualApp for ColorApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new("alpha=0.1").id("w1"))
            .with_child(Static::new("alpha=0.2").id("w2"))
            .with_child(Static::new("alpha=0.3").id("w3"))
            .with_child(Static::new("alpha=0.4").id("w4"))
            .with_child(Static::new("alpha=0.5").id("w5"))
            .with_child(Static::new("alpha=0.6").id("w6"))
            .with_child(Static::new("alpha=0.7").id("w7"))
            .with_child(Static::new("alpha=0.8").id("w8"))
            .with_child(Static::new("alpha=0.9").id("w9"))
            .with_child(Static::new("alpha=1.0").id("w10"))
    }
}

fn main() -> Result<()> {
    run_sync(ColorApp)
}
