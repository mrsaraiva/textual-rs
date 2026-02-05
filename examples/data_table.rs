use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/data_table.py`.
#[tokio::main]
async fn main() -> Result<()> {
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

    let mut root = AppRoot::new().with_child(table);
    let mut app = App::new()?;
    app.run_widget_tree(&mut root).await
}
