use rich_rs::{Segment, Segments};
use textual::message::{
    DataTableCellActivated, DataTableCursorMoved, DataTableHeaderSelected, Message,
};
use textual::prelude::*;
use textual::style::{Color, parse_color_like};

/// Mirrors Python Textual's `docs/examples/widgets/data_table.py`.
struct DataTableApp;

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
            StatusLine::new(),
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

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        let text = match &message.message {
            Message::DataTableCursorMoved(DataTableCursorMoved { row, column }) => {
                format!("cursor=({row},{column})")
            }
            Message::DataTableHeaderSelected(DataTableHeaderSelected { column }) => {
                format!("header=({column})")
            }
            Message::DataTableCellActivated(DataTableCellActivated { row, column }) => {
                format!("activated=({row},{column})")
            }
            _ => return,
        };
        let _ = app.with_query_one_mut_as::<StatusLine, _>("StatusLine", |status_line| {
            status_line.set_text(text);
        });
        ctx.request_repaint();
        ctx.set_handled();
    }
}

struct StatusLine {
    text: String,
}

impl StatusLine {
    fn new() -> Self {
        Self {
            text: String::new(),
        }
    }

    fn set_text(&mut self, text: String) {
        self.text = text;
    }
}

impl Widget for StatusLine {
    fn style_type(&self) -> &'static str {
        "StatusLine"
    }

    fn render(&self, _console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let line = rich_rs::set_cell_size(&format!("Events: {}", self.text), width);
        let mut out = Segments::new();
        out.push(Segment::new(line));
        out
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(DataTableApp)
}
