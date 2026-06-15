/// Port of Python Textual `docs/examples/widgets/data_table_sort.py`.
///
/// Demonstrates DataTable sorting:
/// - `a` sorts by average time (custom key: average of time1/time2 then last name)
/// - `n` sorts by last name of swimmer (lambda: last word of swimmer name)
/// - `c` sorts by country name
/// - `d` sorts by columns only (swimmer then lane, no key)
///
/// NOTE: The Python version uses custom key functions (sort by average time via a
/// closure, sort by last name via a lambda, multi-column sort). The Rust DataTable
/// `sort()` API only supports single-column lexicographic sort (column index +
/// reverse). Fully faithful custom-key and multi-column sorting is a framework gap.
/// This port uses the best available approximation via `sort(col_index, reverse)`.
use textual::prelude::*;
use std::collections::HashSet;

const ROWS: &[(&str, &str, &str, &str, &str)] = &[
    ("lane", "swimmer", "country", "time 1", "time 2"),
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

struct TableApp {
    current_sorts: HashSet<String>,
}

impl TableApp {
    fn new() -> Self {
        Self {
            current_sorts: HashSet::new(),
        }
    }

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
        let header = ROWS[0];
        table.add_columns(&[header.0, header.1, header.2, header.3, header.4]);
        for row in &ROWS[1..] {
            table.add_row(vec![row.0, row.1, row.2, row.3, row.4]);
        }
        AppRoot::new()
            .with_child(table)
            .with_child(Footer::new())
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        match key.name() {
            "a" => {
                // Sort by average time then last name.
                // Framework gap: custom key sort not supported; approximate with time 1 (col 3).
                let reverse = self.sort_reverse("time");
                let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
                    table.sort(3, reverse);
                });
                ctx.set_handled();
                ctx.request_repaint();
            }
            "n" => {
                // Sort by last name of swimmer.
                // Framework gap: last-name sort not supported; approximate with swimmer col (col 1) lexicographic.
                let reverse = self.sort_reverse("swimmer");
                let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
                    table.sort(1, reverse);
                });
                ctx.set_handled();
                ctx.request_repaint();
            }
            "c" => {
                // Sort by country.
                let reverse = self.sort_reverse("country");
                let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
                    table.sort(2, reverse);
                });
                ctx.set_handled();
                ctx.request_repaint();
            }
            "d" => {
                // Sort by columns (swimmer + lane) — no key.
                // Framework gap: multi-column sort not supported; approximate with swimmer (col 1).
                let reverse = self.sort_reverse("columns");
                let _ = app.with_query_one_mut_as::<DataTable, _>("DataTable", |table| {
                    table.sort(1, reverse);
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
