// Button widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
Button {
    width: auto;
    height: auto;
    min-width: 16;
    line-pad: 1;
    content-align: center middle;
    text-align: center;
}

Button.-style-default {
    text-style: bold;
    fg: $button-foreground;
    bg: $surface;
    border: none;
    border-top: tall $surface-lighten-1;
    border-bottom: tall $surface-darken-1;
}

Button.-style-default:focus { text-style: $button-focus-text-style; background-tint: $foreground 5%; }
Button.-style-default:hover { bg: $surface-darken-1; border-top: tall $surface; }

Button.-style-default.-primary { fg: $button-color-foreground; bg: $primary; border-top: tall $primary-lighten-3; border-bottom: tall $primary-darken-3; }
Button.-style-default.-success { fg: $button-color-foreground; bg: $success; border-top: tall $success-lighten-2; border-bottom: tall $success-darken-3; }
Button.-style-default.-warning { fg: $button-color-foreground; bg: $warning; border-top: tall $warning-lighten-2; border-bottom: tall $warning-darken-3; }
Button.-style-default.-error { fg: $button-color-foreground; bg: $error; border-top: tall $error-lighten-2; border-bottom: tall $error-darken-3; }

Button.-style-default.-primary:hover { bg: $primary-darken-2; border-top: tall $primary; }
Button.-style-default.-success:hover { bg: $success-darken-2; border-top: tall $success; }
Button.-style-default.-warning:hover { bg: $warning-darken-2; border-top: tall $warning; }
Button.-style-default.-error:hover { bg: $error-darken-1; border-top: tall $error; }

Button.-style-default:active { border-top: tall $surface-darken-1; border-bottom: tall $surface-lighten-1; background-tint: $background 30%; }
Button.-style-default.-primary:active { border-top: tall $primary-darken-3; border-bottom: tall $primary-lighten-3; background-tint: $background 30%; }
Button.-style-default.-success:active { border-top: tall $success-darken-2; border-bottom: tall $success-lighten-2; background-tint: $background 30%; }
Button.-style-default.-warning:active { border-top: tall $warning-darken-2; border-bottom: tall $warning-lighten-2; background-tint: $background 30%; }
Button.-style-default.-error:active { border-top: tall $error-darken-2; border-bottom: tall $error-lighten-2; background-tint: $background 30%; }

Button.-style-default:disabled { text-opacity: 60%; }
Button:disabled { opacity: 70%; }

Button.-style-flat { text-style: bold; fg: $text; bg: $surface; border: block $surface; }
Button.-style-flat.-primary { fg: $text-primary; bg: $primary-muted; border: block $primary-muted; }
Button.-style-flat.-success { fg: $text-success; bg: $success-muted; border: block $success-muted; }
Button.-style-flat.-warning { fg: $text-warning; bg: $warning-muted; border: block $warning-muted; }
Button.-style-flat.-error { fg: $text-error; bg: $error-muted; border: block $error-muted; }

Button.-style-flat:focus { text-style: $button-focus-text-style; background-tint: $foreground 5%; }

Button.-style-flat:hover { fg: $text; bg: $primary; border: block $primary; }
Button.-style-flat.-primary:hover { fg: $text; bg: $primary; border: block $primary; }
Button.-style-flat.-success:hover { fg: $text; bg: $success; border: block $success; }
Button.-style-flat.-warning:hover { fg: $text; bg: $warning; border: block $warning; }
Button.-style-flat.-error:hover { fg: $text; bg: $error; border: block $error; }

Button.-style-flat:active { background-tint: $background 30%; }
Button.-style-flat.-primary:active { background-tint: $background 30%; }
Button.-style-flat.-success:active { background-tint: $background 30%; }
Button.-style-flat.-warning:active { background-tint: $background 30%; }
Button.-style-flat.-error:active { background-tint: $background 30%; }
Button.-style-flat:disabled { fg: auto 50%; }
"#;
