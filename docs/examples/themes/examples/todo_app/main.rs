/// Port of Python Textual `docs/examples/themes/todo_app.py`.
///
/// Demonstrates a todo-list app with theme cycling. The Python source cycles through
/// named themes ["nord", "gruvbox", "tokyo-night", "textual-dark", "solarized-light"]
/// on Ctrl+T.
///
/// textual-rs does not yet support named themes beyond the built-in dark/light toggle.
/// This port wires Ctrl+T to `toggle_dark` (alternates built-in dark/light).
///
/// NON-PROMOTABLE: color-only; named themes differ from Python at every frame.
/// The layout and widget structure are faithful ports.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}
#content {
    height: auto;
    width: 40;
    padding: 1 2;
}
#header {
    height: 1;
    width: auto;
    margin-bottom: 1;
}
.title {
    text-style: bold;
    padding: 0 1;
    width: 1fr;
}
#overdue {
    color: $text-error;
    background: $error-muted;
    padding: 0 1;
    width: auto;
}
#done {
    color: $text-success;
    background: $success-muted;
    padding: 0 1;
    margin: 0 1;
}
#footer {
    height: auto;
    margin-bottom: 2;
}
#history-header {
    height: 1;
    width: auto;
}
#history-done {
    width: auto;
    padding: 0 1;
    margin: 0 1;
    background: $primary-muted;
    color: $text-primary;
}
"#;

struct TodoList;

impl TextualApp for TodoList {
    fn title(&self) -> &'static str {
        "TodoList"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("ctrl+t", "toggle_dark", "Cycle theme")]
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        // Build header row
        let header_row = ChildDecl::from(Horizontal::new().with_compose(vec![
            ChildDecl::from(Label::new("Today")).with_classes(&["title"]),
            ChildDecl::from(Label::new("1 overdue")).with_id("overdue"),
            ChildDecl::from(Label::new("1 done")).with_id("done"),
        ]))
        .with_id("header");

        // Build todo list
        let todo_list = ChildDecl::from(SelectionList::with_selections(vec![
            Selection::new("Buy milk", 0i32),
            Selection::new("Buy bread", 1),
            Selection::selected("Go and vote", 2),
            Selection::new("Return package", 3),
        ]))
        .with_id("todo-list");

        // Build input footer
        let footer_row =
            ChildDecl::from(Horizontal::new().with_child(Input::new().with_placeholder("Add a task")))
                .with_id("footer");

        // Build history header
        let history_header = ChildDecl::from(Horizontal::new().with_compose(vec![
            ChildDecl::from(Label::new("History")).with_classes(&["title"]),
            ChildDecl::from(Label::new("4 items")).with_id("history-done"),
        ]))
        .with_id("history-header");

        // Main content container
        let content = ChildDecl::from(
            Vertical::new().with_compose(vec![header_row, todo_list, footer_row, history_header]),
        )
        .with_id("content");

        AppRoot::new().with_compose(vec![
            ChildDecl::from(Header::new()),
            content,
            ChildDecl::from(Footer::new()),
        ])
    }
}

fn main() -> textual::Result<()> {
    run_sync(TodoList)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todo_list_composes_without_panic() {
        let mut app = TodoList;
        let _root = app.compose();
    }

    #[test]
    fn bindings_include_cycle_theme() {
        let app = TodoList;
        let bindings = app.bindings();
        assert!(
            bindings.iter().any(|b| b.key == "ctrl+t"),
            "ctrl+t binding missing"
        );
    }
}
