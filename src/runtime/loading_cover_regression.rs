//! End-to-end regression tests for the `loading` reactive's cover-widget path.
//!
//! Python `Widget.loading = True` (via `_watch_loading` → `set_loading`) COVERS
//! the widget with a `LoadingIndicator` carrying the
//! `-textual-loading-indicator` class: the compositor renders the cover in
//! place of the widget's own visuals (`Widget._render_widget`) and skips the
//! widget's children while covered (`_compositor.py`). `loading = False`
//! uncovers it.
//!
//! textual-rs mirrors this with `WidgetNode::cover_widget`, set by
//! `WidgetTree::set_loading` and painted by `render_cover_widget` in the arena
//! render walk. The indicator animates on the frame tick (dot gradient phases),
//! delivered to cover widgets by both the live loop and
//! `headless_advance_ticks`.

use crate::runtime::App;
use crate::widgets::{AppRoot, Static};

struct LoadingApp;

impl crate::TextualApp for LoadingApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("payload-content").id("target"))
    }
}

/// Whether the rendered frame currently shows `needle` anywhere.
fn frame_shows(app: &App, needle: &str) -> bool {
    app.frame
        .as_plain_lines()
        .iter()
        .any(|line| line.contains(needle))
}

fn set_loading(pilot: &mut crate::runtime::pilot::Pilot, loading: bool) {
    pilot
        .app_mut()
        .query_mut("#target")
        .expect("#target exists")
        .set(None, None, None, Some(loading));
}

#[test]
fn loading_covers_widget_with_indicator_and_uncovers() {
    crate::run_test(LoadingApp, |pilot| {
        pilot.pause()?;
        assert!(
            frame_shows(pilot.app(), "payload-content"),
            "the widget's own content renders before loading is set"
        );
        assert!(
            !frame_shows(pilot.app(), "\u{25cf}"),
            "no loading dots before loading is set"
        );

        // Python: `widget.loading = True` — the LoadingIndicator covers the
        // widget's visuals (its own content no longer paints).
        set_loading(pilot, true);
        pilot.pause()?;
        assert!(
            frame_shows(pilot.app(), "\u{25cf}"),
            "loading=true paints the LoadingIndicator's dots over the widget"
        );
        assert!(
            !frame_shows(pilot.app(), "payload-content"),
            "the covered widget's own content is replaced while loading"
        );

        // Python: `widget.loading = False` — the cover is removed.
        set_loading(pilot, false);
        pilot.pause()?;
        assert!(
            frame_shows(pilot.app(), "payload-content"),
            "loading=false uncovers the widget's own content"
        );
        assert!(
            !frame_shows(pilot.app(), "\u{25cf}"),
            "loading=false removes the indicator"
        );
        Ok(())
    })
    .expect("headless run_test must succeed");
}

#[test]
fn loading_indicator_cover_animates_on_frame_ticks() {
    crate::run_test(LoadingApp, |pilot| {
        pilot.pause()?;
        set_loading(pilot, true);
        pilot.pause()?;
        assert!(frame_shows(pilot.app(), "\u{25cf}"));

        // The dot glyphs stay put; their gradient phases advance per tick, so
        // the styled frame fingerprint must change across ticks.
        let before = pilot.app().frame_fingerprint();
        pilot.advance_ticks(8)?;
        let after = pilot.app().frame_fingerprint();
        assert_ne!(
            before, after,
            "the cover LoadingIndicator animates on the frame tick"
        );
        Ok(())
    })
    .expect("headless run_test must succeed");
}
