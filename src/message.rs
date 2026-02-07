use crate::validation::ValidationResult;
use crate::widgets::WidgetId;

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
}

#[derive(Debug, Clone)]
pub struct MessageEvent {
    pub sender: WidgetId,
    pub message: Message,
}
