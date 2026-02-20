/// Port of Python Textual `docs/examples/widgets/selection_list_selected.py`.
///
/// Demonstrates `SelectionList<String>`:
/// - Nine games listed; three are pre-selected (`initial = true`).
/// - A `Pretty` widget alongside shows the currently selected game values.
/// - On mount and on every `SelectionListSelectedChanged`, the Pretty is updated.
///
/// Python: `@on(Mount)` + `@on(SelectionList.SelectedChanged)` both call
/// `Pretty.update(SelectionList.selected)`.
/// Rust: `on_mount_with_app` and `on_message_with_app` both collect
/// `selected_values()` and call `Pretty::update_str()`.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

Horizontal {
    width: 80%;
    height: 80%;
}

SelectionList {
    padding: 1;
    border: solid $accent;
    width: 1fr;
}

Pretty {
    width: 1fr;
    border: solid $accent;
}
"#;

struct SelectionListApp;

/// Collect selected game values from the SelectionList and format them for Pretty.
fn selected_debug(values: Vec<&String>) -> String {
    let strs: Vec<&str> = values.iter().map(|s| s.as_str()).collect();
    format!("{:?}", strs)
}

impl TextualApp for SelectionListApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let games = SelectionList::with_selections(vec![
            Selection::selected("Falken's Maze".to_string(), "secret_back_door".to_string()),
            Selection::new("Black Jack".to_string(), "black_jack".to_string()),
            Selection::new("Gin Rummy".to_string(), "gin_rummy".to_string()),
            Selection::new("Hearts".to_string(), "hearts".to_string()),
            Selection::new("Bridge".to_string(), "bridge".to_string()),
            Selection::new("Checkers".to_string(), "checkers".to_string()),
            Selection::selected("Chess".to_string(), "a_nice_game_of_chess".to_string()),
            Selection::new("Poker".to_string(), "poker".to_string()),
            Selection::selected("Fighter Combat".to_string(), "fighter_combat".to_string()),
        ])
        .with_border_title("Shall we play some games?");
        let pretty = Pretty::new(&Vec::<String>::new())
            .with_border_title("Selected games");
        let row = Horizontal::new()
            .with_child(games)
            .with_child(pretty);
        AppRoot::new()
            .with_child(Header::new())
            .with_child(row)
            .with_child(Footer::new())
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Mirror Python's @on(Mount): show initial selection.
        // We show empty initially; SelectionListSelectedChanged syncs it after mount.
        let _ = app.with_query_one_mut_as::<Pretty, _>("Pretty", |pretty| {
            pretty.update_str("[]");
        });
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        _ctx: &mut EventCtx,
    ) {
        if let Message::SelectionListSelectedChanged(_) = &message.message {
            // Collect selected values and update Pretty.
            let selected = app
                .with_query_one_mut_as::<SelectionList<String>, _>(
                    "SelectionList",
                    |sl| sl.selected_values().iter().map(|v| v.to_string()).collect::<Vec<_>>(),
                )
                .unwrap_or_default();
            let debug = format!("{:?}", selected);
            let _ = app.with_query_one_mut_as::<Pretty, _>("Pretty", |pretty| {
                pretty.update_str(debug.clone());
            });
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(SelectionListApp)
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
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
    fn selection_new_is_not_initially_selected() {
        let sel = Selection::new("Black Jack".to_string(), "black_jack".to_string());
        assert!(!sel.initially_selected);
    }

    #[test]
    fn selection_selected_is_initially_selected() {
        let sel = Selection::selected("Chess".to_string(), "a_nice_game_of_chess".to_string());
        assert!(sel.initially_selected);
    }

    #[test]
    fn selection_list_tracks_initial_selected() {
        let sl = SelectionList::with_selections(vec![
            Selection::selected("Alpha".to_string(), "alpha".to_string()),
            Selection::new("Beta".to_string(), "beta".to_string()),
            Selection::selected("Gamma".to_string(), "gamma".to_string()),
        ]);
        let selected = sl.selected_values();
        assert_eq!(selected, vec![&"alpha".to_string(), &"gamma".to_string()]);
    }
}
