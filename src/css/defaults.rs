use super::StyleSheet;

// Minimal built-in widget defaults to help demos look like Textual (Python) without requiring
// demo-specific CSS for core widget visuals.
//
// Note: this is a pragmatic subset of Textual's built-in widget CSS. We intentionally avoid
// full TCSS features (nesting, `&`, `!important`, advanced opacity) until the style engine grows.
const DEFAULT_WIDGET_CSS: &str = r#"
ScrollView > .scrollview--content { transition: scrollview.offset 140ms ease-out; }

Label { fg: $foreground; }
Markdown { fg: $foreground; }
Markdown > .markdown--h1 { fg: $primary; text-style: bold; }
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

Footer > .footer-key--key {
    fg: $accent;
    text-style: bold;
}

Footer > .footer-key--description {
    fg: $text-disabled;
}

Footer > .footer-key--command-palette {
    fg: $text-muted;
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

OptionList {
    height: auto;
    bg: $surface;
    fg: $foreground;
    border: tall $border-blurred;
    line-pad: 1;
}

OptionList:focus { border: tall $border; background-tint: $foreground 5%; }
OptionList > .option-list--option { fg: $foreground; }
OptionList > .option-list--option.-highlighted { fg: $foreground; bg: $primary-muted; text-style: bold; }
OptionList > .option-list--option.-highlighted.-focus { fg: $text; bg: $primary; text-style: bold; }
OptionList > .option-list--option.-disabled { fg: $text-disabled; }
OptionList > .option-list--option.-hover { bg: $block-hover-background; }
OptionList > .option-list--separator { fg: $text-disabled; }

Select {
    height: auto;
    fg: $foreground;
    border: tall $border-blurred;
    bg: $surface;
}

Select:focus { border: tall $border; background-tint: $foreground 5%; }
Select > .select--current-value { fg: $foreground; bg: $surface; }
Select > .select--current-value.-hover { bg: $surface-lighten-1; }
Select > .select--current-value.-focus { fg: $foreground; bg: $surface-lighten-1; text-style: bold; }
Select > .select--arrow { fg: $text-disabled; bg: $surface; }
Select > .select--arrow.-open { fg: $text; bg: $surface; }
Select > .select--dropdown { bg: $surface; fg: $foreground; }

Placeholder > .placeholder { fg: $text; }
Placeholder.-text { line-pad: 1; }

Rule { fg: $secondary; }
Rule > .rule--horizontal { fg: $secondary; }
Rule > .rule--vertical { fg: $secondary; }

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
Tabs > .tabs--underline { bg: $panel-darken-1; fg: $foreground; text-style: dim; }
Tabs > .tabs--underline.-active {
    bg: $panel-darken-1;
    fg: $primary;
    transition: tabs.underline 300ms ease-in-out;
}

TabbedContent {
    bg: $surface;
    fg: $foreground;
}

TabbedContent > .tabbed-content--bar { bg: $panel; fg: $foreground; }
TabbedContent > .tabbed-content--tab { bg: $panel; fg: $text-disabled; text-style: bold; }
TabbedContent > .tabbed-content--tab.-hover { bg: $surface-lighten-1; fg: $text; }
TabbedContent > .tabbed-content--tab.-active { bg: $primary-muted; fg: $text; }
TabbedContent > .tabbed-content--tab.-active.-focus { bg: $primary; fg: $text; }
TabbedContent > .tabbed-content--underline { bg: $panel-darken-1; fg: $foreground; text-style: dim; }
TabbedContent > .tabbed-content--underline.-active {
    bg: $panel-darken-1;
    fg: $primary;
    transition: tabbed-content.underline 300ms ease-in-out;
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
Button.-style-default.-success:active { border-top: tall $success-darken-2; border-bottom: tall $success-lighten-2; background-tint: $background 30%; }
Button.-style-default.-warning:active { border-top: tall $warning-darken-2; border-bottom: tall $warning-lighten-2; background-tint: $background 30%; }
Button.-style-default.-error:active { border-top: tall $error-darken-2; border-bottom: tall $error-lighten-2; background-tint: $background 30%; }

Button.-style-default:disabled { text-opacity: 60%; }

Button.-style-flat { text-style: bold; fg: $text; bg: $surface; border: block $surface; }
Button.-style-flat.-primary { fg: $text-primary; bg: $primary-muted; border: block $primary-muted; }
Button.-style-flat.-success { fg: $text-success; bg: $success-muted; border: block $success-muted; }
Button.-style-flat.-warning { fg: $text-warning; bg: $warning-muted; border: block $warning-muted; }
Button.-style-flat.-error { fg: $text-error; bg: $error-muted; border: block $error-muted; }

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
Button.-style-flat:disabled { fg: auto 50%; }

SelectionList {
    height: auto;
    bg: $surface;
    fg: $foreground;
    border: tall $border-blurred;
    line-pad: 1;
}

SelectionList:focus { border: tall $border; background-tint: $foreground 5%; }
SelectionList > .selection-list--button { fg: $panel-darken-2; bg: $panel; }
SelectionList > .selection-list--button-highlighted { fg: $panel-darken-2; bg: $panel; }
SelectionList > .selection-list--button-selected { fg: $text-success; bg: $panel; }
SelectionList > .selection-list--button-selected-highlighted { fg: $text-success; bg: $panel; }

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
    height: auto;
    bg: $panel-lighten-1;
    fg: $foreground;
}

Toast > .toast--title { fg: $foreground; text-style: bold; }
Toast.-information > .toast--title { fg: $text-success; }
Toast.-warning > .toast--title { fg: $text-warning; }
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

MaskedInput {
    width: auto;
    height: 3;
    min-width: 16;
    line-pad: 2;
    bg: $surface;
    fg: $foreground;
    border: tall $border-blurred;
}

MaskedInput:focus { border: tall $border; background-tint: $foreground 5%; }
MaskedInput.-invalid { border: tall $error; }
MaskedInput.-invalid:focus { border: tall $error; }
MaskedInput > .input--cursor { bg: $input-cursor-background; fg: $input-cursor-foreground; }
MaskedInput > .input--selection { bg: $input-selection-background; }
MaskedInput > .input--placeholder { fg: $text-disabled; }
"#;

pub fn default_widget_stylesheet() -> StyleSheet {
    StyleSheet::parse(DEFAULT_WIDGET_CSS)
}
