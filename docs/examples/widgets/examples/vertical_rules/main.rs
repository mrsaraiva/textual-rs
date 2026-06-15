/// Port of Python Textual `docs/examples/widgets/vertical_rules.py`.
///
/// Demonstrates the `Rule` widget (vertical separator) with all available
/// line styles: solid (default), heavy, thick, dashed, double, and ascii.
///
/// Each rule is preceded by a `Label` describing the style, all contained in
/// a `Horizontal` container that is centered on the screen.
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    align: center middle;
}

Horizontal {
    width: auto;
    height: 80%;
}

Label {
    width: 6;
    height: 100%;
    text-align: center;
}
"#;

struct VerticalRulesApp;

impl TextualApp for VerticalRulesApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let horizontal = Horizontal::new()
            .with_child(Label::new("solid"))
            .with_child(Rule::vertical())
            .with_child(Label::new("heavy"))
            .with_child(Rule::vertical().line_style(LineStyle::Heavy))
            .with_child(Label::new("thick"))
            .with_child(Rule::vertical().line_style(LineStyle::Thick))
            .with_child(Label::new("dashed"))
            .with_child(Rule::vertical().line_style(LineStyle::Dashed))
            .with_child(Label::new("double"))
            .with_child(Rule::vertical().line_style(LineStyle::Double))
            .with_child(Label::new("ascii"))
            .with_child(Rule::vertical().line_style(LineStyle::Ascii));

        AppRoot::new().with_child(horizontal)
    }
}

fn main() -> textual::Result<()> {
    run_sync(VerticalRulesApp)
}

// ---------------------------------------------------------------------------
// Regression tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_rules_app_composes_without_panic() {
        let mut app = VerticalRulesApp;
        let _root = app.compose();
    }

    #[test]
    fn rule_default_is_solid() {
        let r = Rule::vertical();
        assert_eq!(r.get_line_style(), LineStyle::Solid);
    }

    #[test]
    fn all_line_styles_constructable() {
        let _heavy = Rule::vertical().line_style(LineStyle::Heavy);
        let _thick = Rule::vertical().line_style(LineStyle::Thick);
        let _dashed = Rule::vertical().line_style(LineStyle::Dashed);
        let _double = Rule::vertical().line_style(LineStyle::Double);
        let _ascii = Rule::vertical().line_style(LineStyle::Ascii);
    }
}
