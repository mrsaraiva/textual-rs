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
        // Empty screen; the ToastRack is injected as a system child of the app
        // root, exactly as in a real app.
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

/// A minimal pushed screen (its own tree). Used to prove that notifying while a
/// screen is active degrades gracefully: the base-tree rack is synced (no panic,
/// no wrong-z bleed) even though the toast is not shown over the pushed screen
/// (per-screen racks are a 1.x follow-up).
struct BlankScreen;
impl crate::screen::Screen for BlankScreen {
    fn compose(&self) -> Box<dyn crate::widgets::Widget> {
        Box::new(AppRoot::new())
    }
}

#[test]
fn notify_while_screen_pushed_degrades_gracefully() {
    crate::run_test(NotifyApp, |pilot| {
        pilot.pause()?;
        pilot.app_mut().push_screen(Box::new(BlankScreen));
        pilot.pause()?;

        // Notify while the pushed screen is the active tree. Must not panic; the
        // base-tree rack is targeted (behind the pushed screen).
        pilot.app_mut().notify(
            "under-a-screen",
            "",
            ToastSeverity::Error,
            Some(Duration::from_secs(5)),
        );
        pilot.pause()?;

        // No panic, the app still renders a frame, and the notification is in the
        // store (buffered on the base rack).
        assert!(pilot.app().frame.height > 0, "app still renders a frame");
        assert_eq!(pilot.app().notifications.len(), 1);

        // Popping back to the base screen and pumping keeps things consistent
        // (still no panic).
        pilot.app_mut().action_pop_screen();
        pilot.pause()?;
        Ok(())
    })
    .expect("headless run_test must succeed");
}
