use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rich_rs::Segments;
use textual::compose;
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.";

struct QuitDialogRoot;

impl Widget for QuitDialogRoot {
    fn style_type(&self) -> &'static str {
        "QuitScreen"
    }

    fn compose(&self) -> ComposeResult {
        compose![
            Grid::new(2, 2)
                .id("dialog")
                .with_child(Label::new("Are you sure you want to quit?").with_id("question"))
                .with_child(Button::error("Quit").id("quit"))
                .with_child(Button::primary("Cancel").id("cancel"))
        ]
    }

    fn render(&self, _console: &rich_rs::Console, _options: &rich_rs::ConsoleOptions) -> Segments {
        Segments::new()
    }
}

/// `ModalScreen[bool]` from Python `modal03.py`: the screen owns its
/// `on_button_pressed` handler and dismisses with a typed `bool` result
/// (`dismiss(True)` / `dismiss(False)`) delivered to the push callback.
struct QuitScreen;

impl Screen for QuitScreen {
    fn name(&self) -> &str {
        "QuitScreen"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(QuitDialogRoot)
    }

    fn css(&self) -> Option<&str> {
        Some(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/shared/modal01.tcss"
        ))
    }

    fn on_button_pressed(
        &mut self,
        pressed: &ButtonPressed,
        _control: NodeId,
        ctx: &mut ScreenMessageCtx,
    ) {
        // Python: dismiss(True) for quit, dismiss(False) for cancel.
        let quit = pressed.button_id.as_deref() == Some("quit");
        ctx.dismiss(quit);
    }
}

struct ModalApp {
    /// Set by the screen-result callback when the dialog returns `true`.
    /// Polled in `on_tick_with_app` to stop the app (mirrors Python's
    /// `check_quit` callback calling `self.exit()`).
    should_quit: Arc<AtomicBool>,
}

impl Default for ModalApp {
    fn default() -> Self {
        Self {
            should_quit: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl TextualApp for ModalApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("q", "request_quit", "Quit")]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Header::new())
            .with_child(Label::new(TEXT.repeat(8)))
            .with_child(Footer::new())
    }

    /// `action_request_quit`: push the modal QuitScreen with a callback that
    /// records the dismiss result, exactly like Python's `check_quit`.
    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        if action != "request_quit" {
            return;
        }
        if app.screen_count() == 0 {
            let should_quit = self.should_quit.clone();
            app.push_screen_with_callback(
                Box::new(QuitScreen),
                Box::new(move |result| {
                    // Python: def check_quit(quit): if quit: self.exit()
                    if let ScreenResult::Value(value) = result {
                        if let Ok(quit) = value.downcast::<bool>() {
                            if *quit {
                                should_quit.store(true, Ordering::SeqCst);
                            }
                        }
                    }
                }),
            );
        }
        ctx.set_handled();
    }

    fn on_tick_with_app(&mut self, _app: &mut App, _tick: u64, ctx: &mut EventCtx) {
        if self.should_quit.load(Ordering::SeqCst) {
            ctx.request_stop();
        }
    }
}

fn main() -> Result<()> {
    run_sync(ModalApp::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn press(button_id: &str) -> Option<ScreenResult> {
        let mut screen = QuitScreen;
        let slot: Mutex<Option<ScreenResult>> = Mutex::new(None);
        let mut ctx = EventCtx::default();
        let mut screen_ctx = ScreenMessageCtx::for_test(&mut ctx, &slot);
        screen.on_button_pressed(
            &ButtonPressed {
                description: button_id.into(),
                button_id: Some(button_id.into()),
            },
            NodeId::default(),
            &mut screen_ctx,
        );
        slot.lock().unwrap().take()
    }

    #[test]
    fn modal03_quit_button_dismisses_with_true() {
        match press("quit") {
            Some(ScreenResult::Value(v)) => assert!(*v.downcast::<bool>().unwrap()),
            other => panic!("expected Value(true), got dismissed/none: {}", other.is_none()),
        }
    }

    #[test]
    fn modal03_cancel_button_dismisses_with_false() {
        match press("cancel") {
            Some(ScreenResult::Value(v)) => assert!(!*v.downcast::<bool>().unwrap()),
            _ => panic!("expected Value(false)"),
        }
    }

    /// End-to-end: pressing Quit drives the screen's dismiss(true) into the
    /// push callback, which sets `should_quit`; the tick hook then requests stop.
    #[test]
    fn modal03_quit_flow_requests_stop_via_callback() {
        let mut definition = ModalApp::default();
        let mut app = App::new().expect("app should initialize");

        // action_request_quit pushes the screen with the result callback.
        let mut action_ctx = EventCtx::default();
        definition.on_app_action_str(&mut app, "request_quit", &mut action_ctx);
        assert_eq!(app.screen_count(), 1);

        // The screen dismisses with true via its callback path.
        assert!(app.dismiss_screen(ScreenResult::Value(Box::new(true))));
        assert_eq!(app.screen_count(), 0);

        // The callback set should_quit; the tick hook requests stop.
        let mut tick_ctx = EventCtx::default();
        definition.on_tick_with_app(&mut app, 0, &mut tick_ctx);
        assert!(tick_ctx.stop_requested());
    }

    #[test]
    fn modal03_cancel_flow_does_not_request_stop() {
        let mut definition = ModalApp::default();
        let mut app = App::new().expect("app should initialize");

        let mut action_ctx = EventCtx::default();
        definition.on_app_action_str(&mut app, "request_quit", &mut action_ctx);
        assert_eq!(app.screen_count(), 1);

        assert!(app.dismiss_screen(ScreenResult::Value(Box::new(false))));
        assert_eq!(app.screen_count(), 0);

        let mut tick_ctx = EventCtx::default();
        definition.on_tick_with_app(&mut app, 0, &mut tick_ctx);
        assert!(!tick_ctx.stop_requested());
    }
}
