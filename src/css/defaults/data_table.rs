// DataTable widget defaults
// DC-16: aligned with Python Textual _data_table.py DEFAULT_CSS

pub(super) const DEFAULT_CSS: &str = r#"
DataTable {
    bg: $surface;
    color: $foreground;
    height: auto;
    max-height: 100%;

    &.datatable--fixed-cursor {
        bg: $block-cursor-blurred-background;
    }

    &:focus {
        background-tint: $foreground 5%;
    }

    &:dark {
        & > .datatable--even-row {
            bg: $surface-darken-1 40%;
        }
    }

    & > .datatable--header {
        text-style: bold;
        bg: $panel;
        color: $foreground;
    }

    &:ansi > .datatable--header {
        bg: ansi_bright_blue;
        color: ansi_default;
    }

    & > .datatable--fixed {
        bg: $secondary-muted;
        color: $foreground;
    }

    & > .datatable--even-row {
        bg: $surface-lighten-1 50%;
    }

    & > .datatable--cursor {
        bg: $block-cursor-blurred-background;
        color: $block-cursor-blurred-foreground;
        text-style: $block-cursor-blurred-text-style;
    }

    &:focus > .datatable--cursor {
        bg: $block-cursor-background;
        color: $block-cursor-foreground;
        text-style: $block-cursor-text-style;
    }

    & > .datatable--fixed-cursor {
        bg: $block-cursor-blurred-background;
        color: $foreground;
    }

    &:focus > .datatable--fixed-cursor {
        color: $block-cursor-foreground;
        bg: $block-cursor-background;
    }

    &:focus > .datatable--header {
        background-tint: $foreground 5%;
    }

    & > .datatable--header-cursor {
        bg: $accent-darken-1;
        color: $foreground;
    }

    & > .datatable--header-hover {
        bg: $accent 30%;
    }

    & > .datatable--hover {
        bg: $block-hover-background;
    }
}
"#;
