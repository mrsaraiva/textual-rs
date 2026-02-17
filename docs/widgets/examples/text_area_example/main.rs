use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_example.py`.
const TEXT: &str = r#"def hello(name):
    print("hello" + name)

def goodbye(name):
    print("goodbye" + name)
"#;

struct TextAreaExampleApp;

impl TextualApp for TextAreaExampleApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(TextArea::code_editor(TEXT).with_language("python"))
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(TextAreaExampleApp)
}
