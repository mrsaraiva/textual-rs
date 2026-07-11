//! Regression: a runtime `query_mut().set_styles(..)` mutation that changes a
//! LAYOUT-affecting property (here CSS `offset`) must trigger a relayout, not
//! just a repaint.
//!
//! Python parity: every style-property setter refreshes the widget, and
//! layout-affecting properties refresh with `layout=True` — `OffsetProperty.
//! __set__` calls `refresh(layout=True)` (`css/_style_properties.py`). The
//! guide/input `mouse01` demo depends on this: `Ball.offset = event.
//! screen_offset - (8, 2)` moves the ball every mouse move. Before this fix,
//! Rust applied the new inline style to the node but never requested layout
//! invalidation, so the ball repainted at its stale rect (frozen at the
//! container origin).

use textual::compose;
use textual::prelude::*;
use textual::style::{Offset, OffsetValue};

const CSS: &str = r##"
Screen {
    layers: log ball;
}

#filler {
    layer: log;
}

#ball {
    layer: ball;
    width: auto;
    height: 1;
}
"##;

struct OffsetApp;

impl TextualApp for OffsetApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(compose![
            Static::new("filler").id("filler"),
            Static::new("ball").id("ball"),
        ])
    }
    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }
}

#[test]
fn set_styles_offset_moves_widget_rect() {
    textual::run_test(OffsetApp, |pilot| {
        let ball = pilot.app().query_one("#ball").unwrap();
        let before = pilot
            .app()
            .node_screen_rect(ball)
            .expect("ball has a rendered rect");

        // Runtime offset mutation, as mouse01's MouseMoved handler does.
        pilot
            .app_mut()
            .query_mut("#ball")
            .unwrap()
            .set_styles(|s| {
                s.style.offset = Some(Offset {
                    x: OffsetValue::Cells(12),
                    y: OffsetValue::Cells(5),
                });
            });
        pilot.pause()?;

        let after = pilot
            .app()
            .node_screen_rect(ball)
            .expect("ball still has a rendered rect");
        assert_eq!(
            (after.0, after.1),
            (before.0 + 12, before.1 + 5),
            "offset set via set_styles must relayout and shift the rect \
             (before={before:?}, after={after:?})"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn set_styles_paint_only_change_does_not_move_rect() {
    textual::run_test(OffsetApp, |pilot| {
        let ball = pilot.app().query_one("#ball").unwrap();
        let before = pilot.app().node_screen_rect(ball).unwrap();

        pilot
            .app_mut()
            .query_mut("#ball")
            .unwrap()
            .set_styles(|s| {
                s.style.bold = Some(true);
            });
        pilot.pause()?;

        let after = pilot.app().node_screen_rect(ball).unwrap();
        assert_eq!(before, after, "paint-only style change must not move the rect");
        Ok(())
    })
    .unwrap();
}
