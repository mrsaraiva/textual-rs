/// Port of Python Textual `examples/markdown.py`.
///
/// A Markdown viewer application with Table of Contents toggle, and
/// forward/back navigation history declarations.
///
/// Python: `MarkdownViewer.go(path)`, `back()`, `forward()`, `Navigator`,
/// and `check_action()` to disable footer buttons at history ends.
/// Rust: `MarkdownViewer::new(content).show_table_of_contents(true)`; bindings
/// declared for footer display; back/forward actions are stubs.
///
/// DEFERRED: `MarkdownViewer::go(path)`, `back()`, `forward()`, Navigator history
/// — requires async document loading wired into the runtime event loop.
/// When implemented, back/forward bindings will control the Navigator and
/// `on_message_with_app` will refresh binding hints on NavigatorUpdated.
use textual::prelude::*;

const DEMO_MARKDOWN: &str = r#"# Markdown App

A simple Markdown viewer.

## Section One

- Item 1
- Item 2
- Item 3

## Section Two

> Blockquote text here.

## Code

```rust
fn main() {
    println!("Hello, world!");
}
```
"#;

struct MarkdownApp {
    show_toc: bool,
}

impl MarkdownApp {
    fn new() -> Self {
        Self { show_toc: true }
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
        let viewer = MarkdownViewer::new(DEMO_MARKDOWN).show_table_of_contents(self.show_toc);
        AppRoot::new()
            .with_child(viewer)
            .with_child(Footer::new())
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        match key.name() {
            "t" => {
                // Toggle the table of contents sidebar.
                self.show_toc = !self.show_toc;
                let show = self.show_toc;
                let _ = app.with_query_one_mut_as::<MarkdownViewer, _>(
                    "MarkdownViewer",
                    |viewer| viewer.set_show_table_of_contents(show),
                );
                ctx.set_handled();
                ctx.request_repaint();
            }
            // DEFERRED: "b" back and "f" forward require Navigator history.
            _ => {}
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(MarkdownApp::new())
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
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
    fn bindings_declare_toc_back_forward() {
        let app = MarkdownApp::new();
        let bindings = app.bindings();
        let keys: Vec<&str> = bindings.iter().map(|b| b.key.as_str()).collect();
        assert!(keys.contains(&"t"), "expected 't' for TOC toggle");
        assert!(keys.contains(&"b"), "expected 'b' for back");
        assert!(keys.contains(&"f"), "expected 'f' for forward");
    }

    #[test]
    fn show_toc_starts_true() {
        let app = MarkdownApp::new();
        assert!(app.show_toc);
    }

    #[test]
    fn demo_markdown_has_headings() {
        let viewer = MarkdownViewer::new(DEMO_MARKDOWN);
        let headings = viewer.extract_headings();
        assert!(!headings.is_empty());
        assert_eq!(headings[0], (1, "Markdown App".to_string()));
    }
}
