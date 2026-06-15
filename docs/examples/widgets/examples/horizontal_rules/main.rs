/// Port of Python Textual `docs/examples/widgets/horizontal_rules.py`.
///
/// Demonstrates the `Rule` widget (horizontal separator) with all available
/// line styles: solid (default), heavy, thick, dashed, double, and ascii.
///
/// Each rule is preceded by a `Label` describing the style, all contained in
/// a `Vertical` container that is centered on the screen.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

Vertical {
    height: auto;
    width: 80%;
}

Label {
    width: 100%;
    text-align: center;
}
"#;

struct HorizontalRulesApp;

impl TextualApp for HorizontalRulesApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let vertical = Vertical::new()
            .with_child(Label::new("solid (default)"))
            .with_child(Rule::horizontal())
            .with_child(Label::new("heavy"))
            .with_child(Rule::horizontal().line_style(LineStyle::Heavy))
            .with_child(Label::new("thick"))
            .with_child(Rule::horizontal().line_style(LineStyle::Thick))
            .with_child(Label::new("dashed"))
            .with_child(Rule::horizontal().line_style(LineStyle::Dashed))
            .with_child(Label::new("double"))
            .with_child(Rule::horizontal().line_style(LineStyle::Double))
            .with_child(Label::new("ascii"))
            .with_child(Rule::horizontal().line_style(LineStyle::Ascii));

        AppRoot::new().with_child(vertical)
    }
}

fn main() -> textual::Result<()> {
    run_sync(HorizontalRulesApp)
}

// ---------------------------------------------------------------------------
// Regression tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_rules_app_composes_without_panic() {
        let mut app = HorizontalRulesApp;
        let _root = app.compose();
    }

    #[test]
    fn rule_default_is_solid() {
        let r = Rule::horizontal();
        assert_eq!(r.get_line_style(), LineStyle::Solid);
    }

    #[test]
    fn all_line_styles_constructable() {
        let _heavy = Rule::horizontal().line_style(LineStyle::Heavy);
        let _thick = Rule::horizontal().line_style(LineStyle::Thick);
        let _dashed = Rule::horizontal().line_style(LineStyle::Dashed);
        let _double = Rule::horizontal().line_style(LineStyle::Double);
        let _ascii = Rule::horizontal().line_style(LineStyle::Ascii);
    }
}
