//! RA-1 acceptance test: third-party open-message dispatch.
//!
//! Defines custom widget + message types entirely outside `src/` and verifies they
//! dispatch identically to built-ins through `dispatch_message_queue_tree`.
//!
//! Tests correspond to T5 in the spec test plan (SPEC-RA1-open-messages.md).

use std::sync::{Arc, Mutex};

use rich_rs::{Console, ConsoleOptions};
use textual::event::{EventCtx, WidgetCtx};
use textual::message::{ButtonPressed, MessageEvent};
use textual::message_handlers::MessageHandlers;
use textual::node_id::node_id_from_ffi;
use textual::on;
use textual::runtime::dispatch_message_queue_tree;
use textual::widget_tree::WidgetTree;
use textual::widgets::Widget;

// ---------------------------------------------------------------------------
// Custom message types — defined entirely outside src/
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Ping {
    n: u32,
}
textual::impl_message!(Ping);

#[derive(Debug, Clone)]
struct CursorEcho {
    #[allow(dead_code)]
    pos: usize,
}
textual::impl_message!(CursorEcho, replaceable);

// A second "replaceable" type distinct from CursorEcho, to test TypeId
// refinement (different types with set_replaceable must NOT coalesce).
#[derive(Debug, Clone)]
struct AltEcho {
    #[allow(dead_code)]
    pos: usize,
}
textual::impl_message!(AltEcho, replaceable);

// ---------------------------------------------------------------------------
// Recorded message entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Received {
    is_ping: bool,
    is_cursor_echo: bool,
    sender: textual::node_id::NodeId,
    control: Option<textual::node_id::NodeId>,
}

// ---------------------------------------------------------------------------
// Recorder widget — records every Ping / CursorEcho it receives in on_message
// ---------------------------------------------------------------------------

struct Recorder {
    log: Arc<Mutex<Vec<Received>>>,
    stop_on_ping: bool,
}

impl Recorder {
    fn new(log: Arc<Mutex<Vec<Received>>>) -> Self {
        Self {
            log,
            stop_on_ping: false,
        }
    }

    fn stopping(log: Arc<Mutex<Vec<Received>>>) -> Self {
        Self {
            log,
            stop_on_ping: true,
        }
    }
}

impl Widget for Recorder {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
        if message.is::<Ping>() || message.is::<CursorEcho>() || message.is::<AltEcho>() {
            self.log.lock().unwrap().push(Received {
                is_ping: message.is::<Ping>(),
                is_cursor_echo: message.is::<CursorEcho>(),
                sender: message.sender,
                control: message.control,
            });
            if self.stop_on_ping && message.is::<Ping>() {
                ctx.set_handled();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// T5-1: Bubble — Ping posted from child reaches child then parent (in order)
// ---------------------------------------------------------------------------

#[test]
fn t5_1_ping_bubbles_from_child_to_parent() {
    let log = Arc::new(Mutex::new(Vec::<Received>::new()));

    let mut tree = WidgetTree::new();
    let root_id = tree.set_root(Box::new(Recorder::new(log.clone())));
    let child_id = tree.mount(root_id, Box::new(Recorder::new(log.clone())));

    let messages = vec![MessageEvent::new(child_id, Ping { n: 1 })];
    dispatch_message_queue_tree(&mut tree, messages);

    let entries = log.lock().unwrap();
    assert_eq!(
        entries.len(),
        2,
        "both child and parent should see the Ping"
    );
    assert!(entries[0].is_ping);
    assert!(entries[1].is_ping);
    // Bubble order: child first, then parent.
    assert_eq!(entries[0].sender, child_id, "child sees it first");
    assert_eq!(entries[1].sender, child_id, "parent sees same sender");
}

// ---------------------------------------------------------------------------
// T5-2: Stop — child calls ctx.set_handled(), parent never sees the message
// ---------------------------------------------------------------------------

#[test]
fn t5_2_stop_prevents_parent_from_seeing_ping() {
    let child_log = Arc::new(Mutex::new(Vec::<Received>::new()));
    let parent_log = Arc::new(Mutex::new(Vec::<Received>::new()));

    let mut tree = WidgetTree::new();
    // Root is a non-stopping Recorder; child is a stopping Recorder.
    let root_id = tree.set_root(Box::new(Recorder::new(parent_log.clone())));
    let child_id = tree.mount(root_id, Box::new(Recorder::stopping(child_log.clone())));

    let messages = vec![MessageEvent::new(child_id, Ping { n: 2 })];
    let outcome = dispatch_message_queue_tree(&mut tree, messages);

    assert!(outcome.handled, "child stopped the message → handled");
    assert_eq!(child_log.lock().unwrap().len(), 1, "child sees it");
    assert_eq!(
        parent_log.lock().unwrap().len(),
        0,
        "parent must NOT see it after stop"
    );
}

// ---------------------------------------------------------------------------
// T5-3: can_replace — CursorEcho coalescing semantics
// ---------------------------------------------------------------------------

#[test]
fn t5_3_cursor_echo_same_sender_coalesces() {
    let log = Arc::new(Mutex::new(Vec::<Received>::new()));

    let mut tree = WidgetTree::new();
    let root_id = tree.set_root(Box::new(Recorder::new(log.clone())));

    let sender = root_id;
    let messages = vec![
        MessageEvent::new(sender, CursorEcho { pos: 10 }),
        MessageEvent::new(sender, CursorEcho { pos: 20 }),
    ];
    dispatch_message_queue_tree(&mut tree, messages);

    let entries = log.lock().unwrap();
    assert_eq!(
        entries.len(),
        1,
        "two CursorEcho from the same sender must coalesce to one delivery"
    );
    assert!(entries[0].is_cursor_echo);
}

#[test]
fn t5_3_cursor_echo_different_senders_both_deliver() {
    let log = Arc::new(Mutex::new(Vec::<Received>::new()));

    let mut tree = WidgetTree::new();
    let root_id = tree.set_root(Box::new(Recorder::new(log.clone())));

    // Two distinct senders (only root_id is in tree; use out-of-tree ffi id for sender_b
    // to keep the test simple — the broadcast fallback ensures it still reaches the root).
    let sender_a = root_id;
    let sender_b = node_id_from_ffi(9_999);
    let messages = vec![
        MessageEvent::new(sender_a, CursorEcho { pos: 10 }),
        MessageEvent::new(sender_b, CursorEcho { pos: 20 }),
    ];
    dispatch_message_queue_tree(&mut tree, messages);

    let entries = log.lock().unwrap();
    assert_eq!(
        entries.len(),
        2,
        "CursorEcho from different senders must both deliver"
    );
}

#[test]
fn t5_3_ping_never_coalesces() {
    let log = Arc::new(Mutex::new(Vec::<Received>::new()));

    let mut tree = WidgetTree::new();
    let root_id = tree.set_root(Box::new(Recorder::new(log.clone())));

    let sender = root_id;
    let messages = vec![
        MessageEvent::new(sender, Ping { n: 1 }),
        MessageEvent::new(sender, Ping { n: 2 }),
    ];
    dispatch_message_queue_tree(&mut tree, messages);

    let entries = log.lock().unwrap();
    assert_eq!(
        entries.len(),
        2,
        "Ping is not replaceable — both must deliver"
    );
}

// ---------------------------------------------------------------------------
// T5-4: Control — control survives bubbling; with_control override is observed
// ---------------------------------------------------------------------------

#[test]
fn t5_4_control_survives_bubbling() {
    let log = Arc::new(Mutex::new(Vec::<Received>::new()));

    let mut tree = WidgetTree::new();
    let root_id = tree.set_root(Box::new(Recorder::new(log.clone())));
    let child_id = tree.mount(root_id, Box::new(Recorder::new(log.clone())));

    // Default: control is promoted to sender.
    let messages = vec![MessageEvent::new(child_id, Ping { n: 3 })];
    dispatch_message_queue_tree(&mut tree, messages);

    let entries = log.lock().unwrap();
    for entry in entries.iter() {
        assert_eq!(
            entry.control,
            Some(child_id),
            "control must be the sender (child) for all nodes on the bubble path"
        );
    }
}

#[test]
fn t5_4_with_control_override_observed() {
    let log = Arc::new(Mutex::new(Vec::<Received>::new()));
    let explicit_control = node_id_from_ffi(77);

    let mut tree = WidgetTree::new();
    let root_id = tree.set_root(Box::new(Recorder::new(log.clone())));

    let messages = vec![MessageEvent::new(root_id, Ping { n: 4 }).with_control(explicit_control)];
    dispatch_message_queue_tree(&mut tree, messages);

    let entries = log.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].control,
        Some(explicit_control),
        "explicit with_control must override the default sender promotion"
    );
}

// ---------------------------------------------------------------------------
// T5-5: Built-in indifference — ButtonPressed and Ping queued together use
//        the identical dispatch path (no runtime special-casing for Ping)
// ---------------------------------------------------------------------------

#[test]
fn t5_5_builtin_and_custom_coexist_in_same_queue() {
    let log = Arc::new(Mutex::new(Vec::<Received>::new()));

    // A recorder that also counts ButtonPressed to prove both go through the
    // same dispatch mechanism.
    struct MixedRecorder {
        ping_log: Arc<Mutex<Vec<Received>>>,
        button_count: Arc<Mutex<u32>>,
    }
    impl Widget for MixedRecorder {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }
        fn on_message(&mut self, message: &MessageEvent, _ctx: &mut textual::event::WidgetCtx) {
            if message.is::<Ping>() {
                self.ping_log.lock().unwrap().push(Received {
                    is_ping: true,
                    is_cursor_echo: false,
                    sender: message.sender,
                    control: message.control,
                });
            }
            if message.is::<ButtonPressed>() {
                *self.button_count.lock().unwrap() += 1;
            }
        }
    }

    let button_count = Arc::new(Mutex::new(0u32));

    let mut tree = WidgetTree::new();
    let root_id = tree.set_root(Box::new(MixedRecorder {
        ping_log: log.clone(),
        button_count: button_count.clone(),
    }));

    let messages = vec![
        MessageEvent::new(
            root_id,
            ButtonPressed {
                description: "ok".into(),
                button_id: None,
            },
        ),
        MessageEvent::new(root_id, Ping { n: 5 }),
    ];
    dispatch_message_queue_tree(&mut tree, messages);

    assert_eq!(*button_count.lock().unwrap(), 1, "ButtonPressed delivered");
    assert_eq!(log.lock().unwrap().len(), 1, "Ping delivered");
}

// ---------------------------------------------------------------------------
// T5-6: TypeId refinement regression — two different custom types both marked
//        set_replaceable(true) must NOT coalesce with each other
// ---------------------------------------------------------------------------

#[test]
fn t5_6_different_replaceable_types_do_not_coalesce() {
    // Both CursorEcho and AltEcho are `replaceable`; different concrete types
    // must not coalesce even when set_replaceable is forced on both envelopes.
    //
    // The coalescer uses payload_type_id() comparison, so two distinct types
    // cannot coalesce even if both are registered with the `replaceable` arm.
    let sender = node_id_from_ffi(1);

    let log = Arc::new(Mutex::new(Vec::<Received>::new()));

    let mut tree = WidgetTree::new();
    let _root_id = tree.set_root(Box::new(Recorder::new(log.clone())));

    // Same sender, but different types: CursorEcho and AltEcho.
    let messages = vec![
        MessageEvent::new(sender, CursorEcho { pos: 1 }),
        MessageEvent::new(sender, AltEcho { pos: 2 }),
    ];
    dispatch_message_queue_tree(&mut tree, messages);

    let entries = log.lock().unwrap();
    // CursorEcho and AltEcho have different TypeIds → they must NOT coalesce.
    assert_eq!(
        entries.len(),
        2,
        "CursorEcho and AltEcho are distinct types — must NOT coalesce even though both are replaceable"
    );
}

// ---------------------------------------------------------------------------
// T5-7: Typed registration — MessageHandlers<State> dispatches Ping, ignores
//        CursorEcho
// ---------------------------------------------------------------------------

#[test]
fn t5_7_message_handlers_dispatches_ping_ignores_cursor_echo() {
    struct State {
        ping_count: u32,
        cursor_echo_count: u32,
    }

    let mut handlers: MessageHandlers<State> = MessageHandlers::new();
    handlers.on::<Ping, _>(|state, msg, _mctx, _ctx| {
        state.ping_count += msg.n;
    });
    // No handler registered for CursorEcho.

    let mut state = State {
        ping_count: 0,
        cursor_echo_count: 0,
    };
    let sender = node_id_from_ffi(1);
    let mut ctx = EventCtx::default();

    let ping_event = MessageEvent::new(sender, Ping { n: 7 });
    let ran = handlers.dispatch(&mut state, &ping_event, &mut ctx);
    assert!(ran, "Ping handler must run");
    assert_eq!(state.ping_count, 7);

    let echo_event = MessageEvent::new(sender, CursorEcho { pos: 10 });
    let ran = handlers.dispatch(&mut state, &echo_event, &mut ctx);
    assert!(!ran, "no CursorEcho handler registered — must return false");
    assert_eq!(state.cursor_echo_count, 0);
}

// ---------------------------------------------------------------------------
// T5-8: #[on] for third-party types — the retargeted macro dispatches via
//        downcast for a type defined outside src/
// ---------------------------------------------------------------------------

struct MyApp {
    ping_total: u32,
}

impl MyApp {
    fn new() -> Self {
        Self { ping_total: 0 }
    }

    #[on(Ping)]
    fn handle_ping(&mut self, msg: &Ping, ctx: &mut WidgetCtx) {
        let _ = ctx;
        self.ping_total += msg.n;
    }
}

#[test]
fn t5_8_on_macro_dispatches_custom_message_type() {
    let mut app = MyApp::new();
    let sender = node_id_from_ffi(1);
    let event = MessageEvent::new(sender, Ping { n: 3 });
    let mut ectx = EventCtx::default();
    let mut ctx = WidgetCtx::__from_dispatch(sender, &mut ectx);

    let matched = app.__on_dispatch_handle_ping(&event, &mut ctx);
    assert!(matched, "#[on(Ping)] dispatcher must match a Ping event");
    assert_eq!(app.ping_total, 3);

    // Wrong type must not match.
    let wrong_event = MessageEvent::new(sender, CursorEcho { pos: 99 });
    let matched = app.__on_dispatch_handle_ping(&wrong_event, &mut ctx);
    assert!(!matched, "#[on(Ping)] dispatcher must not match CursorEcho");
    assert_eq!(app.ping_total, 3, "ping_total unchanged after non-match");
}
