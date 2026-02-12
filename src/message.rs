use crate::node_id::NodeId;
use crate::validation::ValidationResult;
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
        target: NodeId,
    },
    TextEditClipboardPaste {
        target: NodeId,
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
        id: String,
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
        panel: NodeId,
        markup: String,
    },
    HelpPanelClearHelp {
        panel: NodeId,
    },
    HelpPanelFocusedHelpChanged {
        source: NodeId,
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
        overlay: NodeId,
        visible: bool,
    },
    OverlaySetAnchor {
        overlay: NodeId,
        x: usize,
        y: usize,
    },
    OverlayClearAnchor {
        overlay: NodeId,
    },
    OverlayToggle {
        overlay: NodeId,
    },
    OverlayDismissRequested {
        overlay: Option<NodeId>,
    },
    OverlayVisibilityChanged {
        overlay: NodeId,
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
        button_id: NodeId,
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
        target: NodeId,
        request: AsyncTaskRequest,
    },
    AsyncTaskCancel {
        task_id: u64,
    },
    AsyncTaskCancelTarget {
        target: NodeId,
    },
    TimerSchedule {
        timer_id: u64,
        target: NodeId,
        delay: Duration,
    },
    TimerCancel {
        timer_id: u64,
    },
    TimerFired {
        timer_id: u64,
        target: NodeId,
    },
    TimerCancelled {
        timer_id: u64,
        target: NodeId,
    },
    AsyncTaskCompleted {
        task_id: u64,
        target: NodeId,
        result: AsyncTaskResult,
    },
    AsyncTaskCancelled {
        task_id: u64,
        target: NodeId,
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
    // TextArea selection change
    TextAreaSelectionChanged {
        start: (usize, usize),
        end: (usize, usize),
    },
    // Input blur
    InputBlurred {
        value: String,
    },
    // Tabs lifecycle messages
    TabDisabled {
        id: String,
    },
    TabEnabled {
        id: String,
    },
    TabHidden {
        id: String,
    },
    TabShown {
        id: String,
    },
    TabsCleared,
    TabPaneFocused {
        id: String,
    },
}

#[derive(Debug, Clone)]
pub struct MessageEvent {
    pub sender: NodeId,
    pub message: Message,
}
