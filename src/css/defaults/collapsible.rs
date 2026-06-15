// Collapsible widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
Collapsible {
    width: 1fr;
    height: auto;
    bg: $surface;
    border-top: hkey $background;
    padding-bottom: 1;
    padding-left: 1;

    &:focus-within {
        background-tint: $foreground 5%;
    }

    &.-collapsed > Contents {
        display: none;
    }
}

Contents {
    width: 100%;
    height: auto;
    padding: 1 0 0 3;
}

CollapsibleTitle {
    width: auto;
    height: auto;
    padding: 0 1;
    text-style: $block-cursor-blurred-text-style;
    color: $block-cursor-blurred-foreground;
    pointer: pointer;

    &:hover {
        bg: $block-hover-background;
        color: $foreground;
    }
    &:focus {
        text-style: $block-cursor-text-style;
        bg: $block-cursor-background;
        color: $block-cursor-foreground;
    }
}
"#;
