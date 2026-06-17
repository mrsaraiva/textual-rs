/// Port of Python Textual `docs/examples/guide/content/renderables.py`.
///
/// Demonstrates using Rich renderables inside Textual widgets.
///
/// Python: A custom `CodeView` widget holds a reactive `code` field and returns
/// `rich.syntax.Syntax(self.code, "python", line_numbers=True, indent_guides=True)`
/// from its `render()` method, displaying the app's own source with syntax
/// highlighting, line numbers, and indent guides.
///
/// Rust port: Uses `Static::update_rich()` with `rich_rs::Syntax` in
/// `on_mount_with_app` to display this file's own source with syntax highlighting,
/// line numbers, and indent guides — the same visual intent as the Python original.
/// Language is "rust" (this file) rather than "python" (the Python original).
use rich_rs::Syntax;
use textual::prelude::*;

/// Source code of this file (read at compile time, mirrors Python's `open(__file__)`).
const SOURCE: &str = include_str!("main.rs");

const CSS: &str = r#"
CodeView {
    height: auto;
}
"#;

struct CodeApp;

impl TextualApp for CodeApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        // A single Static widget fills the screen (mirrors Python's `yield code_view`).
        AppRoot::new().with_child(Static::new("").id("code-view"))
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        // Build a Rich Syntax renderable (mirrors Python's `Syntax(self.code, "python",
        // line_numbers=True, indent_guides=True)`). Language is "rust" here.
        let syntax = Syntax::new(SOURCE, "rust")
            .with_line_numbers(true)
            .with_indent_guides(true);
        let highlighted = syntax.highlight();

        let _ = app.with_query_one_mut_as::<Static, _>("#code-view", |widget| {
            widget.update_rich(highlighted);
        });
        ctx.request_repaint();
    }
}

fn main() -> textual::Result<()> {
    run_sync(CodeApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_app_composes_without_panic() {
        let mut app = CodeApp;
        let _root = app.compose();
    }

    #[test]
    fn source_constant_is_non_empty() {
        assert!(!SOURCE.is_empty());
        assert!(SOURCE.contains("CodeApp"));
    }
}
