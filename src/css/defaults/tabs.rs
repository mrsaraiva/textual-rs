// Tabs and TabbedContent widget defaults
// DC-31: aligned with Python Textual _tabs.py and _tabbed_content.py DEFAULT_CSS

pub(super) const DEFAULT_CSS: &str = r#"
Underline {
    width: 1fr;
    height: 1;

    & > .underline--bar {
        color: $block-cursor-background;
        bg: $foreground 10%;
    }

    &:ansi {
        text-style: dim;
    }
}

Tab {
    width: auto;
    height: 1;
    padding: 0 1;
    text-align: center;
    color: $foreground 50%;
    pointer: pointer;

    &:hover {
        color: $foreground;
    }

    &:disabled {
        color: $foreground 25%;
    }

    &.-active {
        color: $foreground;
    }

    &.-hidden {
        display: none;
    }
}

Tabs {
    width: 100%;
    height: 2;

    & > .tabs--underline {
        color: $foreground 30%;
    }

    & > #tabs-scroll {
        overflow: hidden;
    }

    #tabs-list {
        width: auto;
    }

    #tabs-list-bar, #tabs-list {
        width: auto;
        height: auto;
        min-width: 100%;
        overflow: hidden hidden;
    }

    &:focus {
        & .-active {
            text-style: $block-cursor-text-style;
            color: $block-cursor-foreground;
            bg: $block-cursor-background;
        }
    }

    &:ansi {
        #tabs-list {
            text-style: dim;
        }

        & #tabs-list > .-active {
            text-style: not dim;
        }

        &:focus {
            #tabs-list > .-active {
                text-style: bold not dim;
            }
        }

        & > .tabs--underline {
            color: ansi_bright_blue;
            bg: ansi_default;
        }

        & .-active {
            color: transparent;
            bg: transparent;
        }
    }

    & > .tabs--tab.-active.-focus {
        text-style: $block-cursor-text-style;
        color: $block-cursor-foreground;
        bg: $block-cursor-background;
    }

    & > .tabs--underline.-focus {
        bg: $surface-lighten-1;
    }

    & > .tabs--underline.-active {
        color: $block-cursor-background;
    }

    & > .tabs--underline.-active.-focus {
        bg: $surface-lighten-1;
        color: $block-cursor-background;
    }
}

TabPane {
    height: auto;
}

TabbedContent {
    height: auto;

    & > ContentTabs {
        dock: top;
    }

    & > .tabbed-content--underline {
        color: $foreground 30%;
    }

    & > .tabbed-content--tab.-active.-focus {
        text-style: $block-cursor-text-style;
        color: $block-cursor-foreground;
        bg: $block-cursor-background;
    }

    & > .tabbed-content--underline.-focus {
        bg: $surface-lighten-1;
    }

    & > .tabbed-content--underline.-active {
        color: $block-cursor-background;
    }

    & > .tabbed-content--underline.-active.-focus {
        bg: $surface-lighten-1;
        color: $block-cursor-background;
    }
}
"#;
