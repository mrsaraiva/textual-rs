// TextArea widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
TextArea {
    width: 1fr;
    height: 1fr;
    padding: 0 1;
    bg: $surface;
    fg: $foreground;
    border: tall $border-blurred;
}

TextArea:focus { border: tall $border; }
TextArea > .text-area--cursor { bg: $input-cursor-background; fg: $input-cursor-foreground; }
TextArea > .text-area--selection { bg: $input-selection-background; }
TextArea > .text-area--gutter { fg: $text-disabled; }
TextArea > .text-area--gutter-active { fg: $foreground; }
TextArea > .text-area--cursor-line { bg: $block-hover-background; }
"#;
