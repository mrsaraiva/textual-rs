use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/data_table_renderables.py`.
///
/// The Python version adds `rich.text.Text` objects with `style="italic #03AC13"`
/// and `justify="right"` to every data cell.  The Rust DataTable stores plain
/// strings only (no per-cell style/justification), so cell styling is not
/// replicated here — that is a framework gap.
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
