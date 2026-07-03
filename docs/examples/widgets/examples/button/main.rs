/// Port of Python Textual `docs/examples/widgets/button.py`.
///
/// Demonstrates the `Button` widget variants, disabled states, and flat style.
/// Layout mirrors Python: four `VerticalScroll` columns inside a `Horizontal`.
use textual::compose;
use textual::prelude::*;

struct ButtonsApp {
    selected: Option<String>,
}

impl TextualApp for ButtonsApp {
    fn compose(&mut self) -> AppRoot {
        let buttons = Horizontal::new().with_compose(compose![
            VerticalScroll::new().with_compose(compose![
                Static::new("Standard Buttons").class("header"),
                Button::new("Default"),
                Button::primary("Primary!"),
                Button::success("Success!"),
                Button::warning("Warning!"),
                Button::error("Error!"),
            ]),
            VerticalScroll::new().with_compose(compose![
                Static::new("Disabled Buttons").class("header"),
                Button::new("Default").disabled(true),
                Button::primary("Primary!").disabled(true),
                Button::success("Success!").disabled(true),
                Button::warning("Warning!").disabled(true),
                Button::error("Error!").disabled(true),
            ]),
            VerticalScroll::new().with_compose(compose![
                Static::new("Flat Buttons").class("header"),
                Button::new("Default").flat(true),
                Button::primary("Primary!").flat(true),
                Button::success("Success!").flat(true),
                Button::warning("Warning!").flat(true),
                Button::error("Error!").flat(true),
            ]),
            VerticalScroll::new().with_compose(compose![
                Static::new("Disabled Flat Buttons").class("header"),
                Button::new("Default").disabled(true).flat(true),
                Button::primary("Primary!").disabled(true).flat(true),
                Button::success("Success!").disabled(true).flat(true),
                Button::warning("Warning!").disabled(true).flat(true),
                Button::error("Error!").disabled(true).flat(true),
            ]),
        ]);

        AppRoot::new().with_child(buttons)
    }

    fn css_path(&self) -> Option<&'static str> {
        Some(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/shared/button.tcss"
        ))
    }

    fn on_button_pressed(&mut self, description: &str, ctx: &mut textual::event::WidgetCtx) {
        self.selected = Some(description.to_string());
        ctx.request_stop();
        ctx.set_handled();
    }

    fn take_exit_output(&mut self) -> Option<String> {
        self.selected.take()
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    let app = ButtonsApp { selected: None };
    if let Some(description) = run_sync_snapshot_with_output(app)? {
        println!("{description}");
    }
    Ok(())
}

#[cfg(test)]
mod liveness {
    use super::*;
    use textual::run_test;

    /// LIVENESS: tabbing focus to a button changes the rendered frame (the
    /// focused button renders a distinct, focused appearance). Proves the
    /// Button widget participates in focus/render and is not inert.
    #[test]
    fn tab_focus_changes_frame() {
        run_test(ButtonsApp { selected: None }, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["tab"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "tab must move focus to a Button and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
