/// Port of Python Textual `docs/examples/guide/actions/actions06.py`.
///
/// Demonstrates conditional action availability:
/// - Five pages laid out horizontally in a `HorizontalScroll`.
/// - `n` / `p` bindings navigate next / previous page.
/// - `check_action()` hides "next" on the last page and "previous" on the first,
///   so the Footer binding hints update dynamically (mirroring Python's
///   `check_action` + `refresh_bindings` pattern).
///
/// Framework gap (scroll_visible):
///   Python calls `widget.scroll_visible()` to scroll the parent container so the
///   target child is visible. Rust's `HorizontalScroll` exposes only `scroll_by_x`,
///   so we compute the absolute target column offset (page_no * page_width) and
///   convert it to a delta relative to the previous page. The page width is read
///   from the terminal via `app.driver().size().width`, which is correct when
///   each Placeholder is set to `width: 100vw`.
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
        let old_page = self.page_no;
        match action {
            "next" => {
                if self.page_no < PAGES_COUNT - 1 {
                    self.page_no += 1;
                }
            }
            "previous" => {
                if self.page_no > 0 {
                    self.page_no -= 1;
                }
            }
            _ => return,
        }

        // Refresh bindings so the footer updates "Next"/"Previous" visibility.
        app.refresh_bindings();

        // Scroll the page container to make the new page visible.
        // Python uses `widget.scroll_visible()` which queries the widget's bounding
        // box. Rust's HorizontalScroll only exposes `scroll_by_x(delta)`, so we
        // compute the page width from the terminal size (each page is 100vw wide)
        // and scroll by the delta between the old and new page.
        let page_width = app.driver().size().width as i32;
        let delta = (self.page_no as i32 - old_page as i32) * page_width;
        let _ = app.with_query_one_mut_as::<HorizontalScroll, _>("#page-container", |hs| {
            hs.scroll_by_x(delta);
        });

        ctx.request_repaint();
        ctx.set_handled();
    }
}

fn main() -> textual::Result<()> {
    run_sync(PagesApp::new())
}
