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
        let control = Some(event.sender);
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
            message: Message::ButtonPressed {
                description: "ok".into(),
            },
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
            Message::ButtonPressed { description } => {
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
        assert!(matches!(env.message(), Message::ButtonPressed { .. }));
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
}
