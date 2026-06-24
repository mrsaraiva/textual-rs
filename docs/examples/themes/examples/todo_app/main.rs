/// Port of Python Textual `docs/examples/themes/todo_app.py`.
///
/// Demonstrates a todo-list app with named-theme cycling. The Python source
/// cycles through the named themes
/// `["nord", "gruvbox", "tokyo-night", "textual-dark", "solarized-light"]` on
/// Ctrl+T (`action_cycle_theme` over `itertools.cycle`), and applies the first
/// theme on mount.
///
/// textual-rs now has a faithful named theme catalog/registry (keystone
/// "themereg"): `App::set_theme_cycle` + the `cycle_theme` action reproduce the
/// Python behavior exactly, re-coloring every `$`-token (`$text-error`,
/// `$error-muted`, `$primary-muted`, …) from the active theme.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
    hatch: right $foreground 10%;
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

/// The named themes cycled by Ctrl+T (exact Python `THEMES` list).
const THEME_CYCLE: &[&str] = &["nord", "gruvbox", "tokyo-night", "textual-dark", "solarized-light"];

struct TodoList;

impl TextualApp for TodoList {
    fn title(&self) -> &'static str {
        "TodoList"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("ctrl+t", "cycle_theme", "Cycle theme")]
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        // Python: `THEMES = cycle([...])` + `on_mount` -> `action_cycle_theme()`
        // applies the first theme (nord) before the first paint.
        app.set_theme_cycle(THEME_CYCLE.iter().copied());
        app.cycle_theme();
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
            bindings.iter().any(|b| b.key == "ctrl+t" && b.action == "cycle_theme"),
            "ctrl+t -> cycle_theme binding missing"
        );
    }

    #[test]
    fn configure_applies_named_theme_cycle_like_python() {
        // Faithful to Python: configure sets the THEMES cycle and applies the
        // first theme (nord) before the first paint, re-coloring the UI tokens.
        let mut app = App::new().expect("app init");
        let mut todo = TodoList;
        todo.configure(&mut app).expect("configure ok");
        assert_eq!(app.theme_name(), "nord");
        assert_eq!(
            textual::style::parse_color_like("$primary"),
            Color::parse("#88C0D0")
        );
        // Ctrl+T advances to the next theme in the Python cycle.
        assert!(app.cycle_theme());
        assert_eq!(app.theme_name(), "gruvbox");

        // Restore default so global theme state does not leak.
        app.set_theme_by_name("textual-dark");
    }
}
