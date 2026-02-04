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
    text-style: bold;
    fg: $button-foreground;
    bg: $surface;
    border-top: tall $surface-lighten-1;
    border-bottom: tall $surface-darken-1;
}

.button.primary { fg: $button-color-foreground; bg: $primary; border-top: tall $primary-lighten-3; border-bottom: tall $primary-darken-3; }
.button.success { fg: $button-color-foreground; bg: $success; border-top: tall $success-lighten-2; border-bottom: tall $success-darken-3; }
.button.warning { fg: $button-color-foreground; bg: $warning; border-top: tall $warning-lighten-2; border-bottom: tall $warning-darken-3; }
.button.error { fg: $button-color-foreground; bg: $error; border-top: tall $error-lighten-2; border-bottom: tall $error-darken-3; }

.button.disabled { dim: true; }
.button.flat { border-top: none; border-bottom: none; }
"#;

pub fn default_widget_stylesheet() -> StyleSheet {
    StyleSheet::parse(DEFAULT_WIDGET_CSS)
}
