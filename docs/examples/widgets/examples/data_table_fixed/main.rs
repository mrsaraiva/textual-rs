/// Port of Python Textual `docs/examples/widgets/data_table_fixed.py`.
///
/// Demonstrates DataTable fixed rows/columns and zebra stripes:
/// - Columns: A, B, C
/// - 99 rows (1..=99) with values n, n*2, n*3
/// - fixed_rows = 2, fixed_columns = 1
/// - cursor_type = "row", zebra_stripes = true
use textual::prelude::*;

const CSS: &str = r#"
DataTable {height: 1fr}
"#;

struct TableApp;

impl TextualApp for TableApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let table = DataTable::empty().cursor_type(CursorType::Row);
        AppRoot::new().with_child(table)
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
            table.add_columns(["A", "B", "C"]);
            for number in 1usize..=99 {
                table.add_row(vec![
                    number.to_string(),
                    (number * 2).to_string(),
                    (number * 3).to_string(),
                ]);
            }
        });

        // Set fixed_rows, fixed_columns, zebra_stripes via reactive setters.
        if let Ok(nid) = app.query_one("DataTable") {
            let mut rctx = ReactiveCtx::new(nid);
            let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
                table.set_fixed_rows(2, &mut rctx);
                table.set_fixed_columns(1, &mut rctx);
                table.set_zebra_stripes(true, &mut rctx);
            });
        }

        // Focus the table on mount.
        let _ = app.query_mut("DataTable").map(|q| q.focus());
        ctx.request_repaint();
    }
}

fn main() -> textual::Result<()> {
    run_sync(TableApp)
}

#[cfg(test)]
mod liveness {
    use super::*;
    use textual::run_test;

    /// LIVENESS: the table is focused on mount with `cursor_type = Row`; pressing
    /// `down` moves the row cursor past the fixed rows, re-highlighting a
    /// different row. The rendered frame must change.
    #[test]
    fn arrow_moves_row_cursor() {
        run_test(TableApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["down"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing 'down' must move the row cursor and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
