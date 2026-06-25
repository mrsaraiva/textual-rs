use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_custom_language.py`.
const JAVA_CODE: &str = r#"class HelloWorld {
    public static void main(String[] args) {
        System.out.println("Hello, World!");
    }
}
"#;

const JAVA_HIGHLIGHTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/text_area_custom_language/java_highlights.scm"
));

struct TextAreaCustomLanguageApp;

impl TextualApp for TextAreaCustomLanguageApp {
    fn compose(&mut self) -> AppRoot {
        let mut text_area = TextArea::code_editor(JAVA_CODE).with_cursor_blink(false);
        text_area
            .register_language("java", tree_sitter_java::LANGUAGE.into(), JAVA_HIGHLIGHTS)
            .expect("failed to register Java language");
        let text_area = text_area.with_language("java");
        AppRoot::new().with_child(text_area)
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(TextAreaCustomLanguageApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS: focus the editor and type a character; the edit mutates the
    /// document and changes the rendered frame. A dead TextArea (keys not routed
    /// to editing) leaves both identical.
    #[test]
    fn liveness_type_inserts_text() {
        TextAreaCustomLanguageApp
            .run_test(|pilot| {
                pilot.press(&["tab"])?;
                let before = pilot.app().frame_fingerprint();
                pilot.press(&["X"])?;
                let after = pilot.app().frame_fingerprint();
                assert_ne!(before, after, "typing must change the rendered frame");
                Ok(())
            })
            .expect("run_test");
    }
}
