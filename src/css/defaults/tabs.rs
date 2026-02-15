// Tabs and TabbedContent widget defaults
// DC-31: aligned with Python Textual _tabs.py and _tabbed_content.py DEFAULT_CSS

pub(super) const DEFAULT_CSS: &str = r#"
Underline {
    width: 1fr;
    height: 1;

    & > .underline--bar {
        color: $block-cursor-background;
        background: $foreground 10%;
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

    &:focus {
        .underline--bar {
            background: $foreground 30%;
        }

        & .-active {
            text-style: $block-cursor-text-style;
            color: $block-cursor-foreground;
            background: $block-cursor-background;
        }
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

        & .underline--bar {
            color: ansi_bright_blue;
            background: ansi_default;
        }

        & .-active {
            color: transparent;
            background: transparent;
        }
    }
}

TabPane {
    height: auto;
}

TabbedContent {
    height: auto;

    &> ContentTabs {
        dock: top;
    }
}
"#;
