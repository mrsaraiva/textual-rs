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
        color: $foreground 10%;
    }

    & > .tabs--tab {
        color: $foreground 50%;
    }

    & > .tabs--tab.-hover {
        color: $foreground;
    }

    & > .tabs--tab.-disabled {
        color: $foreground 25%;
    }

    & > .tabs--tab.-active {
        color: $foreground;
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
        #tabs-list, & > .tabs--tab {
            text-style: dim;
        }

        & #tabs-list > .-active, & > .tabs--tab.-active {
            text-style: not dim;
        }

        &:focus {
            #tabs-list > .-active, & > .tabs--tab.-active {
                text-style: bold not dim;
            }
        }

        & > .tabs--underline {
            color: ansi_bright_blue;
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
        color: $foreground 30%;
    }

    & > .tabs--underline.-active {
        color: $block-cursor-background;
    }

    & > .tabs--underline.-active.-focus {
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
        color: $foreground 10%;
    }

    & > .tabbed-content--tab {
        color: $foreground 50%;
    }

    & > .tabbed-content--tab.-hover {
        color: $foreground;
    }

    & > .tabbed-content--tab.-disabled {
        color: $foreground 25%;
    }

    & > .tabbed-content--tab.-active {
        color: $foreground;
    }

    & > .tabbed-content--tab.-active.-focus {
        text-style: $block-cursor-text-style;
        color: $block-cursor-foreground;
        bg: $block-cursor-background;
    }

    & > .tabbed-content--underline.-focus {
        color: $foreground 30%;
    }

    & > .tabbed-content--underline.-active {
        color: $block-cursor-background;
    }

    & > .tabbed-content--underline.-active.-focus {
        color: $block-cursor-background;
    }

    &:ansi {
        & > .tabbed-content--tab {
            text-style: dim;
        }

        & > .tabbed-content--tab.-active {
            text-style: not dim;
        }

        &:focus {
            & > .tabbed-content--tab.-active {
                text-style: bold not dim;
            }
        }

        & > .tabbed-content--underline {
            color: ansi_bright_blue;
        }

        & > .tabbed-content--underline.-active {
            color: ansi_bright_blue;
        }

        & > .tabbed-content--underline.-active.-focus {
            color: ansi_bright_blue;
        }

        & > .tabbed-content--tab.-active {
            color: transparent;
            bg: transparent;
        }

        & > .tabbed-content--tab.-active.-focus {
            color: transparent;
            bg: transparent;
        }
    }
}
"#;
