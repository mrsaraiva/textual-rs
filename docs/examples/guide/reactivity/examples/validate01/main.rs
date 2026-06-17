/// Port of Python Textual `docs/examples/guide/reactivity/validate01.py`.
///
/// Demonstrates reactive validation: a counter clamped between 0 and 10.
/// Two buttons (+1 / -1) adjust the counter; a RichLog displays each new value.
/// The validate_count logic ensures the value never goes below 0 or above 10.
///
/// Python structure:
///   - ValidateApp(App) — count reactive clamped to [0, 10]
///   - Horizontal(#buttons) — Button("+1", id="plus", variant="success") +
///                             Button("-1", id="minus", variant="error")
///   - RichLog(highlight=True) — displays "count = N" on each press
///
/// Rust differences:
///   - Reactive derive not used for the counter; `count` is a plain i32 on the
///     app struct, clamped manually in `on_message_with_app` (same semantics as
///     Python's `validate_count` hook).
///   - `RichLog::write` is called via `app.with_query_one_mut_as`.
use textual::message::ButtonPressed;
use textual::prelude::*;

const CSS: &str = r#"
#buttons {
    dock: top;
    height: auto;
}
"#;

struct ValidateApp {
    count: i32,
}

impl ValidateApp {
    fn new() -> Self {
        Self { count: 0 }
    }

    /// Mirror Python's `validate_count`: clamp to [0, 10].
    fn validate_count(count: i32) -> i32 {
        count.clamp(0, 10)
    }
}

impl TextualApp for ValidateApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let buttons = Node::new(
            Horizontal::new()
                .with_child(Button::success("+1").id("plus"))
                .with_child(Button::error("-1").id("minus")),
        )
        .id("buttons");

        AppRoot::new()
            .with_child(buttons)
            .with_child(RichLog::new().highlight(true))
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            match bp.button_id.as_deref() {
                Some("plus") => {
                    self.count = Self::validate_count(self.count + 1);
                }
                Some("minus") => {
                    self.count = Self::validate_count(self.count - 1);
                }
                _ => return,
            }
            let msg = format!("count = {}", self.count);
            let _ = app.with_query_one_mut_as::<RichLog, _>("RichLog", |log| {
                log.write(msg);
            });
            ctx.request_repaint();
            ctx.set_handled();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(ValidateApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_count_clamps_below_zero() {
        assert_eq!(ValidateApp::validate_count(-1), 0);
        assert_eq!(ValidateApp::validate_count(-100), 0);
    }

    #[test]
    fn validate_count_clamps_above_ten() {
        assert_eq!(ValidateApp::validate_count(11), 10);
        assert_eq!(ValidateApp::validate_count(100), 10);
    }

    #[test]
    fn validate_count_passes_through_valid() {
        for i in 0..=10 {
            assert_eq!(ValidateApp::validate_count(i), i);
        }
    }

    #[test]
    fn compose_does_not_panic() {
        let mut app = ValidateApp::new();
        let _root = app.compose();
    }
}
