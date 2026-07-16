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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rich_rs::{Console, ConsoleOptions};
use textual::message::SelectChanged;
use textual::node_id::NodeId;
use textual::runtime::{
    build_widget_tree_from_root, drain_absorb_outcomes_for_test, drain_mount_posts_for_test,
};
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
    // allow_blank=false auto-selects the first option, so Select posts a
    // SelectChanged for it at mount (Python parity: _watch_value posts Changed
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

// ---------------------------------------------------------------------------
// Gap 6: build-time `on_mount` side effects beyond messages must NOT drop.
// A worker requested from `on_mount` during the initial tree build rides the
// per-node `AbsorbOutcome` bundle on the deferred command queue (the Python
// `on_mount` + `@work` startup idiom).
// ---------------------------------------------------------------------------

/// Minimal arena widget whose `on_mount` requests a closure-backed worker and
/// (optionally) posts a message, mirroring the canonical Python startup idiom.
struct MountWorkRequester {
    ran: Arc<AtomicBool>,
    also_post: bool,
}

impl Widget for MountWorkRequester {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "MountWorkRequester"
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn on_mount(&mut self, ctx: &mut textual::event::WidgetCtx) {
        let ran = Arc::clone(&self.ran);
        ctx.request_worker_task(Some("mount-scan"), move |_cancel| {
            ran.store(true, Ordering::SeqCst);
            Ok(())
        });
        if self.also_post {
            ctx.post_message(MountedPing { n: 42 });
        }
    }
}

#[test]
fn mount_worker_request_rides_one_absorb_outcome_bundle() {
    // Clear any stale commands left on this thread's queue by a prior test.
    let _ = drain_absorb_outcomes_for_test();

    let ran = Arc::new(AtomicBool::new(false));
    let mut root = Container::new().with_child(MountWorkRequester {
        ran: Arc::clone(&ran),
        also_post: false,
    });
    let _tree = build_widget_tree_from_root(&mut root).expect("tree built");

    let bundles = drain_absorb_outcomes_for_test();
    assert_eq!(
        bundles.len(),
        1,
        "one effectful on_mount enqueues exactly one AbsorbOutcome bundle"
    );
    let (node, outcome) = &bundles[0];
    assert_ne!(*node, NodeId::default(), "bundle is labeled with its node");
    assert_eq!(
        outcome.worker_requests.len(),
        1,
        "the mount-time worker request survives the build (was dropped pre-fix)"
    );
    assert_eq!(outcome.worker_requests[0].name.as_deref(), Some("mount-scan"));
    assert_eq!(
        outcome.worker_requests[0].owner, *node,
        "worker owner is the mounted node"
    );
    assert!(
        !ran.load(Ordering::SeqCst),
        "the worker has only been REQUESTED at build time, not spawned"
    );
}

#[test]
fn mount_message_and_worker_share_one_bundle_in_mount_order() {
    let _ = drain_absorb_outcomes_for_test();

    let ran = Arc::new(AtomicBool::new(false));
    let mut root = Container::new().with_child(MountWorkRequester {
        ran: Arc::clone(&ran),
        also_post: true,
    });
    let _tree = build_widget_tree_from_root(&mut root).expect("tree built");

    let bundles = drain_absorb_outcomes_for_test();
    assert_eq!(bundles.len(), 1, "message + worker ride the SAME bundle");
    let (node, outcome) = &bundles[0];
    assert_eq!(outcome.worker_requests.len(), 1);
    assert_eq!(outcome.messages.len(), 1);
    let ping = outcome.messages[0]
        .downcast_ref::<MountedPing>()
        .expect("bundled message is MountedPing");
    assert_eq!(ping.n, 42);
    assert_eq!(
        outcome.messages[0].sender, *node,
        "bundled message keeps its sender (PostUp semantics at the flush)"
    );
}
