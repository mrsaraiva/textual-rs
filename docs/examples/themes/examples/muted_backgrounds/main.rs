/// Port of Python Textual `docs/examples/themes/muted_backgrounds.py`.
///
/// Demonstrates semantic muted-background color tokens with text-color tokens.
use textual::prelude::*;

const CSS: &str = r#"
.text-primary   { padding: 0 1; color: $text-primary;   background: $primary-muted; }
.text-secondary { padding: 0 1; color: $text-secondary; background: $secondary-muted; }
.text-accent    { padding: 0 1; color: $text-accent;    background: $accent-muted; }
.text-warning   { padding: 0 1; color: $text-warning;   background: $warning-muted; }
.text-error     { padding: 0 1; color: $text-error;     background: $error-muted; }
.text-success   { padding: 0 1; color: $text-success;   background: $success-muted; }
"#;

struct MutedBackgrounds;

impl TextualApp for MutedBackgrounds {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(vec![
            ChildDecl::from(Label::new("$text-primary on $primary-muted"))
                .with_classes(&["text-primary"]),
            ChildDecl::from(Label::new("$text-secondary on $secondary-muted"))
                .with_classes(&["text-secondary"]),
            ChildDecl::from(Label::new("$text-accent on $accent-muted"))
                .with_classes(&["text-accent"]),
            ChildDecl::from(Label::new("$text-warning on $warning-muted"))
                .with_classes(&["text-warning"]),
            ChildDecl::from(Label::new("$text-error on $error-muted"))
                .with_classes(&["text-error"]),
            ChildDecl::from(Label::new("$text-success on $success-muted"))
                .with_classes(&["text-success"]),
        ])
    }
}

fn main() -> textual::Result<()> {
    run_sync(MutedBackgrounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn muted_backgrounds_composes_without_panic() {
        let mut app = MutedBackgrounds;
        let _root = app.compose();
    }
}
