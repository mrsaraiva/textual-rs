// Tree widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
Tree {
    bg: $surface;
    fg: $foreground;
}

Tree > .tree--node { fg: $foreground; }
Tree > .tree--node.-hover { bg: $block-hover-background; }
Tree > .tree--node.-highlighted { bg: $primary-muted; fg: $text; text-style: bold; }
Tree > .tree--node.-highlighted.-focus { bg: $primary; fg: $text; text-style: bold; }
Tree > .tree--node.-leaf { dim: true; }
Tree > .tree--node.-disabled { color: $text-disabled; dim: true; }
"#;
