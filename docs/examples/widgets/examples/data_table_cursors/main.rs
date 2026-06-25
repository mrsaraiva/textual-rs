/// Port of Python Textual `docs/examples/widgets/data_table_cursors.py`.
///
/// Demonstrates DataTable cursor types:
/// - zebra_stripes is enabled on mount
/// - cursor_type starts at "column" (first element of the cycle) on mount
/// - pressing 'c' cycles through column → row → cell → none → column → ...
///
/// Python uses `itertools.cycle(["column", "row", "cell", "none"])` with
/// `next(cursors)` on mount (picks "column") and on each 'c' keypress.
use textual::prelude::*;

const ROWS: &[(&str, &str, &str, &str)] = &[
    ("lane", "swimmer", "country", "time"),
    ("4", "Joseph Schooling", "Singapore", "50.39"),
    ("2", "Michael Phelps", "United States", "51.14"),
    ("5", "Chad le Clos", "South Africa", "51.14"),
    ("6", "László Cseh", "Hungary", "51.14"),
    ("3", "Li Zhuhao", "China", "51.26"),
    ("8", "Mehdy Metella", "France", "51.58"),
    ("7", "Tom Shields", "United States", "51.73"),
    ("1", "Aleksandr Sadovnikov", "Russia", "51.84"),
    ("10", "Darren Burns", "Scotland", "51.84"),
];

const CURSOR_CYCLE: &[CursorType] = &[
    CursorType::Column,
    CursorType::Row,
    CursorType::Cell,
    CursorType::None,
];

struct TableApp {
    cursor_index: usize,
}

impl TableApp {
    fn new() -> Self {
        Self { cursor_index: 0 }
    }

    fn next_cursor(&mut self) -> CursorType {
        let ct = CURSOR_CYCLE[self.cursor_index % CURSOR_CYCLE.len()];
        self.cursor_index = (self.cursor_index + 1) % CURSOR_CYCLE.len();
        ct
    }
}

impl TextualApp for TableApp {
    fn compose(&mut self) -> AppRoot {
        let table = DataTable::empty();
        AppRoot::new().with_child(table)
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        let initial_cursor = self.next_cursor();

        if let Ok(nid) = app.query_one("DataTable") {
            let mut rctx = ReactiveCtx::new(nid);
            let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
                table.set_cursor_type(initial_cursor, &mut rctx);
                table.set_zebra_stripes(true, &mut rctx);
                table.add_columns([ROWS[0].0, ROWS[0].1, ROWS[0].2, ROWS[0].3]);
                for row in &ROWS[1..] {
                    table.add_row(vec![row.0, row.1, row.2, row.3]);
                }
            });
        }
        ctx.request_repaint();
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        if key.name() == "c" {
            let next_cursor = self.next_cursor();
            if let Ok(nid) = app.query_one("DataTable") {
                let mut rctx = ReactiveCtx::new(nid);
                let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
                    table.set_cursor_type(next_cursor, &mut rctx);
                });
            }
            ctx.set_handled();
            ctx.request_repaint();
        }
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(TableApp::new())
}

#[cfg(test)]
mod liveness {
    use super::*;
    use textual::run_test;

    /// LIVENESS: cursor_type starts at "column" on mount; pressing `c` cycles to
    /// "row", which re-highlights a different region of the DataTable. The
    /// rendered frame must change. Proves the key -> cursor-type -> render path.
    #[test]
    fn cycle_cursor_changes_frame() {
        run_test(TableApp::new(), |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["c"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing 'c' must cycle the DataTable cursor type and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
