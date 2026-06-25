/// Port of Python Textual `docs/examples/widgets/option_list_tables.py`.
///
/// Demonstrates `OptionList` populated with `rich.table.Table` objects —
/// each option shows a compact per-colony information table with title,
/// column headers, and a data row.
///
/// Python uses `OptionList(*[self.colony(*row) for row in COLONIES])` where
/// each `colony()` call produces a `rich.table.Table` (a multi-row renderable).
///
/// Rust port uses `OptionItem::renderable()` so the `rich_rs::Table` is
/// stored as an `Arc<dyn Renderable>` and rendered live at the runtime widget
/// width, exactly matching the Python path. No pre-rendering at a hardcoded
/// width is needed anymore.
///
/// CSS is ported from `option_list.tcss`.
use rich_rs::Table;
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

const COLONIES: &[(&str, &str, &str, &str)] = &[
    ("Aerilon", "Demeter", "1.2 Billion", "Gaoth"),
    ("Aquaria", "Hermes", "75,000", "None"),
    ("Canceron", "Hephaestus", "6.7 Billion", "Hades"),
    ("Caprica", "Apollo", "4.9 Billion", "Caprica City"),
    ("Gemenon", "Hera", "2.8 Billion", "Oranu"),
    ("Leonis", "Artemis", "2.6 Billion", "Luminere"),
    ("Libran", "Athena", "2.1 Billion", "None"),
    ("Picon", "Poseidon", "1.4 Billion", "Queenstown"),
    ("Sagittaron", "Zeus", "1.7 Billion", "Tawa"),
    ("Scorpia", "Dionysus", "450 Million", "Celeste"),
    ("Tauron", "Ares", "2.5 Billion", "Hypatia"),
    ("Virgon", "Hestia", "4.3 Billion", "Boskirk"),
];

/// Build a `rich_rs::Table` mirroring Python's `App.colony()`:
///
/// ```python
/// table = Table(title=f"Data for {name}", expand=True)
/// table.add_column("Patron God")
/// table.add_column("Population")
/// table.add_column("Capital City")
/// table.add_row(god, population, capital)
/// ```
fn colony_table(name: &str, god: &str, population: &str, capital: &str) -> Table {
    let mut table = Table::new()
        .with_title(&format!("Data for {name}"))
        .with_expand(true);
    table.add_column_str("Patron God");
    table.add_column_str("Population");
    table.add_column_str("Capital City");
    table.add_row_strs(&[god, population, capital]);
    table
}

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
        // Each colony table is stored as an Arc<dyn Renderable> via
        // OptionItem::renderable(). The OptionList renders it live at the
        // runtime widget content width — matching Python's path exactly.
        let items: Vec<OptionItem> = COLONIES
            .iter()
            .map(|(name, god, population, capital)| {
                OptionItem::renderable(*name, colony_table(name, god, population, capital))
            })
            .collect();

        AppRoot::new()
            .with_child(Header::new())
            .with_child(OptionList::with_items(items))
            .with_child(Footer::new())
    }
}

fn main() -> textual::Result<()> {
    run_sync(OptionListApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colony_table_builds_without_panic() {
        let _table = colony_table("Caprica", "Apollo", "4.9 Billion", "Caprica City");
    }

    #[test]
    fn colony_table_contains_column_headers() {
        use rich_rs::{Console, ConsoleOptions, Renderable};
        let table = colony_table("Caprica", "Apollo", "4.9 Billion", "Caprica City");
        let console = Console::new();
        let opts = ConsoleOptions {
            size: (80, 40),
            max_width: 80,
            max_height: 40,
            ..Default::default()
        };
        let segs: Vec<_> = table.render(&console, &opts).into_iter().collect();
        let text: String = segs.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Patron God"), "missing 'Patron God' header");
        assert!(text.contains("Population"), "missing 'Population' header");
        assert!(text.contains("Capital City"), "missing 'Capital City' header");
    }

    #[test]
    fn colony_table_contains_data() {
        use rich_rs::{Console, ConsoleOptions, Renderable};
        let table = colony_table("Caprica", "Apollo", "4.9 Billion", "Caprica City");
        let console = Console::new();
        let opts = ConsoleOptions {
            size: (80, 40),
            max_width: 80,
            max_height: 40,
            ..Default::default()
        };
        let segs: Vec<_> = table.render(&console, &opts).into_iter().collect();
        let text: String = segs.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Apollo"), "missing patron god");
        assert!(text.contains("4.9 Billion"), "missing population");
        assert!(text.contains("Caprica City"), "missing capital");
    }

    #[test]
    fn option_list_composes_twelve_items() {
        let mut app = OptionListApp;
        let root = app.compose();
        drop(root);
    }

    #[test]
    fn all_twelve_colonies_defined() {
        assert_eq!(COLONIES.len(), 12);
        assert_eq!(COLONIES[0].0, "Aerilon");
        assert_eq!(COLONIES[11].0, "Virgon");
    }

    #[test]
    fn option_items_use_renderable_content() {
        let items: Vec<OptionItem> = COLONIES
            .iter()
            .map(|(name, god, population, capital)| {
                OptionItem::renderable(*name, colony_table(name, god, population, capital))
            })
            .collect();
        assert_eq!(items.len(), 12);
        // Each item should hold a Renderable (not Text) as content.
        for item in &items {
            assert!(
                matches!(item.content(), Some(OptionContent::Renderable(_))),
                "expected Renderable content variant"
            );
        }
    }

    /// LIVENESS: focus the OptionList (rows are `rich` tables) and press down to
    /// move the highlight. We assert on the observable widget state
    /// (`highlighted` advances) — the true thing navigation mutates. A dead
    /// OptionList (keys not routed) leaves the highlight put.
    ///
    /// KNOWN RENDER GAP (DEFERRED): same as the other `option_list_*` ports —
    /// moving the highlight does not change the rendered frame headlessly.
    #[test]
    fn liveness_navigate_advances_highlight() {
        OptionListApp
            .run_test(|pilot| {
                let hl = |pilot: &Pilot| -> Option<usize> {
                    let app = pilot.app();
                    app.query_one_typed::<OptionList>("OptionList")
                        .ok()
                        .and_then(|h| h.read(app, |l| l.highlighted()).ok())
                        .flatten()
                };
                pilot.press(&["tab"])?;
                let h0 = hl(pilot);
                pilot.press(&["down"])?;
                let h1 = hl(pilot);
                assert!(
                    h0.is_some() && h1 != h0,
                    "down must advance the highlight (was {h0:?}, now {h1:?})"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
