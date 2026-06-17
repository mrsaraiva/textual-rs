use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use rich_rs::Segments;
use textual::compose;
use textual::prelude::*;

// Represents the answer state communicated from the screen callback to the app.
const ANSWER_NONE: u8 = 0;
const ANSWER_YES: u8 = 1;
const ANSWER_NO: u8 = 2;

// ---------------------------------------------------------------------------
// QuestionScreen root widget
// ---------------------------------------------------------------------------

struct QuestionScreenRoot {
    question: String,
}

impl QuestionScreenRoot {
    fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
        }
    }
}

impl Widget for QuestionScreenRoot {
    fn style_type(&self) -> &'static str {
        "QuestionScreen"
    }

    fn compose(&self) -> ComposeResult {
        compose![
            Label::new(&self.question),
            Button::success("Yes").id("yes"),
            Button::new("No").id("no")
        ]
    }

    fn render(&self, _console: &rich_rs::Console, _options: &rich_rs::ConsoleOptions) -> Segments {
        Segments::new()
    }
}

// ---------------------------------------------------------------------------
// QuestionScreen
// ---------------------------------------------------------------------------

struct QuestionScreen {
    question: String,
}

impl QuestionScreen {
    fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
        }
    }
}

impl Screen for QuestionScreen {
    fn name(&self) -> &str {
        "QuestionScreen"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(QuestionScreenRoot::new(&self.question))
    }

    fn css(&self) -> Option<&str> {
        Some(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/shared/questions01.tcss"
        ))
    }
}

// ---------------------------------------------------------------------------
// QuestionsApp
// ---------------------------------------------------------------------------

struct QuestionsApp {
    /// Shared answer slot: set by the screen-result callback, read in on_message.
    answer: Arc<AtomicU8>,
}

impl Default for QuestionsApp {
    fn default() -> Self {
        Self {
            answer: Arc::new(AtomicU8::new(ANSWER_NONE)),
        }
    }
}

impl TextualApp for QuestionsApp {
    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        let answer = self.answer.clone();
        app.push_screen_with_callback(
            Box::new(QuestionScreen::new("Do you like Textual?")),
            Box::new(move |result| match result {
                ScreenResult::Value(val) => {
                    if let Ok(yes) = val.downcast::<bool>() {
                        answer.store(if *yes { ANSWER_YES } else { ANSWER_NO }, Ordering::SeqCst);
                    }
                }
                ScreenResult::Dismissed => {
                    answer.store(ANSWER_NONE, Ordering::SeqCst);
                }
            }),
        );
    }

    fn compose(&mut self) -> AppRoot {
        // Main screen is empty; the question screen is pushed on mount.
        AppRoot::new()
    }

    fn on_message_with_app(&mut self, app: &mut App, event: &MessageEvent, ctx: &mut EventCtx) {
        // Only handle ButtonPressed while the question screen is active.
        if event.downcast_ref::<ButtonPressed>().is_none() || app.screen_count() == 0 {
            return;
        }

        let msg = event.downcast_ref::<ButtonPressed>().unwrap();
        let button_id = msg.button_id.as_deref();

        if button_id == Some("yes") {
            let _ = app.dismiss_screen(ScreenResult::Value(Box::new(true)));
            // Callback fires synchronously inside dismiss_screen; read result now.
            let answer = self.answer.swap(ANSWER_NONE, Ordering::SeqCst);
            if answer == ANSWER_YES {
                app.notify("Good answer!", "", ToastSeverity::Information, None);
            }
            ctx.request_layout_invalidation();
            ctx.set_handled();
        } else if button_id == Some("no") {
            let _ = app.dismiss_screen(ScreenResult::Value(Box::new(false)));
            let answer = self.answer.swap(ANSWER_NONE, Ordering::SeqCst);
            if answer == ANSWER_NO {
                app.notify(":-(", "", ToastSeverity::Error, None);
            }
            ctx.request_layout_invalidation();
            ctx.set_handled();
        }
    }
}

fn main() -> Result<()> {
    run_sync(QuestionsApp::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn questions01_yes_button_dismisses_with_true() {
        let mut definition = QuestionsApp::default();
        let mut app = App::new().expect("app should initialize");

        // Simulate on_mount_with_app pushing the screen.
        let mut ctx = EventCtx::default();
        definition.on_mount_with_app(&mut app, &mut ctx);
        assert_eq!(app.screen_count(), 1, "question screen should be on the stack");

        // Find the Yes button in the screen.
        let yes_id = app
            .query_one("#yes")
            .expect("yes button should exist in the question screen");

        // Simulate ButtonPressed from the Yes button.
        let msg_event = MessageEvent::new(
            yes_id,
            ButtonPressed {
                description: "Yes".to_string(),
                button_id: Some("yes".to_string()),
            },
        );
        let mut msg_ctx = EventCtx::default();
        definition.on_message_with_app(&mut app, &msg_event, &mut msg_ctx);

        assert!(msg_ctx.handled(), "yes button press should be handled");
        assert_eq!(app.screen_count(), 0, "screen should be dismissed after yes");
    }

    #[test]
    fn questions01_no_button_dismisses_screen() {
        let mut definition = QuestionsApp::default();
        let mut app = App::new().expect("app should initialize");

        let mut ctx = EventCtx::default();
        definition.on_mount_with_app(&mut app, &mut ctx);
        assert_eq!(app.screen_count(), 1);

        let no_id = app
            .query_one("#no")
            .expect("no button should exist in the question screen");

        let msg_event = MessageEvent::new(
            no_id,
            ButtonPressed {
                description: "No".to_string(),
                button_id: Some("no".to_string()),
            },
        );
        let mut msg_ctx = EventCtx::default();
        definition.on_message_with_app(&mut app, &msg_event, &mut msg_ctx);

        assert!(msg_ctx.handled(), "no button press should be handled");
        assert_eq!(app.screen_count(), 0, "screen should be dismissed after no");
    }
}
