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
    TextEditClipboardCopyRequested {
        text: String,
        cut: bool,
    },
    TextEditClipboardPasteRequested {
        target: WidgetId,
    },
    TextEditClipboardPaste {
        target: WidgetId,
        text: String,
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
    HeaderToggled {
        tall: bool,
    },
    FooterBindingsUpdated {
        count: usize,
    },
    HelpPanelSetHelp {
        panel: WidgetId,
        markup: String,
    },
    HelpPanelClearHelp {
        panel: WidgetId,
    },
    PlaceholderVariantChanged {
        variant: String,
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
    DirectoryTreeFileSelected {
        index: usize,
        path: String,
    },
    DirectoryTreeDirectorySelected {
        index: usize,
        path: String,
    },
    OverlaySetVisible {
        overlay: WidgetId,
        visible: bool,
    },
    OverlaySetAnchor {
        overlay: WidgetId,
        x: usize,
        y: usize,
    },
    OverlayClearAnchor {
        overlay: WidgetId,
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
    // Key panel
    KeyPanelBindingsUpdated {
        count: usize,
    },
    KeyPanelScrolled {
        offset: usize,
        max_offset: usize,
    },
    // Rich log
    RichLogScrolled {
        offset: usize,
        max_offset: usize,
    },
}

#[derive(Debug, Clone)]
pub struct MessageEvent {
    pub sender: WidgetId,
    pub message: Message,
}
