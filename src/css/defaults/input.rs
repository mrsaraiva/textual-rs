// Input widget defaults
// DC-14: aligned with Python Textual _input.py DEFAULT_CSS

pub(super) const DEFAULT_CSS: &str = r#"
Input {
    bg: $surface;
    color: $foreground;
    padding: 0 2;
    border: tall $border-blurred;
    width: 100%;
    height: 3;
    scrollbar-size-horizontal: 0;
    pointer: text;

    &.-textual-compact {
        border: none !important;
        height: 1;
        padding: 0;

        &.-invalid {
            background-tint: $error 20%;
        }
    }

    &:focus {
        border: tall $border;
        background-tint: $foreground 5%;
    }

    & > .input--cursor {
        bg: $input-cursor-background;
        color: $input-cursor-foreground;
        text-style: $input-cursor-text-style;
    }

    & > .input--selection {
        bg: $input-selection-background;
    }

    & > .input--placeholder {
        color: $text-disabled;
    }

    & > .input--suggestion {
        color: $text-disabled;
    }

    &.-invalid {
        border: tall $error 60%;
    }

    &.-invalid:focus {
        border: tall $error;
    }

    &:ansi {
        bg: ansi_default;
        color: ansi_default;

        & > .input--cursor {
            bg: ansi_white;
            color: ansi_black;
        }

        & > .input--placeholder {
            text-style: dim;
            color: ansi_default;
        }

        & > .input--suggestion {
            text-style: dim;
            color: ansi_default;
        }

        &.-invalid {
            border: tall ansi_red;
        }

        &.-invalid:focus {
            border: tall ansi_red;
        }
    }
}

MaskedInput {
    bg: $surface;
    color: $foreground;
    padding: 0 2;
    border: tall $border-blurred;
    width: 100%;
    height: 3;
    scrollbar-size-horizontal: 0;
    pointer: text;

    &.-textual-compact {
        border: none !important;
        height: 1;
        padding: 0;

        &.-invalid {
            background-tint: $error 20%;
        }
    }

    &:focus {
        border: tall $border;
        background-tint: $foreground 5%;
    }

    & > .input--cursor {
        bg: $input-cursor-background;
        color: $input-cursor-foreground;
        text-style: $input-cursor-text-style;
    }

    & > .input--selection {
        bg: $input-selection-background;
    }

    & > .input--placeholder {
        color: $text-disabled;
    }

    & > .input--suggestion {
        color: $text-disabled;
    }

    &.-invalid {
        border: tall $error 60%;
    }

    &.-invalid:focus {
        border: tall $error;
    }

    &:ansi {
        bg: ansi_default;
        color: ansi_default;

        & > .input--cursor {
            bg: ansi_white;
            color: ansi_black;
        }

        & > .input--placeholder {
            text-style: dim;
            color: ansi_default;
        }

        & > .input--suggestion {
            text-style: dim;
            color: ansi_default;
        }

        &.-invalid {
            border: tall ansi_red;
        }

        &.-invalid:focus {
            border: tall ansi_red;
        }
    }
}
"#;
