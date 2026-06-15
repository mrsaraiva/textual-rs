/// Port of Python Textual `docs/examples/widgets/option_list_strings.py`.
///
/// Demonstrates `OptionList` populated with plain string options (no explicit
/// IDs, no separators, no disabled items) — the simplest form of `OptionList`.
///
/// Python uses `OptionList("Aerilon", "Aquaria", ...)` constructor syntax with
/// plain strings. Rust uses `OptionList::with_items(vec![OptionItem::new(...)])`.
///
/// CSS is ported from `option_list.tcss`: the screen is centered and the
/// `OptionList` occupies 70% width × 80% height.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

OptionList {
    width: 70%;
    height: 80%;
}
"#;

struct OptionListApp;

impl TextualApp for OptionListApp {
    fn title(&self) -> &'static str {
        "OptionListApp"
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let list = OptionList::with_items(vec![
            OptionItem::new("Aerilon"),
            OptionItem::new("Aquaria"),
            OptionItem::new("Canceron"),
            OptionItem::new("Caprica"),
            OptionItem::new("Gemenon"),
            OptionItem::new("Leonis"),
            OptionItem::new("Libran"),
            OptionItem::new("Picon"),
            OptionItem::new("Sagittaron"),
            OptionItem::new("Scorpia"),
            OptionItem::new("Tauron"),
            OptionItem::new("Virgon"),
        ]);
        AppRoot::new()
            .with_child(Header::new())
            .with_child(list)
            .with_child(Footer::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(OptionListApp)
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_list_app_composes_without_panic() {
        let mut app = OptionListApp;
        let _root = app.compose();
    }

    #[test]
    fn option_items_are_plain_strings() {
        let opt = OptionItem::new("Aerilon");
        match &opt {
            OptionItem::Option {
                prompt,
                id,
                disabled,
                ..
            } => {
                assert_eq!(prompt, "Aerilon");
                assert!(id.is_none(), "plain string options have no id");
                assert!(!disabled, "plain string options are not disabled");
            }
            _ => panic!("expected Option variant"),
        }
    }

    #[test]
    fn all_twelve_colonies_are_present() {
        let items = vec![
            OptionItem::new("Aerilon"),
            OptionItem::new("Aquaria"),
            OptionItem::new("Canceron"),
            OptionItem::new("Caprica"),
            OptionItem::new("Gemenon"),
            OptionItem::new("Leonis"),
            OptionItem::new("Libran"),
            OptionItem::new("Picon"),
            OptionItem::new("Sagittaron"),
            OptionItem::new("Scorpia"),
            OptionItem::new("Tauron"),
            OptionItem::new("Virgon"),
        ];
        assert_eq!(items.len(), 12);
        assert_eq!(items[0].prompt(), Some("Aerilon"));
        assert_eq!(items[11].prompt(), Some("Virgon"));
    }
}
