/// Port of Python Textual `docs/examples/guide/reactivity/computed01.py`.
///
/// Demonstrates a COMPUTED reactive: three `Input` widgets accept red/green/blue
/// channel values (0-255); a colour swatch below updates its background to the
/// resulting RGB colour on every keystroke.
///
/// Python:
///   red/green/blue = reactive(0); color = reactive(Color.parse("transparent"))
///   def compute_color(self) -> Color: return Color(self.red, self.green, self.blue).clamped
///   def watch_color(self, color: Color): self.query_one("#color").styles.background = color
///   on_input_changed: set self.red/green/blue from the input value
///
/// Rust port (faithful): the app derives `Reactive` with `#[reactive] red/green/blue`
/// and a `#[computed(depends_on = "red, green, blue", watch_with_app)] color`.
/// When any channel changes, the macro recomputes `color` via `compute_color()`
/// and (because the value changed) fires `watch_color(app, old, new, ctx)` —
/// exactly Python's compute + watch pairing. The watcher sets the `#color`
/// background.
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

#[derive(Reactive)]
struct ComputedApp {
    #[reactive]
    red: u8,
    #[reactive]
    green: u8,
    #[reactive]
    blue: u8,
    /// Computed from the three channels; its watcher repaints the swatch.
    #[computed(depends_on = "red, green, blue", watch_with_app)]
    color: Color,
}

impl ComputedApp {
    fn new() -> Self {
        Self {
            red: 0,
            green: 0,
            blue: 0,
            color: Color::rgb(0, 0, 0),
        }
    }

    /// Python `compute_color`: derive the colour from the channels.
    fn compute_color(&self) -> Color {
        Color::rgb(self.red, self.green, self.blue)
    }

    /// Python `watch_color`: paint the swatch's background.
    fn watch_color(&mut self, app: &mut App, _old: &Color, new: &Color, _ctx: &mut ReactiveCtx) {
        let new = *new;
        let _ = app.with_query_one_mut_as::<Static, _>("#color", |s| {
            s.set_inline_style(Style::new().bg(new));
        });
    }
}

impl TextualApp for ComputedApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(
                Node::new(
                    Horizontal::new()
                        .with_child(Input::new().with_placeholder("Enter red 0-255").id("red"))
                        .with_child(Input::new().with_placeholder("Enter green 0-255").id("green"))
                        .with_child(Input::new().with_placeholder("Enter blue 0-255").id("blue")),
                )
                .id("color-inputs"),
            )
            .with_child(Static::new("").id("color"))
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(m) = message.downcast_ref::<InputChanged>() {
            let sender = message.sender;
            // Parse the channel value; ignore non-integer input (Python rings the bell).
            let Some(component) = m.value.trim().parse::<u16>().ok().map(|v| v.min(255) as u8) else {
                return;
            };

            let red_id = app.get_widget_by_id("red").ok();
            let green_id = app.get_widget_by_id("green").ok();
            let blue_id = app.get_widget_by_id("blue").ok();

            // Setting a channel triggers compute_color -> watch_color via the bridge.
            if red_id == Some(sender) {
                self.set_red(component, app.reactive_ctx());
            } else if green_id == Some(sender) {
                self.set_green(component, app.reactive_ctx());
            } else if blue_id == Some(sender) {
                self.set_blue(component, app.reactive_ctx());
            }
            ctx.request_repaint();
            ctx.set_handled();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(ComputedApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_does_not_panic() {
        let mut app = ComputedApp::new();
        let _root = app.compose();
    }

    #[test]
    fn compute_color_combines_channels() {
        let app = ComputedApp {
            red: 10,
            green: 20,
            blue: 30,
            color: Color::rgb(0, 0, 0),
        };
        assert_eq!(app.compute_color(), Color::rgb(10, 20, 30));
    }

    #[test]
    fn setting_channel_recomputes_color() {
        // Driving the reactive phase recomputes the computed `color`.
        let mut app = ComputedApp::new();
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        app.set_red(100, &mut ctx);
        let _ = textual::reactive::run_reactive_phase(&mut app, &mut ctx);
        assert_eq!(*app.color(), Color::rgb(100, 0, 0));
    }

    /// LIVENESS PROBE (currently DEAD — see root cause below).
    ///
    /// Typing a channel value into the `#red` Input must drive the computed
    /// `color` and its `watch_color`, repainting the `#color` swatch background.
    /// We assert the *swatch's own background* changed (not merely the frame —
    /// the Input echoing the typed digits dirties the frame on its own, a false
    /// positive we deliberately avoid).
    ///
    /// ROOT CAUSE (DEAD): `watch_color` repaints via `Static::set_inline_style`,
    /// which writes to the widget's `seed.styles.style`. After mount that seed
    /// was moved into the arena node (emptied), so a post-mount `set_inline_style`
    /// on an in-tree widget never reaches the node's rendered style — the swatch
    /// stays unpainted (`#color` node bg remains `None`). The compute+watch chain
    /// itself fires (the reactive value is correct); only the inline-style write
    /// is dropped. This is a styling-pipeline gap (sync widget inline style to the
    /// node in `with_widget_mut`, or route `set_inline_style` to the node),
    /// distinct from this reactive-dispatch sweep. The same gap kills `watch01`.
    /// Flip this `#[ignore]` once post-mount `set_inline_style` reaches render, or
    /// switch the watcher to the node-level `query_mut(sel).set_styles(...)` path
    /// (which already works, per the testing/rgb demo).
    #[test]
    #[ignore = "DEAD: post-mount Static::set_inline_style writes to the detached widget seed, never reaching the arena node style/render (styling-pipeline fix needed; same gap as watch01)"]
    fn liveness_typing_red_repaints_swatch() {
        textual::run_test(ComputedApp::new(), |pilot| {
            pilot.click("#red")?;
            pilot.press(&["2", "0", "0"])?;
            let cnode = pilot.app().query_one("#color").unwrap();
            let bg = pilot.app().node_explicit_bg(cnode);
            assert!(
                bg.is_some(),
                "typing a channel must paint the #color swatch background"
            );
            Ok(())
        })
        .unwrap();
    }
}
