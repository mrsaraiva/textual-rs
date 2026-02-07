use super::StyleSheet;

// Minimal built-in widget defaults to help demos look like Textual (Python) without requiring
// demo-specific CSS for core widget visuals.
//
// Note: this is a pragmatic subset of Textual's built-in widget CSS. We intentionally avoid
// full TCSS features (nesting, `&`, `!important`, advanced opacity) until the style engine grows.
const DEFAULT_WIDGET_CSS: &str = r#"
VerticalScroll { bg: $panel; }

Label { fg: $foreground; }
Markdown { fg: $foreground; }
Markdown > .markdown--h1 { fg: $primary; text-style: bold underline; }
Markdown > .markdown--h2 { fg: $primary; text-style: underline; }
Markdown > .markdown--h3 { fg: $primary; text-style: bold; }
Markdown > .markdown--h4 { fg: $primary; text-style: italic; }
Markdown > .markdown--h5 { text-style: italic; }
Markdown > .markdown--h6 { fg: $text-disabled; text-style: dim; }
Spacer { bg: $background; }

Pretty { fg: $foreground; }
Pretty > .pretty--punct { fg: $text-muted; }
Pretty > .pretty--string { fg: $success; }
Pretty > .pretty--empty { fg: $text-disabled; }

Header {
    bg: $primary;
    fg: $text;
    text-style: bold;
    line-pad: 1;
    height: auto;
}

Header.-tall {
    height: 3;
}

Header > .header--icon {
    fg: $text-muted;
}

Header > .header--icon.-hover {
    bg: $foreground;
    fg: $text;
}

Header > .header--title {
    fg: $text-muted;
}

Header > .header--clock {
    fg: $text-disabled;
}

Footer {
    bg: $panel;
    fg: $foreground;
    line-pad: 1;
    height: auto;
}

RichLog {
    bg: $surface;
    fg: $foreground;
}

RichLog:focus {
    border: tall $border;
}

KeyPanel {
    bg: $panel;
    fg: $foreground;
    border-left: tall $border-blurred;
}

BindingsTable {
    bg: $panel;
    fg: $foreground;
}

KeyPanel > .bindings-table--key {
    fg: $text-accent;
    text-style: bold;
}

KeyPanel > .bindings-table--description {
    fg: $foreground;
}

KeyPanel > .bindings-table--divider {
    fg: $border-blurred;
    text-style: dim;
}

KeyPanel > .bindings-table--header {
    fg: $text;
    text-style: bold underline;
}

TextArea {
    bg: $surface;
    fg: $foreground;
    border: tall $border-blurred;
}

TextArea:focus { border: tall $border; }
TextArea > .text-area--cursor { bg: $input-cursor-background; fg: $input-cursor-foreground; }
TextArea > .text-area--selection { bg: $input-selection-background; }
TextArea > .text-area--gutter { fg: $text-disabled; }
TextArea > .text-area--gutter-active { fg: $foreground; }
TextArea > .text-area--cursor-line { bg: $block-hover-background; }

Input {
    width: auto;
    height: 3;
    min-width: 16;
    line-pad: 2;
    bg: $surface;
    fg: $foreground;
    border: tall $border-blurred;
}

Input:focus { border: tall $border; background-tint: $foreground 5%; }
Input.-invalid { border: tall $error; }
Input.-invalid:focus { border: tall $error; }
Input:disabled { dim: true; }
Input > .input--cursor { bg: $input-cursor-background; fg: $input-cursor-foreground; }
Input > .input--selection { bg: $input-selection-background; }
Input > .input--placeholder { fg: $text-disabled; }

Checkbox {
    width: auto;
    height: auto;
    fg: $foreground;
}

Checkbox:focus { background-tint: $foreground 5%; }
Checkbox:hover { background-tint: $foreground 3%; }
Checkbox:active { background-tint: $foreground 8%; }
Checkbox:disabled { dim: true; }

ListView {
    bg: $surface;
    fg: $foreground;
}

ListView > .list-view--item { fg: $foreground; }
ListView > .list-view--item.-hover { bg: $block-hover-background; }
ListView > .list-view--item.-selected { bg: $primary-muted; fg: $text; text-style: bold; }
ListView > .list-view--item.-selected.-focus { bg: $primary; fg: $text; text-style: bold; }

Tree {
    bg: $surface;
    fg: $foreground;
}

Tree > .tree--node { fg: $foreground; }
Tree > .tree--node.-hover { bg: $block-hover-background; }
Tree > .tree--node.-selected { bg: $primary-muted; fg: $text; text-style: bold; }
Tree > .tree--node.-selected.-focus { bg: $primary; fg: $text; text-style: bold; }
Tree > .tree--node.-leaf { dim: true; }

Tabs {
    bg: $surface;
    fg: $foreground;
}

Tabs > .tabs--bar { bg: $panel; fg: $foreground; }
Tabs > .tabs--tab { bg: $panel; fg: $text-disabled; text-style: bold; }
Tabs > .tabs--tab.-hover { bg: $surface-lighten-1; fg: $text; }
Tabs > .tabs--tab.-active { bg: $primary-muted; fg: $text; }
Tabs > .tabs--tab.-active.-focus { bg: $primary; fg: $text; }

TabbedContent {
    bg: $surface;
    fg: $foreground;
}

TabbedContent > .tabbed-content--bar { bg: $panel; fg: $foreground; }
TabbedContent > .tabbed-content--tab { bg: $panel; fg: $text-disabled; text-style: bold; }
TabbedContent > .tabbed-content--tab.-hover { bg: $surface-lighten-1; fg: $text; }
TabbedContent > .tabbed-content--tab.-active { bg: $primary-muted; fg: $text; }
TabbedContent > .tabbed-content--tab.-active.-focus { bg: $primary; fg: $text; }
TabbedContent > .tabbed-content--underline { bg: $panel-darken-1; fg: $panel-darken-1; }
TabbedContent > .tabbed-content--underline.-active { bg: $primary; fg: $primary; }

CommandPalette {
    bg: $surface;
    fg: $foreground;
}

CommandPalette > .command-palette--panel {
    bg: $panel-darken-2;
    fg: $foreground;
}

CommandPalette > .command-palette--border {
    fg: $primary;
}

Button {
    width: auto;
    height: auto;
    min-width: 16;
    line-pad: 1;
}

Button.-style-default {
    text-style: bold;
    fg: $button-foreground;
    bg: $surface;
    border: none;
    border-top: tall $surface-lighten-1;
    border-bottom: tall $surface-darken-1;
}

Button.-style-default:focus { text-style: $button-focus-text-style; background-tint: $foreground 5%; }
Button.-style-default:hover { bg: $surface-darken-1; border-top: tall $surface; }

Button.-style-default.-primary { fg: $button-color-foreground; bg: $primary; border-top: tall $primary-lighten-3; border-bottom: tall $primary-darken-3; }
Button.-style-default.-success { fg: $button-color-foreground; bg: $success; border-top: tall $success-lighten-2; border-bottom: tall $success-darken-3; }
Button.-style-default.-warning { fg: $button-color-foreground; bg: $warning; border-top: tall $warning-lighten-2; border-bottom: tall $warning-darken-3; }
Button.-style-default.-error { fg: $button-color-foreground; bg: $error; border-top: tall $error-lighten-2; border-bottom: tall $error-darken-3; }

Button.-style-default.-primary:hover { bg: $primary-darken-2; border-top: tall $primary; }
Button.-style-default.-success:hover { bg: $success-darken-2; border-top: tall $success; }
Button.-style-default.-warning:hover { bg: $warning-darken-2; border-top: tall $warning; }
Button.-style-default.-error:hover { bg: $error-darken-1; border-top: tall $error; }

Button.-style-default:active { border-top: tall $surface-darken-1; border-bottom: tall $surface-lighten-1; background-tint: $background 30%; }
Button.-style-default.-primary:active { border-top: tall $primary-darken-3; border-bottom: tall $primary-lighten-3; background-tint: $background 30%; }
Button.-style-default.-success:active { border-top: tall $success-darken-3; border-bottom: tall $success-lighten-2; background-tint: $background 30%; }
Button.-style-default.-warning:active { border-top: tall $warning-darken-3; border-bottom: tall $warning-lighten-2; background-tint: $background 30%; }
Button.-style-default.-error:active { border-top: tall $error-darken-3; border-bottom: tall $error-lighten-2; background-tint: $background 30%; }

Button:disabled { dim: true; }

Button.-style-flat { text-style: bold; fg: $foreground; bg: $surface; border: block $surface; }
Button.-style-flat.-primary { fg: $text; bg: $primary-muted; border: block $primary-muted; }
Button.-style-flat.-success { fg: $text; bg: $success-muted; border: block $success-muted; }
Button.-style-flat.-warning { fg: $text; bg: $warning-muted; border: block $warning-muted; }
Button.-style-flat.-error { fg: $text; bg: $error-muted; border: block $error-muted; }

Button.-style-flat:focus { text-style: $button-focus-text-style; background-tint: $foreground 5%; }

Button.-style-flat:hover { fg: $text; bg: $primary; border: block $primary; }
Button.-style-flat.-primary:hover { fg: $text; bg: $primary; border: block $primary; }
Button.-style-flat.-success:hover { fg: $text; bg: $success; border: block $success; }
Button.-style-flat.-warning:hover { fg: $text; bg: $warning; border: block $warning; }
Button.-style-flat.-error:hover { fg: $text; bg: $error; border: block $error; }

Button.-style-flat:active { background-tint: $background 30%; }
Button.-style-flat.-primary:active { background-tint: $background 30%; }
Button.-style-flat.-success:active { background-tint: $background 30%; }
Button.-style-flat.-warning:active { background-tint: $background 30%; }
Button.-style-flat.-error:active { background-tint: $background 30%; }
"#;

pub fn default_widget_stylesheet() -> StyleSheet {
    StyleSheet::parse(DEFAULT_WIDGET_CSS)
}
