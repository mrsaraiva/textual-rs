use textual::prelude::*;

struct ButtonsApp {
    selected: Option<String>,
}

impl TextualApp for ButtonsApp {
    fn compose(&mut self) -> AppRoot {
        let buttons = Horizontal::new()
            .with_child(
                VerticalScroll::new()
                    .with_child(Static::new("Standard Buttons").class("header"))
                    .with_child(Button::new("Default"))
                    .with_child(Button::primary("Primary!"))
                    .with_child(Button::success("Success!"))
                    .with_child(Button::warning("Warning!"))
                    .with_child(Button::error("Error!")),
            )
            .with_child(
                VerticalScroll::new()
                    .with_child(Static::new("Disabled Buttons").class("header"))
                    .with_child(Button::new("Default").disabled(true))
                    .with_child(Button::primary("Primary!").disabled(true))
                    .with_child(Button::success("Success!").disabled(true))
                    .with_child(Button::warning("Warning!").disabled(true))
                    .with_child(Button::error("Error!").disabled(true)),
            )
            .with_child(
                VerticalScroll::new()
                    .with_child(Static::new("Flat Buttons").class("header"))
                    .with_child(Button::new("Default").flat(true))
                    .with_child(Button::primary("Primary!").flat(true))
                    .with_child(Button::success("Success!").flat(true))
                    .with_child(Button::warning("Warning!").flat(true))
                    .with_child(Button::error("Error!").flat(true)),
            )
            .with_child(
                VerticalScroll::new()
                    .with_child(Static::new("Disabled Flat Buttons").class("header"))
                    .with_child(Button::new("Default").disabled(true).flat(true))
                    .with_child(Button::primary("Primary!").disabled(true).flat(true))
                    .with_child(Button::success("Success!").disabled(true).flat(true))
                    .with_child(Button::warning("Warning!").disabled(true).flat(true))
                    .with_child(Button::error("Error!").disabled(true).flat(true)),
            );

        AppRoot::new().with_child(buttons)
    }

    fn css_path(&self) -> Option<&'static str> {
        Some("examples/button.tcss")
    }

    fn on_button_pressed(&mut self, description: &str, ctx: &mut EventCtx) {
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
