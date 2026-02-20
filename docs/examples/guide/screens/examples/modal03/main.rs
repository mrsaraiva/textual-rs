use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use rich_rs::Segments;
use textual::compose;
use textual::prelude::*;

const TEXT: &str = "I must not fear.\nFear is the mind-killer.\nFear is the little-death that brings total obliteration.\nI will face my fear.\nI will permit it to pass over me and through me.\nAnd when it has gone past, I will turn the inner eye to see its path.\nWhere the fear has gone there will be nothing. Only I will remain.";
const DECISION_NONE: u8 = 0;
const DECISION_QUIT: u8 = 1;

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
                .with_child(Node::new(Button::error("Quit")).id("quit"))
                .with_child(Node::new(Button::primary("Cancel")).id("cancel"))
        ]
    }

    fn render(&self, _console: &rich_rs::Console, _options: &rich_rs::ConsoleOptions) -> Segments {
        Segments::new()
    }
}

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
}

struct ModalApp {
    quit_decision: Arc<AtomicU8>,
}

impl Default for ModalApp {
    fn default() -> Self {
        Self {
            quit_decision: Arc::new(AtomicU8::new(DECISION_NONE)),
        }
    }
}

impl TextualApp for ModalApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        // Route key and clickable-footer invocations through one key path.
        vec![BindingDecl::new("q", "app.simulate_key('q')", "Quit")]
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Header::new().title("ModalApp"))
            .with_child(Label::new(TEXT.repeat(8)))
            .with_child(Footer::new())
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        if key.key != "q" {
            return;
        }
        // Prevent recursive re-dispatch through app.simulate_key('q').
        ctx.set_handled();

        if app.screen_count() > 0 {
            return;
        }

        let quit_decision = self.quit_decision.clone();
        app.push_screen_with_callback(
            Box::new(QuitScreen),
            Box::new(move |result| match result {
                ScreenResult::Dismissed => quit_decision.store(DECISION_NONE, Ordering::SeqCst),
                ScreenResult::Value(value) => {
                    if let Ok(quit) = value.downcast::<bool>() {
                        quit_decision.store(
                            if *quit { DECISION_QUIT } else { DECISION_NONE },
                            Ordering::SeqCst,
                        );
                    } else {
                        quit_decision.store(DECISION_NONE, Ordering::SeqCst);
                    }
                }
            }),
        );
        // Direct screen-stack mutation via app handle needs explicit redraw/layout invalidation.
        ctx.request_layout_invalidation();
    }

    fn on_message_with_app(&mut self, app: &mut App, event: &MessageEvent, ctx: &mut EventCtx) {
        if !matches!(event.message, Message::ButtonPressed(_)) || app.screen_count() == 0 {
            return;
        }

        let control = event.control.unwrap_or(event.sender);
        let quit = app.query_one_optional("#quit Button").ok().flatten();
        let cancel = app.query_one_optional("#cancel Button").ok().flatten();

        if Some(control) == quit {
            let _ = app.dismiss_screen(ScreenResult::Value(Box::new(true)));
            ctx.request_layout_invalidation();
            ctx.set_handled();
        } else if Some(control) == cancel {
            let _ = app.dismiss_screen(ScreenResult::Value(Box::new(false)));
            ctx.request_layout_invalidation();
            ctx.set_handled();
        }
        // Callback executes during dismiss_screen(); consume result immediately.
        let decision = self.quit_decision.swap(DECISION_NONE, Ordering::SeqCst);
        if decision == DECISION_QUIT {
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
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn q_key() -> KeyEventData {
        KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
    }

    fn button_pressed_event(button: NodeId) -> MessageEvent {
        MessageEvent {
            sender: button,
            message: Message::ButtonPressed(ButtonPressed {
                description: "test".to_string(),
            }),
            control: Some(button),
        }
    }

    #[test]
    fn modal03_quit_button_dismisses_and_requests_stop() {
        let mut definition = ModalApp::default();
        let mut app = App::new().expect("app should initialize");

        let mut key_ctx = EventCtx::default();
        definition.on_key_with_app(&mut app, &q_key(), &mut key_ctx);
        assert!(key_ctx.handled());
        assert_eq!(app.screen_count(), 1);

        let quit = app
            .query_one("#quit Button")
            .expect("quit button should exist in pushed screen");
        let mut message_ctx = EventCtx::default();
        definition.on_message_with_app(&mut app, &button_pressed_event(quit), &mut message_ctx);

        assert!(message_ctx.handled());
        assert!(message_ctx.stop_requested());
        assert_eq!(app.screen_count(), 0);
    }

    #[test]
    fn modal03_cancel_button_dismisses_without_stop_request() {
        let mut definition = ModalApp::default();
        let mut app = App::new().expect("app should initialize");

        let mut key_ctx = EventCtx::default();
        definition.on_key_with_app(&mut app, &q_key(), &mut key_ctx);
        assert!(key_ctx.handled());
        assert_eq!(app.screen_count(), 1);

        let cancel = app
            .query_one("#cancel Button")
            .expect("cancel button should exist in pushed screen");
        let mut message_ctx = EventCtx::default();
        definition.on_message_with_app(&mut app, &button_pressed_event(cancel), &mut message_ctx);

        assert!(message_ctx.handled());
        assert!(!message_ctx.stop_requested());
        assert_eq!(app.screen_count(), 0);
    }
}
