/// Port of Python Textual `docs/examples/widgets/data_table_sort.py`.
///
/// Demonstrates DataTable sorting with real key functions + multi-column sort,
/// faithful to Python (no approximations):
/// - `a` sort by average of the two time columns, then last name (custom key over
///   the `swimmer`, `time 1`, `time 2` columns) — `sort_by([1,3,4], …)`
/// - `n` sort by last name of swimmer (lambda over the `swimmer` column)
/// - `c` sort by country name (the country cell is styled `Content`; the key uses
///   its plain text)
/// - `d` multi-column sort by `swimmer` then `lane`, no key — `sort_by_columns`
///
/// Country cells are real styled `Content` (`[italic]`), mirroring Python's
/// `Text("Singapore", style="italic")`.
use std::collections::HashSet;
use textual::prelude::*;

// (lane, swimmer, country, time 1, time 2)
const ROWS: &[(&str, &str, &str, &str, &str)] = &[
    ("4", "Joseph Schooling", "Singapore", "50.39", "51.84"),
    ("2", "Michael Phelps", "United States", "50.39", "51.84"),
    ("5", "Chad le Clos", "South Africa", "51.14", "51.73"),
    ("6", "László Cseh", "Hungary", "51.14", "51.58"),
    ("3", "Li Zhuhao", "China", "51.26", "51.26"),
    ("8", "Mehdy Metella", "France", "51.58", "52.15"),
    ("7", "Tom Shields", "United States", "51.73", "51.12"),
    ("1", "Aleksandr Sadovnikov", "Russia", "51.84", "50.85"),
    ("10", "Darren Burns", "Scotland", "51.84", "51.55"),
];

// Column indices (Python uses column keys; the order matches add_columns).
const COL_LANE: usize = 0;
const COL_SWIMMER: usize = 1;
const COL_COUNTRY: usize = 2;
const COL_TIME1: usize = 3;
const COL_TIME2: usize = 4;

struct TableApp {
    current_sorts: HashSet<String>,
}

impl TableApp {
    fn new() -> Self {
        Self {
            current_sorts: HashSet::new(),
        }
    }

    /// Mirror Python `sort_reverse`: toggle ascending/descending per sort type.
    fn sort_reverse(&mut self, sort_type: &str) -> bool {
        if self.current_sorts.contains(sort_type) {
            self.current_sorts.remove(sort_type);
            true
        } else {
            self.current_sorts.insert(sort_type.to_string());
            false
        }
    }
}

impl TextualApp for TableApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("a", "sort_by_average_time", "Sort By Average Time"),
            BindingDecl::new("n", "sort_by_last_name", "Sort By Last Name"),
            BindingDecl::new("c", "sort_by_country", "Sort By Country"),
            BindingDecl::new("d", "sort_by_columns", "Sort By Columns (Only)"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        let mut table = DataTable::empty();
        table.add_columns(["lane", "swimmer", "country", "time 1", "time 2"]);
        for &(lane, swimmer, country, t1, t2) in ROWS {
            table.add_row_cells(vec![
                DataTableCell::text(lane),
                DataTableCell::text(swimmer),
                // Python: Text("…", style="italic") — a real styled Content cell.
                DataTableCell::markup(format!("[italic]{country}")),
                DataTableCell::text(t1),
                DataTableCell::text(t2),
            ]);
        }
        AppRoot::new().with_child(table).with_child(Footer::new())
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        match key.name() {
            "a" => {
                // Sort by average of time1/time2, then last name. The key receives
                // [swimmer, time 1, time 2] (the selected columns' plain text).
                let reverse = self.sort_reverse("time");
                let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
                    table.sort_by(&[COL_SWIMMER, COL_TIME1, COL_TIME2], reverse, |vals| {
                        let name = vals.first().copied().unwrap_or("");
                        let scores: Vec<f64> = vals[1..]
                            .iter()
                            .filter_map(|s| s.trim().parse::<f64>().ok())
                            .collect();
                        let avg = if scores.is_empty() {
                            0.0
                        } else {
                            scores.iter().sum::<f64>() / scores.len() as f64
                        };
                        let last = name.split_whitespace().last().unwrap_or("").to_string();
                        SortKey::tuple([SortKey::number(avg), SortKey::str(last)])
                    });
                });
                ctx.set_handled();
                ctx.request_repaint();
            }
            "n" => {
                // Sort by last name of swimmer (lambda over the swimmer column).
                let reverse = self.sort_reverse("swimmer");
                let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
                    table.sort_by(&[COL_SWIMMER], reverse, |vals| {
                        let name = vals.first().copied().unwrap_or("");
                        SortKey::str(name.split_whitespace().last().unwrap_or(""))
                    });
                });
                ctx.set_handled();
                ctx.request_repaint();
            }
            "c" => {
                // Sort by country (the country cell is styled Content; the key uses
                // its plain text, mirroring Python `lambda country: country.plain`).
                let reverse = self.sort_reverse("country");
                let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
                    table.sort_by(&[COL_COUNTRY], reverse, |vals| {
                        SortKey::str(vals.first().copied().unwrap_or(""))
                    });
                });
                ctx.set_handled();
                ctx.request_repaint();
            }
            "d" => {
                // Multi-column sort by swimmer then lane (no key).
                let reverse = self.sort_reverse("columns");
                let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
                    table.sort_by_columns(&[COL_SWIMMER, COL_LANE], reverse);
                });
                ctx.set_handled();
                ctx.request_repaint();
            }
            _ => {}
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(TableApp::new())
}
