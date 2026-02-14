// TextArea widget defaults
// DC-15: aligned with Python Textual _text_area.py DEFAULT_CSS

pub(super) const DEFAULT_CSS: &str = r#"
TextArea {
    width: 1fr;
    height: 1fr;
    border: tall $border-blurred;
    padding: 0 1;
    color: $foreground;
    bg: $surface;
    pointer: text;

    &.-textual-compact {
        border: none !important;
    }

    & .text-area--cursor {
        text-style: $input-cursor-text-style;
    }

    & .text-area--gutter {
        color: $foreground 40%;
    }

    & .text-area--cursor-gutter {
        color: $foreground 60%;
        bg: $boost;
        text-style: bold;
    }

    & .text-area--cursor-line {
        bg: $boost;
    }

    & .text-area--selection {
        bg: $input-selection-background;
    }

    & .text-area--matching-bracket {
        bg: $foreground 30%;
    }

    & .text-area--suggestion {
        color: $text-muted;
    }

    & .text-area--placeholder {
        color: $text 40%;
    }

    &:focus {
        border: tall $border;
    }

    &:ansi {
        & .text-area--selection {
            bg: transparent;
            text-style: reverse;
        }
    }

    &:dark {
        & .text-area--cursor {
            color: $input-cursor-foreground;
            bg: $input-cursor-background;
        }

        &.-read-only .text-area--cursor {
            bg: $warning-darken-1;
        }
    }

    &:light {
        & .text-area--cursor {
            color: $text 90%;
            bg: $foreground 70%;
        }

        &.-read-only .text-area--cursor {
            bg: $warning-darken-1;
        }
    }
}
"#;
