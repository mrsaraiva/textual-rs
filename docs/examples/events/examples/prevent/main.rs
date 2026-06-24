/// Port of Python Textual `docs/examples/events/prevent.py`.
///
/// Demonstrates suppressing widget messages:
/// - An `Input` and a "Clear" button are composed.
/// - When the button is pressed, the input is cleared WITHOUT triggering the
///   `Input.Changed` event (Python uses `with input.prevent(Input.Changed):`).
/// - The `on_input_changed` handler rings the bell on every normal keystroke.
///
/// Rust mirrors Python's `with input.prevent(Input.Changed):` using
/// [`EventCtx::prevent`] — the real `prevent(MessageType)` context. The clear is
/// performed inside `ctx.prevent::<InputChanged, _>(...)`, so any `InputChanged`
/// the input would post during the clear is suppressed (never queued), exactly
/// like Python's `prevent_message_types_stack`.
///
/// NOTE: The bell is a no-op in Rust (terminal bell is not implemented yet).
use textual::prelude::*;

struct PreventApp {
    bell_count: u32,
}

impl PreventApp {
    fn new() -> Self {
        Self { bell_count: 0 }
    }
}

impl TextualApp for PreventApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Input::new())
            .with_child(Button::new("Clear").id("clear"))
    }

    /// Called when the user types — rings the bell.
    fn on_input_changed(
        &mut self,
        _value: &str,
        _validation: &ValidationResult,
        _ctx: &mut EventCtx,
    ) {
        // bell() — no-op in Rust; count for test assertions.
        self.bell_count += 1;
    }

    /// Clear the text input WITHOUT triggering `InputChanged`.
    ///
    /// Python: `with input.prevent(Input.Changed): input.value = ""`.
    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            if bp.button_id.as_deref() == Some("clear") {
                // Suppress InputChanged for the duration of the clear, exactly
                // like Python's `with input.prevent(Input.Changed):`. Any
                // InputChanged posted while clearing is dropped, so the
                // bell-on-change handler never fires for a programmatic clear.
                ctx.prevent::<InputChanged, _>(|ctx| {
                    let _ = app.with_query_one_mut_as::<Input, _>("Input", |input| {
                        input.clear();
                    });
                    // Mirror the reactive emission the input would make: even
                    // when explicitly posted, this InputChanged is suppressed.
                    ctx.post_message(InputChanged {
                        value: String::new(),
                        validation: ValidationResult::success(),
                    });
                });
                ctx.request_repaint();
                ctx.set_handled();
            }
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(PreventApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prevent_app_composes_without_panic() {
        let mut app = PreventApp::new();
        let _root = app.compose();
    }

    #[test]
    fn bell_count_starts_at_zero() {
        let app = PreventApp::new();
        assert_eq!(app.bell_count, 0);
    }

    #[test]
    fn prevent_scope_suppresses_input_changed() {
        // Verify the prevent context drops the InputChanged a clear would emit.
        let mut ctx = EventCtx::default();
        ctx.set_node_id(textual::node_id::node_id_from_ffi(1));
        ctx.prevent::<InputChanged, _>(|ctx| {
            ctx.post_message(InputChanged {
                value: String::new(),
                validation: ValidationResult::success(),
            });
        });
        // No message queued: the bell-on-change handler would never run.
        assert_eq!(ctx.pending_message_count(), 0);
        assert!(!ctx.has_pending_message::<InputChanged>());
    }

    #[test]
    fn normal_input_changed_is_not_suppressed() {
        // Outside a prevent scope, InputChanged posts normally.
        let mut ctx = EventCtx::default();
        ctx.set_node_id(textual::node_id::node_id_from_ffi(1));
        ctx.post_message(InputChanged {
            value: "x".into(),
            validation: ValidationResult::success(),
        });
        assert_eq!(ctx.pending_message_count(), 1);
        assert!(ctx.has_pending_message::<InputChanged>());
    }
}
