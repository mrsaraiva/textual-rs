// Select, OptionList, and SelectionList widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
OptionList {
    height: auto;
    bg: $surface;
    fg: $foreground;
    border: tall $border-blurred;
    line-pad: 1;
}

OptionList:focus { border: tall $border; background-tint: $foreground 5%; }
OptionList > .option-list--option { fg: $foreground; }
OptionList > .option-list--option.-highlighted { fg: $foreground; bg: $primary-muted; text-style: bold; }
OptionList > .option-list--option.-highlighted.-focus { fg: $text; bg: $primary; text-style: bold; }
OptionList > .option-list--option.-disabled { fg: $text-disabled; }
OptionList > .option-list--option.-hover { bg: $block-hover-background; }
OptionList > .option-list--separator { fg: $text-disabled; }

Select {
    height: auto;
    fg: $foreground;
    border: tall $border-blurred;
    bg: $surface;
}

Select:focus { border: tall $border; background-tint: $foreground 5%; }
Select > .select--current-value { fg: $foreground; bg: $surface; }
Select > .select--current-value.-hover { bg: $surface-lighten-1; }
Select > .select--current-value.-focus { fg: $foreground; bg: $surface-lighten-1; text-style: bold; }
Select > .select--arrow { fg: $text-disabled; bg: $surface; }
Select > .select--arrow.-open { fg: $text; bg: $surface; }
Select > .select--dropdown { bg: $surface; fg: $foreground; }

SelectionList {
    height: auto;
    bg: $surface;
    fg: $foreground;
    border: tall $border-blurred;
    line-pad: 1;
}

SelectionList:focus { border: tall $border; background-tint: $foreground 5%; }
SelectionList > .selection-list--button { fg: $panel-darken-2; bg: $panel; }
SelectionList > .selection-list--button-highlighted { fg: $panel-darken-2; bg: $panel; }
SelectionList > .selection-list--button-selected { fg: $text-success; bg: $panel; }
SelectionList > .selection-list--button-selected-highlighted { fg: $text-success; bg: $panel; }
"#;
