use textual::prelude::*;

struct ButtonsApp {
    selected: Option<String>,
}

impl TextualApp for ButtonsApp {
    fn compose(&mut self) -> AppRoot {
        build_buttons_widget()
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

fn button_column(title: &str, disabled: bool, flat: bool) -> VerticalScroll {
    VerticalScroll::new()
        .with_child(Static::new(title).class("header"))
        .with_child(Button::new("Default").disabled(disabled).flat(flat))
        .with_child(Button::primary("Primary!").disabled(disabled).flat(flat))
        .with_child(Button::success("Success!").disabled(disabled).flat(flat))
        .with_child(Button::warning("Warning!").disabled(disabled).flat(flat))
        .with_child(Button::error("Error!").disabled(disabled).flat(flat))
}

fn build_buttons_widget() -> AppRoot {
    let buttons = Horizontal::new()
        .with_child(button_column("Standard Buttons", false, false))
        .with_child(button_column("Disabled Buttons", true, false))
        .with_child(button_column("Flat Buttons", false, true))
        .with_child(button_column("Disabled Flat Buttons", true, true));

    AppRoot::new().with_child(buttons)
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
