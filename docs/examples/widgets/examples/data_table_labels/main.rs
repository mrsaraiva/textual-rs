use textual::prelude::*;

/// Port of Python Textual's `docs/examples/widgets/data_table_labels.py`.
///
/// Python attaches a styled row label per row:
/// `table.add_row(*row, label=Text(str(number), style="#B0FC38 italic"))`.
/// Row labels are now styled `Content`, so we pass `Content::from_markup` and the
/// label renders with its own color + italic (faithful to Python).
struct TableApp;

const ROWS: &[[&str; 4]] = &[
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

impl TextualApp for TableApp {
    fn compose(&mut self) -> AppRoot {
        let mut table = DataTable::empty();
        table.add_columns(["lane", "swimmer", "country", "time"]);
        for (number, row) in ROWS.iter().enumerate() {
            let label = Content::from_markup(format!("[#B0FC38 italic]{}", number + 1));
            table.add_row_labeled(row.to_vec(), label);
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
