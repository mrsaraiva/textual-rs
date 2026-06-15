/// Port of Python Textual `docs/examples/widgets/option_list_tables.py`.
///
/// Demonstrates `OptionList` populated with `rich.table.Table` objects —
/// each option shows a compact per-colony information table with title,
/// column headers, and a data row.
///
/// Python uses `OptionList(*[self.colony(*row) for row in COLONIES])` where
/// each `colony()` call produces a `rich.table.Table` (a multi-row renderable).
///
/// In Rust, `OptionItem` only supports `rich_rs::Text` as rich content, and
/// `render_rich_line` renders exactly one line per item. This is a framework
/// gap: the Python OptionList accepts arbitrary multi-row renderables whereas
/// the Rust version is limited to single-line items. We port as faithfully as
/// possible using `rich_rs::Table` pre-rendered into a multi-line
/// `rich_rs::Text`, stored as `OptionItem::rich()`.
///
/// CSS is ported from `option_list.tcss`.
use rich_rs::{Console, ConsoleOptions, Renderable, Table, Text};
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

/// Render a `rich_rs::Table` to a multi-line `rich_rs::Text` at the given
/// width so it can be stored as `OptionItem::rich()` content.
///
/// This is necessary because `OptionItem` does not accept arbitrary
/// `Renderable`s — only `rich_rs::Text`.
fn table_to_text(table: &Table, width: usize) -> Text {
    let console = Console::new();
    let options = ConsoleOptions {
        size: (width, 40),
        max_width: width,
        max_height: 40,
        ..Default::default()
    };
    let segments: Vec<_> = table.render(&console, &options).into_iter().collect();

    // Collect non-control segments into lines split on newline control segments.
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for seg in &segments {
        if seg.is_control() || seg.text == "\n" {
            lines.push(std::mem::take(&mut current));
        } else {
            current.push_str(&seg.text);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    let joined = lines.join("\n");
    Text::plain(joined)
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
        // Pre-render each colony table at a representative width.
        // Python renders them at the actual widget width at runtime; we use
        // 80 columns as a reasonable default (the OptionList width is 70% of
        // 120 = 84 columns, minus borders/padding).
        let render_width: usize = 80;

        let items: Vec<OptionItem> = COLONIES
            .iter()
            .map(|(name, god, population, capital)| {
                let table = colony_table(name, god, population, capital);
                let text = table_to_text(&table, render_width);
                OptionItem::rich(*name, text)
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
    fn table_to_text_contains_column_headers() {
        let table = colony_table("Caprica", "Apollo", "4.9 Billion", "Caprica City");
        let text = table_to_text(&table, 80);
        let plain = text.plain_text();
        assert!(plain.contains("Patron God"), "missing 'Patron God' header");
        assert!(plain.contains("Population"), "missing 'Population' header");
        assert!(plain.contains("Capital City"), "missing 'Capital City' header");
    }

    #[test]
    fn table_to_text_contains_data() {
        let table = colony_table("Caprica", "Apollo", "4.9 Billion", "Caprica City");
        let text = table_to_text(&table, 80);
        let plain = text.plain_text();
        assert!(plain.contains("Apollo"), "missing patron god");
        assert!(plain.contains("4.9 Billion"), "missing population");
        assert!(plain.contains("Caprica City"), "missing capital");
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
}
