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
crate::impl_message!(ClearRequested);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderIconPressed;
crate::impl_message!(HeaderIconPressed);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelpPanelFocusedHelpCleared;
crate::impl_message!(HelpPanelFocusedHelpCleared);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPaletteOpened;
crate::impl_message!(CommandPaletteOpened);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPaletteClosed;
crate::impl_message!(CommandPaletteClosed);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionListSelectedChanged;
crate::impl_message!(SelectionListSelectedChanged);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastDismissed;
crate::impl_message!(ToastDismissed);

/// Posted when markdown navigation state changes.
///
/// Apps/widgets can emit this to refresh bindings (e.g. dim back/forward at
/// history ends via `check_action`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavigatorUpdated;
crate::impl_message!(NavigatorUpdated);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabsCleared;
crate::impl_message!(TabsCleared);

// ---------------------------------------------------------------------------
// Per-message structs — input / text editing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct InputChanged {
    pub value: String,
    pub validation: ValidationResult,
}
crate::impl_message!(InputChanged, replaceable);

#[derive(Debug, Clone)]
pub struct InputSubmitted {
    pub value: String,
}
crate::impl_message!(InputSubmitted);

#[derive(Debug, Clone)]
pub struct InputBlurred {
    pub value: String,
}
crate::impl_message!(InputBlurred);

#[derive(Debug, Clone)]
pub struct TextAreaChanged {
    pub value: String,
}
crate::impl_message!(TextAreaChanged, replaceable);

#[derive(Debug, Clone)]
pub struct TextAreaSelectionChanged {
    pub start: (usize, usize),
    pub end: (usize, usize),
}
crate::impl_message!(TextAreaSelectionChanged, replaceable);

#[derive(Debug, Clone)]
pub struct TextEditClipboardCopyRequested {
    pub text: String,
    pub cut: bool,
}
crate::impl_message!(TextEditClipboardCopyRequested);

#[derive(Debug, Clone)]
pub struct TextEditClipboardPasteRequested {
    pub target: NodeId,
}
crate::impl_message!(TextEditClipboardPasteRequested);

#[derive(Debug, Clone)]
pub struct TextEditClipboardPaste {
    pub target: NodeId,
    pub text: String,
}
crate::impl_message!(TextEditClipboardPaste);

// ---------------------------------------------------------------------------
// Per-message structs — button / checkbox / switch / radio
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ButtonPressed {
    pub description: String,
    /// CSS id of the button that was pressed, if the button has one.
    ///
    /// Mirrors Python's `Button.Pressed.button.id`.
    pub button_id: Option<String>,
}
crate::impl_message!(ButtonPressed);

#[derive(Debug, Clone)]
pub struct CheckboxChanged {
    pub checked: bool,
}
crate::impl_message!(CheckboxChanged);

#[derive(Debug, Clone)]
pub struct SwitchChanged {
    pub value: bool,
}
crate::impl_message!(SwitchChanged);

#[derive(Debug, Clone)]
pub struct RadioButtonChanged {
    pub value: bool,
}
crate::impl_message!(RadioButtonChanged);

#[derive(Debug, Clone)]
pub struct RadioSetChanged {
    pub index: usize,
    pub button_id: NodeId,
}
crate::impl_message!(RadioSetChanged);

// ---------------------------------------------------------------------------
// Per-message structs — list / select / option
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ListViewSelectionChanged {
    pub index: usize,
    pub item: String,
}
crate::impl_message!(ListViewSelectionChanged);

#[derive(Debug, Clone)]
pub struct ListViewItemActivated {
    pub index: usize,
    pub item: String,
}
crate::impl_message!(ListViewItemActivated);

#[derive(Debug, Clone)]
pub struct OptionHighlighted {
    pub index: usize,
}
crate::impl_message!(OptionHighlighted, replaceable);

#[derive(Debug, Clone)]
pub struct OptionSelected {
    pub index: usize,
}
crate::impl_message!(OptionSelected);

#[derive(Debug, Clone)]
pub struct SelectChanged {
    pub index: usize,
    pub label: String,
}
crate::impl_message!(SelectChanged);

#[derive(Debug, Clone)]
pub struct SelectionListToggled {
    pub index: usize,
    pub selected: bool,
}
crate::impl_message!(SelectionListToggled);

// ---------------------------------------------------------------------------
// Per-message structs — tabs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TabActivated {
    pub id: String,
    pub index: usize,
    pub title: String,
}
crate::impl_message!(TabActivated);

#[derive(Debug, Clone)]
pub struct TabClicked {
    pub id: String,
    pub title: String,
}
crate::impl_message!(TabClicked);

#[derive(Debug, Clone)]
pub struct TabDisabled {
    pub id: String,
}
crate::impl_message!(TabDisabled);

#[derive(Debug, Clone)]
pub struct TabEnabled {
    pub id: String,
}
crate::impl_message!(TabEnabled);

#[derive(Debug, Clone)]
pub struct TabHidden {
    pub id: String,
}
crate::impl_message!(TabHidden);

#[derive(Debug, Clone)]
pub struct TabShown {
    pub id: String,
}
crate::impl_message!(TabShown);

#[derive(Debug, Clone)]
pub struct TabPaneFocused {
    pub id: String,
}
crate::impl_message!(TabPaneFocused);

// ---------------------------------------------------------------------------
// Per-message structs — header / footer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HeaderToggled {
    pub tall: bool,
}
crate::impl_message!(HeaderToggled);

#[derive(Debug, Clone)]
pub struct FooterBindingsUpdated {
    pub count: usize,
}
crate::impl_message!(FooterBindingsUpdated);

#[derive(Debug, Clone)]
pub struct ScreenTitleChanged {
    pub title: Option<String>,
    pub sub_title: Option<String>,
}
crate::impl_message!(ScreenTitleChanged);

// ---------------------------------------------------------------------------
// Per-message structs — help panel
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HelpPanelSetHelp {
    pub panel: NodeId,
    pub markup: String,
}
crate::impl_message!(HelpPanelSetHelp);

#[derive(Debug, Clone)]
pub struct HelpPanelClearHelp {
    pub panel: NodeId,
}
crate::impl_message!(HelpPanelClearHelp);

#[derive(Debug, Clone)]
pub struct HelpPanelFocusedHelpChanged {
    pub source: NodeId,
    pub markup: String,
}
crate::impl_message!(HelpPanelFocusedHelpChanged);

// ---------------------------------------------------------------------------
// Per-message structs — tree / directory tree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TreeNodeSelected {
    pub index: usize,
    pub label: String,
    /// Optional user data from the selected TreeNode.
    pub data: Option<String>,
}
crate::impl_message!(TreeNodeSelected);

#[derive(Debug, Clone)]
pub struct TreeNodeActivated {
    pub index: usize,
    pub label: String,
    /// Optional user data from the activated TreeNode.
    pub data: Option<String>,
}
crate::impl_message!(TreeNodeActivated);

#[derive(Debug, Clone)]
pub struct TreeNodeToggled {
    pub index: usize,
    pub label: String,
    pub expanded: bool,
}
crate::impl_message!(TreeNodeToggled);

#[derive(Debug, Clone)]
pub struct TreeNodeCollapsed {
    pub index: usize,
    pub label: String,
}
crate::impl_message!(TreeNodeCollapsed);

#[derive(Debug, Clone)]
pub struct TreeNodeExpanded {
    pub index: usize,
    pub label: String,
}
crate::impl_message!(TreeNodeExpanded);

#[derive(Debug, Clone)]
pub struct TreeNodeHighlighted {
    pub index: usize,
    pub label: String,
}
crate::impl_message!(TreeNodeHighlighted, replaceable);

// ---------------------------------------------------------------------------
// Per-message structs — MarkdownViewer
// ---------------------------------------------------------------------------

/// Posted by `MarkdownTableOfContents` when a TOC heading is selected/activated.
///
/// Mirrors Python's `Markdown.TableOfContentsSelected`. The `block_id` identifies the
/// heading block in the document (e.g. `"h2--section-title"`).
#[derive(Debug, Clone)]
pub struct MarkdownTableOfContentsSelected {
    pub block_id: String,
}
crate::impl_message!(MarkdownTableOfContentsSelected);

/// Posted when markdown heading metadata changes.
///
/// Mirrors Python's `Markdown.TableOfContentsUpdated` message flow.
#[derive(Debug, Clone)]
pub struct MarkdownTableOfContentsUpdated {
    pub headings: Vec<(usize, String, String)>,
}
crate::impl_message!(MarkdownTableOfContentsUpdated);

#[derive(Debug, Clone)]
pub struct DirectoryTreeFileSelected {
    pub index: usize,
    pub path: String,
}
crate::impl_message!(DirectoryTreeFileSelected);

#[derive(Debug, Clone)]
pub struct DirectoryTreeDirectorySelected {
    pub index: usize,
    pub path: String,
}
crate::impl_message!(DirectoryTreeDirectorySelected);

// ---------------------------------------------------------------------------
// Per-message structs — overlay
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OverlaySetVisible {
    pub overlay: NodeId,
    pub visible: bool,
}
crate::impl_message!(OverlaySetVisible);

#[derive(Debug, Clone)]
pub struct OverlaySetAnchor {
    pub overlay: NodeId,
    pub x: usize,
    pub y: usize,
}
crate::impl_message!(OverlaySetAnchor);

#[derive(Debug, Clone)]
pub struct OverlayClearAnchor {
    pub overlay: NodeId,
}
crate::impl_message!(OverlayClearAnchor);

#[derive(Debug, Clone)]
pub struct OverlayToggle {
    pub overlay: NodeId,
}
crate::impl_message!(OverlayToggle);

#[derive(Debug, Clone)]
pub struct OverlayDismissRequested {
    pub overlay: Option<NodeId>,
}
crate::impl_message!(OverlayDismissRequested);

#[derive(Debug, Clone)]
pub struct OverlayVisibilityChanged {
    pub overlay: NodeId,
    pub visible: bool,
}
crate::impl_message!(OverlayVisibilityChanged);

// ---------------------------------------------------------------------------
// Per-message structs — app actions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppBack;
crate::impl_message!(AppBack);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppBell;
crate::impl_message!(AppBell);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppChangeTheme;
crate::impl_message!(AppChangeTheme);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppCommandPalette;
crate::impl_message!(AppCommandPalette);

#[derive(Debug, Clone)]
pub struct AppFocus {
    pub widget_id: String,
}
crate::impl_message!(AppFocus);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppFocusNext;
crate::impl_message!(AppFocusNext);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppFocusPrevious;
crate::impl_message!(AppFocusPrevious);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppHelpQuit;
crate::impl_message!(AppHelpQuit);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppCopySelectedText;
crate::impl_message!(AppCopySelectedText);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppHideHelpPanel;
crate::impl_message!(AppHideHelpPanel);

#[derive(Debug, Clone)]
pub struct AppAddClass {
    pub selector: String,
    pub class_name: String,
}
crate::impl_message!(AppAddClass);

#[derive(Debug, Clone)]
pub struct AppRemoveClass {
    pub selector: String,
    pub class_name: String,
}
crate::impl_message!(AppRemoveClass);

#[derive(Debug, Clone)]
pub struct AppToggleClass {
    pub selector: String,
    pub class_name: String,
}
crate::impl_message!(AppToggleClass);

#[derive(Debug, Clone)]
pub struct AppSetDisabled {
    pub selector: String,
    pub disabled: bool,
}
crate::impl_message!(AppSetDisabled);

#[derive(Debug, Clone)]
pub struct AppNotify {
    pub message: String,
    pub title: String,
    pub severity: String,
}
crate::impl_message!(AppNotify);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppPopScreen;
crate::impl_message!(AppPopScreen);

#[derive(Debug, Clone)]
pub struct AppPushScreen {
    pub screen: String,
}
crate::impl_message!(AppPushScreen);

#[derive(Debug, Clone)]
pub struct AppScreenshot {
    pub filename: Option<String>,
    pub path: Option<String>,
}
crate::impl_message!(AppScreenshot);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppShowHelpPanel;
crate::impl_message!(AppShowHelpPanel);

#[derive(Debug, Clone)]
pub struct AppSimulateKey {
    pub key: String,
}
crate::impl_message!(AppSimulateKey);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppSuspendProcess;
crate::impl_message!(AppSuspendProcess);

#[derive(Debug, Clone)]
pub struct AppSwitchMode {
    pub mode: String,
}
crate::impl_message!(AppSwitchMode);

#[derive(Debug, Clone)]
pub struct AppSwitchScreen {
    pub screen: String,
}
crate::impl_message!(AppSwitchScreen);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppToggleDark;
crate::impl_message!(AppToggleDark);

#[derive(Debug, Clone)]
pub struct ActionDispatchRequested {
    pub action: String,
}
crate::impl_message!(ActionDispatchRequested);

// ---------------------------------------------------------------------------
// Per-message structs — command palette
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CommandPaletteCommandSelected {
    pub id: String,
    pub title: String,
}
crate::impl_message!(CommandPaletteCommandSelected);

#[derive(Debug, Clone)]
pub struct CommandPaletteSetCommands {
    pub commands: Vec<CommandPaletteCommand>,
}
crate::impl_message!(CommandPaletteSetCommands);

// ---------------------------------------------------------------------------
// Per-message structs — scrollbars
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarScrollTo {
    pub axis: ScrollbarAxis,
    pub offset: f32,
    pub animate: bool,
    /// Optional explicit animation duration override.
    ///
    /// When `None`, scroll hosts use their CSS transition configuration.
    pub scroll_duration: Option<Duration>,
}
crate::impl_message!(ScrollbarScrollTo);

// ---------------------------------------------------------------------------
// Per-message structs — data table
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DataTableCursorMoved {
    pub row: usize,
    pub column: usize,
}
crate::impl_message!(DataTableCursorMoved, replaceable);

#[derive(Debug, Clone)]
pub struct DataTableHeaderSelected {
    pub column: usize,
}
crate::impl_message!(DataTableHeaderSelected);

#[derive(Debug, Clone)]
pub struct DataTableCellActivated {
    pub row: usize,
    pub column: usize,
}
crate::impl_message!(DataTableCellActivated);

#[derive(Debug, Clone)]
pub struct DataTableCellHighlighted {
    pub row: usize,
    pub col: usize,
}
crate::impl_message!(DataTableCellHighlighted, replaceable);

#[derive(Debug, Clone)]
pub struct DataTableRowHighlighted {
    pub row: usize,
}
crate::impl_message!(DataTableRowHighlighted, replaceable);

#[derive(Debug, Clone)]
pub struct DataTableRowSelected {
    pub row: usize,
}
crate::impl_message!(DataTableRowSelected);

#[derive(Debug, Clone)]
pub struct DataTableColumnHighlighted {
    pub col: usize,
}
crate::impl_message!(DataTableColumnHighlighted, replaceable);

#[derive(Debug, Clone)]
pub struct DataTableColumnSelected {
    pub col: usize,
}
crate::impl_message!(DataTableColumnSelected);

// ---------------------------------------------------------------------------
// Per-message structs — misc widgets
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PlaceholderVariantChanged {
    pub variant: String,
}
crate::impl_message!(PlaceholderVariantChanged);

#[derive(Debug, Clone)]
pub struct CollapsibleToggled {
    pub collapsed: bool,
}
crate::impl_message!(CollapsibleToggled);

#[derive(Debug, Clone)]
pub struct LinkClicked {
    pub url: String,
}
crate::impl_message!(LinkClicked);

#[derive(Debug, Clone)]
pub struct KeyPanelBindingsUpdated {
    pub count: usize,
}
crate::impl_message!(KeyPanelBindingsUpdated);

#[derive(Debug, Clone)]
pub struct KeyPanelScrolled {
    pub offset: usize,
    pub max_offset: usize,
}
crate::impl_message!(KeyPanelScrolled, replaceable);

#[derive(Debug, Clone)]
pub struct RichLogScrolled {
    pub offset: usize,
    pub max_offset: usize,
}
crate::impl_message!(RichLogScrolled, replaceable);

// ---------------------------------------------------------------------------
// Per-message structs — async tasks / timers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AsyncTaskSpawn {
    pub task_id: u64,
    pub target: NodeId,
    pub request: AsyncTaskRequest,
}
crate::impl_message!(AsyncTaskSpawn);

#[derive(Debug, Clone)]
pub struct AsyncTaskCancel {
    pub task_id: u64,
}
crate::impl_message!(AsyncTaskCancel);

#[derive(Debug, Clone)]
pub struct AsyncTaskCancelTarget {
    pub target: NodeId,
}
crate::impl_message!(AsyncTaskCancelTarget);

#[derive(Debug, Clone)]
pub struct AsyncTaskCompleted {
    pub task_id: u64,
    pub target: NodeId,
    pub result: AsyncTaskResult,
}
crate::impl_message!(AsyncTaskCompleted);

#[derive(Debug, Clone)]
pub struct AsyncTaskCancelled {
    pub task_id: u64,
    pub target: NodeId,
}
crate::impl_message!(AsyncTaskCancelled);

#[derive(Debug, Clone)]
pub struct TimerSchedule {
    pub timer_id: u64,
    pub target: NodeId,
    pub delay: Duration,
}
crate::impl_message!(TimerSchedule);

#[derive(Debug, Clone)]
pub struct TimerCancel {
    pub timer_id: u64,
}
crate::impl_message!(TimerCancel);

#[derive(Debug, Clone)]
pub struct TimerFired {
    pub timer_id: u64,
    pub target: NodeId,
}
crate::impl_message!(TimerFired);

#[derive(Debug, Clone)]
pub struct TimerCancelled {
    pub timer_id: u64,
    pub target: NodeId,
}
crate::impl_message!(TimerCancelled);

#[derive(Debug, Clone)]
pub struct WorkerStateChanged {
    pub worker_id: WorkerId,
    pub state: WorkerState,
}
crate::impl_message!(WorkerStateChanged);

// ---------------------------------------------------------------------------
// Open message trait (transition name: Msg; renamed to Message at Step 19)
// ---------------------------------------------------------------------------

/// Open message trait. Every message — built-in or third-party — is a plain
/// struct implementing this trait. Dispatch is by `TypeId`.
///
/// Use [`impl_message!`] to implement this trait for your types.
pub trait Msg: std::any::Any + Send + Sync + std::fmt::Debug + 'static {
    /// Downcast support.
    fn as_any(&self) -> &dyn std::any::Any;
    /// Clone into a boxed trait object.
    fn clone_box(&self) -> Box<dyn Msg>;
    /// Whether this (newer) message can replace an older `pending` message in
    /// the queue. Called by the coalescer with the newer message as `self`.
    /// Mirrors Python Textual's `Message.can_replace`. Default: `false`.
    fn can_replace(&self, _pending: &dyn Msg) -> bool {
        false
    }
}

impl Clone for Box<dyn Msg> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

// ---------------------------------------------------------------------------
// impl_message! macro — the only way to implement Msg for a struct
// ---------------------------------------------------------------------------

/// Implements the message trait for a `Clone + Debug + Send + Sync` struct.
///
/// - `impl_message!(T)` — plain message (cannot replace pending messages).
/// - `impl_message!(T, replaceable)` — newer instances replace queued pending
///   instances of the same concrete type (same-sender gating is applied by the
///   queue coalescer, not here).
///
/// Third-party crates: `textual::impl_message!(MyMessage);`
#[macro_export]
macro_rules! impl_message {
    ($T:ty) => {
        impl $crate::message::Msg for $T {
            fn as_any(&self) -> &dyn::std::any::Any {
                self
            }
            fn clone_box(&self) -> ::std::boxed::Box<dyn $crate::message::Msg> {
                ::std::boxed::Box::new(::std::clone::Clone::clone(self))
            }
        }
    };
    ($T:ty, replaceable) => {
        impl $crate::message::Msg for $T {
            fn as_any(&self) -> &dyn::std::any::Any {
                self
            }
            fn clone_box(&self) -> ::std::boxed::Box<dyn $crate::message::Msg> {
                ::std::boxed::Box::new(::std::clone::Clone::clone(self))
            }
            fn can_replace(&self, pending: &dyn $crate::message::Msg) -> bool {
                pending.as_any().is::<$T>()
            }
        }
    };
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
    NavigatorUpdated(NavigatorUpdated),
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
    // MarkdownViewer
    MarkdownTableOfContentsSelected(MarkdownTableOfContentsSelected),
    MarkdownTableOfContentsUpdated(MarkdownTableOfContentsUpdated),
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
    AppCopySelectedText(AppCopySelectedText),
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
    ActionDispatchRequested(ActionDispatchRequested),
    // Command palette
    CommandPaletteCommandSelected(CommandPaletteCommandSelected),
    CommandPaletteSetCommands(CommandPaletteSetCommands),
    // Scrollbars
    ScrollbarScrollTo(ScrollbarScrollTo),
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
    Custom(Box<dyn Msg>),
}

impl_message_from!(
    ClearRequested,
    HeaderIconPressed,
    HelpPanelFocusedHelpCleared,
    CommandPaletteOpened,
    CommandPaletteClosed,
    SelectionListSelectedChanged,
    ToastDismissed,
    NavigatorUpdated,
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
    MarkdownTableOfContentsSelected,
    MarkdownTableOfContentsUpdated,
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
    AppCopySelectedText,
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
    ActionDispatchRequested,
    CommandPaletteCommandSelected,
    CommandPaletteSetCommands,
    ScrollbarScrollTo,
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

impl Message {
    /// Payload of this variant as `&dyn Any`.
    ///
    /// Migration shim — deleted at Step 18 when the enum is removed.
    pub(crate) fn payload_any(&self) -> &dyn std::any::Any {
        match self {
            Message::ClearRequested(m) => m,
            Message::HeaderIconPressed(m) => m,
            Message::HelpPanelFocusedHelpCleared(m) => m,
            Message::CommandPaletteOpened(m) => m,
            Message::CommandPaletteClosed(m) => m,
            Message::SelectionListSelectedChanged(m) => m,
            Message::ToastDismissed(m) => m,
            Message::NavigatorUpdated(m) => m,
            Message::TabsCleared(m) => m,
            Message::InputChanged(m) => m,
            Message::InputSubmitted(m) => m,
            Message::InputBlurred(m) => m,
            Message::TextAreaChanged(m) => m,
            Message::TextAreaSelectionChanged(m) => m,
            Message::TextEditClipboardCopyRequested(m) => m,
            Message::TextEditClipboardPasteRequested(m) => m,
            Message::TextEditClipboardPaste(m) => m,
            Message::ButtonPressed(m) => m,
            Message::CheckboxChanged(m) => m,
            Message::SwitchChanged(m) => m,
            Message::RadioButtonChanged(m) => m,
            Message::RadioSetChanged(m) => m,
            Message::ListViewSelectionChanged(m) => m,
            Message::ListViewItemActivated(m) => m,
            Message::OptionHighlighted(m) => m,
            Message::OptionSelected(m) => m,
            Message::SelectChanged(m) => m,
            Message::SelectionListToggled(m) => m,
            Message::TabActivated(m) => m,
            Message::TabClicked(m) => m,
            Message::TabDisabled(m) => m,
            Message::TabEnabled(m) => m,
            Message::TabHidden(m) => m,
            Message::TabShown(m) => m,
            Message::TabPaneFocused(m) => m,
            Message::HeaderToggled(m) => m,
            Message::FooterBindingsUpdated(m) => m,
            Message::ScreenTitleChanged(m) => m,
            Message::HelpPanelSetHelp(m) => m,
            Message::HelpPanelClearHelp(m) => m,
            Message::HelpPanelFocusedHelpChanged(m) => m,
            Message::TreeNodeSelected(m) => m,
            Message::TreeNodeActivated(m) => m,
            Message::TreeNodeToggled(m) => m,
            Message::TreeNodeCollapsed(m) => m,
            Message::TreeNodeExpanded(m) => m,
            Message::TreeNodeHighlighted(m) => m,
            Message::DirectoryTreeFileSelected(m) => m,
            Message::DirectoryTreeDirectorySelected(m) => m,
            Message::MarkdownTableOfContentsSelected(m) => m,
            Message::MarkdownTableOfContentsUpdated(m) => m,
            Message::OverlaySetVisible(m) => m,
            Message::OverlaySetAnchor(m) => m,
            Message::OverlayClearAnchor(m) => m,
            Message::OverlayToggle(m) => m,
            Message::OverlayDismissRequested(m) => m,
            Message::OverlayVisibilityChanged(m) => m,
            Message::AppBack(m) => m,
            Message::AppBell(m) => m,
            Message::AppChangeTheme(m) => m,
            Message::AppCommandPalette(m) => m,
            Message::AppFocus(m) => m,
            Message::AppFocusNext(m) => m,
            Message::AppFocusPrevious(m) => m,
            Message::AppHelpQuit(m) => m,
            Message::AppCopySelectedText(m) => m,
            Message::AppHideHelpPanel(m) => m,
            Message::AppAddClass(m) => m,
            Message::AppRemoveClass(m) => m,
            Message::AppToggleClass(m) => m,
            Message::AppSetDisabled(m) => m,
            Message::AppNotify(m) => m,
            Message::AppPopScreen(m) => m,
            Message::AppPushScreen(m) => m,
            Message::AppScreenshot(m) => m,
            Message::AppShowHelpPanel(m) => m,
            Message::AppSimulateKey(m) => m,
            Message::AppSuspendProcess(m) => m,
            Message::AppSwitchMode(m) => m,
            Message::AppSwitchScreen(m) => m,
            Message::AppToggleDark(m) => m,
            Message::ActionDispatchRequested(m) => m,
            Message::CommandPaletteCommandSelected(m) => m,
            Message::CommandPaletteSetCommands(m) => m,
            Message::ScrollbarScrollTo(m) => m,
            Message::DataTableCursorMoved(m) => m,
            Message::DataTableHeaderSelected(m) => m,
            Message::DataTableCellActivated(m) => m,
            Message::DataTableCellHighlighted(m) => m,
            Message::DataTableRowHighlighted(m) => m,
            Message::DataTableRowSelected(m) => m,
            Message::DataTableColumnHighlighted(m) => m,
            Message::DataTableColumnSelected(m) => m,
            Message::PlaceholderVariantChanged(m) => m,
            Message::CollapsibleToggled(m) => m,
            Message::LinkClicked(m) => m,
            Message::KeyPanelBindingsUpdated(m) => m,
            Message::KeyPanelScrolled(m) => m,
            Message::RichLogScrolled(m) => m,
            Message::AsyncTaskSpawn(m) => m,
            Message::AsyncTaskCancel(m) => m,
            Message::AsyncTaskCancelTarget(m) => m,
            Message::AsyncTaskCompleted(m) => m,
            Message::AsyncTaskCancelled(m) => m,
            Message::TimerSchedule(m) => m,
            Message::TimerCancel(m) => m,
            Message::TimerFired(m) => m,
            Message::TimerCancelled(m) => m,
            Message::WorkerStateChanged(m) => m,
            Message::Custom(b) => b.as_any(),
        }
    }

    /// Payload as the `Msg` trait object.
    ///
    /// Migration shim — deleted at Step 18 when the enum is removed.
    pub(crate) fn payload_msg(&self) -> &dyn Msg {
        match self {
            Message::ClearRequested(m) => m,
            Message::HeaderIconPressed(m) => m,
            Message::HelpPanelFocusedHelpCleared(m) => m,
            Message::CommandPaletteOpened(m) => m,
            Message::CommandPaletteClosed(m) => m,
            Message::SelectionListSelectedChanged(m) => m,
            Message::ToastDismissed(m) => m,
            Message::NavigatorUpdated(m) => m,
            Message::TabsCleared(m) => m,
            Message::InputChanged(m) => m,
            Message::InputSubmitted(m) => m,
            Message::InputBlurred(m) => m,
            Message::TextAreaChanged(m) => m,
            Message::TextAreaSelectionChanged(m) => m,
            Message::TextEditClipboardCopyRequested(m) => m,
            Message::TextEditClipboardPasteRequested(m) => m,
            Message::TextEditClipboardPaste(m) => m,
            Message::ButtonPressed(m) => m,
            Message::CheckboxChanged(m) => m,
            Message::SwitchChanged(m) => m,
            Message::RadioButtonChanged(m) => m,
            Message::RadioSetChanged(m) => m,
            Message::ListViewSelectionChanged(m) => m,
            Message::ListViewItemActivated(m) => m,
            Message::OptionHighlighted(m) => m,
            Message::OptionSelected(m) => m,
            Message::SelectChanged(m) => m,
            Message::SelectionListToggled(m) => m,
            Message::TabActivated(m) => m,
            Message::TabClicked(m) => m,
            Message::TabDisabled(m) => m,
            Message::TabEnabled(m) => m,
            Message::TabHidden(m) => m,
            Message::TabShown(m) => m,
            Message::TabPaneFocused(m) => m,
            Message::HeaderToggled(m) => m,
            Message::FooterBindingsUpdated(m) => m,
            Message::ScreenTitleChanged(m) => m,
            Message::HelpPanelSetHelp(m) => m,
            Message::HelpPanelClearHelp(m) => m,
            Message::HelpPanelFocusedHelpChanged(m) => m,
            Message::TreeNodeSelected(m) => m,
            Message::TreeNodeActivated(m) => m,
            Message::TreeNodeToggled(m) => m,
            Message::TreeNodeCollapsed(m) => m,
            Message::TreeNodeExpanded(m) => m,
            Message::TreeNodeHighlighted(m) => m,
            Message::DirectoryTreeFileSelected(m) => m,
            Message::DirectoryTreeDirectorySelected(m) => m,
            Message::MarkdownTableOfContentsSelected(m) => m,
            Message::MarkdownTableOfContentsUpdated(m) => m,
            Message::OverlaySetVisible(m) => m,
            Message::OverlaySetAnchor(m) => m,
            Message::OverlayClearAnchor(m) => m,
            Message::OverlayToggle(m) => m,
            Message::OverlayDismissRequested(m) => m,
            Message::OverlayVisibilityChanged(m) => m,
            Message::AppBack(m) => m,
            Message::AppBell(m) => m,
            Message::AppChangeTheme(m) => m,
            Message::AppCommandPalette(m) => m,
            Message::AppFocus(m) => m,
            Message::AppFocusNext(m) => m,
            Message::AppFocusPrevious(m) => m,
            Message::AppHelpQuit(m) => m,
            Message::AppCopySelectedText(m) => m,
            Message::AppHideHelpPanel(m) => m,
            Message::AppAddClass(m) => m,
            Message::AppRemoveClass(m) => m,
            Message::AppToggleClass(m) => m,
            Message::AppSetDisabled(m) => m,
            Message::AppNotify(m) => m,
            Message::AppPopScreen(m) => m,
            Message::AppPushScreen(m) => m,
            Message::AppScreenshot(m) => m,
            Message::AppShowHelpPanel(m) => m,
            Message::AppSimulateKey(m) => m,
            Message::AppSuspendProcess(m) => m,
            Message::AppSwitchMode(m) => m,
            Message::AppSwitchScreen(m) => m,
            Message::AppToggleDark(m) => m,
            Message::ActionDispatchRequested(m) => m,
            Message::CommandPaletteCommandSelected(m) => m,
            Message::CommandPaletteSetCommands(m) => m,
            Message::ScrollbarScrollTo(m) => m,
            Message::DataTableCursorMoved(m) => m,
            Message::DataTableHeaderSelected(m) => m,
            Message::DataTableCellActivated(m) => m,
            Message::DataTableCellHighlighted(m) => m,
            Message::DataTableRowHighlighted(m) => m,
            Message::DataTableRowSelected(m) => m,
            Message::DataTableColumnHighlighted(m) => m,
            Message::DataTableColumnSelected(m) => m,
            Message::PlaceholderVariantChanged(m) => m,
            Message::CollapsibleToggled(m) => m,
            Message::LinkClicked(m) => m,
            Message::KeyPanelBindingsUpdated(m) => m,
            Message::KeyPanelScrolled(m) => m,
            Message::RichLogScrolled(m) => m,
            Message::AsyncTaskSpawn(m) => m,
            Message::AsyncTaskCancel(m) => m,
            Message::AsyncTaskCancelTarget(m) => m,
            Message::AsyncTaskCompleted(m) => m,
            Message::AsyncTaskCancelled(m) => m,
            Message::TimerSchedule(m) => m,
            Message::TimerCancel(m) => m,
            Message::TimerFired(m) => m,
            Message::TimerCancelled(m) => m,
            Message::WorkerStateChanged(m) => m,
            Message::Custom(b) => b.as_ref(),
        }
    }

    /// Whether this (newer) message can replace the provided older pending message.
    ///
    /// Delegates to the `Msg` trait implementations (single source of truth).
    pub fn can_replace(&self, pending: &Message) -> bool {
        self.payload_msg().can_replace(pending.payload_msg())
    }
}

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

impl MessageEvent {
    /// Construct a `MessageEvent` from any type that converts `Into<Message>`.
    ///
    /// Migration form: the bound will change to `M: Msg` at Step 18.
    pub fn new<M: Into<Message>>(sender: NodeId, message: M) -> Self {
        Self {
            sender,
            message: message.into(),
            control: None,
        }
    }

    /// Builder: set the control node.
    pub fn with_control(mut self, control: NodeId) -> Self {
        self.control = Some(control);
        self
    }

    /// Downcast the payload to a concrete type `T`.
    ///
    /// Returns `Some(&T)` if the payload is of type `T`, `None` otherwise.
    pub fn downcast_ref<T: Msg>(&self) -> Option<&T> {
        self.message.payload_any().downcast_ref::<T>()
    }

    /// Returns `true` if the payload is of type `T`.
    pub fn is<T: Msg>(&self) -> bool {
        self.downcast_ref::<T>().is_some()
    }

    /// The `TypeId` of the concrete payload type.
    pub fn payload_type_id(&self) -> std::any::TypeId {
        self.message.payload_any().type_id()
    }
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

    /// Downcast the envelope's payload to a concrete type `T`.
    pub fn downcast_ref<T: Msg>(&self) -> Option<&T> {
        self.event.downcast_ref::<T>()
    }

    /// Returns `true` if the envelope's payload is of type `T`.
    pub fn is<T: Msg>(&self) -> bool {
        self.event.is::<T>()
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
                button_id: None,
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
            Message::ButtonPressed(ButtonPressed { description, .. }) => {
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
            button_id: None,
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
                button_id: None,
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

    // --- Message::can_replace ---

    #[test]
    fn message_can_replace_known_variants() {
        let a = Message::InputChanged(InputChanged {
            value: "a".into(),
            validation: ValidationResult::success(),
        });
        let b = Message::InputChanged(InputChanged {
            value: "ab".into(),
            validation: ValidationResult::success(),
        });
        assert!(b.can_replace(&a));
        assert!(!Message::ButtonPressed(ButtonPressed {
            description: "x".into(),
            button_id: None,
        })
        .can_replace(&Message::ButtonPressed(ButtonPressed {
            description: "y".into(),
            button_id: None,
        })));
    }

    #[derive(Debug, Clone)]
    struct ReplaceableCustom {
        key: u8,
    }

    impl Msg for ReplaceableCustom {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn clone_box(&self) -> Box<dyn Msg> {
            Box::new(self.clone())
        }

        fn can_replace(&self, pending: &dyn Msg) -> bool {
            pending
                .as_any()
                .downcast_ref::<ReplaceableCustom>()
                .map(|older| older.key == self.key)
                .unwrap_or(false)
        }
    }

    #[test]
    fn custom_message_can_replace_uses_user_hook() {
        let older = Message::Custom(Box::new(ReplaceableCustom { key: 7 }));
        let newer_same_key = Message::Custom(Box::new(ReplaceableCustom { key: 7 }));
        let newer_other_key = Message::Custom(Box::new(ReplaceableCustom { key: 9 }));
        assert!(newer_same_key.can_replace(&older));
        assert!(!newer_other_key.can_replace(&older));
    }
}
