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

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS probe (Pilot, headless): clicking a button posts `ButtonPressed`,
    /// which the app handles by recording the reply and requesting stop (the
    /// Python demo exits and prints the button id). The exit demo leaves the
    /// frame unchanged, so liveness is asserted via `headless_stop_requested()`:
    /// no stop before the click, stop requested after. Proves the button handler
    /// is wired and fires.
    #[test]
    fn question01_button_press_exits_is_live() {
        run_test(QuestionApp { reply: None }, |pilot| {
            assert!(!pilot.app().headless_stop_requested(), "no stop before interaction");
            pilot.click("#yes")?;
            assert!(
                pilot.app().headless_stop_requested(),
                "clicking #yes must fire the handler and request app exit"
            );
            Ok(())
        })
        .expect("question01 button-exit harness should run");
    }
}
