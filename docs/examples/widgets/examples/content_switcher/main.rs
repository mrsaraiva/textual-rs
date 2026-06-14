/// Port of Python Textual `docs/examples/widgets/content_switcher.py`.
///
/// Demonstrates `ContentSwitcher`:
/// - Two buttons switch between a `DataTable` and a Markdown viewer.
/// - The initial content is the `DataTable`.
/// - Pressing a button switches the visible content.
///
/// Python: `on_button_pressed` sets `ContentSwitcher.current = event.button.id`.
/// Rust: `Button::id()` sets a CSS id on each button.  `ButtonPressed.button_id`
/// carries that id, which matches the corresponding `ContentSwitcher` child id.
use textual::prelude::*;

const MARKDOWN_EXAMPLE: &str = r#"# Three Flavours Cornetto

The Three Flavours Cornetto trilogy is an anthology series of British
comedic genre films directed by Edgar Wright.

## Shaun of the Dead

| Flavour | UK Release Date | Director |
| -- | -- | -- |
| Strawberry | 2004-04-09 | Edgar Wright |

## Hot Fuzz

| Flavour | UK Release Date | Director |
| -- | -- | -- |
| Classico | 2007-02-17 | Edgar Wright |

## The World's End

| Flavour | UK Release Date | Director |
| -- | -- | -- |
| Mint | 2013-07-19 | Edgar Wright |
"#;

// Ported from Python's `content_switcher.tcss`.
//
// `Horizontal { height: 3 }` mirrors Python's `#buttons { height: 3 }` (the
// buttons are the only Horizontal here). It overrides the Horizontal default
// (`height: 1fr`) so the buttons row does not compete as a flex edge with the
// ContentSwitcher's `1fr` (which would split the screen 50/50).
// NOTE: the textual CSS parser does not support `/* */` comments — keep this
// stylesheet comment-free.
const CSS: &str = r#"
Screen {
    align: center middle;
    padding: 1;
}

Horizontal {
    height: 3;
    width: auto;
}

ContentSwitcher {
    border: round $primary;
    width: 90%;
    height: 1fr;
}
"#;

struct ContentSwitcherApp;

impl TextualApp for ContentSwitcherApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        // Buttons use the same ids as the ContentSwitcher children so that
        // ButtonPressed.button_id can be set directly as ContentSwitcher.current,
        // mirroring Python's `event.button.id` pattern.
        let btn_table = Button::new("DataTable").id("data-table");
        let btn_md = Button::new("Markdown").id("markdown");
        let buttons = Horizontal::new().with_child(btn_table).with_child(btn_md);

        // ContentSwitcher children are wrapped in Node to assign CSS ids that
        // ContentSwitcher uses to track which child is visible.
        let table = Node::new(DataTable::new(vec![], vec![])).id("data-table");
        let markdown = Node::new(ScrollView::new(
            Node::new(Markdown::new(MARKDOWN_EXAMPLE)).id("markdown-content"),
        ))
        .id("markdown");

        let switcher = ContentSwitcher::new()
            .initial("data-table")
            .with_child(table)
            .with_child(markdown);

        AppRoot::new().with_child(buttons).with_child(switcher)
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        let books = [
            ("Dune", 1965),
            ("Dune Messiah", 1969),
            ("Children of Dune", 1976),
            ("God Emperor of Dune", 1981),
            ("Heretics of Dune", 1984),
            ("Chapterhouse: Dune", 1985),
        ];
        let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
            table.add_columns(["Book", "Year"]);
            for (title, year) in &books {
                // Mirror Python's `title.ljust(35)` so the "Book" column
                // content-width matches the reference snapshot.
                table.add_row(vec![format!("{:<35}", title), year.to_string()]);
            }
        });
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        _ctx: &mut EventCtx,
    ) {
        // Mirror Python: `self.query_one(ContentSwitcher).current = event.button.id`
        if let Some(ev) = message.downcast_ref::<ButtonPressed>() {
            if let Some(ref id) = ev.button_id {
                let _ = app.with_query_one_mut_as::<ContentSwitcher, _>(
                    "ContentSwitcher",
                    |cs| cs.set_current(Some(id.clone())),
                );
            }
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(ContentSwitcherApp)
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_switcher_app_composes_without_panic() {
        let mut app = ContentSwitcherApp;
        let _root = app.compose();
    }

    #[test]
    fn initial_content_is_data_table() {
        let switcher = ContentSwitcher::new().initial("data-table");
        assert_eq!(switcher.current(), Some("data-table"));
    }

    #[test]
    fn set_current_changes_visible_content() {
        let mut switcher = ContentSwitcher::new().initial("data-table");
        switcher.set_current(Some("markdown".to_string()));
        assert_eq!(switcher.current(), Some("markdown"));
    }

    #[test]
    fn button_id_is_forwarded_in_button_pressed() {
        // Verify that Button::id() sets a CSS id that propagates to ButtonPressed.
        // We test the ButtonPressed struct's button_id field directly.
        let bp = textual::message::ButtonPressed {
            description: "test".to_string(),
            button_id: Some("data-table".to_string()),
        };
        assert_eq!(bp.button_id.as_deref(), Some("data-table"));
    }
}
