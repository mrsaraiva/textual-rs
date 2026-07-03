/// Port of Python Textual `docs/examples/app/question03.py`.
///
/// Demonstrates a grid layout with a Label (spanning 2 columns) and two Buttons.
/// When a button is pressed the app exits and prints which button was pressed.
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
        if let Some(m) = message.downcast_ref::<ButtonPressed>() {
            if let Some(id) = &m.button_id {
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

fn main() -> textual::Result<()> {
    if let Some(reply) = run_sync_with_output(QuestionApp::new())? {
        println!("{reply}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS probe (Pilot, headless): clicking a button posts `ButtonPressed`,
    /// handled by recording the answer and requesting stop (the Python demo
    /// exits printing the button id). Liveness via `headless_stop_requested()`.
    #[test]
    fn question03_button_press_exits_is_live() {
        run_test(QuestionApp::new(), |pilot| {
            assert!(!pilot.app().headless_stop_requested(), "no stop before interaction");
            pilot.click("#yes")?;
            assert!(
                pilot.app().headless_stop_requested(),
                "clicking #yes must fire the handler and request app exit"
            );
            Ok(())
        })
        .expect("question03 button-exit harness should run");
    }
}
