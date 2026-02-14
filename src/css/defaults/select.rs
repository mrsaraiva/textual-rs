// Select, OptionList, and SelectionList widget defaults
// DC-29: OptionList aligned with Python Textual _option_list.py DEFAULT_CSS
// DC-30: SelectionList aligned with Python Textual _selection_list.py DEFAULT_CSS

pub(super) const DEFAULT_CSS: &str = r#"
OptionList {
    height: auto;
    max-height: 100%;
    color: $foreground;
    overflow-x: hidden;
    border: tall $border-blurred;
    padding: 0 1;
    bg: $surface;

    &.-textual-compact {
        border: none !important;
        padding: 0;

        & > .option-list--option {
            padding: 0;
        }
    }

    & > .option-list--option-highlighted {
        color: $block-cursor-blurred-foreground;
        bg: $block-cursor-blurred-background;
        text-style: $block-cursor-blurred-text-style;
    }

    &:focus {
        border: tall $border;
        background-tint: $foreground 5%;

        & > .option-list--option-highlighted {
            color: $block-cursor-foreground;
            bg: $block-cursor-background;
            text-style: $block-cursor-text-style;
        }
    }

    & > .option-list--separator {
        color: $foreground 15%;
    }

    & > .option-list--option-highlighted {
        color: $foreground;
        bg: $block-cursor-blurred-background;
    }

    & > .option-list--option-disabled {
        color: $text-disabled;
    }

    & > .option-list--option-hover {
        bg: $block-hover-background;
    }
}

SelectCurrent {
    border: tall $border-blurred;
    color: $foreground;
    bg: $surface;
    width: 1fr;
    height: auto;
    padding: 0 2;
    pointer: pointer;

    &.-textual-compact {
        border: none !important;
    }

    &:ansi {
        border: tall ansi_blue;
        color: ansi_default;
        bg: ansi_default;
    }

    Static#label {
        width: 1fr;
        height: auto;
        color: $foreground 50%;
        bg: transparent;
    }

    &.-has-value Static#label {
        color: $foreground;
    }

    .arrow {
        box-sizing: content-box;
        width: 1;
        height: 1;
        padding: 0 0 0 1;
        color: $foreground 50%;
        bg: transparent;
    }
}

Select {
    height: auto;
    color: $foreground;

    &.-textual-compact {
        & > SelectCurrent {
            padding: 0 1 0 0;
            border: none !important;
        }
    }

    .up-arrow {
        display: none;
    }

    &:focus > SelectCurrent {
        border: tall $border;
        background-tint: $foreground 5%;
    }

    & > SelectOverlay {
        width: 1fr;
        display: none;
        height: auto;
        max-height: 12;
        overlay: screen;
        constrain-x: none;
        constrain-y: inside;
        color: $foreground;
        border: tall $border-blurred;
        bg: $surface;

        &:focus {
            background-tint: $foreground 5%;
        }

        & > .option-list--option {
            padding: 0 1;
        }
    }

    &.-expanded {
        .down-arrow {
            display: none;
        }

        .up-arrow {
            display: block;
        }

        & > SelectOverlay {
            display: block;
        }
    }
}

SelectionList {
    height: auto;
    text-wrap: nowrap;
    text-overflow: ellipsis;

    & > .selection-list--button {
        color: $panel-darken-2;
        bg: $panel;
    }

    & > .selection-list--button-highlighted {
        color: $panel-darken-2;
        bg: $panel;
    }

    & > .selection-list--button-selected {
        color: $text-success;
        bg: $panel;
    }

    & > .selection-list--button-selected-highlighted {
        color: $text-success;
        bg: $panel;
    }
}
"#;
