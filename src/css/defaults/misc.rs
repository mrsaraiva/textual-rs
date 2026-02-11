// Miscellaneous widget defaults: Markdown, Pretty, RichLog, HelpPanel, KeyPanel,
// BindingsTable, Tooltip, Switch, RadioButton, RadioSet, Placeholder, Rule,
// ProgressBar, Collapsible, ContentSwitcher, Link, Toast, LoadingIndicator,
// Sparkline, Digits, CommandPalette

pub(super) const DEFAULT_CSS: &str = r#"
Markdown { fg: $foreground; }
Markdown > .markdown--h1 { fg: $primary; text-style: bold; }
Markdown > .markdown--h2 { fg: $primary; text-style: underline; }
Markdown > .markdown--h3 { fg: $primary; text-style: bold; }
Markdown > .markdown--h4 { fg: $primary; text-style: italic; }
Markdown > .markdown--h5 { text-style: italic; }
Markdown > .markdown--h6 { fg: $text-disabled; text-style: dim; }

Log {
    bg: $surface;
    fg: $text;
}

Log:focus {
    background-tint: $foreground 5%;
}

Pretty { fg: $foreground; }

RichLog {
    bg: $surface;
    fg: $foreground;
}

RichLog:focus {
    background-tint: $foreground 5%;
}

KeyPanel {
    bg: $panel;
    fg: $foreground;
    border-left: vkey $border-blurred;
}

HelpPanel {
    width: 33;
    min-width: 30;
    max-width: 60;
    height: 1fr;
    line-pad: 1;
    bg: $panel;
    fg: $foreground;
    border-left: vkey $border-blurred;
}

HelpPanel > Markdown {
    width: 1fr;
    height: auto;
    bg: $panel;
    fg: $foreground;
}

HelpPanel > KeyPanel {
    width: 1fr;
    height: 1fr;
    border-left: none;
    bg: $panel;
}

Welcome {
    width: 100%;
    height: 100%;
    bg: $surface;
    fg: $foreground;
    line-pad: 1;
}

Welcome > Markdown {
    bg: $surface;
    fg: $foreground;
}

Welcome > Button {
    width: 100%;
}

Tooltip > .tooltip--bubble {
    bg: $panel;
    fg: $foreground;
}

Tooltip > .tooltip--text {
    fg: $foreground;
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

Switch {
    width: auto;
    height: auto;
    border: tall $border-blurred;
    bg: $surface;
    line-pad: 2;
}

Switch:focus { border: tall $border; background-tint: $foreground 5%; }
Switch > .switch--slider { fg: $panel; bg: $panel-darken-2; }
Switch.-on > .switch--slider { fg: $success; bg: $panel-darken-2; }
Switch:hover > .switch--slider { fg: $panel-lighten-1; }
Switch.-on:hover > .switch--slider { fg: $success-lighten-1; }

RadioButton {
    width: auto;
    height: auto;
    border: tall $border-blurred;
    bg: $surface;
    line-pad: 1;
}

RadioButton:focus { border: tall $border; background-tint: $foreground 5%; }
RadioButton > .radio-button--button { fg: $panel-darken-2; bg: $panel; }
RadioButton.-on > .radio-button--button { fg: $success; bg: $panel; }
RadioButton > .radio-button--label { fg: $foreground; }
RadioButton > .radio-button--label.-hover { bg: $block-hover-background; }
RadioButton > .radio-button--label.-focus { fg: $text; bg: $primary; text-style: bold; }

RadioSet {
    border: tall $border-blurred;
    bg: $surface;
    line-pad: 1;
    height: auto;
    width: 1fr;
}

RadioSet:focus { border: tall $border; background-tint: $foreground 5%; }
RadioSet > .radio-button--button { fg: $panel-darken-2; bg: $panel; }
RadioSet > .radio-button--button.-on { fg: $success; bg: $panel; }
RadioSet > .radio-button--label { fg: $foreground; }
RadioSet > .radio-button--label.-hover { bg: $block-hover-background; }
RadioSet > .radio-button--label.-selected { bg: $primary-muted; fg: $text; text-style: bold; }
RadioSet > .radio-button--label.-selected.-focus { bg: $primary; fg: $text; text-style: bold; }

Placeholder > .placeholder { fg: $text; }
Placeholder.-text { line-pad: 1; }
Placeholder:disabled { opacity: 70%; }

Rule { fg: $secondary; }
Rule > .rule--horizontal { fg: $secondary; }
Rule > .rule--vertical { fg: $secondary; }

ProgressBar {
    width: 32;
    height: 1;
    fg: $foreground;
}

ProgressBar > .bar--bar { fg: $primary; bg: $surface; }
ProgressBar > .bar--complete { fg: $success; bg: $surface; }
ProgressBar > .bar--indeterminate { fg: $error; bg: $surface; }

Collapsible {
    width: 1fr;
    height: auto;
    bg: $surface;
}

Collapsible:focus { background-tint: $foreground 5%; }
Collapsible > .collapsible--title { fg: $foreground; text-style: bold; }
Collapsible > .collapsible--title.-focus { fg: $text; bg: $primary; }

ContentSwitcher {
    height: auto;
}

Link {
    width: auto;
    height: auto;
    fg: $text-accent;
    text-style: underline;
}

Link:hover { fg: $accent; }
Link:focus { text-style: bold; }

Toast {
    width: 60;
    max-width: 50%;
    height: auto;
    line-pad: 1;
    bg: $panel-lighten-1;
    fg: $foreground;
}

Toast > .toast--title { fg: $foreground; text-style: bold; }
Toast.-information { border-left: outer $success; }
Toast.-information > .toast--title { fg: $text-success; }
Toast.-warning { border-left: outer $warning; }
Toast.-warning > .toast--title { fg: $text-warning; }
Toast.-error { border-left: outer $error; }
Toast.-error > .toast--title { fg: $text-error; }

LoadingIndicator {
    width: 1fr;
    height: 1fr;
    min-height: 1;
    fg: $primary;
}

Sparkline { height: 1; }
Sparkline > .sparkline--max-color { fg: $primary; }
Sparkline > .sparkline--min-color { fg: $primary 30%; }

Digits {
    width: 1fr;
    height: auto;
}

CommandPalette {
    bg: $surface;
    fg: $foreground;
}

CommandPalette > .command-palette--panel {
    bg: $panel-darken-1;
    fg: $foreground;
    transition: command-palette.panel-y 180ms ease-out;
}
CommandPalette > .command-palette--key-panel {
    transition: command-palette.key-panel 220ms ease-out;
}

CommandPalette > .command-palette--border {
    fg: $primary;
}

CommandPalette > .command-palette--search-icon {
    fg: $primary;
}

CommandPalette > .command-palette--item-title {
    fg: $foreground;
    text-style: bold;
}

CommandPalette > .command-palette--item-help {
    fg: $text-muted;
}

CommandPalette > .command-palette--item-selected {
    fg: $text;
    bg: $primary;
}
"#;
