use super::StyleSheet;

// Minimal built-in widget defaults to help demos look like Textual (Python) without requiring
// demo-specific CSS for core widget visuals.
//
// Note: this is a pragmatic subset of Textual's built-in widget CSS. We intentionally avoid
// full TCSS features (nesting, `&`, `!important`, tint / opacity) until the style engine grows.
const DEFAULT_WIDGET_CSS: &str = r#"
Button {
    width: auto;
    height: auto;
    min-width: 16;
    line-pad: 1;
}

Button.-style-default {
    text-style: bold;
    fg: $button-foreground;
    bg: $surface;
    border: none;
    border-top: tall $surface-lighten-1;
    border-bottom: tall $surface-darken-1;
}

Button.-style-default.-primary { fg: $button-color-foreground; bg: $primary; border-top: tall $primary-lighten-3; border-bottom: tall $primary-darken-3; }
Button.-style-default.-success { fg: $button-color-foreground; bg: $success; border-top: tall $success-lighten-2; border-bottom: tall $success-darken-3; }
Button.-style-default.-warning { fg: $button-color-foreground; bg: $warning; border-top: tall $warning-lighten-2; border-bottom: tall $warning-darken-3; }
Button.-style-default.-error { fg: $button-color-foreground; bg: $error; border-top: tall $error-lighten-2; border-bottom: tall $error-darken-3; }

Button:disabled { dim: true; }

Button.-style-flat { text-style: bold; fg: $foreground; bg: $surface; border: block $surface; }
Button.-style-flat.-primary { fg: $text; bg: $primary-muted; border: block $primary-muted; }
Button.-style-flat.-success { fg: $text; bg: $success-muted; border: block $success-muted; }
Button.-style-flat.-warning { fg: $text; bg: $warning-muted; border: block $warning-muted; }
Button.-style-flat.-error { fg: $text; bg: $error-muted; border: block $error-muted; }
"#;

pub fn default_widget_stylesheet() -> StyleSheet {
    StyleSheet::parse(DEFAULT_WIDGET_CSS)
}
