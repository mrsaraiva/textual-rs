/// Port of Python Textual `docs/examples/themes/colored_text.py`.
///
/// Demonstrates semantic text-color tokens. The Python source generates CSS
/// dynamically and yields one Label per color class.
use textual::prelude::*;

const CSS: &str = r#"
.text-primary   { color: $text-primary; }
.text-secondary { color: $text-secondary; }
.text-accent    { color: $text-accent; }
.text-warning   { color: $text-warning; }
.text-error     { color: $text-error; }
.text-success   { color: $text-success; }
"#;

struct ColoredText;

impl TextualApp for ColoredText {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(vec![
            ChildDecl::from(Label::new("$text-primary")).with_classes(&["text-primary"]),
            ChildDecl::from(Label::new("$text-secondary")).with_classes(&["text-secondary"]),
            ChildDecl::from(Label::new("$text-accent")).with_classes(&["text-accent"]),
            ChildDecl::from(Label::new("$text-warning")).with_classes(&["text-warning"]),
            ChildDecl::from(Label::new("$text-error")).with_classes(&["text-error"]),
            ChildDecl::from(Label::new("$text-success")).with_classes(&["text-success"]),
        ])
    }
}

fn main() -> textual::Result<()> {
    run_sync(ColoredText)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colored_text_composes_without_panic() {
        let mut app = ColoredText;
        let _root = app.compose();
    }
}
