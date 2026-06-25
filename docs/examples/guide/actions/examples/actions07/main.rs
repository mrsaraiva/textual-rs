/// Port of Python Textual `docs/examples/guide/actions/actions07.py`.
///
/// Demonstrates dynamic action checking with `check_action` returning `None`:
/// - 'n' / 'p' navigate between 5 pages in a HorizontalScroll.
/// - `check_action` returns `None` (disabled+hidden) at the first/last page,
///   dimming the corresponding footer hint.
///
/// Python calls `widget.scroll_visible()` to scroll the parent container so
/// the target child is visible. Rust mirrors this via `app.scroll_visible(node_id)`.
use textual::prelude::*;

const PAGES_COUNT: i32 = 5;

const CSS: &str = r##"
#page-container {
    scrollbar-size: 0 0;
}

Placeholder {
    width: 100vw;
    height: 100%;
}
"##;

struct PagesApp {
    page_no: i32,
}

impl PagesApp {
    fn new() -> Self {
        Self { page_no: 0 }
    }
}

impl TextualApp for PagesApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let mut container = HorizontalScroll::new().id("page-container");
        for i in 0..PAGES_COUNT {
            container = container.with_child(
                Placeholder::new(&format!("Page {}", i)).id(format!("page-{}", i)),
            );
        }
        AppRoot::new().with_child(container).with_child(Footer::new())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("n", "next", "Next"),
            BindingDecl::new("p", "previous", "Previous"),
        ]
    }

    fn check_action(&self, action: &str, _parameters: &[String]) -> Option<bool> {
        if action == "next" && self.page_no == PAGES_COUNT - 1 {
            return None;
        }
        if action == "previous" && self.page_no == 0 {
            return None;
        }
        Some(true)
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        match action {
            "next" => {
                if self.page_no < PAGES_COUNT - 1 {
                    self.page_no += 1;
                } else {
                    return;
                }
            }
            "previous" => {
                if self.page_no > 0 {
                    self.page_no -= 1;
                } else {
                    return;
                }
            }
            _ => return,
        }

        // Scroll to make the new page visible — mirrors Python:
        //   self.query_one(f"#page-{self.page_no}").scroll_visible()
        if let Ok(page_id) = app.query_one(&format!("#page-{}", self.page_no)) {
            app.scroll_visible(page_id);
        }

        // Mark the action handled so the runtime absorbs the repaint (the app
        // custom-action fallback only merges effects when the hook signals it
        // consumed the action; see actions02 for the same pattern). Without this
        // a pure page scroll — which changes no binding hints — would request a
        // repaint that is dropped, leaving the frame stale.
        ctx.set_handled();
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS PROBE (LIVE).
    ///
    /// Pressing 'n' scrolls the `HorizontalScroll` from one full-viewport page
    /// to the next. Two fixes make a pure page scroll (page1 -> page2) change
    /// the rendered frame:
    ///   1. Each page is sized `width: 100vw` (full viewport width), so the 5
    ///      pages lay out side-by-side with horizontal overflow — the CSS engine
    ///      resolves the `vw` viewport unit against the viewport width, so a 40-
    ///      col viewport yields a 200-col content strip. (Previously `width:
    ///      100%` sized each page to the 40-col container, with no overflow to
    ///      scroll.)
    ///   2. `action_next`/`action_previous` mark the action handled
    ///      (`ctx.set_handled()`), so the runtime absorbs the repaint request for
    ///      a pure scroll that changes no binding hints. (Without it the first
    ///      'n' still rendered — the footer "Previous" hint appears — but the
    ///      second 'n', a pure scroll, dropped its repaint and the frame went
    ///      stale.)
    #[test]
    fn liveness_next_prev_navigation_changes_frame() {
        textual::run_test_sized(PagesApp::new(), 40, 12, |pilot| {
            pilot.press(&["n"])?; // page0 -> page1 (footer "Previous" appears)
            let page1 = pilot.app().frame_fingerprint();
            pilot.press(&["n"])?; // page1 -> page2: a pure page scroll
            let page2 = pilot.app().frame_fingerprint();
            assert_ne!(
                page1, page2,
                "scrolling from page 1 to page 2 must change the rendered frame"
            );
            Ok(())
        })
        .unwrap();
    }
}

fn main() -> textual::Result<()> {
    run_sync(PagesApp::new())
}
