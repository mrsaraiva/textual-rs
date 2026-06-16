// Header and Footer widget defaults
// DC-27: Header aligned with Python Textual _header.py DEFAULT_CSS
// DC-28: Footer aligned with Python Textual _footer.py DEFAULT_CSS

pub(super) const DEFAULT_CSS: &str = r#"
HeaderIcon {
    dock: left;
    padding: 0 1;
    width: 8;
    content-align: left middle;

    &:hover {
        bg: $foreground 10%;
    }
}

HeaderClockSpace {
    dock: right;
    width: 10;
    padding: 0 1;
}

HeaderClock {
    /* Python parity: `class HeaderClock(HeaderClockSpace)` inherits the space's
       dock/width/padding. Rust has no widget-type inheritance, so restate them
       here. Without the dock, the clock lays out as a normal flow sibling of
       `HeaderTitle` and stacks below it instead of pinning to the right edge. */
    dock: right;
    width: 10;
    padding: 0 1;
    bg: $foreground-darken-1 5%;
    color: $foreground;
    text-opacity: 85%;
    content-align: center middle;
}

HeaderTitle {
    text-wrap: nowrap;
    text-overflow: ellipsis;
    content-align: center middle;
    width: 100%;
}

App:blur HeaderTitle {
    text-opacity: 50%;
}

Header {
    dock: top;
    width: 100%;
    bg: $panel;
    color: $foreground;
    height: 1;

    &.-tall {
        height: 3;
    }
}

KeyGroup {
    width: auto;
}

FooterKey {
    width: auto;
    height: 1;
    text-wrap: nowrap;
    bg: $footer-item-background;

    & .footer-key--key {
        color: $footer-key-foreground;
        bg: $footer-key-background;
        text-style: bold;
        padding: 0 1;
    }

    & .footer-key--description {
        padding: 0 1 0 0;
        color: $footer-description-foreground;
        bg: $footer-description-background;
    }

    &:hover {
        color: $footer-key-foreground;
        bg: $block-hover-background;
    }

    &.-disabled {
        text-style: dim;
    }

    &.-compact .footer-key--key {
        padding: 0;
    }

    &.-compact .footer-key--description {
        padding: 0 0 0 1;
    }
}

Footer {
    layout: horizontal;
    color: $footer-foreground;
    bg: $footer-background;
    dock: bottom;
    height: 1;
    scrollbar-size: 0 0;

    &.-compact {
        FooterLabel {
            margin: 0;
        }

        FooterKey {
            margin-right: 1;
        }

        FooterKey.-grouped {
            margin: 0 1;
        }

        FooterKey.-command-palette {
            padding-right: 0;
        }
    }

    FooterKey.-command-palette {
        dock: right;
        padding-right: 1;
        border-left: vkey $foreground 20%;

        & .footer-key--key {
            padding-left: 0;
        }
    }

    .footer-key--palette-separator {
        color: $foreground 20%;
        bg: $footer-background;
    }

    HorizontalGroup.binding-group {
        width: auto;
        height: 1;
        layout: horizontal;
    }

    KeyGroup.-compact {
        FooterKey.-grouped {
            margin: 0;
        }
        margin: 0 1 0 0;
        padding-left: 1;
    }

    FooterKey.-grouped {
        margin: 0 1;
    }

    FooterLabel {
        margin: 0 1 0 0;
        color: $footer-description-foreground;
        bg: $footer-description-background;
    }

    &:ansi {
        bg: ansi_default;

        .footer-key--key {
            bg: ansi_default;
            color: ansi_magenta;
        }

        .footer-key--description {
            bg: ansi_default;
            color: ansi_default;
        }

        FooterKey:hover {
            text-style: underline;
            bg: ansi_default;
            color: ansi_default;

            .footer-key--key {
                bg: ansi_default;
            }
        }

        FooterKey.-command-palette {
            bg: ansi_default;
            border-left: vkey ansi_black;
        }

        .footer-key--palette-separator {
            color: ansi_black;
            bg: ansi_default;
        }
    }
}
"#;
