// Tabs and TabbedContent widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
Tabs {
    bg: $surface;
    fg: $foreground;
}

Tabs > .tabs--bar { bg: $panel; fg: $foreground; }
Tabs > .tabs--tab { bg: $panel; fg: $text-disabled; text-style: bold; }
Tabs > .tabs--tab.-hover { bg: $panel; fg: $foreground; }
Tabs > .tabs--tab.-active { bg: $panel; fg: $foreground; }
Tabs > .tabs--tab.-active.-focus {
    bg: $block-cursor-background;
    fg: $block-cursor-foreground;
    text-style: $button-focus-text-style;
}
Tabs > .tabs--underline { bg: $panel-darken-1; fg: $foreground; text-style: dim; }
Tabs > .tabs--underline.-focus { bg: $surface-lighten-1; fg: $foreground; text-style: dim; }
Tabs > .tabs--underline.-active {
    bg: $panel-darken-1;
    fg: $primary;
    transition: tabs.underline 300ms ease-in-out;
}
Tabs > .tabs--underline.-active.-focus {
    bg: $surface-lighten-1;
    fg: $primary;
    transition: tabs.underline 300ms ease-in-out;
}

TabbedContent {
    bg: $surface;
    fg: $foreground;
}

TabbedContent > .tabbed-content--bar { bg: $panel; fg: $foreground; }
TabbedContent > .tabbed-content--tab { bg: $panel; fg: $text-disabled; text-style: bold; }
TabbedContent > .tabbed-content--tab.-hover { bg: $panel; fg: $foreground; }
TabbedContent > .tabbed-content--tab.-active { bg: $panel; fg: $foreground; }
TabbedContent > .tabbed-content--tab.-active.-focus {
    bg: $block-cursor-background;
    fg: $block-cursor-foreground;
    text-style: $button-focus-text-style;
}
TabbedContent > .tabbed-content--underline { bg: $panel-darken-1; fg: $foreground; text-style: dim; }
TabbedContent > .tabbed-content--underline.-focus {
    bg: $surface-lighten-1;
    fg: $foreground;
    text-style: dim;
}
TabbedContent > .tabbed-content--underline.-active {
    bg: $panel-darken-1;
    fg: $primary;
    transition: tabbed-content.underline 300ms ease-in-out;
}
TabbedContent > .tabbed-content--underline.-active.-focus {
    bg: $surface-lighten-1;
    fg: $primary;
    transition: tabbed-content.underline 300ms ease-in-out;
}
"#;
