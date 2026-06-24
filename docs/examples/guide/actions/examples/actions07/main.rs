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
    width: 100%;
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

        ctx.request_repaint();
    }
}

fn main() -> textual::Result<()> {
    run_sync(PagesApp::new())
}
