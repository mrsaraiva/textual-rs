use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_custom_language.py`.
const JAVA_CODE: &str = r#"class HelloWorld {
    public static void main(String[] args) {
        System.out.println("Hello, World!");
    }
}
"#;

const JAVA_HIGHLIGHTS: &str =
    include_str!("../../textual/docs/examples/widgets/java_highlights.scm");

struct TextAreaCustomLanguageApp {
    text_area: Option<TextArea>,
}

impl TextAreaCustomLanguageApp {
    fn new() -> Result<Self> {
        let mut text_area = TextArea::code_editor(JAVA_CODE).with_cursor_blink(false);
        text_area.register_language("java", tree_sitter_java::LANGUAGE.into(), JAVA_HIGHLIGHTS)?;
        text_area.set_language("java");
        Ok(Self {
            text_area: Some(text_area),
        })
    }
}

impl TextualApp for TextAreaCustomLanguageApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            self.text_area
                .take()
                .unwrap_or_else(|| TextArea::code_editor(JAVA_CODE).with_cursor_blink(false)),
        )
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(TextAreaCustomLanguageApp::new()?)
}
