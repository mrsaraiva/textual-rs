//! Regression: a runtime CONTENT update (`Label::set_text` via
//! `with_query_one_mut_as` / `with_widget_mut`) that changes the widget's
//! intrinsic size must trigger a relayout, not just a repaint.
//!
//! Python parity: `Static.update()` calls `refresh(layout=True)`
//! (`widgets/_static.py`), so an empty auto-width Label that later receives
//! text is re-measured and re-arranged (e.g. re-centered by an `align: center`
//! parent). This is the CONTENT-update sibling of the style-mutation path
//! guarded by `tests/set_styles_relayout.rs`: `set_text` does not go through
//! `set_styles`, so the intrinsic-size diff in `App::with_widget_mut` must
//! observe the `auto_content_width`/`auto_content_height` channels (a bare
//! `Label` reports `content_width() == None`; its `width: auto` measurement
//! only shows up through `auto_content_width()`). Mirrors the
//! `radio_set_changed` demo, where `#pressed`/`#index` labels start empty.

use textual::compose;
use textual::prelude::*;

const CSS: &str = r##"
Horizontal {
    align: center middle;
    height: auto;
}
"##;

struct ContentUpdateApp;

impl TextualApp for ContentUpdateApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(compose![
            Horizontal::new().with_child(Label::new("").with_id("pressed")),
        ])
    }
    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }
}

#[test]
fn label_set_text_relayouts_auto_width_box() {
    textual::run_test(ContentUpdateApp, |pilot| {
        pilot.pause()?;
        let label = pilot.app().query_one("#pressed").unwrap();
        // The empty label may not have a recorded rect at all (zero-width box);
        // both cases are valid starting states for this regression.
        let before = pilot.app().node_screen_rect(label);

        // Runtime content mutation, as radio_set_changed's Changed handler does.
        let text = "Pressed button label: Battlestar Galactica";
        pilot
            .app_mut()
            .with_query_one_mut_as::<Label, _>("#pressed", |l| l.set_text(text))
            .unwrap();
        pilot.pause()?;

        let after = pilot
            .app()
            .node_screen_rect(label)
            .expect("label has a rendered rect after set_text");
        let after_w = after.2.saturating_sub(after.0);
        // Hit-test rect edge conventions may be inclusive; assert the box grew
        // to (at least almost) the new intrinsic width rather than staying at
        // its stale empty-label width.
        assert!(
            (after_w as usize) >= text.len() - 1,
            "set_text on an auto-width label must relayout to the new intrinsic \
             width (text.len()={}, before={before:?}, after={after:?})",
            text.len()
        );
        // The `align: center middle` parent must re-center the grown box, so its
        // left edge moves left of the (near-degenerate) empty-label position.
        if let Some(before) = before {
            assert!(
                after.0 < before.0,
                "grown label must be re-centered by the align parent \
                 (before={before:?}, after={after:?})"
            );
        }
        Ok(())
    })
    .unwrap();
}
