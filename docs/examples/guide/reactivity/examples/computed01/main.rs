/// Port of Python Textual `docs/examples/guide/reactivity/computed01.py`.
///
/// Demonstrates a "computed reactive" pattern: three Input widgets accept
/// red/green/blue channel values (0-255); a colour swatch below updates its
/// background to the resulting RGB colour on every keystroke.
///
/// Python uses `reactive` + `compute_color` + `watch_color`.  Rust does not
/// yet have first-class computed reactives, so the equivalent is performed
/// imperatively inside `on_message_with_app`: parse the new value, identify
/// which channel changed by matching the sender NodeId, recompute the colour,
/// and update the `#color` widget's inline background style.
///
/// Framework gaps noted at the bottom of this file.
use textual::message::InputChanged;
use textual::prelude::*;

const CSS: &str = r#"
#color-inputs {
    dock: top;
    height: auto;
}

Input {
    width: 1fr;
}

#color {
    height: 100%;
    border: tall $secondary;
}
"#;

struct ComputedApp {
    red: u8,
    green: u8,
    blue: u8,
}

impl ComputedApp {
    fn new() -> Self {
        Self {
            red: 0,
            green: 0,
            blue: 0,
        }
    }

    fn current_color(&self) -> Color {
        Color::rgb(self.red, self.green, self.blue)
    }
}

impl TextualApp for ComputedApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Node::new(
                    Horizontal::new()
                        .with_child(
                            Input::new()
                                .with_placeholder("Enter red 0-255")
                                .id("red"),
                        )
                        .with_child(
                            Input::new()
                                .with_placeholder("Enter green 0-255")
                                .id("green"),
                        )
                        .with_child(
                            Input::new()
                                .with_placeholder("Enter blue 0-255")
                                .id("blue"),
                        ),
                )
                .id("color-inputs"),
            )
            .with_child(Static::new("").id("color"))
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Some(m) = message.downcast_ref::<InputChanged>() {
            let sender = message.sender;

            // Parse the new numeric value; ignore non-integer input (like Python's bell).
            let component: Option<u8> = m.value.trim().parse::<u16>().ok().map(|v| v.min(255) as u8);

            let Some(component) = component else {
                // Non-integer input: Python calls self.bell(); we silently ignore.
                return;
            };

            // Identify which channel changed by comparing sender to the known input NodeIds.
            let red_id = app.get_widget_by_id("red").ok();
            let green_id = app.get_widget_by_id("green").ok();
            let blue_id = app.get_widget_by_id("blue").ok();

            if red_id == Some(sender) {
                self.red = component;
            } else if green_id == Some(sender) {
                self.green = component;
            } else if blue_id == Some(sender) {
                self.blue = component;
            }

            // Recompute and apply the colour to the #color widget's background.
            let color = self.current_color();
            let _ = app.query_mut("#color").map(|q| q.set_styles(|s| s.set_bg(color)));
            ctx.request_repaint();
            ctx.set_handled();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(ComputedApp::new())
}

// ---------------------------------------------------------------------------
// Framework gaps
// ---------------------------------------------------------------------------
//
// 1. COMPUTED REACTIVES: Python uses `compute_color()` as a derived reactive
//    that auto-updates `color` whenever `red`/`green`/`blue` change.  textual-rs
//    has no first-class computed-reactive mechanism.  This port uses an
//    imperative `on_message_with_app` handler as a faithful equivalent.
//
// 2. BELL: Python calls `self.bell()` on invalid (non-integer) input.
//    textual-rs has no `App::bell()` API.  Invalid input is silently ignored.
//
// 3. INITIAL VALUE: Python `Input("0", ...)` sets the initial text to "0".
//    `Input::new()` starts empty.  There is no `Input::with_value()` builder
//    in textual-rs yet, so the initial channel values remain 0 but the
//    input boxes are visually empty.
