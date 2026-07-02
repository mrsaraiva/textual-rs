//! Port of Python `docs/examples/guide/screens/questions01.py`.
//!
//! Demonstrates `App::push_screen_wait` — the Rust analogue of Python's
//! `await self.push_screen_wait(QuestionScreen(...))`. A background worker
//! (Python `@work`) pushes a `QuestionScreen` and suspends until the screen is
//! dismissed with a `bool`, then notifies based on the answer.
//!
//! Faithful mapping:
//! - `QuestionScreen` owns its dismiss handlers (`on_button_pressed` →
//!   `ctx.dismiss(true/false)`), mirroring Python's `@on(Button.Pressed, "#yes")`
//!   `self.dismiss(True)` / `#no` `self.dismiss(False)`.
//! - `on_mount` spawns a worker (Python `@work async def on_mount`) that calls
//!   `App::push_screen_wait(...)` and branches on the returned result, posting
//!   the notification back onto the UI thread via `App::call_from_thread`.

use rich_rs::Segments;
use textual::compose;
use textual::prelude::*;

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

    fn compose(&mut self) -> ComposeResult {
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

/// Python `class QuestionScreen(Screen[bool])`.
///
/// The screen owns the dismiss decision: a press of `#yes` dismisses with
/// `true`, `#no` with `false` (Python's `handle_yes`/`handle_no`).
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

    fn on_button_pressed(
        &mut self,
        pressed: &ButtonPressed,
        _control: NodeId,
        ctx: &mut ScreenMessageCtx,
    ) {
        match pressed.button_id.as_deref() {
            Some("yes") => ctx.dismiss(true), // Python: self.dismiss(True)
            Some("no") => ctx.dismiss(false), // Python: self.dismiss(False)
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// QuestionsApp
// ---------------------------------------------------------------------------

struct QuestionsApp;

impl TextualApp for QuestionsApp {
    /// Python:
    /// ```python
    /// @work
    /// async def on_mount(self) -> None:
    ///     if await self.push_screen_wait(QuestionScreen("Do you like Textual?")):
    ///         self.notify("Good answer!")
    ///     else:
    ///         self.notify(":-(", severity="error")
    /// ```
    ///
    /// Rust: spawn a worker that pushes the screen and suspends on the result.
    /// `App::push_screen_wait` blocks the worker until the screen dismisses; the
    /// returned `bool` selects the notification, posted back onto the UI thread.
    fn on_mount_with_app(&mut self, _app: &mut App, ctx: &mut EventCtx) {
        ctx.request_worker_task(Some("questions"), |_token| {
            let result =
                App::push_screen_wait(Box::new(QuestionScreen::new("Do you like Textual?")))
                    .map_err(|e| e.to_string())?;

            let liked = match result {
                ScreenResult::Value(value) => *value.downcast::<bool>().unwrap_or_default(),
                ScreenResult::Dismissed => false,
            };

            App::call_from_thread(move |app| {
                if liked {
                    app.notify("Good answer!", "", ToastSeverity::Information, None);
                } else {
                    app.notify(":-(", "", ToastSeverity::Error, None);
                }
            })
            .map_err(|e| e.to_string())?;
            Ok(())
        });
    }

    fn compose(&mut self) -> AppRoot {
        // Main screen is empty; the question screen is pushed from the worker.
        AppRoot::new()
    }
}

fn main() -> Result<()> {
    run_sync(QuestionsApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The screen dismisses with `true` when `#yes` is pressed (Python
    /// `handle_yes` → `self.dismiss(True)`). Verified by driving a
    /// `ButtonPressed("#yes")` through a `ScreenMessageCtx` and inspecting the
    /// staged dismissal.
    #[test]
    fn question_screen_yes_dismisses_with_true() {
        let mut screen = QuestionScreen::new("Do you like Textual?");
        let slot = std::sync::Mutex::new(None);
        let mut ctx = EventCtx::default();
        let mut screen_ctx = ScreenMessageCtx::for_test(&mut ctx, &slot);

        screen.on_button_pressed(
            &ButtonPressed {
                description: "Yes".into(),
                button_id: Some("yes".into()),
            },
            node_id_from_ffi(1),
            &mut screen_ctx,
        );

        let staged = slot.lock().unwrap().take().expect("yes should dismiss");
        match staged {
            ScreenResult::Value(v) => assert!(*v.downcast::<bool>().unwrap()),
            _ => panic!("expected Value(true)"),
        }
    }

    #[test]
    fn question_screen_no_dismisses_with_false() {
        let mut screen = QuestionScreen::new("Do you like Textual?");
        let slot = std::sync::Mutex::new(None);
        let mut ctx = EventCtx::default();
        let mut screen_ctx = ScreenMessageCtx::for_test(&mut ctx, &slot);

        screen.on_button_pressed(
            &ButtonPressed {
                description: "No".into(),
                button_id: Some("no".into()),
            },
            node_id_from_ffi(2),
            &mut screen_ctx,
        );

        let staged = slot.lock().unwrap().take().expect("no should dismiss");
        match staged {
            ScreenResult::Value(v) => assert!(!*v.downcast::<bool>().unwrap()),
            _ => panic!("expected Value(false)"),
        }
    }

    /// LIVENESS probe (Pilot, headless): on mount a worker calls
    /// `push_screen_wait(QuestionScreen)`. The screen should appear (screen_count
    /// 0 -> 1) and the rendered frame change; clicking `#yes` should dismiss it.
    ///
    /// Now LIVE: this demo's interaction is driven by a background *worker
    /// thread* that calls the blocking `App::push_screen_wait` (suspending the
    /// worker until the screen dismisses, with the push marshaled to the UI
    /// thread via `App::call_from_thread`). The headless pump now owns a
    /// `WorkerRegistry`, registers the test thread as the UI thread, and drains
    /// `call_from_thread` jobs — so the on_mount worker's screen push lands. The
    /// worker then parks on the dismiss channel; the pump detects quiescence and
    /// hands control back so the body's `#yes` click can dismiss the screen,
    /// unblocking the worker.
    #[test]
    fn questions01_worker_push_and_dismiss_is_live() {
        run_test(QuestionsApp, |pilot| {
            // The on_mount worker runs `push_screen_wait(QuestionScreen)` during
            // headless startup (the worker phase + `call_from_thread` bridge run
            // in the startup pump), so the QuestionScreen is already up by the
            // time the body begins.
            assert_eq!(
                pilot.app().screen_count(),
                1,
                "the on_mount worker must push the QuestionScreen"
            );
            let pushed = pilot.app().frame_fingerprint();

            // Answering Yes dismisses the screen, resuming the parked worker.
            pilot.click("#yes")?;
            pilot.pause()?;
            assert_eq!(
                pilot.app().screen_count(),
                0,
                "answering Yes must dismiss the QuestionScreen"
            );
            let dismissed = pilot.app().frame_fingerprint();
            assert_ne!(pushed, dismissed, "dismissing the question screen must change the frame");
            Ok(())
        })
        .expect("questions01 worker push/dismiss harness should run");
    }
}
