/// Port of Python Textual `docs/examples/guide/layout/layers.py`.
///
/// Demonstrates layered layout: two Static widgets assigned to different
/// layers so they can visually overlap. box1 is on the "above" layer and
/// box2 is on the "below" layer, offset so they partially overlap.
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    align: center middle;
    layers: below above;
}

Static {
    width: 28;
    height: 8;
    color: auto;
    content-align: center middle;
}

#box1 {
    layer: above;
    background: darkcyan;
}

#box2 {
    layer: below;
    background: orange;
    offset: 12 6;
}
"##;

struct LayersExample;

impl TextualApp for LayersExample {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Static::new("box1 (layer = above)").id("box1"))
            .with_child(Static::new("box2 (layer = below)").id("box2"))
    }
}

fn main() -> Result<()> {
    run_sync(LayersExample)
}
