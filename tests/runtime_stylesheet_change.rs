//! Changing the app stylesheet at runtime takes effect on the next frame.
//! The runtime memoizes the merged default + app stylesheet, so an in-place
//! edit through `stylesheet_mut()` must not leave a stale merge behind.

use textual::css::StyleSheet;
use textual::prelude::*;

struct PaintApp;

impl TextualApp for PaintApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("p", "paint", "Paint")]
    }
    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet("Static { background: #ff0000; }");
        Ok(())
    }
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("x"))
    }
    fn on_app_action_str(&mut self, _app: &mut App, action: &str, ctx: &mut WidgetCtx) {
        if action == "paint" {
            ctx.request_repaint();
            ctx.set_handled();
        }
    }
}

#[test]
fn in_place_stylesheet_edit_applies_on_next_frame() {
    PaintApp
        .run_test(|pilot| {
            let red = Color::parse("#ff0000").unwrap();
            let blue = Color::parse("#0000ff").unwrap();
            assert_eq!(pilot.app().frame_cell_bg(0, 0), Some(red));

            pilot
                .app_mut()
                .stylesheet_mut()
                .extend(&StyleSheet::parse("Static { background: #0000ff; }"));
            pilot.press(&["p"])?;
            assert_eq!(pilot.app().frame_cell_bg(0, 0), Some(blue));
            Ok(())
        })
        .unwrap();
}
