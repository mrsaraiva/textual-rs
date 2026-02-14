// Checkbox / ToggleButton widget defaults
// DC-35: aligned with Python Textual _toggle_button.py DEFAULT_CSS

pub(super) const DEFAULT_CSS: &str = r#"
ToggleButton {
    width: auto;
    border: tall $border-blurred;
    padding: 0 1;
    bg: $surface;
    text-wrap: nowrap;
    text-overflow: ellipsis;
    pointer: pointer;

    &.-textual-compact {
        border: none !important;
        padding: 0;

        &:focus {
            border: tall $border;
            background-tint: $foreground 5%;

            & > .toggle--label {
                color: $block-cursor-foreground;
                bg: $block-cursor-background;
                text-style: $block-cursor-text-style;
            }
        }
    }

    & > .toggle--button {
        color: $panel-darken-2;
        bg: $panel;
    }

    &.-on > .toggle--button {
        color: $text-success;
        bg: $panel;
    }

    &:focus {
        border: tall $border;
        background-tint: $foreground 5%;

        & > .toggle--label {
            color: $block-cursor-foreground;
            bg: $block-cursor-background;
            text-style: $block-cursor-text-style;
        }
    }

    &:blur:hover {
        & > .toggle--label {
            bg: $block-hover-background;
        }
    }
}

Checkbox {
    width: auto;
    border: tall $border-blurred;
    padding: 0 1;
    bg: $surface;
    text-wrap: nowrap;
    text-overflow: ellipsis;
    pointer: pointer;

    &.-textual-compact {
        border: none !important;
        padding: 0;

        &:focus {
            border: tall $border;
            background-tint: $foreground 5%;

            & > .toggle--label {
                color: $block-cursor-foreground;
                bg: $block-cursor-background;
                text-style: $block-cursor-text-style;
            }
        }
    }

    & > .toggle--button {
        color: $panel-darken-2;
        bg: $panel;
    }

    &.-on > .toggle--button {
        color: $text-success;
        bg: $panel;
    }

    &:focus {
        border: tall $border;
        background-tint: $foreground 5%;

        & > .toggle--label {
            color: $block-cursor-foreground;
            bg: $block-cursor-background;
            text-style: $block-cursor-text-style;
        }
    }

    &:blur:hover {
        & > .toggle--label {
            bg: $block-hover-background;
        }
    }
}
"#;
