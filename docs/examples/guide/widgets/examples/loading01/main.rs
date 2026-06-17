/// Port of Python Textual `docs/examples/guide/widgets/loading01.py`.
///
/// Demonstrates the `loading` state on widgets:
/// - Four DataTables laid out in a 2-column grid.
/// - On mount, each table is set to `loading = true` (shows LoadingIndicator
///   overlay) and a background worker is spawned to simulate a slow data fetch.
/// - When each worker finishes it deposits its data into a shared queue; on
///   `WorkerStateChanged::Success` the app drains the queue, populates the
///   matching table, and sets `loading = false`.
///
/// Python uses `asyncio.sleep(randint(2, 10))` per table.  The Rust port
/// simulates this with `std::thread::sleep` and a deterministic random-ish
/// delay derived from the table index (2-5 seconds per table) so the test
/// remains reproducible without an external RNG dependency.
use std::sync::{Arc, Mutex};
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

/// Pending data deposited by a background worker: (table_id, rows).
type PendingData = Vec<(String, Vec<Vec<String>>)>;

struct DataApp {
    /// Shared queue between workers and the app message handler.
    pending: Arc<Mutex<PendingData>>,
}

impl DataApp {
    fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(Vec::new())),
        }
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

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        // Set all four tables into loading state.
        for i in 0..4usize {
            let selector = format!("#table{i}");
            if let Ok(q) = app.query_mut(&selector) {
                q.set(None, None, None, Some(true));
            }
        }

        // Spawn one background worker per table.
        for i in 0..4usize {
            let pending = Arc::clone(&self.pending);
            let table_id = format!("table{i}");
            // Simulate randint(2, 10) seconds; use a deterministic spread so
            // the demo shows staggered completion (2, 3, 4, 5 seconds).
            let delay_secs = 2 + i;
            ctx.request_worker_task(Some(&format!("load-{i}")), move |token| {
                std::thread::sleep(std::time::Duration::from_secs(delay_secs as u64));
                if token.is_cancelled() {
                    return Ok(());
                }
                // Convert &[&str] rows to owned Vec<Vec<String>>.
                let rows: Vec<Vec<String>> = ROWS
                    .iter()
                    .map(|row| row.iter().map(|c| (*c).to_string()).collect())
                    .collect();
                pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((table_id, rows));
                Ok(())
            });
        }
        ctx.request_repaint();
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Some(w) = message.downcast_ref::<WorkerStateChanged>() {
            if matches!(w.state, WorkerState::Success) {
                // Drain completed table data from the shared queue.
                let completed: PendingData = {
                    let mut guard = self.pending.lock().unwrap_or_else(|e| e.into_inner());
                    std::mem::take(&mut *guard)
                };
                for (table_id, rows) in completed {
                    let selector = format!("#{table_id}");
                    // Populate the table widget.
                    let _ = app.with_query_one_mut_as::<DataTable, _>(&selector, |table| {
                        table.add_columns(HEADERS);
                        table.add_rows(rows.iter());
                    });
                    // Clear loading state.
                    if let Ok(q) = app.query_mut(&selector) {
                        q.set(None, None, None, Some(false));
                    }
                }
                ctx.request_repaint();
            }
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(DataApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
