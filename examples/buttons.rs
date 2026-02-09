use textual::prelude::*;
use std::sync::{Arc, Mutex};

struct ButtonsApp {
    selected: Arc<Mutex<Option<String>>>,
}

impl TextualApp for ButtonsApp {
    fn compose(&mut self) -> AppRoot {
        build_buttons_widget()
    }

    fn css_path(&self) -> Option<&'static str> {
        Some("examples/button.tcss")
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Message::ButtonPressed { description } = &message.message {
            *self.selected.lock().unwrap_or_else(|e| e.into_inner()) = Some(description.clone());
            ctx.request_stop();
            ctx.set_handled();
        }
    }
}

fn build_buttons_widget() -> AppRoot {
    let buttons = Horizontal::new()
        .with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Standard Buttons")).class("header"))
                .with_child(Button::new("Default"))
                .with_child(Button::primary("Primary!"))
                .with_child(Button::success("Success!"))
                .with_child(Button::warning("Warning!"))
                .with_child(Button::error("Error!")),
        )
        .with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Disabled Buttons")).class("header"))
                .with_child(Button::new("Default").disabled(true))
                .with_child(Button::primary("Primary!").disabled(true))
                .with_child(Button::success("Success!").disabled(true))
                .with_child(Button::warning("Warning!").disabled(true))
                .with_child(Button::error("Error!").disabled(true)),
        )
        .with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Flat Buttons")).class("header"))
                .with_child(Button::new("Default").flat(true))
                .with_child(Button::primary("Primary!").flat(true))
                .with_child(Button::success("Success!").flat(true))
                .with_child(Button::warning("Warning!").flat(true))
                .with_child(Button::error("Error!").flat(true)),
        )
        .with_child(
            VerticalScroll::new()
                .with_child(Node::new(Static::new("Disabled Flat Buttons")).class("header"))
                .with_child(Button::new("Default").disabled(true).flat(true))
                .with_child(Button::primary("Primary!").disabled(true).flat(true))
                .with_child(Button::success("Success!").disabled(true).flat(true))
                .with_child(Button::warning("Warning!").disabled(true).flat(true))
                .with_child(Button::error("Error!").disabled(true).flat(true)),
        );

    AppRoot::new().with_child(buttons)
}

#[tokio::main]
async fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    let selected = Arc::new(Mutex::new(None));
    let app = ButtonsApp {
        selected: selected.clone(),
    };
    run_textual_app_or_snapshot(app).await?;
    if let Some(description) = selected.lock().unwrap_or_else(|e| e.into_inner()).take() {
        println!("{description}");
    }
    Ok(())
}
