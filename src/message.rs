use crate::validation::ValidationResult;
use crate::widgets::WidgetId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPaletteCommand {
    pub id: String,
    pub title: String,
    pub help: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    ClearRequested,
    InputChanged {
        value: String,
        validation: ValidationResult,
    },
    InputSubmitted {
        value: String,
    },
    TextAreaChanged {
        value: String,
    },
    ButtonPressed {
        description: String,
    },
    CheckboxChanged {
        checked: bool,
    },
    ListViewSelectionChanged {
        index: usize,
        item: String,
    },
    TabActivated {
        index: usize,
        title: String,
    },
    TreeNodeSelected {
        index: usize,
        label: String,
    },
    TreeNodeToggled {
        index: usize,
        label: String,
        expanded: bool,
    },
    OverlaySetVisible {
        overlay: WidgetId,
        visible: bool,
    },
    OverlayToggle {
        overlay: WidgetId,
    },
    OverlayDismissRequested {
        overlay: Option<WidgetId>,
    },
    OverlayVisibilityChanged {
        overlay: WidgetId,
        visible: bool,
    },
    CommandPaletteOpened,
    CommandPaletteClosed,
    CommandPaletteCommandSelected {
        id: String,
        title: String,
    },
    CommandPaletteSetCommands {
        commands: Vec<CommandPaletteCommand>,
    },
    DataTableCursorMoved {
        row: usize,
        column: usize,
    },
    DataTableHeaderSelected {
        column: usize,
    },
    DataTableCellActivated {
        row: usize,
        column: usize,
    },
    SwitchChanged {
        value: bool,
    },
    RadioButtonChanged {
        value: bool,
    },
    RadioSetChanged {
        index: usize,
        button_id: WidgetId,
    },
    OptionHighlighted {
        index: usize,
    },
    OptionSelected {
        index: usize,
    },
    SelectChanged {
        index: usize,
        label: String,
    },
    // SelectionList
    SelectionListToggled {
        index: usize,
        selected: bool,
    },
    SelectionListSelectedChanged,
    // Collapsible
    CollapsibleToggled {
        collapsed: bool,
    },
    // Link
    LinkClicked {
        url: String,
    },
    // Toast
    ToastDismissed,
}

#[derive(Debug, Clone)]
pub struct MessageEvent {
    pub sender: WidgetId,
    pub message: Message,
}
