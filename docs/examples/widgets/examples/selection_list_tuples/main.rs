/// Port of Python Textual `docs/examples/widgets/selection_list_tuples.py`.
///
/// Demonstrates `SelectionList` populated with tuple-style entries:
/// - (prompt, value)         → unselected by default
/// - (prompt, value, true)   → pre-selected
///
/// Python uses `SelectionList[int](("Falken's Maze", 0, True), ...)` and
/// sets `border_title = "Shall we play some games?"` in `on_mount`.
///
/// CSS is ported from `selection_list.tcss`: screen centered, SelectionList
/// at 80% width / 80% height with padding=1 and solid $accent border.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

SelectionList {
    padding: 1;
    border: solid $accent;
    width: 80%;
    height: 80%;
}
"#;

struct SelectionListApp;

impl TextualApp for SelectionListApp {
    fn title(&self) -> &'static str {
        "SelectionListApp"
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let list = SelectionList::with_selections(vec![
            Selection::selected("Falken's Maze", 0i32),
            Selection::new("Black Jack", 1),
            Selection::new("Gin Rummy", 2),
            Selection::new("Hearts", 3),
            Selection::new("Bridge", 4),
            Selection::new("Checkers", 5),
            Selection::selected("Chess", 6),
            Selection::new("Poker", 7),
            Selection::selected("Fighter Combat", 8),
        ])
        .with_border_title("Shall we play some games?");

        AppRoot::new()
            .with_child(Header::new())
            .with_child(list)
            .with_child(Footer::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(SelectionListApp)
}

// ---------------------------------------------------------------------------
// Regression tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_list_app_composes_without_panic() {
        let mut app = SelectionListApp;
        let _root = app.compose();
    }

    #[test]
    fn initial_selections_are_correct() {
        let list = SelectionList::with_selections(vec![
            Selection::selected("Falken's Maze", 0i32),
            Selection::new("Black Jack", 1),
            Selection::new("Gin Rummy", 2),
            Selection::new("Hearts", 3),
            Selection::new("Bridge", 4),
            Selection::new("Checkers", 5),
            Selection::selected("Chess", 6),
            Selection::new("Poker", 7),
            Selection::selected("Fighter Combat", 8),
        ]);
        assert_eq!(list.item_count(), 9);
        assert!(list.is_selected(0), "Falken's Maze should be pre-selected");
        assert!(!list.is_selected(1), "Black Jack should be unselected");
        assert!(list.is_selected(6), "Chess should be pre-selected");
        assert!(list.is_selected(8), "Fighter Combat should be pre-selected");
        assert_eq!(list.selected(), vec![0, 6, 8]);
    }

    #[test]
    fn all_nine_games_are_present() {
        let list = SelectionList::with_selections(vec![
            Selection::selected("Falken's Maze", 0i32),
            Selection::new("Black Jack", 1),
            Selection::new("Gin Rummy", 2),
            Selection::new("Hearts", 3),
            Selection::new("Bridge", 4),
            Selection::new("Checkers", 5),
            Selection::selected("Chess", 6),
            Selection::new("Poker", 7),
            Selection::selected("Fighter Combat", 8),
        ]);
        assert_eq!(list.item_count(), 9);
        assert_eq!(list.value_at(0), Some(&0i32));
        assert_eq!(list.value_at(8), Some(&8i32));
    }
}
