// Input and MaskedInput widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
Input {
    width: 100%;
    height: 3;
    min-width: 16;
    line-pad: 2;
    bg: $surface;
    fg: $foreground;
    border: tall $border-blurred;
}

Input:focus { border: tall $border; background-tint: $foreground 5%; }
Input.-invalid { border: tall $error; }
Input.-invalid:focus { border: tall $error; }
Input:disabled { dim: true; }
Input > .input--cursor { bg: $input-cursor-background; fg: $input-cursor-foreground; }
Input > .input--selection { bg: $input-selection-background; }
Input > .input--placeholder { fg: $text-disabled; }
Input > .input--suggestion { fg: $text-disabled; }

MaskedInput {
    width: 100%;
    height: 3;
    min-width: 16;
    line-pad: 2;
    bg: $surface;
    fg: $foreground;
    border: tall $border-blurred;
}

MaskedInput:focus { border: tall $border; background-tint: $foreground 5%; }
MaskedInput.-invalid { border: tall $error; }
MaskedInput.-invalid:focus { border: tall $error; }
MaskedInput > .input--cursor { bg: $input-cursor-background; fg: $input-cursor-foreground; }
MaskedInput > .input--selection { bg: $input-selection-background; }
MaskedInput > .input--placeholder { fg: $text-disabled; }
"#;
