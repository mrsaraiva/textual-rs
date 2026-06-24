use textual::prelude::*;

/// Port of Python Textual's `docs/examples/widgets/data_table_renderables.py`.
///
/// The Python version wraps every data cell in a `rich.text.Text` with
/// `style="italic #03AC13"` and `justify="right"`. We now build real styled
/// `Content` cells (`DataTableCell::markup(...).with_align(TextAlign::Right)`) so
/// the cell carries its own color + italic + right-justification through the
/// content rendering subsystem — no parallel justify hack, faithful to Python.
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
        for row in ROWS {
            // Each cell: italic #03AC13, right-justified — mirrors
            // `Text(str(cell), style="italic #03AC13", justify="right")`.
            let cells: Vec<DataTableCell> = row
                .iter()
                .map(|cell| {
                    DataTableCell::markup(format!("[italic #03AC13]{cell}"))
                        .with_align(TextAlign::Right)
                })
                .collect();
            table.add_row_cells(cells);
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
