use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_selection.py`.
const TEXT: &str = r#"def hello(name):
    print("hello" + name)

def goodbye(name):
    print("goodbye" + name)
"#;

#[tokio::main]
async fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }

    let selection = TextAreaSelection {
        start: TextAreaCursor { row: 0, col: 0 },
        end: TextAreaCursor { row: 2, col: 0 },
    };

    let editor = TextArea::code_editor(TEXT)
        .with_language("python")
        .with_selection(selection);

    let mut root = AppRoot::new().with_child(editor);
    let mut app = App::new()?;
    app.run_widget_tree(&mut root).await
}
