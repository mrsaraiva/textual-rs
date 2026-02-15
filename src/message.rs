use crate::node_id::NodeId;
use crate::validation::ValidationResult;
use crate::worker::{WorkerId, WorkerState};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helper types (unchanged)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Per-message structs — unit messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClearRequested;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderIconPressed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelpPanelFocusedHelpCleared;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPaletteOpened;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPaletteClosed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionListSelectedChanged;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastDismissed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabsCleared;

// ---------------------------------------------------------------------------
// Per-message structs — input / text editing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct InputChanged {
    pub value: String,
    pub validation: ValidationResult,
}

#[derive(Debug, Clone)]
pub struct InputSubmitted {
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct InputBlurred {
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct TextAreaChanged {
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct TextAreaSelectionChanged {
    pub start: (usize, usize),
    pub end: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct TextEditClipboardCopyRequested {
    pub text: String,
    pub cut: bool,
}

#[derive(Debug, Clone)]
pub struct TextEditClipboardPasteRequested {
    pub target: NodeId,
}

#[derive(Debug, Clone)]
pub struct TextEditClipboardPaste {
    pub target: NodeId,
    pub text: String,
}

// ---------------------------------------------------------------------------
// Per-message structs — button / checkbox / switch / radio
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ButtonPressed {
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct CheckboxChanged {
    pub checked: bool,
}

#[derive(Debug, Clone)]
pub struct SwitchChanged {
    pub value: bool,
}

#[derive(Debug, Clone)]
pub struct RadioButtonChanged {
    pub value: bool,
}

#[derive(Debug, Clone)]
pub struct RadioSetChanged {
    pub index: usize,
    pub button_id: NodeId,
}

// ---------------------------------------------------------------------------
// Per-message structs — list / select / option
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ListViewSelectionChanged {
    pub index: usize,
    pub item: String,
}

#[derive(Debug, Clone)]
pub struct ListViewItemActivated {
    pub index: usize,
    pub item: String,
}

#[derive(Debug, Clone)]
pub struct OptionHighlighted {
    pub index: usize,
}

#[derive(Debug, Clone)]
pub struct OptionSelected {
    pub index: usize,
}

#[derive(Debug, Clone)]
pub struct SelectChanged {
    pub index: usize,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct SelectionListToggled {
    pub index: usize,
    pub selected: bool,
}

// ---------------------------------------------------------------------------
// Per-message structs — tabs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TabActivated {
    pub id: String,
    pub index: usize,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct TabClicked {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct TabDisabled {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct TabEnabled {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct TabHidden {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct TabShown {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct TabPaneFocused {
    pub id: String,
}

// ---------------------------------------------------------------------------
// Per-message structs — header / footer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HeaderToggled {
    pub tall: bool,
}

#[derive(Debug, Clone)]
pub struct FooterBindingsUpdated {
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct ScreenTitleChanged {
    pub title: Option<String>,
    pub sub_title: Option<String>,
}

// ---------------------------------------------------------------------------
// Per-message structs — help panel
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HelpPanelSetHelp {
    pub panel: NodeId,
    pub markup: String,
}

#[derive(Debug, Clone)]
pub struct HelpPanelClearHelp {
    pub panel: NodeId,
}

#[derive(Debug, Clone)]
pub struct HelpPanelFocusedHelpChanged {
    pub source: NodeId,
    pub markup: String,
}

// ---------------------------------------------------------------------------
// Per-message structs — tree / directory tree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TreeNodeSelected {
    pub index: usize,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct TreeNodeActivated {
    pub index: usize,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct TreeNodeToggled {
    pub index: usize,
    pub label: String,
    pub expanded: bool,
}

#[derive(Debug, Clone)]
pub struct TreeNodeCollapsed {
    pub index: usize,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct TreeNodeExpanded {
    pub index: usize,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct TreeNodeHighlighted {
    pub index: usize,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct DirectoryTreeFileSelected {
    pub index: usize,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct DirectoryTreeDirectorySelected {
    pub index: usize,
    pub path: String,
}

// ---------------------------------------------------------------------------
// Per-message structs — overlay
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OverlaySetVisible {
    pub overlay: NodeId,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct OverlaySetAnchor {
    pub overlay: NodeId,
    pub x: usize,
    pub y: usize,
}

#[derive(Debug, Clone)]
pub struct OverlayClearAnchor {
    pub overlay: NodeId,
}

#[derive(Debug, Clone)]
pub struct OverlayToggle {
    pub overlay: NodeId,
}

#[derive(Debug, Clone)]
pub struct OverlayDismissRequested {
    pub overlay: Option<NodeId>,
}

#[derive(Debug, Clone)]
pub struct OverlayVisibilityChanged {
    pub overlay: NodeId,
    pub visible: bool,
}

// ---------------------------------------------------------------------------
// Per-message structs — app actions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppBack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppBell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppChangeTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppCommandPalette;

#[derive(Debug, Clone)]
pub struct AppFocus {
    pub widget_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppFocusNext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppFocusPrevious;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppHelpQuit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppHideHelpPanel;

#[derive(Debug, Clone)]
pub struct AppAddClass {
    pub selector: String,
    pub class_name: String,
}

#[derive(Debug, Clone)]
pub struct AppRemoveClass {
    pub selector: String,
    pub class_name: String,
}

#[derive(Debug, Clone)]
pub struct AppToggleClass {
    pub selector: String,
    pub class_name: String,
}

#[derive(Debug, Clone)]
pub struct AppSetDisabled {
    pub selector: String,
    pub disabled: bool,
}

#[derive(Debug, Clone)]
pub struct AppNotify {
    pub message: String,
    pub title: String,
    pub severity: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppPopScreen;

#[derive(Debug, Clone)]
pub struct AppPushScreen {
    pub screen: String,
}

#[derive(Debug, Clone)]
pub struct AppScreenshot {
    pub filename: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppShowHelpPanel;

#[derive(Debug, Clone)]
pub struct AppSimulateKey {
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppSuspendProcess;

#[derive(Debug, Clone)]
pub struct AppSwitchMode {
    pub mode: String,
}

#[derive(Debug, Clone)]
pub struct AppSwitchScreen {
    pub screen: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppToggleDark;

// ---------------------------------------------------------------------------
// Per-message structs — command palette
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CommandPaletteCommandSelected {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct CommandPaletteSetCommands {
    pub commands: Vec<CommandPaletteCommand>,
}

// ---------------------------------------------------------------------------
// Per-message structs — data table
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DataTableCursorMoved {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct DataTableHeaderSelected {
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct DataTableCellActivated {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct DataTableCellHighlighted {
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct DataTableRowHighlighted {
    pub row: usize,
}

#[derive(Debug, Clone)]
pub struct DataTableRowSelected {
    pub row: usize,
}

#[derive(Debug, Clone)]
pub struct DataTableColumnHighlighted {
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct DataTableColumnSelected {
    pub col: usize,
}

// ---------------------------------------------------------------------------
// Per-message structs — misc widgets
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PlaceholderVariantChanged {
    pub variant: String,
}

#[derive(Debug, Clone)]
pub struct CollapsibleToggled {
    pub collapsed: bool,
}

#[derive(Debug, Clone)]
pub struct LinkClicked {
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct KeyPanelBindingsUpdated {
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct KeyPanelScrolled {
    pub offset: usize,
    pub max_offset: usize,
}

#[derive(Debug, Clone)]
pub struct RichLogScrolled {
    pub offset: usize,
    pub max_offset: usize,
}

// ---------------------------------------------------------------------------
// Per-message structs — async tasks / timers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AsyncTaskSpawn {
    pub task_id: u64,
    pub target: NodeId,
    pub request: AsyncTaskRequest,
}

#[derive(Debug, Clone)]
pub struct AsyncTaskCancel {
    pub task_id: u64,
}

#[derive(Debug, Clone)]
pub struct AsyncTaskCancelTarget {
    pub target: NodeId,
}

#[derive(Debug, Clone)]
pub struct AsyncTaskCompleted {
    pub task_id: u64,
    pub target: NodeId,
    pub result: AsyncTaskResult,
}

#[derive(Debug, Clone)]
pub struct AsyncTaskCancelled {
    pub task_id: u64,
    pub target: NodeId,
}

#[derive(Debug, Clone)]
pub struct TimerSchedule {
    pub timer_id: u64,
    pub target: NodeId,
    pub delay: Duration,
}

#[derive(Debug, Clone)]
pub struct TimerCancel {
    pub timer_id: u64,
}

#[derive(Debug, Clone)]
pub struct TimerFired {
    pub timer_id: u64,
    pub target: NodeId,
}

#[derive(Debug, Clone)]
pub struct TimerCancelled {
    pub timer_id: u64,
    pub target: NodeId,
}

#[derive(Debug, Clone)]
pub struct WorkerStateChanged {
    pub worker_id: WorkerId,
    pub state: WorkerState,
}

// ---------------------------------------------------------------------------
// User-defined message extensibility
// ---------------------------------------------------------------------------

/// Trait for user-defined messages that can be sent through the framework's
/// message system. Framework messages use the closed `Message` enum; user
/// messages implement this trait and are carried via `Message::Custom`.
pub trait UserMessage: std::any::Any + Send + Sync + std::fmt::Debug + 'static {
    /// Downcast to concrete type.
    fn as_any(&self) -> &dyn std::any::Any;
    /// Clone into a boxed trait object.
    fn clone_box(&self) -> Box<dyn UserMessage>;
}

impl Clone for Box<dyn UserMessage> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

// ---------------------------------------------------------------------------
// Message enum — newtype wrappers around individual structs
// ---------------------------------------------------------------------------

/// Generates `From<Struct> for Message` for each variant.
macro_rules! impl_message_from {
    ($($Variant:ident),* $(,)?) => {
        $(
            impl From<$Variant> for Message {
                fn from(v: $Variant) -> Self {
                    Message::$Variant(v)
                }
            }
        )*
    };
}

#[derive(Debug, Clone)]
pub enum Message {
    // Unit messages
    ClearRequested(ClearRequested),
    HeaderIconPressed(HeaderIconPressed),
    HelpPanelFocusedHelpCleared(HelpPanelFocusedHelpCleared),
    CommandPaletteOpened(CommandPaletteOpened),
    CommandPaletteClosed(CommandPaletteClosed),
    SelectionListSelectedChanged(SelectionListSelectedChanged),
    ToastDismissed(ToastDismissed),
    TabsCleared(TabsCleared),
    // Input / text editing
    InputChanged(InputChanged),
    InputSubmitted(InputSubmitted),
    InputBlurred(InputBlurred),
    TextAreaChanged(TextAreaChanged),
    TextAreaSelectionChanged(TextAreaSelectionChanged),
    TextEditClipboardCopyRequested(TextEditClipboardCopyRequested),
    TextEditClipboardPasteRequested(TextEditClipboardPasteRequested),
    TextEditClipboardPaste(TextEditClipboardPaste),
    // Button / checkbox / switch / radio
    ButtonPressed(ButtonPressed),
    CheckboxChanged(CheckboxChanged),
    SwitchChanged(SwitchChanged),
    RadioButtonChanged(RadioButtonChanged),
    RadioSetChanged(RadioSetChanged),
    // List / select / option
    ListViewSelectionChanged(ListViewSelectionChanged),
    ListViewItemActivated(ListViewItemActivated),
    OptionHighlighted(OptionHighlighted),
    OptionSelected(OptionSelected),
    SelectChanged(SelectChanged),
    SelectionListToggled(SelectionListToggled),
    // Tabs
    TabActivated(TabActivated),
    TabClicked(TabClicked),
    TabDisabled(TabDisabled),
    TabEnabled(TabEnabled),
    TabHidden(TabHidden),
    TabShown(TabShown),
    TabPaneFocused(TabPaneFocused),
    // Header / footer
    HeaderToggled(HeaderToggled),
    FooterBindingsUpdated(FooterBindingsUpdated),
    ScreenTitleChanged(ScreenTitleChanged),
    // Help panel
    HelpPanelSetHelp(HelpPanelSetHelp),
    HelpPanelClearHelp(HelpPanelClearHelp),
    HelpPanelFocusedHelpChanged(HelpPanelFocusedHelpChanged),
    // Tree / directory tree
    TreeNodeSelected(TreeNodeSelected),
    TreeNodeActivated(TreeNodeActivated),
    TreeNodeToggled(TreeNodeToggled),
    TreeNodeCollapsed(TreeNodeCollapsed),
    TreeNodeExpanded(TreeNodeExpanded),
    TreeNodeHighlighted(TreeNodeHighlighted),
    DirectoryTreeFileSelected(DirectoryTreeFileSelected),
    DirectoryTreeDirectorySelected(DirectoryTreeDirectorySelected),
    // Overlay
    OverlaySetVisible(OverlaySetVisible),
    OverlaySetAnchor(OverlaySetAnchor),
    OverlayClearAnchor(OverlayClearAnchor),
    OverlayToggle(OverlayToggle),
    OverlayDismissRequested(OverlayDismissRequested),
    OverlayVisibilityChanged(OverlayVisibilityChanged),
    // App actions
    AppBack(AppBack),
    AppBell(AppBell),
    AppChangeTheme(AppChangeTheme),
    AppCommandPalette(AppCommandPalette),
    AppFocus(AppFocus),
    AppFocusNext(AppFocusNext),
    AppFocusPrevious(AppFocusPrevious),
    AppHelpQuit(AppHelpQuit),
    AppHideHelpPanel(AppHideHelpPanel),
    AppAddClass(AppAddClass),
    AppRemoveClass(AppRemoveClass),
    AppToggleClass(AppToggleClass),
    AppSetDisabled(AppSetDisabled),
    AppNotify(AppNotify),
    AppPopScreen(AppPopScreen),
    AppPushScreen(AppPushScreen),
    AppScreenshot(AppScreenshot),
    AppShowHelpPanel(AppShowHelpPanel),
    AppSimulateKey(AppSimulateKey),
    AppSuspendProcess(AppSuspendProcess),
    AppSwitchMode(AppSwitchMode),
    AppSwitchScreen(AppSwitchScreen),
    AppToggleDark(AppToggleDark),
    // Command palette
    CommandPaletteCommandSelected(CommandPaletteCommandSelected),
    CommandPaletteSetCommands(CommandPaletteSetCommands),
    // Data table
    DataTableCursorMoved(DataTableCursorMoved),
    DataTableHeaderSelected(DataTableHeaderSelected),
    DataTableCellActivated(DataTableCellActivated),
    DataTableCellHighlighted(DataTableCellHighlighted),
    DataTableRowHighlighted(DataTableRowHighlighted),
    DataTableRowSelected(DataTableRowSelected),
    DataTableColumnHighlighted(DataTableColumnHighlighted),
    DataTableColumnSelected(DataTableColumnSelected),
    // Misc widgets
    PlaceholderVariantChanged(PlaceholderVariantChanged),
    CollapsibleToggled(CollapsibleToggled),
    LinkClicked(LinkClicked),
    KeyPanelBindingsUpdated(KeyPanelBindingsUpdated),
    KeyPanelScrolled(KeyPanelScrolled),
    RichLogScrolled(RichLogScrolled),
    // Async tasks / timers
    AsyncTaskSpawn(AsyncTaskSpawn),
    AsyncTaskCancel(AsyncTaskCancel),
    AsyncTaskCancelTarget(AsyncTaskCancelTarget),
    AsyncTaskCompleted(AsyncTaskCompleted),
    AsyncTaskCancelled(AsyncTaskCancelled),
    TimerSchedule(TimerSchedule),
    TimerCancel(TimerCancel),
    TimerFired(TimerFired),
    TimerCancelled(TimerCancelled),
    WorkerStateChanged(WorkerStateChanged),
    // User-defined messages
    Custom(Box<dyn UserMessage>),
}

impl_message_from!(
    ClearRequested,
    HeaderIconPressed,
    HelpPanelFocusedHelpCleared,
    CommandPaletteOpened,
    CommandPaletteClosed,
    SelectionListSelectedChanged,
    ToastDismissed,
    TabsCleared,
    InputChanged,
    InputSubmitted,
    InputBlurred,
    TextAreaChanged,
    TextAreaSelectionChanged,
    TextEditClipboardCopyRequested,
    TextEditClipboardPasteRequested,
    TextEditClipboardPaste,
    ButtonPressed,
    CheckboxChanged,
    SwitchChanged,
    RadioButtonChanged,
    RadioSetChanged,
    ListViewSelectionChanged,
    ListViewItemActivated,
    OptionHighlighted,
    OptionSelected,
    SelectChanged,
    SelectionListToggled,
    TabActivated,
    TabClicked,
    TabDisabled,
    TabEnabled,
    TabHidden,
    TabShown,
    TabPaneFocused,
    HeaderToggled,
    FooterBindingsUpdated,
    ScreenTitleChanged,
    HelpPanelSetHelp,
    HelpPanelClearHelp,
    HelpPanelFocusedHelpChanged,
    TreeNodeSelected,
    TreeNodeActivated,
    TreeNodeToggled,
    TreeNodeCollapsed,
    TreeNodeExpanded,
    TreeNodeHighlighted,
    DirectoryTreeFileSelected,
    DirectoryTreeDirectorySelected,
    OverlaySetVisible,
    OverlaySetAnchor,
    OverlayClearAnchor,
    OverlayToggle,
    OverlayDismissRequested,
    OverlayVisibilityChanged,
    AppBack,
    AppBell,
    AppChangeTheme,
    AppCommandPalette,
    AppFocus,
    AppFocusNext,
    AppFocusPrevious,
    AppHelpQuit,
    AppHideHelpPanel,
    AppAddClass,
    AppRemoveClass,
    AppToggleClass,
    AppSetDisabled,
    AppNotify,
    AppPopScreen,
    AppPushScreen,
    AppScreenshot,
    AppShowHelpPanel,
    AppSimulateKey,
    AppSuspendProcess,
    AppSwitchMode,
    AppSwitchScreen,
    AppToggleDark,
    CommandPaletteCommandSelected,
    CommandPaletteSetCommands,
    DataTableCursorMoved,
    DataTableHeaderSelected,
    DataTableCellActivated,
    DataTableCellHighlighted,
    DataTableRowHighlighted,
    DataTableRowSelected,
    DataTableColumnHighlighted,
    DataTableColumnSelected,
    PlaceholderVariantChanged,
    CollapsibleToggled,
    LinkClicked,
    KeyPanelBindingsUpdated,
    KeyPanelScrolled,
    RichLogScrolled,
    AsyncTaskSpawn,
    AsyncTaskCancel,
    AsyncTaskCancelTarget,
    AsyncTaskCompleted,
    AsyncTaskCancelled,
    TimerSchedule,
    TimerCancel,
    TimerFired,
    TimerCancelled,
    WorkerStateChanged,
);

// ---------------------------------------------------------------------------
// MessageEvent / MessageEnvelope (unchanged structure)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MessageEvent {
    pub sender: NodeId,
    pub message: Message,
    /// The originating widget ("control") — defaults to sender.
    /// Handlers can use this to identify which widget produced the message,
    /// even when the message has bubbled through containers.
    pub control: Option<NodeId>,
}

/// Wraps a [`MessageEvent`] with propagation control metadata.
///
/// Handlers can stop bubbling, prevent default handling, or mark the
/// envelope as replaceable so the queue can coalesce repeated updates
/// (e.g. rapid cursor-position changes).
#[derive(Debug, Clone)]
pub struct MessageEnvelope {
    /// The original message event.
    pub event: MessageEvent,
    /// The originating widget's [`NodeId`] (the "control").
    ///
    /// By default this is the sender, but it can be overridden to point at
    /// a different widget (e.g. when a container re-emits a child's message).
    control: Option<NodeId>,
    /// If true, message propagation stops (won't bubble further).
    stopped: bool,
    /// If true, the default handler for this message is skipped.
    prevented: bool,
    /// If true, this message can replace a previous identical message in the queue.
    replaceable: bool,
}

impl MessageEnvelope {
    /// Create a new envelope with all control flags set to `false`.
    ///
    /// The `control` field is initialised to `Some(event.sender)` — the
    /// originating widget is the sender by default.
    pub fn new(event: MessageEvent) -> Self {
        let control = event.control.or(Some(event.sender));
        Self {
            event,
            control,
            stopped: false,
            prevented: false,
            replaceable: false,
        }
    }

    /// Stop message bubbling — the message won't propagate further up the tree.
    pub fn stop(&mut self) {
        self.stopped = true;
    }

    /// Whether bubbling has been stopped.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Skip the default handler for this message.
    pub fn prevent_default(&mut self) {
        self.prevented = true;
    }

    /// Whether the default handler should be skipped.
    pub fn is_default_prevented(&self) -> bool {
        self.prevented
    }

    /// Mark whether this envelope can replace a previous identical message
    /// in the queue (useful for coalescing rapid updates).
    pub fn set_replaceable(&mut self, replaceable: bool) {
        self.replaceable = replaceable;
    }

    /// Whether this envelope can replace a queued duplicate.
    pub fn can_replace(&self) -> bool {
        self.replaceable
    }

    /// The originating widget (the "control") that produced this message.
    ///
    /// Returns `Some(node_id)` — by default the sender.  Stays constant as
    /// the message bubbles up the tree (it always refers to the widget that
    /// *originated* the message, not the current handler).
    pub fn control(&self) -> Option<NodeId> {
        self.control
    }

    /// Override the control node.
    ///
    /// Useful when a container re-emits a child's message under its own
    /// sender but still wants to preserve which child was the true source.
    pub fn set_control(&mut self, node: NodeId) {
        self.control = Some(node);
    }

    /// Convenience accessor: the sender [`NodeId`].
    pub fn sender(&self) -> NodeId {
        self.event.sender
    }

    /// Convenience accessor: a reference to the inner [`Message`].
    pub fn message(&self) -> &Message {
        &self.event.message
    }
}

impl From<MessageEvent> for MessageEnvelope {
    fn from(event: MessageEvent) -> Self {
        Self::new(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_id::node_id_from_ffi;

    /// Helper: build a simple `MessageEvent` for testing.
    fn test_event() -> MessageEvent {
        MessageEvent {
            sender: node_id_from_ffi(1),
            message: Message::ButtonPressed(ButtonPressed {
                description: "ok".into(),
            }),
            control: None,
        }
    }

    // --- Construction defaults ---

    #[test]
    fn new_envelope_defaults_stopped_false() {
        let env = MessageEnvelope::new(test_event());
        assert!(!env.is_stopped());
    }

    #[test]
    fn new_envelope_defaults_prevented_false() {
        let env = MessageEnvelope::new(test_event());
        assert!(!env.is_default_prevented());
    }

    #[test]
    fn new_envelope_defaults_replaceable_false() {
        let env = MessageEnvelope::new(test_event());
        assert!(!env.can_replace());
    }

    // --- stop() ---

    #[test]
    fn stop_sets_stopped_flag() {
        let mut env = MessageEnvelope::new(test_event());
        env.stop();
        assert!(env.is_stopped());
    }

    #[test]
    fn stop_is_idempotent() {
        let mut env = MessageEnvelope::new(test_event());
        env.stop();
        env.stop();
        assert!(env.is_stopped());
    }

    // --- prevent_default() ---

    #[test]
    fn prevent_default_sets_prevented_flag() {
        let mut env = MessageEnvelope::new(test_event());
        env.prevent_default();
        assert!(env.is_default_prevented());
    }

    #[test]
    fn prevent_default_is_idempotent() {
        let mut env = MessageEnvelope::new(test_event());
        env.prevent_default();
        env.prevent_default();
        assert!(env.is_default_prevented());
    }

    // --- set_replaceable / can_replace ---

    #[test]
    fn set_replaceable_true() {
        let mut env = MessageEnvelope::new(test_event());
        env.set_replaceable(true);
        assert!(env.can_replace());
    }

    #[test]
    fn set_replaceable_false_after_true() {
        let mut env = MessageEnvelope::new(test_event());
        env.set_replaceable(true);
        env.set_replaceable(false);
        assert!(!env.can_replace());
    }

    // --- Convenience accessors ---

    #[test]
    fn sender_returns_event_sender() {
        let evt = test_event();
        let expected_sender = evt.sender;
        let env = MessageEnvelope::new(evt);
        assert_eq!(env.sender(), expected_sender);
    }

    #[test]
    fn message_returns_event_message() {
        let env = MessageEnvelope::new(test_event());
        match env.message() {
            Message::ButtonPressed(ButtonPressed { description }) => {
                assert_eq!(description, "ok");
            }
            other => panic!("unexpected message variant: {:?}", other),
        }
    }

    // --- From<MessageEvent> ---

    #[test]
    fn from_message_event_defaults_all_flags_false() {
        let env: MessageEnvelope = test_event().into();
        assert!(!env.is_stopped());
        assert!(!env.is_default_prevented());
        assert!(!env.can_replace());
    }

    #[test]
    fn from_preserves_sender_and_message() {
        let evt = test_event();
        let expected_sender = evt.sender;
        let env: MessageEnvelope = evt.into();
        assert_eq!(env.sender(), expected_sender);
        assert!(matches!(env.message(), Message::ButtonPressed(..)));
    }

    // --- Flag independence ---

    #[test]
    fn stop_does_not_affect_prevented() {
        let mut env = MessageEnvelope::new(test_event());
        env.stop();
        assert!(!env.is_default_prevented());
    }

    #[test]
    fn prevent_default_does_not_affect_stopped() {
        let mut env = MessageEnvelope::new(test_event());
        env.prevent_default();
        assert!(!env.is_stopped());
    }

    #[test]
    fn all_flags_independent() {
        let mut env = MessageEnvelope::new(test_event());
        env.stop();
        env.prevent_default();
        env.set_replaceable(true);
        assert!(env.is_stopped());
        assert!(env.is_default_prevented());
        assert!(env.can_replace());
    }

    // --- Clone preserves flags ---

    #[test]
    fn clone_preserves_flags() {
        let mut env = MessageEnvelope::new(test_event());
        env.stop();
        env.set_replaceable(true);
        let cloned = env.clone();
        assert!(cloned.is_stopped());
        assert!(!cloned.is_default_prevented());
        assert!(cloned.can_replace());
        // control should also survive the clone.
        assert_eq!(cloned.control(), env.control());
    }

    // --- control() / set_control() ---

    #[test]
    fn control_defaults_to_sender() {
        let evt = test_event();
        let expected = evt.sender;
        let env = MessageEnvelope::new(evt);
        assert_eq!(env.control(), Some(expected));
    }

    #[test]
    fn from_message_event_sets_control_to_sender() {
        let evt = test_event();
        let expected = evt.sender;
        let env: MessageEnvelope = evt.into();
        assert_eq!(env.control(), Some(expected));
    }

    #[test]
    fn set_control_overrides_default() {
        let mut env = MessageEnvelope::new(test_event());
        let other = node_id_from_ffi(42);
        env.set_control(other);
        assert_eq!(env.control(), Some(other));
        // sender() is unchanged.
        assert_ne!(env.sender(), other);
    }

    #[test]
    fn clone_preserves_control() {
        let mut env = MessageEnvelope::new(test_event());
        let other = node_id_from_ffi(99);
        env.set_control(other);
        let cloned = env.clone();
        assert_eq!(cloned.control(), Some(other));
    }

    // --- From impls ---

    #[test]
    fn from_unit_struct_into_message() {
        let msg: Message = ClearRequested.into();
        assert!(matches!(msg, Message::ClearRequested(..)));
    }

    #[test]
    fn from_field_struct_into_message() {
        let msg: Message = ButtonPressed {
            description: "test".into(),
        }
        .into();
        assert!(matches!(msg, Message::ButtonPressed(..)));
    }

    // --- MessageEvent control field ---

    #[test]
    fn event_control_none_promoted_to_sender_by_envelope() {
        // When control is None, MessageEnvelope::new() should set it to Some(sender).
        let evt = test_event();
        assert!(evt.control.is_none());
        let env = MessageEnvelope::new(evt);
        assert_eq!(env.control(), Some(node_id_from_ffi(1)));
    }

    #[test]
    fn explicit_control_preserved_by_envelope() {
        let other = node_id_from_ffi(42);
        let evt = MessageEvent {
            sender: node_id_from_ffi(1),
            message: Message::ButtonPressed(ButtonPressed {
                description: "ctrl".into(),
            }),
            control: Some(other),
        };
        let env = MessageEnvelope::new(evt);
        // Envelope should use the explicit control, not sender.
        assert_eq!(env.control(), Some(other));
    }

    #[test]
    fn control_none_is_allowed() {
        let evt = MessageEvent {
            sender: node_id_from_ffi(1),
            message: Message::ClearRequested(ClearRequested),
            control: None,
        };
        // None is allowed on the event; envelope promotes it.
        assert!(evt.control.is_none());
        let env = MessageEnvelope::new(evt);
        assert_eq!(env.control(), Some(node_id_from_ffi(1)));
    }
}
