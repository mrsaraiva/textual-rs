/// Port of Python Textual `docs/examples/app/question01.py`.
///
/// Displays a question label and two buttons (Yes/No). When a button is
/// pressed the app exits and prints the button id ("yes" or "no").
use textual::prelude::*;

struct QuestionApp {
    reply: Option<String>,
}

impl TextualApp for QuestionApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("Do you love Textual?"))
            .with_child(Button::primary("Yes").id("yes"))
            .with_child(Button::error("No").id("no"))
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(m) = message.downcast_ref::<ButtonPressed>() {
            if let Some(id) = &m.button_id {
                self.reply = Some(id.clone());
            }
            ctx.request_stop();
            ctx.set_handled();
        }
    }

    fn take_exit_output(&mut self) -> Option<String> {
        self.reply.take()
    }
}

fn main() -> Result<()> {
    let app = QuestionApp { reply: None };
    if let Some(reply) = run_sync_with_output(app)? {
        println!("{reply}");
    }
    Ok(())
}
