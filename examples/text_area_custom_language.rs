use textual::prelude::*;

/// Mirrors Python Textual's `docs/examples/widgets/text_area_custom_language.py`.
const JAVA_CODE: &str = r#"class HelloWorld {
    public static void main(String[] args) {
        System.out.println("Hello, World!");
    }
}
"#;

const JAVA_HIGHLIGHTS: &str = include_str!("../../textual/docs/examples/widgets/java_highlights.scm");

#[tokio::main]
async fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }

    let mut text_area = TextArea::code_editor(JAVA_CODE).with_cursor_blink(false);
    text_area.register_language("java", tree_sitter_java::LANGUAGE.into(), JAVA_HIGHLIGHTS)?;
    text_area.set_language("java");

    let mut root = AppRoot::new().with_child(text_area);
    let mut app = App::new()?;
    app.run_widget_tree(&mut root).await
}
