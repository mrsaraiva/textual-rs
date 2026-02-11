use crate::validation::ValidationResult;
use crate::widgets::WidgetId;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPaletteCommand {
    pub id: String,
    pub title: String,
    pub help: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncDirectoryEntry {
    pub path: String,
    pub label: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncTaskRequest {
    ReadDirectory { path: String, show_hidden: bool },
    Sleep { duration: Duration, label: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncTaskResult {
    DirectoryEntries {
        path: String,
        entries: Vec<AsyncDirectoryEntry>,
    },
    SleepFinished {
        label: String,
        elapsed: Duration,
    },
    Failed {
        path: String,
        error: String,
    },
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
    ListViewItemActivated {
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
    HeaderIconPressed,
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
    HelpPanelFocusedHelpChanged {
        source: WidgetId,
        markup: String,
    },
    HelpPanelFocusedHelpCleared,
    PlaceholderVariantChanged {
        variant: String,
    },
    TreeNodeSelected {
        index: usize,
        label: String,
    },
    TreeNodeActivated {
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
    AsyncTaskSpawn {
        task_id: u64,
        target: WidgetId,
        request: AsyncTaskRequest,
    },
    AsyncTaskCancel {
        task_id: u64,
    },
    AsyncTaskCancelTarget {
        target: WidgetId,
    },
    TimerSchedule {
        timer_id: u64,
        target: WidgetId,
        delay: Duration,
    },
    TimerCancel {
        timer_id: u64,
    },
    TimerFired {
        timer_id: u64,
        target: WidgetId,
    },
    TimerCancelled {
        timer_id: u64,
        target: WidgetId,
    },
    AsyncTaskCompleted {
        task_id: u64,
        target: WidgetId,
        result: AsyncTaskResult,
    },
    AsyncTaskCancelled {
        task_id: u64,
        target: WidgetId,
    },
    // Tree: separate expand/collapse + highlight messages
    TreeNodeCollapsed {
        index: usize,
        label: String,
    },
    TreeNodeExpanded {
        index: usize,
        label: String,
    },
    TreeNodeHighlighted {
        index: usize,
        label: String,
    },
    // DataTable highlight/select messages
    DataTableCellHighlighted {
        row: usize,
        col: usize,
    },
    DataTableRowHighlighted {
        row: usize,
    },
    DataTableRowSelected {
        row: usize,
    },
    DataTableColumnHighlighted {
        col: usize,
    },
    DataTableColumnSelected {
        col: usize,
    },
}

#[derive(Debug, Clone)]
pub struct MessageEvent {
    pub sender: WidgetId,
    pub message: Message,
}
