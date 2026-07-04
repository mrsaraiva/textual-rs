/// Port of Python Textual `docs/examples/widgets/switch.py`.
///
/// Demonstrates the `Switch` widget with four rows:
/// - An always-off switch (animate=False in Python; Rust always animates but
///   starts in the off position).
/// - An on switch (value=True).
/// - A focused switch (third switch receives initial focus via on_mount_with_app).
/// - A custom-styled switch (#custom-design with a custom slider color/background).
///
/// Python: `focused_switch.focus()` called before yielding. Rust: uses
/// `on_mount_with_app` to call `app.action_focus("focused-switch")`.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

Horizontal {
    height: auto;
    width: auto;
}

Switch {
    height: auto;
    width: auto;
}

.label {
    height: 3;
    content-align: center middle;
    width: auto;
}

#custom-design {
    background: darkslategrey;
}

#custom-design > .switch--slider {
    color: dodgerblue;
    background: darkslateblue;
}
"#;

struct SwitchApp;

impl TextualApp for SwitchApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            // Title row
            .with_child(Static::new("[b]Example switches\n").class("label"))
            // Row 1: "off" switch (starts off, no initial animation)
            .with_child(
                Horizontal::new()
                    .with_child(Static::new("off:     ").class("label"))
                    .with_child(Switch::new(false)),
            )
            // Row 2: "on" switch
            .with_child(
                Horizontal::new()
                    .with_child(Static::new("on:      ").class("label"))
                    .with_child(Switch::new(true)),
            )
            // Row 3: "focused" switch — receives initial focus via on_mount_with_app
            .with_child(
                Horizontal::new()
                    .with_child(Static::new("focused: ").class("label"))
                    .with_child(Switch::new(false).id("focused-switch")),
            )
            // Row 4: "custom" switch — #custom-design id for CSS styling
            .with_child(
                Horizontal::new()
                    .with_child(Static::new("custom:  ").class("label"))
                    .with_child(Switch::new(false).id("custom-design")),
            )
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut textual::event::WidgetCtx) {
        // Mirror Python: `focused_switch.focus()` — give the third switch initial focus.
        let _ = app.action_focus("focused-switch");
    }
}

fn main() -> textual::Result<()> {
    run_sync(SwitchApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_app_composes_without_panic() {
        let mut app = SwitchApp;
        let _root = app.compose();
    }

    /// LIVENESS: clicking the first Switch flips its boolean value (off -> on),
    /// emitting `Switch.Changed`. We assert on the observable widget state (the
    /// `value` flip) — the true thing the demo mutates. A dead Switch (mouse
    /// unhandled) leaves the value at `false`.
    ///
    /// NOTE: the slider *knob* is animated (`slider_pos` eases toward
    /// `slider_target`), and that easing is driven by the per-frame animation
    /// tick which the headless Pilot does not synthesise — so the rendered frame
    /// fingerprint does NOT change here even though the value flipped. The value
    /// transition is the honest headless-observable proof the toggle works; the
    /// knob slide is a live-only visual.
    #[test]
    fn liveness_click_toggles_switch_value() {
        SwitchApp
            .run_test(|pilot| {
                let read = |pilot: &Pilot| -> bool {
                    let app = pilot.app();
                    app.query_one_typed::<Switch>("Switch")
                        .ok()
                        .and_then(|h| h.read(app, |s| s.value()).ok())
                        .unwrap_or(false)
                };
                assert_eq!(read(pilot), false, "first switch starts off");
                pilot.click("Switch")?;
                assert_eq!(read(pilot), true, "clicking the switch must toggle it on");
                Ok(())
            })
            .expect("run_test");
    }
}
