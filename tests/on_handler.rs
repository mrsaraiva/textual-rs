//! Integration tests for the `#[on(MessageType)]` attribute macro.

use textual::event::{EventCtx, WidgetCtx};
use textual::message::{ButtonPressed, CheckboxChanged, MessageEvent};
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
    fn handle_button(&mut self, event: &ButtonPressed, ctx: &mut WidgetCtx) {
        let _ = (event, ctx);
        self.button_count += 1;
    }

    // Type-only handler for CheckboxChanged.
    #[on(CheckboxChanged)]
    fn handle_checkbox(&mut self, event: &CheckboxChanged, ctx: &mut WidgetCtx) {
        let _ = ctx;
        self.last_checkbox = Some(event.checked);
    }

    // Selector handler: matches ButtonPressed from widget matching "#save".
    #[on(ButtonPressed, selector = "#save")]
    fn handle_save(&mut self, event: &ButtonPressed, ctx: &mut WidgetCtx) {
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

fn button_event() -> MessageEvent {
    MessageEvent::new(
        dummy_sender(),
        ButtonPressed {
            description: "ok".into(),
            button_id: None,
        },
    )
}

fn checkbox_event(checked: bool) -> MessageEvent {
    MessageEvent::new(dummy_sender(), CheckboxChanged { checked })
}

// ---------------------------------------------------------------------------
// Tests — type-only dispatch (new signature: &MessageEvent, &mut textual::event::WidgetCtx)
// ---------------------------------------------------------------------------

#[test]
fn dispatch_matches_correct_message_type() {
    let mut app = MyApp::new();
    let event = button_event();
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_button(&event, &mut ctx);
    assert!(matched);
    assert_eq!(app.button_count, 1);
}

#[test]
fn dispatch_ignores_wrong_message_type() {
    let mut app = MyApp::new();
    let event = checkbox_event(true);
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_button(&event, &mut ctx);
    assert!(!matched);
    assert_eq!(app.button_count, 0);
}

#[test]
fn dispatch_checkbox_handler() {
    let mut app = MyApp::new();
    let event = checkbox_event(true);
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_checkbox(&event, &mut ctx);
    assert!(matched);
    assert_eq!(app.last_checkbox, Some(true));
}

#[test]
fn dispatch_checkbox_ignores_button() {
    let mut app = MyApp::new();
    let event = button_event();
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_checkbox(&event, &mut ctx);
    assert!(!matched);
    assert!(app.last_checkbox.is_none());
}

// ---------------------------------------------------------------------------
// Tests — selector dispatch (same signature)
// ---------------------------------------------------------------------------

#[test]
fn selector_dispatch_matches_matching_control() {
    let mut app = MyApp::new();
    let sender = node_id_from_ffi(42);
    let event = MessageEvent::new(
        sender,
        ButtonPressed {
            description: "save".into(),
            button_id: Some("save".into()),
        },
    );
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_save(&event, &mut ctx);
    assert!(matched);
    assert_eq!(app.save_count, 1);
}

#[test]
fn selector_dispatch_skips_message_without_control_identity() {
    // A message with no control id must NOT satisfy a `selector = "#save"`
    // handler. Mirrors Python where `@on(Message, selector)` matches against
    // `message.control` and a non-matching control skips the handler.
    let mut app = MyApp::new();
    let event = button_event(); // button_id: None
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_save(&event, &mut ctx);
    assert!(!matched);
    assert_eq!(app.save_count, 0);
}

#[test]
fn selector_dispatch_ignores_wrong_type() {
    let mut app = MyApp::new();
    let sender = node_id_from_ffi(42);
    let event = MessageEvent::new(sender, CheckboxChanged { checked: false });
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_save(&event, &mut ctx);
    assert!(!matched);
    assert_eq!(app.save_count, 0);
}

#[test]
fn selector_const_is_generated() {
    assert_eq!(MyApp::__ON_SELECTOR_HANDLE_SAVE, "#save");
}

/// Port of Python `test_on.py::test_on_button_pressed`: with two controls, a
/// selector-filtered handler fires ONLY for the message whose originating
/// control matches the selector, while a type-only handler fires for every
/// message of that type.
#[test]
fn selector_filters_between_two_controls() {
    let mut app = MyApp::new();
    let save_pressed = MessageEvent::new(
        node_id_from_ffi(10),
        ButtonPressed {
            description: "save".into(),
            button_id: Some("save".into()),
        },
    );
    let cancel_pressed = MessageEvent::new(
        node_id_from_ffi(11),
        ButtonPressed {
            description: "cancel".into(),
            button_id: Some("cancel".into()),
        },
    );
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    // Press #save: both the type-only and the "#save" handler fire.
    app.__on_dispatch_handle_button(&save_pressed, &mut ctx);
    app.__on_dispatch_handle_save(&save_pressed, &mut ctx);
    // Press #cancel: only the type-only handler fires.
    app.__on_dispatch_handle_button(&cancel_pressed, &mut ctx);
    let cancel_matched_save = app.__on_dispatch_handle_save(&cancel_pressed, &mut ctx);

    assert!(!cancel_matched_save, "#save handler must skip #cancel");
    assert_eq!(app.button_count, 2, "type-only handler fires for both");
    assert_eq!(app.save_count, 1, "selector handler fires only for #save");
}

// ---------------------------------------------------------------------------
// Tests — multiple dispatches accumulate
// ---------------------------------------------------------------------------

#[test]
fn multiple_dispatches_accumulate() {
    let mut app = MyApp::new();
    let event = button_event();
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    app.__on_dispatch_handle_button(&event, &mut ctx);
    app.__on_dispatch_handle_button(&event, &mut ctx);
    app.__on_dispatch_handle_button(&event, &mut ctx);

    assert_eq!(app.button_count, 3);
}
