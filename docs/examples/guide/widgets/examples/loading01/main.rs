/// Port of Python Textual `docs/examples/guide/widgets/loading01.py`.
///
/// Demonstrates the `loading` state on widgets:
/// - Four DataTables laid out in a 2-column grid.
/// - On mount, each table is set to `loading = true` (shows LoadingIndicator
///   overlay) and a delayed load is scheduled to simulate a slow data fetch.
/// - When each delay elapses, the matching table is populated and its loading
///   state cleared.
///
/// Python uses an `@work` coroutine that `await sleep(randint(2, 10))` then
/// fills the table and clears `loading`.  The Rust port mirrors that with a
/// framework `set_timer(delay, ...)` — `set_timer` *is* the deterministic
/// analogue of `await sleep(delay)`: it schedules a one-shot callback on the
/// runtime timer clock, so it fires on the real wall clock when the app runs
/// live, and is driven by `Pilot::advance_clock(delay)` under the headless
/// test harness (no wall-clock `thread::sleep`, which the manual clock cannot
/// fast-forward). A deterministic staggered delay (2-5 s) derived from the
/// table index stands in for `randint(2, 10)`.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    layout: grid;
    grid-size: 2;
}
DataTable {
    height: 1fr;
}
"#;

const HEADERS: &[&str] = &["lane", "swimmer", "country", "time"];

const ROWS: &[&[&str]] = &[
    &["4", "Joseph Schooling", "Singapore", "50.39"],
    &["2", "Michael Phelps", "United States", "51.14"],
    &["5", "Chad le Clos", "South Africa", "51.14"],
    &["6", "László Cseh", "Hungary", "51.14"],
    &["3", "Li Zhuhao", "China", "51.26"],
    &["8", "Mehdy Metella", "France", "51.58"],
    &["7", "Tom Shields", "United States", "51.73"],
    &["1", "Aleksandr Sadovnikov", "Russia", "51.84"],
    &["10", "Darren Burns", "Scotland", "51.84"],
];

/// Populate `selector`'s DataTable with the swimming data and clear its loading
/// state — the body of Python's `load_data` after the `await sleep(...)`.
fn finish_load(app: &mut App, selector: &str) {
    let rows: Vec<Vec<String>> = ROWS
        .iter()
        .map(|row| row.iter().map(|c| (*c).to_string()).collect())
        .collect();
    let _ = app.with_query_one_mut_as::<DataTable, _>(selector, |table| {
        table.add_columns(HEADERS);
        table.add_rows(rows.iter());
    });
    if let Ok(q) = app.query_mut(selector) {
        q.set(None, None, None, Some(false));
    }
}

struct DataApp;

impl DataApp {
    fn new() -> Self {
        Self
    }
}

impl TextualApp for DataApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(vec![
            ChildDecl::from(DataTable::empty()).with_id("table0"),
            ChildDecl::from(DataTable::empty()).with_id("table1"),
            ChildDecl::from(DataTable::empty()).with_id("table2"),
            ChildDecl::from(DataTable::empty()).with_id("table3"),
        ])
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Python on_mount: for each DataTable -> loading = True; load_data(...).
        for i in 0..4usize {
            let selector = format!("#table{i}");
            if let Ok(q) = app.query_mut(&selector) {
                q.set(None, None, None, Some(true));
            }

            // Schedule the deferred load. `set_timer` is the framework analogue
            // of Python's `await sleep(delay)` inside the `@work` coroutine:
            // fires on the wall clock live, driven by `advance_clock` headless.
            // randint(2, 10) -> deterministic staggered 2/3/4/5 s per table.
            let delay = std::time::Duration::from_secs((2 + i) as u64);
            app.set_timer(
                delay,
                Box::new(move |app, _ctx| {
                    finish_load(app, &selector);
                }),
            );
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(DataApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn data_app_composes_without_panic() {
        let mut app = DataApp::new();
        let root = app.compose();
        // Four DataTable children expected.
        assert_eq!(root.children().len(), 4);
    }

    #[test]
    fn rows_data_is_consistent() {
        // Verify every row has the same column count as the headers.
        let ncols = HEADERS.len();
        for row in ROWS {
            assert_eq!(row.len(), ncols, "Row column count mismatch");
        }
    }

    /// LIVENESS PROBE (Pilot run_test) — the loading → data transition.
    ///
    /// On mount each table is set `loading = true` (LoadingIndicator overlay)
    /// and a `set_timer(2..5 s, ...)` load is scheduled. Because the demo now
    /// schedules the delay on the framework timer clock (not a wall-clock
    /// `thread::sleep`), `Pilot::advance_clock` drives it deterministically:
    /// before advancing, every table is empty (loading shown); after advancing
    /// past the longest delay, every table is populated and loading is cleared.
    /// The rendered frame must change across that transition, and the row count
    /// of each DataTable must go from 0 to the full ROWS length.
    #[test]
    fn liveness_advance_clock_fills_tables() {
        textual::run_test(DataApp::new(), |pilot| {
            assert!(pilot.clock_is_manual());

            // Before the load completes: every table is still empty.
            for i in 0..4usize {
                let sel = format!("#table{i}");
                let count = pilot
                    .app_mut()
                    .with_query_one_mut_as::<DataTable, _>(&sel, |t| t.row_count())
                    .expect("table present");
                assert_eq!(count, 0, "{sel} must be empty before the load fires");
            }
            let before = pilot.app().frame_fingerprint();

            // Advance past the longest staggered delay (table3 = 5 s).
            pilot.advance_clock(Duration::from_secs(6))?;

            // After the load: every table is populated and loading cleared.
            for i in 0..4usize {
                let sel = format!("#table{i}");
                let count = pilot
                    .app_mut()
                    .with_query_one_mut_as::<DataTable, _>(&sel, |t| t.row_count())
                    .expect("table present");
                assert_eq!(
                    count,
                    ROWS.len(),
                    "{sel} must be fully populated after advance_clock"
                );
            }
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "the loading -> data transition must change the rendered frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
