/// Port of Python Textual `docs/examples/widgets/option_list_options.py`.
///
/// Demonstrates `OptionList` with named options (`Option(..., id=...)`) and
/// separators (`None` in Python → `OptionItem::Separator` in Rust), including
/// a disabled option (`Option("Caprica", disabled=True)`).
///
/// Python uses `OptionList(Option(...), None, ...)` constructor syntax.
/// Rust uses `OptionList::with_items(vec![...])` with `OptionItem::with_id`,
/// `OptionItem::disabled_with_id`, and `OptionItem::Separator`.
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
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let list = OptionList::with_items(vec![
            OptionItem::with_id("Aerilon", "aer"),
            OptionItem::with_id("Aquaria", "aqu"),
            OptionItem::Separator,
            OptionItem::with_id("Canceron", "can"),
            OptionItem::disabled_with_id("Caprica", "cap"),
            OptionItem::Separator,
            OptionItem::with_id("Gemenon", "gem"),
            OptionItem::Separator,
            OptionItem::with_id("Leonis", "leo"),
            OptionItem::with_id("Libran", "lib"),
            OptionItem::Separator,
            OptionItem::with_id("Picon", "pic"),
            OptionItem::Separator,
            OptionItem::with_id("Sagittaron", "sag"),
            OptionItem::with_id("Scorpia", "sco"),
            OptionItem::Separator,
            OptionItem::with_id("Tauron", "tau"),
            OptionItem::Separator,
            OptionItem::with_id("Virgon", "vir"),
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
    fn option_items_build_correctly() {
        let opt = OptionItem::with_id("Aerilon", "aer");
        match &opt {
            OptionItem::Option { prompt, id, disabled, .. } => {
                assert_eq!(prompt, "Aerilon");
                assert_eq!(id.as_ref().map(|o| o.as_str()), Some("aer"));
                assert!(!disabled);
            }
            _ => panic!("expected Option variant"),
        }
    }

    #[test]
    fn disabled_option_has_disabled_flag() {
        let opt = OptionItem::disabled_with_id("Caprica", "cap");
        match &opt {
            OptionItem::Option { disabled, .. } => assert!(*disabled),
            _ => panic!("expected Option variant"),
        }
    }

    #[test]
    fn separator_variant_is_separator() {
        assert!(matches!(OptionItem::Separator, OptionItem::Separator));
    }
}
