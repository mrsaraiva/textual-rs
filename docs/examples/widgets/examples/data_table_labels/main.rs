use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/data_table_labels.py`.
///
/// The Python example uses `table.add_row(*row, label=Text(str(number)))` to
/// attach a row label (1–9) to each row, which DataTable renders as a non-data
/// label column to the left of the data cells. Rust uses `add_row_labeled`.
struct TableApp;

impl TextualApp for TableApp {
    fn compose(&mut self) -> AppRoot {
        let mut table = DataTable::empty();
        table.add_columns(&["lane", "swimmer", "country", "time"]);
        let rows: [[&str; 4]; 9] = [
            ["4", "Joseph Schooling", "Singapore", "50.39"],
            ["2", "Michael Phelps", "United States", "51.14"],
            ["5", "Chad le Clos", "South Africa", "51.14"],
            ["6", "László Cseh", "Hungary", "51.14"],
            ["3", "Li Zhuhao", "China", "51.26"],
            ["8", "Mehdy Metella", "France", "51.58"],
            ["7", "Tom Shields", "United States", "51.73"],
            ["1", "Aleksandr Sadovnikov", "Russia", "51.84"],
            ["10", "Darren Burns", "Scotland", "51.84"],
        ];
        for (number, row) in rows.iter().enumerate() {
            table.add_row_labeled(row.to_vec(), (number + 1).to_string());
        }
        AppRoot::new().with_child(table)
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(TableApp)
}
