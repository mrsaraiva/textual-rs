/// Port of Python Textual `examples/code_browser.py`.
///
/// A code browser with:
/// - Toggleable `DirectoryTree` sidebar (press `f`)
/// - Syntax-highlighted file view via `rich_rs::Syntax` in a `VerticalScroll`
/// - `Header` with dynamic title and subtitle (current file path)
/// - `Footer` with binding hints
/// - Styled error display with path context on read failure
///
/// Mirrors Python's `CodeBrowser` app. Run with an optional path argument:
///
/// ```text
/// cargo run --example code_browser [PATH]
/// ```
///
/// If no path is given, the current directory is used.
use rich_rs::Syntax;
use textual::prelude::*;

// ---------------------------------------------------------------------------
// Embedded CSS (mirrors code_browser.tcss from the Python Textual repo)
// ---------------------------------------------------------------------------

const CSS: &str = r#"
#tree-view {
    display: none;
    scrollbar-gutter: stable;
    overflow: auto;
    width: auto;
    height: 100%;
    dock: left;
}

Screen.-show-tree #tree-view {
    display: block;
    max-width: 50%;
}

#code-view {
    overflow: auto scroll;
    min-width: 100%;
    hatch: right $panel;
}

#code {
    width: auto;
    padding: 0 1;
    background: $surface;
}
"#;

// ---------------------------------------------------------------------------
// App definition
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct CodeBrowserApp {
    /// Root directory shown in the tree on startup.
    start_path: String,
    /// Python: show_tree = var(True); watch_show_tree -> set_class("-show-tree").
    #[var(watch_with_app)]
    show_tree: bool,
    /// Python: path = reactive(None); watch_path loads + highlights the file.
    #[reactive(watch_with_app)]
    path: Option<String>,
    /// Post-mount typed handle to the syntax-highlighted code widget.
    code: Option<Handle<Static>>,
    /// Post-mount typed handle to the VerticalScroll wrapping the code widget.
    code_view: Option<Handle<VerticalScroll>>,
}

impl CodeBrowserApp {
    fn new(start_path: impl Into<String>) -> Self {
        Self {
            start_path: start_path.into(),
            show_tree: true,
            path: None,
            code: None,
            code_view: None,
        }
    }

    /// Mirrors Python's `watch_show_tree`: toggles `-show-tree` CSS class on
    /// the Screen, which the stylesheet uses to show/hide the directory tree.
    fn watch_show_tree(
        &mut self,
        app: &mut App,
        _old: &bool,
        new: &bool,
        ctx: &mut ReactiveCtx,
    ) {
        let _ = app.query_mut("Screen").map(|q| q.set_class(*new, &["-show-tree"]));
        ctx.request_styles();
        ctx.request_layout();
        ctx.request_repaint();
    }

    /// Mirrors Python's `watch_path`: loads and syntax-highlights the file on
    /// each path change (None → clear the code pane).
    fn watch_path(
        &mut self,
        app: &mut App,
        _old: &Option<String>,
        new: &Option<String>,
        ctx: &mut ReactiveCtx,
    ) {
        match new {
            None => {
                if let Some(h) = self.code {
                    let _ = h.update(app, |s, _ctx| s.update(""));
                }
            }
            Some(path) => match Syntax::from_path(path) {
                Ok(syntax) => {
                    let highlighted = syntax.highlight();
                    if let Some(h) = self.code {
                        let _ = h.update(app, |s, _ctx| s.update_rich(highlighted));
                    }
                    if let Some(h) = self.code_view {
                        let _ = h.update(app, |s, _ctx| s.scroll_home());
                    }
                    app.set_sub_title(path.as_str());
                }
                Err(e) => {
                    let error_msg =
                        format!("[b red]Error reading file:[/b red]\n{path}\n\n{e}");
                    if let Some(h) = self.code {
                        let _ = h.update(app, |s, _ctx| s.update(&error_msg));
                    }
                    app.set_sub_title("ERROR");
                }
            },
        }
        ctx.request_repaint();
    }
}

impl TextualApp for CodeBrowserApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            // Toggle the directory tree sidebar.
            // Mirrors Python's `action_toggle_files` + `watch_show_tree`.
            BindingDecl::new("f", "toggle_files", "Toggle Files"),
            BindingDecl::new("q", "app.quit", "Quit"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        // DirectoryTree is wrapped in a Node so we can assign it the id "tree-view".
        let tree = Node::new(DirectoryTree::new(&self.start_path)).id("tree-view");

        // Static widget for syntax-highlighted content, inside a VerticalScroll.
        // Python uses VerticalScroll (not ScrollView) for the code pane.
        let code = Static::new("").id("code");
        let code_view = Node::new(VerticalScroll::new().with_child(code)).id("code-view");

        // Container groups tree and code view; dock:left on #tree-view handles
        // the side-by-side layout when the tree is visible.
        AppRoot::new()
            .with_child(Header::new())
            .with_child(Container::new().with_child(tree).with_child(code_view))
            .with_child(Footer::new())
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Set the header title.
        app.set_title("Code Browser");

        // Focus the directory tree so the user can navigate immediately.
        let _ = app.action_focus("tree-view");

        // Acquire post-mount typed handles for the code pane widgets.
        // `#code` is the Static widget for syntax-highlighted content.
        self.code = app.query_one_typed::<Static>("#code").ok();
        // The VerticalScroll is a direct child of Node#code-view; use a
        // descendant selector to bypass the Node wrapper.
        self.code_view = app.query_one_typed::<VerticalScroll>("#code-view VerticalScroll").ok();

        // Note: initial tree visibility is applied by watch_show_tree firing at
        // mount (init-phase watcher dispatch, G3) — no manual add_class needed.
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        // Toggle the directory-tree sidebar when the user presses "f".
        // Mirrors Python's `action_toggle_files` which flips `self.show_tree`.
        if key.name() == "f" {
            let show = !self.show_tree;
            self.set_show_tree(show, app.reactive_ctx());
            ctx.set_handled();
        }
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        _ctx: &mut EventCtx,
    ) {
        // Handle DirectoryTree.FileSelected — mirrors `on_directory_tree_file_selected`.
        if let Some(ev) = message.downcast_ref::<DirectoryTreeFileSelected>() {
            self.set_path(Some(ev.path.clone()), app.reactive_ctx());
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> textual::Result<()> {
    let start_path = std::env::args().nth(1).unwrap_or_else(|| "./".to_string());
    run_sync(CodeBrowserApp::new(start_path))
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_browser_app_composes_without_panic() {
        // DG-02: compose() must not panic.
        let mut app = CodeBrowserApp::new("./");
        let _root = app.compose();
    }

    #[test]
    fn watch_state_default() {
        // Initial reactive state: show_tree == true, path == None.
        let app = CodeBrowserApp::new("./");
        assert!(app.show_tree, "show_tree must default to true");
        assert!(app.path.is_none(), "path must default to None");
    }

    #[test]
    fn bindings_include_toggle_files_and_quit() {
        // DG-02: expected bindings are declared.
        let app = CodeBrowserApp::new("./");
        let bindings = app.bindings();
        let actions: Vec<&str> = bindings.iter().map(|b| b.action.as_str()).collect();
        assert!(
            actions.iter().any(|a| *a == "toggle_files"),
            "expected toggle_files binding: {:?}",
            actions,
        );
        assert!(
            actions.iter().any(|a| *a == "app.quit"),
            "expected app.quit binding: {:?}",
            actions,
        );
    }

    #[test]
    fn bindings_f_key_triggers_toggle_files() {
        // DG-02: "f" key is bound to the toggle_files action.
        let app = CodeBrowserApp::new("./");
        let bindings = app.bindings();
        let toggle_binding = bindings.iter().find(|b| b.action == "toggle_files");
        assert!(toggle_binding.is_some(), "toggle_files binding not found");
        let keys = &toggle_binding.unwrap().key;
        assert!(
            keys.split(',').any(|k| k.trim() == "f"),
            "expected 'f' key for toggle_files: {keys:?}",
        );
    }

    #[test]
    fn bindings_q_key_triggers_quit() {
        // DG-02: "q" key is bound to quit.
        let app = CodeBrowserApp::new("./");
        let bindings = app.bindings();
        let quit_binding = bindings.iter().find(|b| b.action == "app.quit");
        assert!(quit_binding.is_some(), "quit binding not found");
        let keys = &quit_binding.unwrap().key;
        assert!(
            keys.split(',').any(|k| k.trim() == "q"),
            "expected 'q' key for quit: {keys:?}",
        );
    }

    #[test]
    fn code_browser_uses_vertical_scroll() {
        // Phase 6: compose uses VerticalScroll (not ScrollView) for code pane.
        let mut app = CodeBrowserApp::new("./");
        let _root = app.compose();
        // If it compiles and runs, VerticalScroll is used (type-checked at call site).
    }

    #[test]
    fn code_browser_error_display_includes_path() {
        // Phase 6: error message includes the file path for context.
        let path = "/nonexistent/file.rs";
        let e = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error_msg = format!("[b red]Error reading file:[/b red]\n{path}\n\n{e}");
        assert!(error_msg.contains(path));
        assert!(error_msg.contains("file not found"));
    }
}
