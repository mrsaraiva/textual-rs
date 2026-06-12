/// Port of Python Textual `examples/markdown.py`.
///
/// A Markdown viewer application with Table of Contents toggle, and
/// forward/back navigation history.
///
/// Python: `MarkdownViewer.go(path)`, `back()`, `forward()`, `Navigator`,
/// and `check_action()` to dim footer bindings at history ends.
///
/// Rust: `MarkdownViewer::register_content()` + `go()` for navigation,
/// `check_action()` for dimming, `NavigatorUpdated` for refresh_bindings.
use textual::message::NavigatorUpdated;
use textual::prelude::*;

const DEMO_MD: &str = include_str!("demo.md");
const EXAMPLE_MD: &str = include_str!("example.md");

struct MarkdownApp {
    /// Cached navigator state for check_action (avoids querying widget).
    navigator_at_start: bool,
    navigator_at_end: bool,
    /// Optional file path from CLI args.
    initial_path: Option<String>,
}

impl MarkdownApp {
    fn new() -> Self {
        Self {
            navigator_at_start: true,
            navigator_at_end: true,
            initial_path: None,
        }
    }
}

impl TextualApp for MarkdownApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("t", "toggle_table_of_contents", "TOC"),
            BindingDecl::new("b", "back", "Back"),
            BindingDecl::new("f", "forward", "Forward"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        let mut viewer = MarkdownViewer::new(DEMO_MD);
        viewer.set_style_id(Some("markdown-viewer".to_string()));
        viewer.register_content("demo.md", DEMO_MD);
        viewer.register_content("example.md", EXAMPLE_MD);
        AppRoot::new().with_child(Footer::new()).with_child(viewer)
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        // Load initial content: CLI arg path or demo.md.
        if let Some(ref path) = self.initial_path {
            if let Ok(content) = std::fs::read_to_string(path) {
                let _ =
                    app.with_query_one_mut_as::<MarkdownViewer, _>("#markdown-viewer", |viewer| {
                        viewer.register_content(path.clone(), content);
                        viewer.go(path.clone());
                    });
                ctx.post_message(NavigatorUpdated);
            }
        } else {
            let _ = app.with_query_one_mut_as::<MarkdownViewer, _>("#markdown-viewer", |viewer| {
                viewer.go("demo.md");
            });
            ctx.post_message(NavigatorUpdated);
        }
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        match key.name() {
            "t" | "T" => {
                // Python: self.markdown_viewer.show_table_of_contents = not ...
                let _ =
                    app.with_query_one_mut_as::<MarkdownViewer, _>("#markdown-viewer", |viewer| {
                        let show = !viewer.is_showing_table_of_contents();
                        viewer.set_show_table_of_contents(show);
                    });
                ctx.set_handled();
                ctx.request_style_invalidation();
                ctx.request_layout_invalidation();
                ctx.request_repaint();
            }
            "b" => {
                let navigated = app
                    .with_query_one_mut_as::<MarkdownViewer, _>("#markdown-viewer", |viewer| {
                        viewer.back()
                    })
                    .unwrap_or(false);
                if navigated {
                    ctx.post_message(NavigatorUpdated);
                }
                ctx.set_handled();
                ctx.request_repaint();
            }
            "f" => {
                let navigated = app
                    .with_query_one_mut_as::<MarkdownViewer, _>("#markdown-viewer", |viewer| {
                        viewer.forward()
                    })
                    .unwrap_or(false);
                if navigated {
                    ctx.post_message(NavigatorUpdated);
                }
                ctx.set_handled();
                ctx.request_repaint();
            }
            _ => {}
        }
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if message.is::<NavigatorUpdated>() {
            self.update_navigator_state(app);
            app.refresh_bindings();
            ctx.request_repaint();
        }
    }

    fn check_action(&self, action: &str, _parameters: &[String]) -> Option<bool> {
        match action {
            "forward" if self.navigator_at_end => None,
            "back" if self.navigator_at_start => None,
            _ => Some(true),
        }
    }
}

impl MarkdownApp {
    fn update_navigator_state(&mut self, app: &mut App) {
        let mut at_start = true;
        let mut at_end = true;
        let _ = app.with_query_one_mut_as::<MarkdownViewer, _>("#markdown-viewer", |viewer| {
            at_start = viewer.navigator.at_start();
            at_end = viewer.navigator.at_end();
        });
        self.navigator_at_start = at_start;
        self.navigator_at_end = at_end;
    }
}

fn main() -> textual::Result<()> {
    let mut app = MarkdownApp::new();
    if let Some(path) = std::env::args().nth(1) {
        if std::path::Path::new(&path).exists() {
            app.initial_path = Some(path);
        }
    }
    run_sync(app)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_app_composes_without_panic() {
        let mut app = MarkdownApp::new();
        let _root = app.compose();
    }

    #[test]
    fn markdown_viewer_has_stable_style_id_for_queries() {
        let mut app = MarkdownApp::new();
        let mut root = app.compose();
        let children = root.take_composed_children();
        let viewer = children
            .iter()
            .find(|child| child.style_type() == "MarkdownViewer")
            .expect("expected composed MarkdownViewer child");
        assert_eq!(viewer.style_id(), Some("markdown-viewer"));
    }

    #[test]
    fn bindings_declare_toc_back_forward() {
        let app = MarkdownApp::new();
        let bindings = app.bindings();
        let keys: Vec<&str> = bindings.iter().map(|b| b.key.as_str()).collect();
        assert!(keys.contains(&"t"), "expected 't' for TOC toggle");
        assert!(keys.contains(&"b"), "expected 'b' for back");
        assert!(keys.contains(&"f"), "expected 'f' for forward");
    }

    #[test]
    fn markdown_check_action_disables_back_at_start() {
        let app = MarkdownApp {
            navigator_at_start: true,
            navigator_at_end: false,
            initial_path: None,
        };
        assert_eq!(app.check_action("back", &[]), None); // dimmed
        assert_eq!(app.check_action("forward", &[]), Some(true)); // enabled
    }

    #[test]
    fn markdown_check_action_disables_forward_at_end() {
        let app = MarkdownApp {
            navigator_at_start: false,
            navigator_at_end: true,
            initial_path: None,
        };
        assert_eq!(app.check_action("forward", &[]), None); // dimmed
        assert_eq!(app.check_action("back", &[]), Some(true)); // enabled
    }

    #[test]
    fn markdown_loads_demo_md_content() {
        let mut viewer = MarkdownViewer::new("");
        viewer.register_content("demo.md", DEMO_MD);
        assert!(viewer.go("demo.md"));
        assert!(!viewer.extract_headings().is_empty());
    }

    #[test]
    fn markdown_loads_example_md_content() {
        let mut viewer = MarkdownViewer::new("");
        viewer.register_content("example.md", EXAMPLE_MD);
        assert!(viewer.go("example.md"));
        assert!(!viewer.extract_headings().is_empty());
    }
}
