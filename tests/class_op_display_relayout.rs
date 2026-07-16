//! Regression: a runtime class op (`query_mut().add_class(..)` and friends)
//! whose CSS rule flips a layout-affecting property (here `display`) must
//! trigger a relayout on its own, without the app author also calling
//! `request_layout()`.
//!
//! Python parity: `DOMNode.add_class`/`remove_class`/`toggle_class` funnel into
//! `_update_styles()`, which re-applies the stylesheet and refreshes with
//! `layout=True` when layout-affecting rules changed (`dom.py`). Before this
//! fix, the dispatch-path class-op sites (EventCtx/WidgetCtx/reactive/commands)
//! already requested layout, but the app-level `DomQueryMut` class helpers in
//! `src/runtime/mod.rs` mutated the tree's class sets with NO invalidation at
//! all, so a widget whose `.visible` class sets `display: block` stayed
//! invisible until an unrelated relayout happened.

use textual::compose;
use textual::prelude::*;

const CSS: &str = r##"
#panel {
    display: none;
    height: 3;
}

#panel.visible {
    display: block;
}
"##;

struct ClassDisplayApp;

impl TextualApp for ClassDisplayApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(compose![
            Static::new("filler").id("filler"),
            Static::new("panel").id("panel"),
        ])
    }
    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }
}

fn panel_is_laid_out(pilot: &textual::runtime::Pilot) -> bool {
    let panel = pilot.app().query_one("#panel").unwrap();
    match pilot.app().node_screen_rect(panel) {
        Some((_, _, w, h)) => w > 0 && h > 0,
        None => false,
    }
}

#[test]
fn add_class_flipping_display_relayouts_without_explicit_request() {
    ClassDisplayApp.run_test(|pilot| {
        assert!(
            !panel_is_laid_out(pilot),
            "precondition: #panel starts display: none (no layout rect)"
        );

        // Runtime class mutation, NO explicit request_layout() afterwards.
        pilot.app_mut().query_mut("#panel").unwrap().add_class("visible");
        pilot.pause()?;

        assert!(
            panel_is_laid_out(pilot),
            "add_class(\"visible\") flips display: none -> block; the class op \
             must relayout on its own (no explicit request_layout())"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn remove_class_flipping_display_relayouts_without_explicit_request() {
    ClassDisplayApp.run_test(|pilot| {
        pilot.app_mut().query_mut("#panel").unwrap().add_class("visible");
        pilot.pause()?;
        assert!(panel_is_laid_out(pilot), "precondition: panel shown");

        pilot
            .app_mut()
            .query_mut("#panel")
            .unwrap()
            .remove_class("visible");
        pilot.pause()?;

        assert!(
            !panel_is_laid_out(pilot),
            "remove_class(\"visible\") flips display back to none; the class op \
             must relayout on its own"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn toggle_class_flipping_display_relayouts_without_explicit_request() {
    ClassDisplayApp.run_test(|pilot| {
        pilot
            .app_mut()
            .query_mut("#panel")
            .unwrap()
            .toggle_class("visible");
        pilot.pause()?;
        assert!(
            panel_is_laid_out(pilot),
            "toggle_class on must show the panel without an explicit relayout"
        );

        pilot
            .app_mut()
            .query_mut("#panel")
            .unwrap()
            .toggle_class("visible");
        pilot.pause()?;
        assert!(
            !panel_is_laid_out(pilot),
            "toggle_class off must hide the panel without an explicit relayout"
        );
        Ok(())
    })
    .unwrap();
}

// The no-op guard (a class op that does not change the node's class set must
// not force a relayout) is unit-tested next to the DomQueryMut implementation
// in src/runtime/mod.rs, where `pending_force_relayout` is visible.
