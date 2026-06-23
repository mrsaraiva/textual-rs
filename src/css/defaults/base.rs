// Base/shared widget defaults: Screen, ModalScreen, ScrollView, Widget, Label, Spacer
// DC-01: Screen aligned with Python Textual _screen.py DEFAULT_CSS
// DC-03: ModalScreen aligned with Python Textual _screen.py DEFAULT_CSS
// DC-04: Widget base aligned with Python Textual widget.py DEFAULT_CSS
// DC-05: Label aligned with Python Textual _label.py DEFAULT_CSS

pub(super) const DEFAULT_CSS: &str = r#"
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
    /* NOTE: Python Textual Widget DEFAULT_CSS includes `background: transparent` here.
       In Rust, `bg: None` and `bg: Some(transparent)` have different rendering behavior:
       None skips background fill entirely, while Some(transparent) goes through the
       flatten_over path. Intermediate widgets (Grid, Container, etc.) with bg: None
       correctly inherit parent backgrounds via the composited-background stack;
       with bg: Some(transparent), `apply_border_edges` receives parent_bg=transparent
       from the nearest ancestor's resolved style (which is also transparent), losing
       the actual screen background. Until `apply_border_edges` is updated to use
       `current_composited_background()` instead of parent_style.bg directly, we omit
       this property from defaults so bg stays None for widgets without explicit bg.
       DEFERRED(render-transparent-bg): fix apply_border_edges + apply_style_to_segments
       to use current_composited_background() so bg: transparent composites correctly. */
}

Screen {
    layout: vertical;
    overflow-y: auto;
    overflow-x: hidden;
    bg: $background;
    color: $foreground;

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

ScrollView {
    overflow-y: auto;
    overflow-x: auto;
}

ScrollView > .scrollview--content { transition: scrollview.offset 140ms ease-out; }

Label {
    width: auto;
    height: auto;
    min-height: 1;
    /* No `color`/`fg` rule: Python Textual's Label DEFAULT_CSS sets none —
       the foreground is inherited from `Screen { color: $foreground }` via the
       ancestor cascade. Setting it here would shadow an explicit ancestor
       `color` (e.g. `Screen { color: black }`), breaking inheritance. */

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
