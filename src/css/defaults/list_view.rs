// ListView widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
ListView {
    bg: $surface;
}

ListView > ListItem {
    fg: $foreground;
    height: auto;
    overflow-x: hidden;
    overflow-y: hidden;
    width: 1fr;
}

ListView > ListItem.-hovered {
    bg: $block-hover-background;
}

ListView > ListItem.-highlight {
    fg: $block-cursor-blurred-foreground;
    bg: $block-cursor-blurred-background;
    text-style: $block-cursor-blurred-text-style;
}

ListView:focus {
    background-tint: $foreground 5%;
}

ListView:focus > ListItem.-highlight {
    fg: $block-cursor-foreground;
    bg: $block-cursor-background;
    text-style: $block-cursor-text-style;
}
"#;
