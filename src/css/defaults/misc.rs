// Miscellaneous widget defaults: Markdown, Pretty, RichLog, HelpPanel, KeyPanel,
// BindingsTable, Tooltip, Switch, RadioButton, RadioSet, Placeholder, Rule,
// ProgressBar, Collapsible, ContentSwitcher, Link, Toast, LoadingIndicator,
// Sparkline, Digits, CommandPalette

pub(super) const DEFAULT_CSS: &str = r#"
Markdown {
    height: auto;
    padding: 0 2 0 2;
    layout: vertical;
    fg: $foreground;
    overflow-y: hidden;

    & .markdown--h1 {
        content-align: center middle;
        fg: $markdown-h1-color;
        bg: $markdown-h1-background;
        text-style: $markdown-h1-text-style;
    }

    & .markdown--h2 {
        fg: $markdown-h2-color;
        bg: $markdown-h2-background;
        text-style: $markdown-h2-text-style;
    }

    & .markdown--h3 {
        fg: $markdown-h3-color;
        bg: $markdown-h3-background;
        text-style: $markdown-h3-text-style;
    }

    & .markdown--h4 {
        fg: $markdown-h4-color;
        bg: $markdown-h4-background;
        text-style: $markdown-h4-text-style;
    }

    & .markdown--h5 {
        fg: $markdown-h5-color;
        bg: $markdown-h5-background;
        text-style: $markdown-h5-text-style;
    }

    & .markdown--h6 {
        fg: $markdown-h6-color;
        bg: $markdown-h6-background;
        text-style: $markdown-h6-text-style;
    }
}

MarkdownBlock {
    width: 1fr;
    height: auto;
}

MarkdownHeader {
    fg: $text;
    margin: 2 0 1 0;
}

MarkdownH1 {
    content-align: center middle;
    fg: $markdown-h1-color;
    bg: $markdown-h1-background;
    text-style: $markdown-h1-text-style;
}

MarkdownH2 {
    fg: $markdown-h2-color;
    bg: $markdown-h2-background;
    text-style: $markdown-h2-text-style;
}

MarkdownH3 {
    fg: $markdown-h3-color;
    bg: $markdown-h3-background;
    text-style: $markdown-h3-text-style;
    margin: 1 0;
    width: auto;
}

MarkdownH4 {
    fg: $markdown-h4-color;
    bg: $markdown-h4-background;
    text-style: $markdown-h4-text-style;
    margin: 1 0;
}

MarkdownH5 {
    fg: $markdown-h5-color;
    bg: $markdown-h5-background;
    text-style: $markdown-h5-text-style;
    margin: 1 0;
}

MarkdownH6 {
    fg: $markdown-h6-color;
    bg: $markdown-h6-background;
    text-style: $markdown-h6-text-style;
    margin: 1 0;
}

MarkdownHorizontalRule {
    border-bottom: solid $secondary;
    height: 1;
    padding-top: 1;
    margin-bottom: 1;
}

Markdown > MarkdownParagraph {
    margin: 0 0 1 0;
}

MarkdownBlockQuote {
    bg: $boost;
    border-left: outer $text-primary 50%;
    margin: 1 0;
    padding: 0 1;
}

MarkdownBlockQuote:light {
    border-left: outer $text-secondary;
}

MarkdownList {
    width: 1fr;
}

MarkdownList MarkdownList {
    margin: 0;
    padding-top: 0;
}

MarkdownBulletList {
    margin: 0 0 1 0;
    padding: 0 0;
}

MarkdownBulletList Horizontal {
    height: auto;
    width: 1fr;
}

MarkdownBulletList Vertical {
    height: auto;
    width: 1fr;
}

MarkdownOrderedList {
    margin: 0 0 1 0;
    padding: 0 0;
}

MarkdownOrderedList Horizontal {
    height: auto;
    width: 1fr;
}

MarkdownOrderedList Vertical {
    height: auto;
    width: 1fr;
}

MarkdownListItem {
    layout: horizontal;
    margin-right: 1;
    height: auto;
}

MarkdownListItem > Vertical {
    width: 1fr;
    height: auto;
}

MarkdownBullet {
    width: auto;
    fg: $text-primary;
}

MarkdownBullet:light {
    fg: $text-secondary;
}

MarkdownFence {
    padding: 0;
    margin: 1 0;
    overflow: scroll hidden;
    scrollbar-size-horizontal: 0;
    scrollbar-size-vertical: 0;
    width: 1fr;
    height: auto;
    color: rgb(210, 210, 210);
    background: black 10%;
}

MarkdownFence:light {
    background: white 30%;
}

MarkdownFence > Label {
    padding: 1 2;
}

MarkdownTableContent {
    width: 1fr;
    height: auto;
    layout: grid;
    grid-columns: auto;
    grid-rows: auto;
    grid-gutter: 1 1;
    keyline: thin $foreground 20%;
}

MarkdownTableContent > .header {
    height: auto;
    margin: 0 0;
    padding: 0 1;
    fg: $primary;
    text-overflow: ellipsis;
    content-align: left bottom;
}

MarkdownTableContent > .markdown-table--header {
    text-style: bold;
}

MarkdownTableContent > .cell {
    margin: 0 0;
    height: auto;
    padding: 0 1;
    text-overflow: ellipsis;
}

MarkdownTable {
    width: 1fr;
    margin-bottom: 1;
}

MarkdownTable:light {
    background: white 30%;
}

MarkdownTableOfContents {
    width: auto;
    height: 1fr;
    bg: $panel;
}

MarkdownTableOfContents:focus-within {
    background-tint: $foreground 5%;
}

MarkdownTableOfContents > Tree {
    padding: 1;
    width: auto;
    height: 1fr;
    bg: $panel;
}

MarkdownViewer {
    height: 1fr;
    scrollbar-gutter: stable;
    bg: $surface;
}

MarkdownViewer > MarkdownTableOfContents {
    display: none;
    dock: left;
}

MarkdownViewer.-show-table-of-contents > MarkdownTableOfContents {
    display: block;
}

Log {
    bg: $surface;
    fg: $text;
    overflow: scroll;
}

Log:focus {
    background-tint: $foreground 5%;
}

Static { height: auto; }

Label { width: auto; height: auto; min-height: 1; }

Pretty { fg: $foreground; height: auto; }

RichLog {
    bg: $surface;
    fg: $foreground;
    overflow-y: scroll;
}

RichLog:focus {
    background-tint: $foreground 5%;
}

HelpPanel {
    split: right;
    width: 33%;
    min-width: 30;
    max-width: 60;
    border-left: vkey $foreground 30%;
    padding: 0 1;
    height: 1fr;
    padding-right: 1;
    layout: vertical;
    height: 100%;
}

HelpPanel:ansi {
    bg: ansi_default;
    border-left: vkey ansi_black;
}

HelpPanel:ansi Markdown {
    bg: ansi_default;
}

HelpPanel:ansi KeyPanel {
    bg: ansi_default;
}

HelpPanel:ansi .bindings-table--divider {
    fg: transparent;
}

HelpPanel > #widget-help {
    height: auto;
    max-height: 50%;
    width: 1fr;
    padding: 1 0;
    margin-top: 1;
    display: none;
    bg: $panel;
}

HelpPanel.-show-help > #widget-help {
    display: block;
}

HelpPanel > #widget-help > MarkdownBlock {
    padding-left: 2;
    padding-right: 2;
}

HelpPanel > KeyPanel {
    width: 1fr;
    height: 1fr;
    border-left: none;
    padding: 0;
}

KeyPanel {
    split: right;
    width: 33%;
    min-width: 30;
    max-width: 60;
    border-left: vkey $foreground 30%;
    padding: 0 1;
    height: 1fr;
    padding-right: 1;
    align: center top;
}

KeyPanel > BindingsTable > .bindings-table--key {
    fg: $text-accent;
    text-style: bold;
    padding: 0 1;
}

KeyPanel > BindingsTable > .bindings-table--description {
    fg: $foreground;
}

KeyPanel > BindingsTable > .bindings-table--divider {
    fg: transparent;
}

KeyPanel > BindingsTable > .bindings-table--header {
    fg: $text-primary;
    text-style: underline;
}

KeyPanel > #bindings-table {
    width: auto;
    height: auto;
}

BindingsTable {
    width: auto;
    height: auto;
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

Switch {
    width: auto;
    height: auto;
    border: tall $border-blurred;
    bg: $surface;
    padding: 0 2;
    pointer: pointer;
}

Switch:focus {
    border: tall $border;
    background-tint: $foreground 5%;
}

Switch .switch--slider {
    fg: $panel;
    bg: $panel-darken-2;
}

Switch.-on .switch--slider {
    fg: $success;
}

Switch:hover .switch--slider {
    fg: $panel-lighten-1;
}

Switch.-on:hover .switch--slider {
    fg: $success-lighten-1;
}

Switch:light .switch--slider {
    fg: $primary 15%;
    bg: $panel-darken-2;
}

Switch:light.-on .switch--slider {
    fg: $success;
}

Switch:light:hover .switch--slider {
    fg: $primary 25%;
}

Switch:light.-on:hover .switch--slider {
    fg: $success-lighten-1;
}

RadioButton {
    width: auto;
    height: auto;
    border: tall $border-blurred;
    bg: $surface;
    padding: 0 1;
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
    padding: 0 1;
    height: auto;
    width: 1fr;
    pointer: pointer;
}

RadioSet.-textual-compact {
    border: none;
    padding: 0;
}

RadioSet > RadioButton {
    bg: transparent;
    border: none;
    padding: 0;
    width: 1fr;
}

RadioSet > RadioButton > .toggle--button {
    fg: $panel-darken-2;
    bg: $panel;
}

RadioSet > RadioButton.-on > .toggle--button {
    fg: $text-success;
}

RadioSet:blur > RadioButton.-selected > .toggle--label {
    bg: $block-cursor-blurred-background;
}

RadioSet:focus {
    border: tall $border;
    background-tint: $foreground 5%;
}

RadioSet:focus > RadioButton.-selected > .toggle--label {
    bg: $block-cursor-background;
    fg: $block-cursor-foreground;
    text-style: $block-cursor-text-style;
}

Placeholder {
    content-align: center middle;
    overflow: hidden;
    fg: $text;
}

Placeholder.-text { padding: 1; }
Placeholder:disabled { opacity: 70%; }

Rule { fg: $secondary; }

Rule.-horizontal {
    height: 1;
    margin: 1 0;
    width: 1fr;
    fg: $secondary;
}

Rule.-vertical {
    width: 1;
    margin: 0 2;
    height: 1fr;
    fg: $secondary;
}

ProgressBar {
    layout: horizontal;
    width: auto;
    height: 1;
    fg: $foreground;
}

Bar {
    width: 32;
    height: 1;
}

Bar > .bar--bar { fg: $primary; bg: $surface; }
Bar > .bar--complete { fg: $success; bg: $surface; }
Bar > .bar--indeterminate { fg: $error; bg: $surface; }

PercentageStatus {
    width: 5;
    content-align-horizontal: right;
}

ETAStatus {
    width: 9;
    content-align-horizontal: right;
}

Collapsible {
    width: 1fr;
    height: auto;
    bg: $surface;
    border-top: hkey $background;
    padding-bottom: 1;
    padding-left: 1;
}

Collapsible:focus-within {
    background-tint: $foreground 5%;
}

Collapsible.-collapsed > Contents {
    display: none;
}

CollapsibleTitle {
    width: auto;
    height: auto;
    padding: 0 1;
    text-style: $block-cursor-blurred-text-style;
    fg: $block-cursor-blurred-foreground;
    pointer: pointer;
}

CollapsibleTitle:hover {
    bg: $block-hover-background;
    fg: $foreground;
}

CollapsibleTitle:focus {
    text-style: $block-cursor-text-style;
    bg: $block-cursor-background;
    fg: $block-cursor-foreground;
}

Contents {
    width: 100%;
    height: auto;
    padding: 1 0 0 3;
}

ContentSwitcher {
    height: auto;
}

Link {
    width: auto;
    height: auto;
    min-height: 1;
    fg: $text-accent;
    text-style: underline;
    pointer: pointer;
}

Link:hover { fg: $accent; }
Link:focus { text-style: bold reverse; }

Toast {
    width: 60;
    max-width: 50%;
    height: auto;
    margin-top: 1;
    visibility: visible;
    padding: 1 1;
    bg: $panel-lighten-1;
    fg: $foreground;
    link-background: initial;
    link-color: $foreground;
    link-style: underline;
    link-background-hover: $primary;
    link-color-hover: $foreground;
    link-style-hover: bold;
}

Toast .toast--title { fg: $foreground; text-style: bold; }
Toast.-information { border-left: outer $success; }
Toast.-information .toast--title { fg: $text-success; }
Toast.-warning { border-left: outer $warning; }
Toast.-warning .toast--title { fg: $text-warning; }
Toast.-error { border-left: outer $error; }
Toast.-error .toast--title { fg: $text-error; }

ToastHolder {
    align-horizontal: right;
    width: 1fr;
    height: auto;
    visibility: hidden;
}

ToastRack {
    display: none;
    layer: _toastrack;
    width: 1fr;
    height: auto;
    dock: bottom;
    align: right bottom;
    visibility: hidden;
    layout: vertical;
    overflow-y: scroll;
    margin-bottom: 1;
}

LoadingIndicator {
    width: 100%;
    height: 100%;
    min-height: 1;
    content-align: center middle;
    fg: $primary;
    text-style: not reverse;
}

LoadingIndicator.-textual-loading-indicator {
    layer: _loading;
    bg: $boost;
    dock: top;
}

Sparkline { height: 1; }
Sparkline > .sparkline--max-color { fg: $primary; }
Sparkline > .sparkline--min-color { fg: $primary 30%; }

Digits {
    width: 1fr;
    height: auto;
    text-align: left;
    box-sizing: border-box;
}

CommandPalette {
    bg: $surface;
    fg: $foreground;
    align-horizontal: center;
}

CommandPalette > .command-palette--panel {
    bg: $panel-darken-1;
    fg: $foreground;
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
