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

/// Posted when a toast should be dismissed — either its auto-dismiss timer
/// (owned by the persistent `ToastRack` node) elapsed, or the user clicked it.
///
/// The runtime intercepts this in `split_runtime_control_messages` (like
/// [`OverlayVisibilityChanged`]) and removes the identified notification from
/// the app's notification store, which re-syncs the rack (a real node unmount).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationExpired {
    /// Stable id of the notification (assigned by `App::notify`).
    pub id: u64,
}
crate::impl_message!(NotificationExpired);

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
    /// The pressed button's **label text** (Python parity: `event.button.label`),
    /// so a handler can `match` on the human-readable label. (Widgets that
    /// re-emit `ButtonPressed` may set this to a routing string of their own,
    /// e.g. `Welcome` emits `"Welcome.close"`.) For a debug repr of the button
    /// use `Button::describe()`.
    pub description: String,
    /// CSS id of the button that was pressed, if the button has one.
    ///
    /// Mirrors Python's `Button.Pressed.button.id`. Prefer this over
    /// `description` when several buttons share a label.
    pub button_id: Option<String>,
}

// Manual `Message` impl (instead of `impl_message!`) so `ButtonPressed` can
// expose its control identity for `@on(Button.Pressed, "#id")`-style routing,
// mirroring Python's `Button.Pressed.control` / `.button.id`.
impl Message for ButtonPressed {
    fn as_any(&self) -> &dyn ::std::any::Any {
        self
    }
    fn clone_box(&self) -> ::std::boxed::Box<dyn Message> {
        ::std::boxed::Box::new(::std::clone::Clone::clone(self))
    }
    fn control_meta(&self) -> Option<crate::routing::ControlMeta> {
        let mut meta = crate::routing::ControlMeta::default().type_named("Button");
        if let Some(id) = &self.button_id {
            meta.id = Some(id.clone());
        }
        Some(meta)
    }
}

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
    /// The ordinal of the emitting `RadioButton` within its parent `RadioSet`
    /// (set at compose time). Lets the set route the change to the right button
    /// without owning the child's `NodeId` (mirrors `ListItemChildClicked`).
    pub ordinal: usize,
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

/// Posted by a `ListItem` when it is clicked, informing the owning `ListView`
/// (Python: `ListItem._ChildClicked`). The `ordinal` is the item's position in
/// the list, assigned by `ListView` at compose time.
#[derive(Debug, Clone)]
pub struct ListItemChildClicked {
    pub ordinal: usize,
    pub item: String,
}
crate::impl_message!(ListItemChildClicked);

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

/// Posted by a `SelectCurrent` (the closed-state bar) when clicked, asking its
/// ancestor `Select` to toggle the overlay. Mirrors Python `SelectCurrent.Toggle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectCurrentToggle;
crate::impl_message!(SelectCurrentToggle);

/// Posted by a `SelectOverlay` to ask its ancestor `Select` to dismiss the
/// overlay. Mirrors Python `SelectOverlay.Dismiss`. `lost_focus` is `true` when
/// the overlay dismissed because it lost focus (a click outside), in which case
/// the `Select` does NOT re-focus itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectOverlayDismiss {
    pub lost_focus: bool,
}
crate::impl_message!(SelectOverlayDismiss);

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

/// Advance to the next theme in the app's configured theme cycle
/// (Python `action_cycle_theme` over `cycle([...])`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppCycleTheme;
crate::impl_message!(AppCycleTheme);

/// Activate a named theme directly (Python `App.theme = name`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSetTheme {
    pub name: String,
}
crate::impl_message!(AppSetTheme);

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
// Open message trait (final name: Message)
// ---------------------------------------------------------------------------

/// Canonical open message trait. Every message — built-in or third-party —
/// is a plain struct implementing this trait. Dispatch is by `TypeId`.
///
/// Use [`impl_message!`] to implement this trait for your types.
pub trait Message: std::any::Any + Send + Sync + std::fmt::Debug + 'static {
    /// Downcast support.
    fn as_any(&self) -> &dyn std::any::Any;
    /// Clone into a boxed trait object.
    fn clone_box(&self) -> Box<dyn Message>;
    /// Whether this (newer) message can replace an older `pending` message in
    /// the queue. Called by the coalescer with the newer message as `self`.
    /// Mirrors Python Textual's `Message.can_replace`. Default: `false`.
    fn can_replace(&self, _pending: &dyn Message) -> bool {
        false
    }

    /// Identity (`#id`, `.class`, `Type`) of the widget this message originated
    /// from — its "control", in Python terms.
    ///
    /// Used by [`crate::routing::MessageRouter`] (the `@on(Message, selector)`
    /// analogue) to filter handlers by CSS selector, exactly as Python matches a
    /// selector against `message.control`. Messages that don't identify a
    /// control return `None` (the default), in which case only selector-less
    /// (`@on(Message)`) handlers run for them.
    fn control_meta(&self) -> Option<crate::routing::ControlMeta> {
        None
    }
}

impl Clone for Box<dyn Message> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

// ---------------------------------------------------------------------------
// impl_message! macro — the only way to implement the Message trait
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
        impl $crate::message::Message for $T {
            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }
            fn clone_box(&self) -> ::std::boxed::Box<dyn $crate::message::Message> {
                ::std::boxed::Box::new(::std::clone::Clone::clone(self))
            }
        }
    };
    ($T:ty, replaceable) => {
        impl $crate::message::Message for $T {
            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }
            fn clone_box(&self) -> ::std::boxed::Box<dyn $crate::message::Message> {
                ::std::boxed::Box::new(::std::clone::Clone::clone(self))
            }
            fn can_replace(&self, pending: &dyn $crate::message::Message) -> bool {
                pending.as_any().is::<$T>()
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Prevented message types (ambient scope)
// ---------------------------------------------------------------------------

thread_local! {
    /// The stack of active `prevent(...)` scopes on this (UI) thread.
    ///
    /// Mirrors Python's `prevent_message_types_stack` `ContextVar`
    /// (`message_pump.py`): the active prevented set is the union of every
    /// frame on the stack. It is ambient — not stored on any dispatch context —
    /// so a prevent scope opened in an event/message handler also covers
    /// synchronous work started inside it (e.g. a `Handle::update` reactive
    /// mutation), exactly like Python's context-managed set covers a reactive
    /// `_check_watchers` cascade.
    static PREVENTED_MESSAGE_TYPES: std::cell::RefCell<Vec<Vec<std::any::TypeId>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Whether a concrete message type id is suppressed by an active
/// `prevent(...)` scope on this thread. Mirrors Python
/// `MessagePump._is_prevented` (checked at post time by
/// `Widget.check_message_enabled`).
pub(crate) fn is_message_type_prevented(type_id: std::any::TypeId) -> bool {
    PREVENTED_MESSAGE_TYPES.with(|stack| {
        stack
            .borrow()
            .iter()
            .any(|frame| frame.contains(&type_id))
    })
}

/// Snapshot the union of all active prevent frames (deduplicated). Mirrors
/// Python `MessagePump._get_prevented_messages`; the snapshot is stamped onto
/// every posted [`MessageEvent`] so prevention is honoured across dispatch
/// cycles (`message_pump.py`: `message._prevent.update(...)`).
pub(crate) fn active_prevented_message_types() -> Vec<std::any::TypeId> {
    PREVENTED_MESSAGE_TYPES.with(|stack| {
        let stack = stack.borrow();
        let mut union: Vec<std::any::TypeId> = Vec::new();
        for frame in stack.iter() {
            for type_id in frame {
                if !union.contains(type_id) {
                    union.push(*type_id);
                }
            }
        }
        union
    })
}

/// RAII guard for one prevent frame. Dropping pops the frame, so scopes nest
/// and unwind correctly (Python's `with self.prevent(...)`).
pub(crate) struct PreventScope {
    pushed: bool,
}

/// Push `frame` as an active prevent scope for the guard's lifetime.
/// An empty frame is a no-op (nothing pushed), keeping the hot path free.
pub(crate) fn enter_prevent_scope(frame: &[std::any::TypeId]) -> PreventScope {
    if frame.is_empty() {
        return PreventScope { pushed: false };
    }
    PREVENTED_MESSAGE_TYPES.with(|stack| stack.borrow_mut().push(frame.to_vec()));
    PreventScope { pushed: true }
}

impl Drop for PreventScope {
    fn drop(&mut self) {
        if self.pushed {
            PREVENTED_MESSAGE_TYPES.with(|stack| {
                stack.borrow_mut().pop();
            });
        }
    }
}

// ---------------------------------------------------------------------------
// MessageEvent / MessageEnvelope
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MessageEvent {
    pub sender: NodeId,
    message: Box<dyn Message>,
    /// The originating widget ("control") — defaults to sender.
    /// Handlers can use this to identify which widget produced the message,
    /// even when the message has bubbled through containers.
    pub control: Option<NodeId>,
    /// Prevent-set snapshot riding on this message (Python `Message._prevent`,
    /// stamped by `MessagePump.post_message`): the message types that were
    /// prevented when this message was posted. The dispatcher re-activates the
    /// snapshot for the duration of this message's dispatch
    /// (`message_pump.py`: `with self.prevent(*message._prevent):`), so a
    /// prevented type stays prevented across dispatch cycles it triggers.
    prevent: Vec<std::any::TypeId>,
}

impl MessageEvent {
    /// Construct a `MessageEvent` from any type implementing [`Message`].
    pub fn new<M: Message>(sender: NodeId, message: M) -> Self {
        Self {
            sender,
            message: Box::new(message),
            control: None,
            prevent: active_prevented_message_types(),
        }
    }

    /// Construct a `MessageEvent` from a pre-boxed [`Message`] trait object.
    pub fn from_boxed(sender: NodeId, message: Box<dyn Message>) -> Self {
        Self {
            sender,
            message,
            control: None,
            prevent: active_prevented_message_types(),
        }
    }

    /// The prevent-set snapshot this message carries (Python
    /// `Message._prevent`). Re-activated by the dispatcher around this
    /// message's handlers.
    pub(crate) fn prevent_snapshot(&self) -> &[std::any::TypeId] {
        &self.prevent
    }

    /// Builder: set the control node.
    pub fn with_control(mut self, control: NodeId) -> Self {
        self.control = Some(control);
        self
    }

    /// A reference to the message payload as a [`Message`] trait object.
    pub fn payload(&self) -> &dyn Message {
        self.message.as_ref()
    }

    /// Downcast the payload to a concrete type `T`.
    ///
    /// Returns `Some(&T)` if the payload is of type `T`, `None` otherwise.
    pub fn downcast_ref<T: Message>(&self) -> Option<&T> {
        self.payload().as_any().downcast_ref::<T>()
    }

    /// Returns `true` if the payload is of type `T`.
    pub fn is<T: Message>(&self) -> bool {
        self.downcast_ref::<T>().is_some()
    }

    /// The `TypeId` of the concrete payload type.
    pub fn payload_type_id(&self) -> std::any::TypeId {
        self.payload().as_any().type_id()
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

    /// A reference to the message payload.
    pub fn message(&self) -> &dyn Message {
        self.event.payload()
    }

    /// Downcast the envelope's payload to a concrete type `T`.
    pub fn downcast_ref<T: Message>(&self) -> Option<&T> {
        self.event.downcast_ref::<T>()
    }

    /// Returns `true` if the envelope's payload is of type `T`.
    pub fn is<T: Message>(&self) -> bool {
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

    /// Helper: build a simple `MessageEvent` for testing (uses trait form).
    fn test_event() -> MessageEvent {
        MessageEvent::new(
            node_id_from_ffi(1),
            ButtonPressed {
                description: "ok".into(),
                button_id: None,
            },
        )
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
    fn message_returns_button_pressed_payload() {
        let env = MessageEnvelope::new(test_event());
        let bp = env
            .downcast_ref::<ButtonPressed>()
            .expect("expected ButtonPressed");
        assert_eq!(bp.description, "ok");
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
        assert!(env.is::<ButtonPressed>());
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
        let evt = MessageEvent::new(
            node_id_from_ffi(1),
            ButtonPressed {
                description: "ctrl".into(),
                button_id: None,
            },
        )
        .with_control(other);
        let env = MessageEnvelope::new(evt);
        // Envelope should use the explicit control, not sender.
        assert_eq!(env.control(), Some(other));
    }

    #[test]
    fn control_none_is_allowed() {
        let evt = MessageEvent::new(node_id_from_ffi(1), ClearRequested);
        // None is allowed on the event; envelope promotes it.
        assert!(evt.control.is_none());
        let env = MessageEnvelope::new(evt);
        assert_eq!(env.control(), Some(node_id_from_ffi(1)));
    }

    // --- Message trait can_replace ---

    #[test]
    fn replaceable_trait_impl_returns_true_for_same_type() {
        let a = InputChanged {
            value: "a".into(),
            validation: ValidationResult::success(),
        };
        let b = InputChanged {
            value: "ab".into(),
            validation: ValidationResult::success(),
        };
        // b can replace a because InputChanged is impl_message!(_, replaceable)
        assert!(b.can_replace(&a));
    }

    #[test]
    fn non_replaceable_trait_impl_returns_false() {
        let x = ButtonPressed {
            description: "x".into(),
            button_id: None,
        };
        let y = ButtonPressed {
            description: "y".into(),
            button_id: None,
        };
        // ButtonPressed is plain impl_message! — can_replace defaults to false
        assert!(!x.can_replace(&y));
    }

    #[test]
    fn can_replace_false_across_different_types() {
        let a = InputChanged {
            value: "a".into(),
            validation: ValidationResult::success(),
        };
        let b = ButtonPressed {
            description: "x".into(),
            button_id: None,
        };
        // Different types — never replace each other
        assert!(!b.can_replace(&a));
        assert!(!a.can_replace(&b));
    }

    // --- downcast_ref / is / payload_type_id ---

    #[test]
    fn downcast_ref_hits_correct_type() {
        let evt = test_event();
        let bp = evt.downcast_ref::<ButtonPressed>().unwrap();
        assert_eq!(bp.description, "ok");
    }

    #[test]
    fn downcast_ref_misses_wrong_type() {
        let evt = test_event();
        assert!(evt.downcast_ref::<ClearRequested>().is_none());
    }

    #[test]
    fn is_returns_true_for_correct_type() {
        let evt = test_event();
        assert!(evt.is::<ButtonPressed>());
    }

    #[test]
    fn is_returns_false_for_wrong_type() {
        let evt = test_event();
        assert!(!evt.is::<ClearRequested>());
    }

    #[test]
    fn payload_type_id_distinguishes_two_zero_sized_types() {
        let e1 = MessageEvent::new(node_id_from_ffi(1), ClearRequested);
        let e2 = MessageEvent::new(node_id_from_ffi(1), HeaderIconPressed);
        assert_ne!(e1.payload_type_id(), e2.payload_type_id());
    }

    #[test]
    fn box_dyn_msg_clone_preserves_payload() {
        let original: Box<dyn Message> = Box::new(ButtonPressed {
            description: "clone-me".into(),
            button_id: None,
        });
        let cloned = original.clone();
        let bp = cloned.as_any().downcast_ref::<ButtonPressed>().unwrap();
        assert_eq!(bp.description, "clone-me");
    }

    // --- Custom Message impl (key-based replacement) ---

    #[derive(Debug, Clone)]
    struct ReplaceableCustom {
        key: u8,
    }

    impl Message for ReplaceableCustom {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn clone_box(&self) -> Box<dyn Message> {
            Box::new(self.clone())
        }

        fn can_replace(&self, pending: &dyn Message) -> bool {
            pending
                .as_any()
                .downcast_ref::<ReplaceableCustom>()
                .map(|older| older.key == self.key)
                .unwrap_or(false)
        }
    }

    #[derive(Debug, Clone)]
    struct OtherCustom;
    crate::impl_message!(OtherCustom);

    #[test]
    fn custom_message_can_replace_uses_user_hook() {
        let older = ReplaceableCustom { key: 7 };
        let newer_same_key = ReplaceableCustom { key: 7 };
        let newer_other_key = ReplaceableCustom { key: 9 };
        assert!(newer_same_key.can_replace(&older));
        assert!(!newer_other_key.can_replace(&older));
    }

    #[test]
    fn two_different_custom_types_do_not_coalesce() {
        // TypeId refinement: two distinct custom types must not coalesce each other.
        let a = ReplaceableCustom { key: 1 };
        let b = OtherCustom;
        // a.can_replace checks downcast_ref::<ReplaceableCustom> on b — fails.
        assert!(!a.can_replace(&b));
        // b.can_replace (default false) also returns false.
        assert!(!b.can_replace(&a));
    }
}
