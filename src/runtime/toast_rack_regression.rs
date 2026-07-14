//! End-to-end regression tests for the docked `ToastRack` notification path.
//!
//! `App::notify` records a notification in the store; the event loop syncs the
//! store into the docked `ToastRack` node, which mounts a real `Toast` child and
//! registers a widget-owned one-shot auto-dismiss timer on the (persistent) rack
//! node. `Pilot::advance_clock` drives those timers deterministically. When a
//! timer elapses (or a toast is clicked) a `NotificationExpired` message is
//! intercepted by the runtime, which removes the notification and re-syncs the
//! rack (a real node unmount).
//!
//! These tests pin two behaviours the design turns on:
//! 1. a toast auto-dismisses after *its own* timeout;
//! 2. posting a later toast never resets an earlier toast's countdown (timers
//!    live on the persistent rack node, keyed by notification id — not on the
//!    ephemeral `Toast` children that a full recompose rebuilds).

use std::time::Duration;

use crate::runtime::App;
use crate::widgets::{AppRoot, ToastSeverity};

struct NotifyApp;

impl crate::TextualApp for NotifyApp {
    fn compose(&mut self) -> AppRoot {
        // Empty screen; a system ToastRack is mounted on every screen tree by
        // the runtime, exactly as in a real app.
        AppRoot::new()
    }
}

/// Whether the rendered frame currently shows `needle` anywhere.
fn frame_shows(app: &App, needle: &str) -> bool {
    app.frame
        .as_plain_lines()
        .iter()
        .any(|line| line.contains(needle))
}

#[test]
fn toast_auto_dismisses_after_its_timeout() {
    crate::run_test(NotifyApp, |pilot| {
        pilot.pause()?;
        assert!(pilot.clock_is_manual(), "run_test uses the manual clock");

        pilot.app_mut().notify(
            "hello there",
            "",
            ToastSeverity::Information,
            Some(Duration::from_secs(5)),
        );
        pilot.pause()?;

        assert_eq!(
            pilot.app().notifications.len(),
            1,
            "notification is in the store after notify+sync"
        );
        assert!(
            frame_shows(pilot.app(), "hello there"),
            "toast text renders (real mounted Toast node), not a framebuffer blit"
        );

        // Advance past the 5s timeout: the rack-owned timer fires, posts
        // NotificationExpired, the runtime removes the notification and re-syncs.
        pilot.advance_clock(Duration::from_secs(6))?;

        assert_eq!(
            pilot.app().notifications.len(),
            0,
            "toast auto-dismissed after its timeout"
        );
        assert!(
            !frame_shows(pilot.app(), "hello there"),
            "dismissed toast node is unmounted and no longer painted"
        );
        Ok(())
    })
    .expect("headless run_test must succeed");
}

#[test]
fn later_toast_does_not_reset_earlier_toast_countdown() {
    crate::run_test(NotifyApp, |pilot| {
        pilot.pause()?;

        // Toast A: 5s timeout.
        pilot.app_mut().notify(
            "toast-A",
            "",
            ToastSeverity::Information,
            Some(Duration::from_secs(5)),
        );
        pilot.pause()?;
        assert_eq!(pilot.app().notifications.len(), 1);

        // 3s elapse for A.
        pilot.advance_clock(Duration::from_secs(3))?;
        assert_eq!(pilot.app().notifications.len(), 1, "A still alive at 3s");

        // Toast B: 5s timeout, posted while A has ~2s left. A full child
        // recompose rebuilds both Toast views — but A's timer lives on the
        // persistent rack node and is NOT reset.
        pilot.app_mut().notify(
            "toast-B",
            "",
            ToastSeverity::Warning,
            Some(Duration::from_secs(5)),
        );
        pilot.pause()?;
        assert_eq!(pilot.app().notifications.len(), 2, "A and B both live");
        assert!(frame_shows(pilot.app(), "toast-A"));
        assert!(frame_shows(pilot.app(), "toast-B"));

        // 3s more: A reaches 6s (expired), B reaches 3s (alive). If B had reset
        // A's timer, A would still be alive here — this is the reset-trap guard.
        pilot.advance_clock(Duration::from_secs(3))?;
        assert!(
            !frame_shows(pilot.app(), "toast-A"),
            "A dismissed on its own schedule (6s), independent of B"
        );
        assert!(
            frame_shows(pilot.app(), "toast-B"),
            "B still alive at 3s"
        );
        assert_eq!(pilot.app().notifications.len(), 1);

        // 3s more: B reaches 6s and dismisses.
        pilot.advance_clock(Duration::from_secs(3))?;
        assert_eq!(pilot.app().notifications.len(), 0, "B dismissed at its 6s");
        Ok(())
    })
    .expect("headless run_test must succeed");
}

/// A minimal pushed (modal) screen with its own tree.
struct BlankScreen;
impl crate::screen::Screen for BlankScreen {
    fn compose(&self) -> Box<dyn crate::widgets::Widget> {
        Box::new(AppRoot::new())
    }
}

/// Number of live toasts held by the `ToastRack` in `tree` (every screen tree
/// mounts exactly one system rack via `App::mount_system_toast_rack`).
fn rack_len(tree: &crate::widget_tree::WidgetTree) -> usize {
    let root = tree.root().expect("tree has a root");
    let rack_id = tree
        .walk_depth_first(root)
        .into_iter()
        .find(|&id| {
            tree.get(id)
                .map(|node| node.widget.style_type() == "ToastRack")
                .unwrap_or(false)
        })
        .expect("every screen tree mounts a system ToastRack");
    let widget: &dyn crate::widgets::Widget = tree.get(rack_id).unwrap().widget.as_ref();
    (widget as &dyn std::any::Any)
        .downcast_ref::<crate::widgets::ToastRack>()
        .expect("ToastRack node downcasts to ToastRack")
        .len()
}

/// Per-screen racks (Python parity): a toast posted while a modal screen is
/// active mounts on THAT screen's own `ToastRack` — rendered above the modal —
/// not on the occluded base tree's rack, and it auto-dismisses on the modal's
/// rack. Popping back leaves the base screen consistent (a later toast mounts
/// on the base rack again).
#[test]
fn toast_over_modal_mounts_on_modal_rack_and_dismisses() {
    crate::run_test(NotifyApp, |pilot| {
        pilot.pause()?;
        pilot.app_mut().push_screen(Box::new(BlankScreen));
        pilot.pause()?;

        pilot.app_mut().notify(
            "over-the-modal",
            "",
            ToastSeverity::Error,
            Some(Duration::from_secs(5)),
        );
        pilot.pause()?;

        assert_eq!(pilot.app().notifications.len(), 1);
        // The MODAL screen's rack holds the toast; the base rack does not.
        {
            let app = pilot.app();
            let screen_tree = &app.screen_stack.top().expect("pushed screen").widget_tree;
            assert_eq!(rack_len(screen_tree), 1, "toast mounts on the modal's rack");
            let base_tree = app.widget_tree.as_ref().expect("base tree");
            assert_eq!(rack_len(base_tree), 0, "base rack stays empty");
        }
        assert!(
            frame_shows(pilot.app(), "over-the-modal"),
            "toast renders above the active modal screen"
        );

        // The rack-owned timer lives on the modal's rack node and dismisses there.
        pilot.advance_clock(Duration::from_secs(6))?;
        assert_eq!(pilot.app().notifications.len(), 0, "auto-dismissed over the modal");
        {
            let app = pilot.app();
            let screen_tree = &app.screen_stack.top().expect("pushed screen").widget_tree;
            assert_eq!(rack_len(screen_tree), 0, "modal rack empties on dismiss");
        }
        assert!(!frame_shows(pilot.app(), "over-the-modal"));

        // Back on the base screen the plain path still works.
        pilot.app_mut().action_pop_screen();
        pilot.pause()?;
        pilot.app_mut().notify(
            "back-on-base",
            "",
            ToastSeverity::Information,
            Some(Duration::from_secs(5)),
        );
        pilot.pause()?;
        assert_eq!(pilot.app().notifications.len(), 1);
        assert_eq!(
            rack_len(pilot.app().widget_tree.as_ref().expect("base tree")),
            1,
            "base rack works again after pop"
        );
        assert!(frame_shows(pilot.app(), "back-on-base"));
        Ok(())
    })
    .expect("headless run_test must succeed");
}

/// Live notifications follow screen transitions (Python `ScreenResume` →
/// `App._refresh_notifications`): a toast posted on the base screen re-shows on
/// a subsequently pushed screen's rack, and re-shows on the base rack after pop.
#[test]
fn notifications_follow_screen_transitions() {
    crate::run_test(NotifyApp, |pilot| {
        pilot.pause()?;
        pilot.app_mut().notify(
            "sticky-toast",
            "",
            ToastSeverity::Warning,
            Some(Duration::from_secs(60)),
        );
        pilot.pause()?;
        assert_eq!(
            rack_len(pilot.app().widget_tree.as_ref().expect("base tree")),
            1,
            "toast mounts on the base rack first"
        );

        // Push: the still-live notification re-syncs onto the new screen's rack.
        pilot.app_mut().push_screen(Box::new(BlankScreen));
        pilot.pause()?;
        {
            let app = pilot.app();
            let screen_tree = &app.screen_stack.top().expect("pushed screen").widget_tree;
            assert_eq!(
                rack_len(screen_tree),
                1,
                "live toast re-shows on the pushed screen's rack"
            );
        }
        assert!(frame_shows(pilot.app(), "sticky-toast"));

        // Pop: the base rack still holds it and it renders again.
        pilot.app_mut().action_pop_screen();
        pilot.pause()?;
        assert_eq!(
            rack_len(pilot.app().widget_tree.as_ref().expect("base tree")),
            1,
            "toast still lives on the base rack after pop"
        );
        assert!(frame_shows(pilot.app(), "sticky-toast"));
        Ok(())
    })
    .expect("headless run_test must succeed");
}
