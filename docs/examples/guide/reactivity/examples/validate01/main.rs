/// Port of Python Textual `docs/examples/guide/reactivity/validate01.py`.
///
/// Demonstrates reactive validation: a counter clamped between 0 and 10.
/// Two buttons (+1 / -1) adjust the counter; a RichLog displays each new value.
/// The `validate_count` hook ensures the value never goes below 0 or above 10.
///
/// Python structure:
///   count = reactive(0)
///   def validate_count(self, count: int) -> int: ...   # clamp [0, 10]
///   def on_button_pressed(self, event): self.count += 1 (or -1); log self.count
///
/// Rust port (faithful): the app derives `Reactive` and declares
/// `#[reactive(validate)] count`. The generated `set_count(value, ctx)` runs
/// `validate_count(value)` BEFORE storing — exactly like Python's `_set`. The
/// button handler calls `set_count(self.count() + 1, app.reactive_ctx())`, then
/// reads back the (clamped) `self.count()` to write the RichLog line.
use textual::message::ButtonPressed;
use textual::prelude::*;

const CSS: &str = r#"
#buttons {
    dock: top;
    height: auto;
}
"#;

#[derive(Reactive)]
struct ValidateApp {
    #[reactive(validate)]
    count: i32,
}

impl ValidateApp {
    fn new() -> Self {
        Self { count: 0 }
    }

    /// Python `validate_count`: clamp the incoming value to [0, 10].
    /// Called by the generated `set_count` setter before the value is stored.
    fn validate_count(&self, count: i32) -> i32 {
        count.clamp(0, 10)
    }
}

impl TextualApp for ValidateApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn compose(&mut self) -> AppRoot {
        let buttons = 
            Horizontal::new()
                .with_child(Button::success("+1").id("plus"))
                .with_child(Button::error("-1").id("minus"))
        .id("buttons");

        AppRoot::new()
            .with_child(buttons)
            .with_child(RichLog::new().highlight(true))
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            match bp.button_id.as_deref() {
                // `set_count` validates (clamps) before storing, so reading
                // `self.count()` afterwards yields the clamped value.
                Some("plus") => self.set_count(self.count() + 1, app.reactive_ctx()),
                Some("minus") => self.set_count(self.count() - 1, app.reactive_ctx()),
                _ => return,
            }
            let msg = format!("count = {}", self.count());
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
        let app = ValidateApp::new();
        assert_eq!(app.validate_count(-1), 0);
        assert_eq!(app.validate_count(-100), 0);
    }

    #[test]
    fn validate_count_clamps_above_ten() {
        let app = ValidateApp::new();
        assert_eq!(app.validate_count(11), 10);
        assert_eq!(app.validate_count(100), 10);
    }

    #[test]
    fn validate_count_passes_through_valid() {
        let app = ValidateApp::new();
        for i in 0..=10 {
            assert_eq!(app.validate_count(i), i);
        }
    }

    #[test]
    fn setter_clamps_via_validate() {
        // The generated setter runs validate_count before storing.
        let mut app = ValidateApp::new();
        let mut ctx = ReactiveCtx::new(textual::node_id::NodeId::default());
        app.set_count(99, &mut ctx);
        assert_eq!(*app.count(), 10);
        app.set_count(-5, &mut ctx);
        assert_eq!(*app.count(), 0);
    }

    #[test]
    fn compose_does_not_panic() {
        let mut app = ValidateApp::new();
        let _root = app.compose();
    }
}
