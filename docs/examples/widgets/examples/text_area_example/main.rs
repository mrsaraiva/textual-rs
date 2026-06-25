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

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS: focus the TextArea and type a character; the edit must mutate
    /// the document and change the rendered frame. A dead TextArea (keys not
    /// routed to editing) leaves both identical.
    #[test]
    fn liveness_type_inserts_text() {
        TextAreaExampleApp
            .run_test(|pilot| {
                pilot.press(&["tab"])?; // focus the editor
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["X"])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(
                    before, after,
                    "typing into the TextArea must change the rendered frame"
                );
                Ok(())
            })
            .expect("run_test");
    }
}
