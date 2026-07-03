/// Port of Python Textual `docs/examples/app/question02.py`.
///
/// Demonstrates a simple question app with two buttons ("Yes" / "No").
/// On button press the app exits and prints the id of the pressed button.
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

struct QuestionApp {
    answer: Option<String>,
}

impl QuestionApp {
    fn new() -> Self {
        Self { answer: None }
    }
}

impl TextualApp for QuestionApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Label::new("Do you love Textual?").with_id("question"))
            .with_child(Button::primary("Yes").id("yes"))
            .with_child(Button::error("No").id("no"))
    }

    fn on_message_with_app(
        &mut self,
        _app: &mut App,
        message: &MessageEvent,
        ctx: &mut textual::event::WidgetCtx,
    ) {
        if let Some(ButtonPressed { button_id, .. }) = message.downcast_ref::<ButtonPressed>() {
            if let Some(id) = button_id {
                self.answer = Some(id.clone());
            }
            ctx.request_stop();
            ctx.set_handled();
        }
    }

    fn take_exit_output(&mut self) -> Option<String> {
        self.answer.take()
    }
}

fn main() -> Result<()> {
    let app = QuestionApp::new();
    if let Some(reply) = run_sync_snapshot_with_output(app)? {
        println!("{reply}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS probe (Pilot, headless): clicking a button posts `ButtonPressed`,
    /// which the app handles by recording the answer and requesting stop (the
    /// Python demo exits printing the button id). Liveness is asserted via
    /// `headless_stop_requested()` since the exit demo's frame is unchanged.
    #[test]
    fn question02_button_press_exits_is_live() {
        run_test(QuestionApp::new(), |pilot| {
            assert!(!pilot.app().headless_stop_requested(), "no stop before interaction");
            pilot.click("#no")?;
            assert!(
                pilot.app().headless_stop_requested(),
                "clicking #no must fire the handler and request app exit"
            );
            Ok(())
        })
        .expect("question02 button-exit harness should run");
    }
}
