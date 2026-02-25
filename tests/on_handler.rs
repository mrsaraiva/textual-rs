//! Integration tests for the `#[on(MessageType)]` attribute macro.

use textual::event::EventCtx;
use textual::message::{ButtonPressed, CheckboxChanged, Message};
use textual::node_id::node_id_from_ffi;
use textual::on;

// ---------------------------------------------------------------------------
// Test struct with `#[on]`-annotated handler methods
// ---------------------------------------------------------------------------

struct MyApp {
    button_count: u32,
    last_checkbox: Option<bool>,
    save_count: u32,
}

impl MyApp {
    fn new() -> Self {
        Self {
            button_count: 0,
            last_checkbox: None,
            save_count: 0,
        }
    }

    // Type-only handler: matches any ButtonPressed message.
    #[on(ButtonPressed)]
    fn handle_button(&mut self, event: &ButtonPressed, ctx: &mut EventCtx) {
        let _ = (event, ctx);
        self.button_count += 1;
    }

    // Type-only handler for CheckboxChanged.
    #[on(CheckboxChanged)]
    fn handle_checkbox(&mut self, event: &CheckboxChanged, ctx: &mut EventCtx) {
        let _ = ctx;
        self.last_checkbox = Some(event.checked);
    }

    // Selector handler: matches ButtonPressed from widget matching "#save".
    #[on(ButtonPressed, selector = "#save")]
    fn handle_save(&mut self, event: &ButtonPressed, ctx: &mut EventCtx) {
        let _ = (event, ctx);
        self.save_count += 1;
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_ctx() -> EventCtx {
    EventCtx::default()
}

fn dummy_sender() -> textual::node_id::NodeId {
    node_id_from_ffi(1)
}

// ---------------------------------------------------------------------------
// Tests — type-only dispatch (uniform signature: msg, sender, ctx)
// ---------------------------------------------------------------------------

#[test]
fn dispatch_matches_correct_message_type() {
    let mut app = MyApp::new();
    let msg = Message::ButtonPressed(ButtonPressed {
        description: "ok".into(),
        button_id: None,
    });
    let mut ctx = test_ctx();

    let matched = app.__on_dispatch_handle_button(&msg, dummy_sender(), &mut ctx);
    assert!(matched);
    assert_eq!(app.button_count, 1);
}

#[test]
fn dispatch_ignores_wrong_message_type() {
    let mut app = MyApp::new();
    let msg = Message::CheckboxChanged(CheckboxChanged { checked: true });
    let mut ctx = test_ctx();

    let matched = app.__on_dispatch_handle_button(&msg, dummy_sender(), &mut ctx);
    assert!(!matched);
    assert_eq!(app.button_count, 0);
}

#[test]
fn dispatch_checkbox_handler() {
    let mut app = MyApp::new();
    let msg = Message::CheckboxChanged(CheckboxChanged { checked: true });
    let mut ctx = test_ctx();

    let matched = app.__on_dispatch_handle_checkbox(&msg, dummy_sender(), &mut ctx);
    assert!(matched);
    assert_eq!(app.last_checkbox, Some(true));
}

#[test]
fn dispatch_checkbox_ignores_button() {
    let mut app = MyApp::new();
    let msg = Message::ButtonPressed(ButtonPressed {
        description: "no".into(),
        button_id: None,
    });
    let mut ctx = test_ctx();

    let matched = app.__on_dispatch_handle_checkbox(&msg, dummy_sender(), &mut ctx);
    assert!(!matched);
    assert!(app.last_checkbox.is_none());
}

// ---------------------------------------------------------------------------
// Tests — selector dispatch (same uniform signature)
// ---------------------------------------------------------------------------

#[test]
fn selector_dispatch_matches_message_type() {
    let mut app = MyApp::new();
    let msg = Message::ButtonPressed(ButtonPressed {
        description: "save".into(),
        button_id: None,
    });
    let sender = node_id_from_ffi(42);
    let mut ctx = test_ctx();

    let matched = app.__on_dispatch_handle_save(&msg, sender, &mut ctx);
    assert!(matched);
    assert_eq!(app.save_count, 1);
}

#[test]
fn selector_dispatch_ignores_wrong_type() {
    let mut app = MyApp::new();
    let msg = Message::CheckboxChanged(CheckboxChanged { checked: false });
    let sender = node_id_from_ffi(42);
    let mut ctx = test_ctx();

    let matched = app.__on_dispatch_handle_save(&msg, sender, &mut ctx);
    assert!(!matched);
    assert_eq!(app.save_count, 0);
}

#[test]
fn selector_const_is_generated() {
    assert_eq!(MyApp::__ON_SELECTOR_HANDLE_SAVE, "#save");
}

// ---------------------------------------------------------------------------
// Tests — multiple dispatches accumulate
// ---------------------------------------------------------------------------

#[test]
fn multiple_dispatches_accumulate() {
    let mut app = MyApp::new();
    let msg = Message::ButtonPressed(ButtonPressed {
        description: "click".into(),
        button_id: None,
    });
    let mut ctx = test_ctx();

    app.__on_dispatch_handle_button(&msg, dummy_sender(), &mut ctx);
    app.__on_dispatch_handle_button(&msg, dummy_sender(), &mut ctx);
    app.__on_dispatch_handle_button(&msg, dummy_sender(), &mut ctx);

    assert_eq!(app.button_count, 3);
}
