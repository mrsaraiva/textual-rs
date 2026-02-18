// Base/shared widget defaults: Screen, ModalScreen, ScrollView, Widget, Label, Spacer
// DC-01: Screen aligned with Python Textual _screen.py DEFAULT_CSS
// DC-03: ModalScreen aligned with Python Textual _screen.py DEFAULT_CSS
// DC-04: Widget base aligned with Python Textual widget.py DEFAULT_CSS
// DC-05: Label aligned with Python Textual _label.py DEFAULT_CSS

pub(super) const DEFAULT_CSS: &str = r#"
Screen {
    layout: vertical;
    overflow-y: auto;
    overflow-x: hidden;
    bg: $background;

    &:inline {
        height: auto;
        min-height: 1;
        border-top: tall $background;
        border-bottom: tall $background;
    }

    &:ansi {
        background: ansi_default;
        color: ansi_default;
    }

    & .screen--selection {
        background: $primary 50%;
    }
}

Screen:ansi.-screen-suspended {
    text-style: dim;
}

Screen:ansi.-screen-suspended ScrollBar {
    text-style: not dim;
}

ModalScreen {
    layout: vertical;
    overflow-y: auto;
    bg: $background 60%;

    &:ansi {
        background: transparent;
    }
}

Widget {
    scrollbar-background: $scrollbar-background;
    scrollbar-background-hover: $scrollbar-background-hover;
    scrollbar-background-active: $scrollbar-background-active;
    scrollbar-color: $scrollbar;
    scrollbar-color-active: $scrollbar-active;
    scrollbar-color-hover: $scrollbar-hover;
    scrollbar-corner-color: $scrollbar-corner-color;
    scrollbar-size-vertical: 2;
    scrollbar-size-horizontal: 1;
    link-color: $link-color;
    link-background: $link-background;
    link-background-hover: $link-background-hover;
    link-color-hover: $link-color-hover;
    link-style: $link-style;
    link-style-hover: $link-style-hover;
    background: transparent;
}

ScrollView {
    overflow-y: auto;
    overflow-x: auto;
}

ScrollView > .scrollview--content { transition: scrollview.offset 140ms ease-out; }

Label {
    width: auto;
    height: auto;
    min-height: 1;
    fg: $foreground;

    &.success {
        color: $text-success;
        bg: $success-muted;
    }
    &.error {
        color: $text-error;
        bg: $error-muted;
    }
    &.warning {
        color: $text-warning;
        bg: $warning-muted;
    }
    &.primary {
        color: $text-primary;
        bg: $primary-muted;
    }
    &.secondary {
        color: $text-secondary;
        bg: $secondary-muted;
    }
    &.accent {
        color: $text-accent;
        bg: $accent-muted;
    }
}

Spacer { bg: $background; }

*:disabled:can-focus {
    opacity: 70%;
}
"#;
