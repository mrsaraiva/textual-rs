//! Regression: arena widgets can post a message at mount time.
//!
//! Python Textual widgets can `post_message(..)` from `on_mount`. In the arena
//! runtime `on_mount(&mut self, ctx)` receives a `WidgetCtx`, so widgets post
//! mount-time messages directly via `ctx.post_message(..)`. When `on_mount`
//! fires during tree build (`WidgetTree::fire_mount_callbacks`, where no `App`
//! exists to absorb the message), the post is routed through the deferred
//! command queue as a `PostMessage` command and bubbled by the first shared
//! flush. RA2.3 retired the former `take_pending_mount_messages` staging hook.
//!
//! These tests build a real tree (which fires `on_mount`) and drain the posted
//! mount-time messages from the command queue, plus the concrete `Select` case.

use rich_rs::{Console, ConsoleOptions};
use textual::message::SelectChanged;
use textual::node_id::NodeId;
use textual::runtime::{build_widget_tree_from_root, drain_mount_posts_for_test};
use textual::widgets::{Container, Select, Widget};

// ---------------------------------------------------------------------------
// A custom mount-time message + a minimal widget that posts it from on_mount.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MountedPing {
    n: u32,
}
textual::impl_message!(MountedPing);

/// Minimal arena widget that posts `MountedPing` from `on_mount`.
struct MountPoster {
    n: u32,
}

impl Widget for MountPoster {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "MountPoster"
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn on_mount(&mut self, ctx: &mut textual::event::WidgetCtx) {
        ctx.post_message(MountedPing { n: self.n });
    }
}

#[test]
fn arena_widget_posts_message_at_mount() {
    // Clear any stale commands left on this thread's queue by a prior test.
    let _ = drain_mount_posts_for_test();

    let mut root = Container::new().with_child(MountPoster { n: 7 });
    // Building the tree fires `on_mount`, which posts through the command queue.
    let _tree = build_widget_tree_from_root(&mut root).expect("tree built");

    let posts = drain_mount_posts_for_test();
    assert_eq!(posts.len(), 1, "MountPoster posts exactly one mount message");
    let ping = posts[0]
        .downcast_ref::<MountedPing>()
        .expect("posted message is MountedPing");
    assert_eq!(ping.n, 7, "received MountedPing with n=7");
    // The message bubbles from the mounted node (its sender), not the root.
    assert_ne!(posts[0].sender, NodeId::default());
}

#[test]
fn select_allow_blank_false_posts_select_changed_at_mount() {
    // Default allow_blank=false auto-selects the first option, so Select posts
    // a SelectChanged for it at mount (Python parity: _watch_value posts Changed
    // for the initial value).
    let _ = drain_mount_posts_for_test();

    let select: Select<i32> = Select::new(
        vec![
            ("Alpha".to_string(), 1),
            ("Beta".to_string(), 2),
            ("Gamma".to_string(), 3),
        ],
        "Pick one...",
    )
    .with_allow_blank(false);

    let mut root = Container::new().with_child(select);
    let _tree = build_widget_tree_from_root(&mut root).expect("tree built");

    let posts = drain_mount_posts_for_test();
    let changed = posts
        .iter()
        .find_map(|m| m.downcast_ref::<SelectChanged>())
        .expect("mount posts include SelectChanged");
    assert_eq!(changed.index, 0, "first option auto-selected");
    assert_eq!(changed.label, "Alpha");
}

#[test]
fn select_allow_blank_true_posts_nothing_at_mount() {
    // allow_blank=true starts with no selection → no mount-time message.
    let _ = drain_mount_posts_for_test();

    let select: Select<i32> = Select::new(
        vec![("Alpha".to_string(), 1), ("Beta".to_string(), 2)],
        "Pick one...",
    )
    .with_allow_blank(true);

    let mut root = Container::new().with_child(select);
    let _tree = build_widget_tree_from_root(&mut root).expect("tree built");

    let posts = drain_mount_posts_for_test();
    assert!(
        posts.iter().all(|m| m.downcast_ref::<SelectChanged>().is_none()),
        "blank Select posts no mount-time SelectChanged"
    );
}
