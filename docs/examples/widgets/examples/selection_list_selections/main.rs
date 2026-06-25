/// Port of Python Textual `docs/examples/widgets/selection_list_selections.py`.
///
/// Demonstrates `SelectionList<i32>` with pre-selected items:
/// - Nine games listed; three start selected: "Falken's Maze" (0),
///   "Chess" (6), and "Fighter Combat" (8).
/// - A border title "Shall we play some games?" is set on mount.
///
/// CSS is ported from `selection_list.tcss`:
/// - Screen is center-middle aligned
/// - SelectionList occupies 80% width × 80% height with solid accent border
///   and 1-cell padding
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
        let games = SelectionList::with_selections(vec![
            Selection::selected("Falken's Maze".to_string(), 0i32),
            Selection::new("Black Jack".to_string(), 1i32),
            Selection::new("Gin Rummy".to_string(), 2i32),
            Selection::new("Hearts".to_string(), 3i32),
            Selection::new("Bridge".to_string(), 4i32),
            Selection::new("Checkers".to_string(), 5i32),
            Selection::selected("Chess".to_string(), 6i32),
            Selection::new("Poker".to_string(), 7i32),
            Selection::selected("Fighter Combat".to_string(), 8i32),
        ])
        .with_border_title("Shall we play some games?");
        AppRoot::new()
            .with_child(Header::new())
            .with_child(games)
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
    fn three_games_are_initially_selected() {
        let sl = SelectionList::with_selections(vec![
            Selection::selected("Falken's Maze".to_string(), 0i32),
            Selection::new("Black Jack".to_string(), 1i32),
            Selection::new("Gin Rummy".to_string(), 2i32),
            Selection::new("Hearts".to_string(), 3i32),
            Selection::new("Bridge".to_string(), 4i32),
            Selection::new("Checkers".to_string(), 5i32),
            Selection::selected("Chess".to_string(), 6i32),
            Selection::new("Poker".to_string(), 7i32),
            Selection::selected("Fighter Combat".to_string(), 8i32),
        ]);
        let selected = sl.selected_values();
        assert_eq!(selected, vec![&0i32, &6i32, &8i32]);
    }

    /// LIVENESS: focus the SelectionList, highlight the first row and press
    /// space to toggle its checkbox — the checkbox glyph flips, so the rendered
    /// frame must change. A dead toggle (keys not routed) leaves it identical.
    #[test]
    fn liveness_toggle_selection() {
        SelectionListApp
            .run_test(|pilot| {
                pilot.press(&["tab"])?;
                pilot.press(&["down"])?;
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["space"])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(
                    before, after,
                    "toggling a selection must change the rendered frame"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
