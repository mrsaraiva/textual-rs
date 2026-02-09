use std::sync::{Arc, Mutex};

use rich_rs::{Segment, Segments};
use textual::prelude::*;
use textual::style::{Color, parse_color_like};

/// Mirrors Python Textual's `docs/examples/widgets/data_table.py`.
struct DataTableApp {
    status: Arc<Mutex<String>>,
}

impl DataTableApp {
    fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(String::new())),
        }
    }
}

impl TextualApp for DataTableApp {
    fn compose(&mut self) -> AppRoot {
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

        let status_line = Styled::new(
            StatusLine::new(self.status.clone()),
            Style::new()
                .line_pad(1)
                .bg(parse_color_like("$panel").unwrap_or(Color::parse("#303a43").unwrap()))
                .border_top(Color::parse("#44cc44").unwrap())
                .border_right(Color::parse("#44cc44").unwrap())
                .border_bottom(Color::parse("#44cc44").unwrap())
                .border_left(Color::parse("#44cc44").unwrap()),
        );

        AppRoot::new().with_child(
            Dock::new()
                .push_fill(ScrollView::new(table))
                .push_bottom(Some(3), status_line),
        )
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        let text = match &message.message {
            Message::DataTableCursorMoved { row, column } => {
                format!("cursor=({row},{column})")
            }
            Message::DataTableHeaderSelected { column } => {
                format!("header=({column})")
            }
            Message::DataTableCellActivated { row, column } => {
                format!("activated=({row},{column})")
            }
            _ => return,
        };
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = text;
        ctx.request_repaint();
        ctx.set_handled();
    }
}

struct StatusLine {
    id: WidgetId,
    text: Arc<Mutex<String>>,
}

impl StatusLine {
    fn new(text: Arc<Mutex<String>>) -> Self {
        Self {
            id: WidgetId::new(),
            text,
        }
    }
}

impl Widget for StatusLine {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, _console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let text = self.text.lock().unwrap_or_else(|e| e.into_inner());
        let line = rich_rs::set_cell_size(&format!("Events: {text}"), width);
        let mut out = Segments::new();
        out.push(Segment::new(line));
        out
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(DataTableApp::new())
}
