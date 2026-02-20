/// Port of Python Textual `docs/examples/widgets/list_view.py`.
///
/// Demonstrates `ListView`:
/// - Three items ("One", "Two", "Three") in a centered, auto-height list.
/// - Arrow keys navigate; Enter/click selects.
///
/// Python uses `ListView(ListItem(Label("One")), ...)` — a composition of
/// `ListItem` wrappers containing `Label` widgets.
/// Rust `ListView::new(items: Vec<String>)` renders items directly as styled
/// rows, removing the need for a `ListItem`/`Label` wrapper layer.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

ListView {
    width: 30;
    height: auto;
    margin: 2 2;
}
"#;

struct ListViewApp;

impl TextualApp for ListViewApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let list = ListView::new(vec![
            "One".to_string(),
            "Two".to_string(),
            "Three".to_string(),
        ]);
        AppRoot::new().with_child(list).with_child(Footer::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(ListViewApp)
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_view_app_composes_without_panic() {
        let mut app = ListViewApp;
        let _root = app.compose();
    }

    #[test]
    fn list_view_has_three_items() {
        let list = ListView::new(vec![
            "One".to_string(),
            "Two".to_string(),
            "Three".to_string(),
        ]);
        assert_eq!(list.items().len(), 3);
        assert_eq!(list.items()[0], "One");
        assert_eq!(list.items()[2], "Three");
    }

    #[test]
    fn list_view_initial_selected_is_zero() {
        let list = ListView::new(vec!["One".to_string(), "Two".to_string()]);
        assert_eq!(list.selected(), 0);
    }

    #[test]
    fn list_view_selected_item_returns_first() {
        let list = ListView::new(vec!["One".to_string(), "Two".to_string()]);
        assert_eq!(list.selected_item(), Some("One"));
    }
}
