/// Port of Python Textual `docs/examples/app/question_title01.py`.
///
/// Demonstrates App TITLE and SUB_TITLE with a question-and-buttons layout:
/// - Header shows the app title and sub-title.
/// - A label asks "Do you love Textual?".
/// - Two buttons ("Yes" / "No") exit the app with the pressed button's id.
///
/// CSS mirrors `question02.tcss`: 2-column grid, question spans both columns.
use textual::compose;
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
    reply: Option<String>,
}

impl TextualApp for MyApp {
    fn title(&self) -> &'static str {
        "A Question App"
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        app.set_sub_title("The most important question");
        ctx.request_repaint();
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(compose![
            Header::new(),
            Label::new("Do you love Textual?").with_id("question"),
            Button::primary("Yes").id("yes"),
            Button::error("No").id("no"),
        ])
    }

    fn on_message_with_app(&mut self, _app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(ev) = message.downcast_ref::<ButtonPressed>() {
            self.reply = ev.button_id.clone();
            ctx.request_stop();
            ctx.set_handled();
        }
    }

    fn take_exit_output(&mut self) -> Option<String> {
        self.reply.take()
    }
}

fn main() -> textual::Result<()> {
    let app = MyApp { reply: None };
    if let Some(reply) = run_sync_snapshot_with_output(app)? {
        println!("{reply}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_title01_composes_without_panic() {
        let mut app = MyApp { reply: None };
        let root = app.compose();
        assert!(!root.children().is_empty());
    }

    #[test]
    fn question_title01_title_is_correct() {
        let app = MyApp { reply: None };
        assert_eq!(app.title(), "A Question App");
    }
}
