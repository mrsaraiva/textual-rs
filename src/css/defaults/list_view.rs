// ListView widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
ListView {
    bg: $surface;
    fg: $foreground;
}

ListView > .list-view--item { fg: $foreground; }
ListView > .list-view--item.-hover { bg: $block-hover-background; }
ListView > .list-view--item.-highlighted { bg: $primary-muted; fg: $text; text-style: bold; }
ListView > .list-view--item.-highlighted.-focus { bg: $primary; fg: $text; text-style: bold; }
ListView > .list-view--item.-disabled { color: $text-disabled; }
"#;
