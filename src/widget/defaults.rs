use super::StyleSheet;

// Minimal built-in widget defaults to help demos look like Textual (Python) without requiring
// demo-specific CSS for core widget visuals.
//
// Note: this intentionally uses class selectors (e.g. `.button.primary`) rather than pseudo-classes
// like `:disabled` / `:focus` since our selector engine currently supports type/id/class only.
const DEFAULT_WIDGET_CSS: &str = r#"
Button {
    width: auto;
    height: auto;
    min-width: 16;
    line-pad: 1;
    fg: color(15);
    bg: color(236);
    border-top: color(239);
    border-bottom: color(233);
}

.button.primary { fg: color(15); bg: color(27); border-top: color(33); border-bottom: color(19); }
.button.success { fg: color(15); bg: color(34); border-top: color(40); border-bottom: color(28); }
.button.warning { fg: color(16); bg: color(220); border-top: color(228); border-bottom: color(178); }
.button.error { fg: color(15); bg: color(196); border-top: color(203); border-bottom: color(160); }

.button.disabled { dim: true; }
.button.flat { border-top: none; border-bottom: none; }
"#;

pub fn default_widget_stylesheet() -> StyleSheet {
    StyleSheet::parse(DEFAULT_WIDGET_CSS)
}

