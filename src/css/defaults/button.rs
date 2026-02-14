// Button widget defaults
// DC-13: aligned with Python Textual _button.py DEFAULT_CSS

pub(super) const DEFAULT_CSS: &str = r#"
Button {
    width: auto;
    min-width: 16;
    height: auto;
    line-pad: 1;
    text-align: center;
    content-align: center middle;
    pointer: pointer;

    &.-style-flat {
        text-style: bold;
        color: auto 90%;
        bg: $surface;
        border: block $surface;

        &:hover {
            bg: $primary;
            border: block $primary;
        }

        &:focus {
            text-style: $button-focus-text-style;
        }

        &.-active {
            bg: $surface;
            border: block $surface;
            tint: $background 30%;
        }

        &:disabled {
            color: auto 50%;
            pointer: not-allowed;
        }

        &.-primary {
            bg: $primary-muted;
            border: block $primary-muted;
            color: $text-primary;
        }

        &.-primary:hover {
            color: $text;
            bg: $primary;
            border: block $primary;
        }

        &.-success {
            bg: $success-muted;
            border: block $success-muted;
            color: $text-success;
        }

        &.-success:hover {
            color: $text;
            bg: $success;
            border: block $success;
        }

        &.-warning {
            bg: $warning-muted;
            border: block $warning-muted;
            color: $text-warning;
        }

        &.-warning:hover {
            color: $text;
            bg: $warning;
            border: block $warning;
        }

        &.-error {
            bg: $error-muted;
            border: block $error-muted;
            color: $text-error;
        }

        &.-error:hover {
            color: $text;
            bg: $error;
            border: block $error;
        }
    }

    &.-style-default {
        text-style: bold;
        color: $button-foreground;
        bg: $surface;
        border: none;
        border-top: tall $surface-lighten-1;
        border-bottom: tall $surface-darken-1;

        &.-textual-compact {
            border: none !important;
        }

        &:disabled {
            text-opacity: 60%;
            pointer: not-allowed;
        }

        &:focus {
            text-style: $button-focus-text-style;
            background-tint: $foreground 5%;
        }

        &:hover {
            border-top: tall $surface;
            bg: $surface-darken-1;
        }

        &.-active {
            bg: $surface;
            border-bottom: tall $surface-lighten-1;
            border-top: tall $surface-darken-1;
            tint: $background 30%;
        }

        &.-primary {
            color: $button-color-foreground;
            bg: $primary;
            border-top: tall $primary-lighten-3;
            border-bottom: tall $primary-darken-3;
        }

        &.-primary:hover {
            bg: $primary-darken-2;
            border-top: tall $primary;
        }

        &.-primary.-active {
            bg: $primary;
            border-bottom: tall $primary-lighten-3;
            border-top: tall $primary-darken-3;
        }

        &.-success {
            color: $button-color-foreground;
            bg: $success;
            border-top: tall $success-lighten-2;
            border-bottom: tall $success-darken-3;
        }

        &.-success:hover {
            bg: $success-darken-2;
            border-top: tall $success;
        }

        &.-success.-active {
            bg: $success;
            border-bottom: tall $success-lighten-2;
            border-top: tall $success-darken-2;
        }

        &.-warning {
            color: $button-color-foreground;
            bg: $warning;
            border-top: tall $warning-lighten-2;
            border-bottom: tall $warning-darken-3;
        }

        &.-warning:hover {
            bg: $warning-darken-2;
            border-top: tall $warning;
        }

        &.-warning.-active {
            bg: $warning;
            border-bottom: tall $warning-lighten-2;
            border-top: tall $warning-darken-2;
        }

        &.-error {
            color: $button-color-foreground;
            bg: $error;
            border-top: tall $error-lighten-2;
            border-bottom: tall $error-darken-3;
        }

        &.-error:hover {
            bg: $error-darken-1;
            border-top: tall $error;
        }

        &.-error.-active {
            bg: $error;
            border-bottom: tall $error-lighten-2;
            border-top: tall $error-darken-2;
        }
    }
}
"#;
