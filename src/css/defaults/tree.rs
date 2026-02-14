// Tree and DirectoryTree widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
Tree {
    bg: $surface;
    fg: $foreground;
}

Tree > .tree--label {}

Tree > .tree--guides {
    fg: $surface-lighten-2;
}

Tree > .tree--guides-hover {
    fg: $surface-lighten-2;
}

Tree > .tree--guides-selected {
    fg: $block-cursor-blurred-background;
}

Tree > .tree--cursor {
    text-style: $block-cursor-blurred-text-style;
    bg: $block-cursor-blurred-background;
}

Tree > .tree--highlight {}

Tree > .tree--highlight-line {
    bg: $block-hover-background;
}

Tree:focus {
    background-tint: $foreground 5%;
}

Tree:focus > .tree--cursor {
    fg: $block-cursor-foreground;
    bg: $block-cursor-background;
    text-style: $block-cursor-text-style;
}

Tree:focus > .tree--guides {
    fg: $surface-lighten-3;
}

Tree:focus > .tree--guides-hover {
    fg: $surface-lighten-3;
}

Tree:focus > .tree--guides-selected {
    fg: $block-cursor-background;
}

Tree:light > .tree--guides {
    fg: $surface-darken-1;
}

Tree:light > .tree--guides-hover {
    fg: $block-cursor-background;
}

Tree:light > .tree--guides-selected {
    fg: $block-cursor-background;
}

Tree:ansi {
    fg: ansi_default;
}

Tree:ansi > .tree--guides {
    fg: ansi_green;
}

DirectoryTree > .directory-tree--folder { text-style: bold; }
DirectoryTree > .directory-tree--extension { text-style: italic; }
DirectoryTree > .directory-tree--hidden { text-style: dim; }

DirectoryTree:ansi > .tree--guides { fg: transparent; }
DirectoryTree:ansi > .directory-tree--folder { text-style: bold; }
DirectoryTree:ansi > .directory-tree--extension { text-style: italic; }
DirectoryTree:ansi > .directory-tree--hidden { fg: ansi_default; text-style: dim; }
"#;
