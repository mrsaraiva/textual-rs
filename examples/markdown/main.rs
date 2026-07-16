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

#[derive(Reactive)]
struct MarkdownApp {
    /// (at_start, at_end) of the viewer's navigator — drives check_action.
    #[reactive(watch_with_app, init = false)]
    nav_state: (bool, bool),
    /// Optional file path from CLI args.
    initial_path: Option<String>,
    /// Typed handle slot for the MarkdownViewer child.
    viewer: HandleSlot<MarkdownViewer>,
}

impl MarkdownApp {
    fn new() -> Self {
        Self {
            nav_state: (true, true),
            initial_path: None,
            viewer: HandleSlot::new(),
        }
    }

    fn watch_nav_state(
        &mut self,
        app: &mut App,
        _old: &(bool, bool),
        _new: &(bool, bool),
        ctx: &mut ReactiveCtx,
    ) {
        app.refresh_bindings();
        ctx.request_repaint();
    }
}

impl TextualApp for MarkdownApp {
    fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("t", "toggle_table_of_contents", "TOC"),
            BindingDecl::new("b", "back", "Back"),
            BindingDecl::new("f", "forward", "Forward"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        let mut viewer = MarkdownViewer::new(DEMO_MD).with_id("markdown-viewer");
        viewer.register_content("demo.md", DEMO_MD);
        viewer.register_content("example.md", EXAMPLE_MD);
        AppRoot::new()
            .with_child(Footer::new())
            .with_child_handle(viewer, &self.viewer)
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut textual::event::WidgetCtx) {
        // Load initial content: CLI arg path or demo.md.
        if let Some(ref path) = self.initial_path {
            if let Ok(content) = std::fs::read_to_string(path) {
                let _ = self.viewer.handle().and_then(|h| {
                    h.update(app, |viewer, _ctx| {
                        viewer.register_content(path.clone(), content);
                        viewer.go(path.clone());
                    })
                });
                ctx.post_message(NavigatorUpdated);
            }
        } else {
            let _ = self.viewer.handle().and_then(|h| {
                h.update(app, |viewer, _ctx| {
                    viewer.go("demo.md");
                })
            });
            ctx.post_message(NavigatorUpdated);
        }
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut textual::event::WidgetCtx) {
        match key.name() {
            "t" | "T" => {
                // Python: self.markdown_viewer.show_table_of_contents = not ...
                let _ = self.viewer.handle().and_then(|h| {
                    h.update(app, |viewer, ctx| {
                        let show = !viewer.is_showing_table_of_contents();
                        viewer.set_show_table_of_contents(show);
                        // Apply the class to the arena node through the reactive
                        // ctx (the node seed is drained at mount, so mutating the
                        // widget's own class list alone would not reach the tree).
                        ctx.set_class(show, "-show-table-of-contents");
                    })
                });
                ctx.set_handled();
                ctx.request_style_invalidation();
                ctx.request_layout_invalidation();
                ctx.request_repaint();
            }
            "b" => {
                let navigated = self
                    .viewer
                    .handle()
                    .and_then(|h| h.update(app, |viewer, _ctx| viewer.back()))
                    .unwrap_or(false);
                if navigated {
                    ctx.post_message(NavigatorUpdated);
                }
                ctx.set_handled();
                ctx.request_repaint();
            }
            "f" => {
                let navigated = self
                    .viewer
                    .handle()
                    .and_then(|h| h.update(app, |viewer, _ctx| viewer.forward()))
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

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, _ctx: &mut textual::event::WidgetCtx) {
        if message.is::<NavigatorUpdated>() {
            if let Some(state) = self
                .viewer
                .handle()
                .ok()
                .and_then(|h| h.read(app, |v| (v.navigator.at_start(), v.navigator.at_end())).ok())
            {
                self.set_nav_state(state, app.reactive_ctx());
            }
        }
    }

    fn check_action(
        &self,
        action: &str,
        _parameters: &[textual::action::ActionArgument],
    ) -> Option<bool> {
        match action {
            "forward" if self.nav_state.1 => None,
            "back" if self.nav_state.0 => None,
            _ => Some(true),
        }
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
    use textual::runtime::build_widget_tree_from_root;

    #[test]
    fn markdown_app_composes_without_panic() {
        let mut app = MarkdownApp::new();
        let _root = app.compose();
    }

    #[test]
    fn markdown_viewer_has_stable_style_id_for_queries() {
        // Verify that MarkdownViewer is built with CSS id "markdown-viewer"
        // so that app-level `#markdown-viewer` queries work correctly.
        // We check the NodeSeed directly — the seed is consumed at mount and
        // transferred to the tree node record, so testing the seed is equivalent.
        let mut viewer = MarkdownViewer::new(DEMO_MD).with_id("markdown-viewer");
        let seed = viewer.take_node_seed();
        assert_eq!(seed.css_id.as_deref(), Some("markdown-viewer"));
    }

    #[test]
    fn viewer_slot_is_bound_after_tree_build() {
        let mut app = MarkdownApp::new();
        assert!(app.viewer.get().is_none(), "slot should be unfilled before compose");
        let mut root = app.compose();
        let tree = build_widget_tree_from_root(&mut root).expect("tree should build");
        let handle = app.viewer.handle().expect("slot should be filled after compose+build");
        // The mounted node should still be readable.
        let _ = handle
            .read_in(&tree, |_viewer| ())
            .expect("read_in should succeed");
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
            nav_state: (true, false),
            initial_path: None,
        };
        assert_eq!(app.check_action("back", &[]), None); // dimmed
        assert_eq!(app.check_action("forward", &[]), Some(true)); // enabled
    }

    #[test]
    fn markdown_check_action_disables_forward_at_end() {
        let app = MarkdownApp {
            nav_state: (false, true),
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
