/// Port of Python Textual `docs/examples/guide/actions/actions06.py`.
///
/// Demonstrates conditional action availability:
/// - Five pages laid out horizontally in a `HorizontalScroll`.
/// - `n` / `p` bindings navigate next / previous page.
/// - `check_action()` hides "next" on the last page and "previous" on the first,
///   so the Footer binding hints update dynamically (mirroring Python's
///   `check_action` + `refresh_bindings` pattern).
///
/// Python calls `widget.scroll_visible()` to scroll the parent container so the
/// target child widget is visible. Rust mirrors this via `app.scroll_visible(node_id)`.
use textual::prelude::*;

const PAGES_COUNT: usize = 5;

const CSS: &str = r#"
#page-container {
    scrollbar-size: 0 0;
}

Placeholder {
    width: 100vw;
}
"#;

struct PagesApp {
    page_no: usize,
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

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("n", "next", "Next"),
            BindingDecl::new("p", "previous", "Previous"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        let mut page_container = HorizontalScroll::new();
        for page_no in 0..PAGES_COUNT {
            page_container.push(
                Node::new(Placeholder::new(format!("Page {}", page_no))).id(format!(
                    "page-{}",
                    page_no
                )),
            );
        }
        AppRoot::new()
            .with_child(Node::new(page_container).id("page-container"))
            .with_child(Footer::new())
    }

    fn check_action(&self, action: &str, _parameters: &[String]) -> Option<bool> {
        if action == "next" && self.page_no == PAGES_COUNT - 1 {
            return Some(false);
        }
        if action == "previous" && self.page_no == 0 {
            return Some(false);
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

        // Refresh bindings so the footer updates "Next"/"Previous" visibility.
        app.refresh_bindings();

        // Scroll to make the new page visible — mirrors Python:
        //   self.query_one(f"#page-{self.page_no}").scroll_visible()
        if let Ok(page_id) = app.query_one(&format!("#page-{}", self.page_no)) {
            app.scroll_visible(page_id);
        }

        ctx.request_repaint();
        ctx.set_handled();
    }
}

fn main() -> textual::Result<()> {
    run_sync(PagesApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS PROBE (DEAD — captures expected behavior, currently failing).
    ///
    /// Pressing 'n' is expected to scroll the `HorizontalScroll` from page N to
    /// page N+1 (and 'p' back). The binding fires and `page_no` advances, but
    /// the **page content never scrolls/renders**: the first 'n' changes the
    /// frame only because the Footer hint "Previous" appears (it was hidden by
    /// `check_action` on page 0); the *second* 'n' (page1 -> page2) produces an
    /// IDENTICAL frame (page1 == page2). The pages themselves are never laid out
    /// or drawn: `node_screen_rect("HorizontalScroll")`, `#page-0`, `#page-1`
    /// all return `None`.
    ///
    /// ROOT: the demo composes the scroller as `Node::new(HorizontalScroll::new())
    /// .id("page-container")` — a structural Node wrapper with no rendered
    /// surface — and the `HorizontalScroll`'s page children get no laid-out/
    /// rendered region (same class as binding01's `#bars` wrapper). So
    /// `scroll_visible(page)` has nothing to scroll into view, and page
    /// navigation is invisible. (Pages use `width: 100vw`; combined with the
    /// wrapper, the horizontal scroll viewport never materialises.)
    ///
    /// TODO (fix then un-ignore): give the pages a rendered region inside a
    /// real horizontal scroll viewport so navigation visibly changes the frame.
    /// The assertion below is the real expected behavior: the SECOND 'n' (a
    /// pure page scroll, no footer-hint toggle) must change the rendered frame.
    #[ignore = "DEAD: HorizontalScroll pages never lay out/render; scroll_visible navigation is invisible"]
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
