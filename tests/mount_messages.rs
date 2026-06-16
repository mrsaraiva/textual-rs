//! Regression: arena widgets can post a message at mount time.
//!
//! Python Textual widgets can `post_message(..)` from `on_mount`. In the arena
//! runtime `on_mount(&mut self)` has no `EventCtx`, so widgets stage mount-time
//! messages via `Widget::take_pending_mount_messages`. The runtime drains that
//! once right after the node is mounted and routes each message through the
//! normal message bus with the mounted node as sender/control.
//!
//! These tests exercise that contract end-to-end at the message-bus level
//! (the same drain + route the runtime performs in
//! `App::drain_pending_mount_messages`), plus the concrete `Select` case.

use std::sync::{Arc, Mutex};

use rich_rs::{Console, ConsoleOptions};
use textual::event::EventCtx;
use textual::message::{Message, MessageEvent, SelectChanged};
use textual::node_id::NodeId;
use textual::runtime::dispatch_message_queue_tree;
use textual::widget_tree::WidgetTree;
use textual::widgets::{Select, Widget};

// ---------------------------------------------------------------------------
// A custom mount-time message + a minimal widget that stages it at mount.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MountedPing {
    n: u32,
}
textual::impl_message!(MountedPing);

/// Minimal arena widget that wants to post `MountedPing` at mount time.
struct MountPoster {
    n: u32,
}

impl Widget for MountPoster {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }

    fn take_pending_mount_messages(&mut self) -> Vec<Box<dyn Message>> {
        vec![Box::new(MountedPing { n: self.n })]
    }
}

/// Records every message it sees (used as the listening parent / app stand-in).
struct Recorder {
    seen: Arc<Mutex<Vec<(NodeId, Option<NodeId>, Option<u32>, Option<SelectChanged>)>>>,
}

impl Widget for Recorder {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }

    fn on_message(&mut self, message: &MessageEvent, _ctx: &mut EventCtx) {
        let ping_n = message.downcast_ref::<MountedPing>().map(|p| p.n);
        let select_changed = message.downcast_ref::<SelectChanged>().cloned();
        if ping_n.is_some() || select_changed.is_some() {
            self.seen.lock().unwrap().push((
                message.sender,
                message.control,
                ping_n,
                select_changed,
            ));
        }
    }
}

/// Build the mount-time `MessageEvent`s a widget stages, exactly as the runtime
/// does after mounting `node` (sender = control = node).
fn staged_mount_events(widget: &mut dyn Widget, node: NodeId) -> Vec<MessageEvent> {
    widget
        .take_pending_mount_messages()
        .into_iter()
        .map(|payload| MessageEvent::from_boxed(node, payload).with_control(node))
        .collect()
}

#[test]
fn arena_widget_posts_message_at_mount() {
    let seen = Arc::new(Mutex::new(Vec::new()));

    let mut tree = WidgetTree::new();
    let root = tree.set_root(Box::new(Recorder { seen: seen.clone() }));
    let child = tree.mount(root, Box::new(MountPoster { n: 7 }));

    // Reconstruct the staged mount messages for the mounted child and route
    // them through the message bus, mirroring the runtime drain-at-mount.
    let mut poster = MountPoster { n: 7 };
    let staged = staged_mount_events(&mut poster, child);
    assert_eq!(staged.len(), 1, "MountPoster stages exactly one message");

    dispatch_message_queue_tree(&mut tree, staged);

    let entries = seen.lock().unwrap();
    assert_eq!(entries.len(), 1, "recorder must receive the mount message");
    let (sender, control, ping_n, _) = &entries[0];
    assert_eq!(*ping_n, Some(7), "received message is MountedPing with n=7");
    assert_eq!(*sender, child, "sender is the mounted node");
    assert_eq!(*control, Some(child), "control is the mounted node");
}

#[test]
fn select_allow_blank_false_posts_select_changed_at_mount() {
    // Default allow_blank=false auto-selects the first option, so Select stages
    // a SelectChanged for it at mount (Python parity: _watch_value posts Changed
    // for the initial value).
    let mut select: Select<i32> = Select::new(
        vec![
            ("Alpha".to_string(), 1),
            ("Beta".to_string(), 2),
            ("Gamma".to_string(), 3),
        ],
        "Pick one...",
    )
    .with_allow_blank(false);

    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut tree = WidgetTree::new();
    let root = tree.set_root(Box::new(Recorder { seen: seen.clone() }));
    let node = NodeId::default();

    let staged = staged_mount_events(&mut select, node);
    assert_eq!(staged.len(), 1, "Select(allow_blank=false) stages one message");

    // Route via the bus to the recorder root.
    let _ = root;
    dispatch_message_queue_tree(&mut tree, staged);

    let entries = seen.lock().unwrap();
    assert_eq!(entries.len(), 1, "recorder receives the mount SelectChanged");
    let changed = entries[0]
        .3
        .as_ref()
        .expect("payload is SelectChanged");
    assert_eq!(changed.index, 0, "first option auto-selected");
    assert_eq!(changed.label, "Alpha");
}

#[test]
fn select_allow_blank_true_posts_nothing_at_mount() {
    // allow_blank=true starts with no selection → no mount-time message.
    let mut select: Select<i32> = Select::new(
        vec![("Alpha".to_string(), 1), ("Beta".to_string(), 2)],
        "Pick one...",
    )
    .with_allow_blank(true);

    let staged = staged_mount_events(&mut select, NodeId::default());
    assert!(
        staged.is_empty(),
        "blank Select stages no mount-time message"
    );
}
