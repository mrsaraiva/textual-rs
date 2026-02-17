use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_selection.py`.
const TEXT: &str = r#"def hello(name):
    print("hello" + name)

def goodbye(name):
    print("goodbye" + name)
"#;

struct TextAreaSelectionApp;

impl TextualApp for TextAreaSelectionApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            TextArea::code_editor(TEXT)
                .with_language("python")
                .with_selection(TextAreaSelection {
                    start: TextAreaCursor { row: 0, col: 0 },
                    end: TextAreaCursor { row: 2, col: 0 },
                }),
        )
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(TextAreaSelectionApp)
}
