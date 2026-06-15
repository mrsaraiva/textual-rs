use textual::message::ButtonPressed;
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    layout: grid;
    grid-size: 2;
    grid-gutter: 2;
    padding: 2;
}
#question {
    width: 100%;
    height: 100%;
    column-span: 2;
    content-align: center bottom;
    text-style: bold;
}
Button {
    width: 100%;
}
"#;

struct MyApp {
    exit_value: Option<String>,
}

impl MyApp {
    fn new() -> Self {
        Self { exit_value: None }
    }
}

impl TextualApp for MyApp {
    fn title(&self) -> &'static str {
        "A Question App"
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        app.set_sub_title("The most important question");
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Header::new())
            .with_child(Label::new("Do you love Textual?").with_id("question"))
            .with_child(Button::primary("Yes").id("yes"))
            .with_child(Button::error("No").id("no"))
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            if let Some(id) = &bp.button_id {
                self.exit_value = Some(id.clone());
            }
            ctx.request_stop();
            ctx.set_handled();
            let _ = app;
        }
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, _ctx: &mut EventCtx) {
        let key_name = key.name().to_string();
        app.set_title(format!("{}", key_name));
        app.set_sub_title(format!("You just pressed {}!", key_name));
    }

    fn take_exit_output(&mut self) -> Option<String> {
        self.exit_value.take()
    }
}

fn main() -> textual::Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    if let Some(reply) = run_sync_snapshot_with_output(MyApp::new())? {
        println!("{reply}");
    }
    Ok(())
}
