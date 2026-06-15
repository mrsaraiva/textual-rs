use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/data_table_labels.py`.
///
/// The Python example uses `table.add_row(*row, label=Text(str(number), style="#B0FC38 italic"))`
/// to attach styled row labels (1–9 in italic green) to each row. The Rust DataTable has a
/// `show_row_labels` boolean flag and `add_row_with_key`, but does not yet support per-row
/// label text or styled label rendering. This is a known framework gap.
struct TableApp;

impl TextualApp for TableApp {
    fn compose(&mut self) -> AppRoot {
        let mut table = DataTable::empty();
        table.add_columns(&["lane", "swimmer", "country", "time"]);
        table.add_rows(&[
            &["4", "Joseph Schooling", "Singapore", "50.39"],
            &["2", "Michael Phelps", "United States", "51.14"],
            &["5", "Chad le Clos", "South Africa", "51.14"],
            &["6", "László Cseh", "Hungary", "51.14"],
            &["3", "Li Zhuhao", "China", "51.26"],
            &["8", "Mehdy Metella", "France", "51.58"],
            &["7", "Tom Shields", "United States", "51.73"],
            &["1", "Aleksandr Sadovnikov", "Russia", "51.84"],
            &["10", "Darren Burns", "Scotland", "51.84"],
        ]);
        AppRoot::new().with_child(table)
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(TableApp)
}
