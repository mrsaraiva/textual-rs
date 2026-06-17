/// Port of Python Textual `docs/examples/guide/animator/animation01.py`.
///
/// Demonstrates opacity animation: a Static widget with red background fades
/// from fully opaque to transparent over 2 seconds on mount.
use std::time::Duration;
use textual::event::{AnimationEase, StyleValue};
use textual::prelude::*;

const CSS: &str = r##"
#box {
    background: red;
    color: black;
    padding: 1 2;
}
"##;

struct AnimationApp;

impl TextualApp for AnimationApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("Hello, World!").id("box"))
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        if let Ok(node_id) = app.query_one("#box") {
            ctx.animate_style(
                node_id,
                "opacity",
                StyleValue::Float(100.0),
                StyleValue::Float(0.0),
                Duration::from_secs_f64(2.0),
                AnimationEase::InOutCubic,
            );
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(AnimationApp)
}
