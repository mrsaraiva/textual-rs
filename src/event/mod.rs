use crate::debug::debug_message;
use crate::keys::KeyEventData;
use crate::keys::format_key_display;
use crate::message::{AsyncTaskRequest, CommandPaletteCommand, Message, MessageEvent};
use crate::node_id::{NodeId, node_id_to_ffi};
use crate::style::{Color, Scalar, Spacing, Tint};
use crate::worker::{CancellationToken, WorkerRequest, WorkerRequestPayload};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseDownEvent {
    pub target: NodeId,
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates (origin at widget content top-left).
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseUpEvent {
    pub target: Option<NodeId>,
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates (origin at widget content top-left of `target`, if any).
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseMoveEvent {
    pub target: NodeId,
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates (origin at widget content top-left).
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseScrollEvent {
    pub target: Option<NodeId>,
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates (origin at widget content top-left of `target`, if any).
    pub x: u16,
    pub y: u16,
    pub delta_x: i32,
    pub delta_y: i32,
    pub modifiers: KeyModifiers,
}

/// Fired when the pointer enters a widget's region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEnterEvent {
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates.
    pub x: u16,
    pub y: u16,
}

/// Fired when the pointer leaves a widget's region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseLeaveEvent {
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates.
    pub x: u16,
    pub y: u16,
}

/// Fired when a mousedown+mouseup pair hits the same widget (synthesised click).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickEvent {
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates.
    pub x: u16,
    pub y: u16,
    /// 0=left, 1=middle, 2=right.
    pub button: u8,
}

/// Fired when the terminal delivers a bracketed-paste payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteEvent {
    pub text: String,
}

/// Fired when a widget is mounted into the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountEvent {
    pub node: NodeId,
}

/// Fired when a widget is unmounted from the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnmountEvent {
    pub node: NodeId,
}

/// Fired once after the first successful render frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyEvent;

/// Fired when a widget gains focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusEvent {
    pub node: NodeId,
}

/// Fired when a widget loses focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlurEvent {
    pub node: NodeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationLevel {
    None,
    Basic,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationEase {
    None,
    Round,
    Linear,
    InOutCubic,
    OutCubic,
    // Quad
    InQuad,
    OutQuad,
    InOutQuad,
    // Cubic (In only — Out and InOut already exist above)
    InCubic,
    // Quart
    InQuart,
    OutQuart,
    InOutQuart,
    // Quint
    InQuint,
    OutQuint,
    InOutQuint,
    // Expo
    InExpo,
    OutExpo,
    InOutExpo,
    // Circ
    InCirc,
    OutCirc,
    InOutCirc,
    // Back (overshoot)
    InBack,
    OutBack,
    InOutBack,
    // Bounce
    InBounce,
    OutBounce,
    InOutBounce,
    // Elastic
    InElastic,
    OutElastic,
    InOutElastic,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationRequest {
    pub target: NodeId,
    pub attribute: String,
    pub start: f32,
    pub end: f32,
    pub duration: Duration,
    pub delay: Duration,
    pub ease: AnimationEase,
    pub level: AnimationLevel,
}

impl AnimationRequest {
    pub fn new(
        target: NodeId,
        attribute: impl Into<String>,
        start: f32,
        end: f32,
        duration: Duration,
    ) -> Self {
        Self {
            target,
            attribute: attribute.into(),
            start,
            end,
            duration,
            delay: Duration::ZERO,
            ease: AnimationEase::InOutCubic,
            level: AnimationLevel::Full,
        }
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn with_ease(mut self, ease: AnimationEase) -> Self {
        self.ease = ease;
        self
    }

    pub fn with_level(mut self, level: AnimationLevel) -> Self {
        self.level = level;
        self
    }
}

/// Represents a typed value for CSS property animation.
#[derive(Debug, Clone, PartialEq)]
pub enum StyleValue {
    /// RGBA color value (for `fg`, `bg`).
    Color(Color),
    /// Float value (for `opacity`, `text_opacity` — 0.0–100.0 range).
    Float(f32),
    /// Scalar dimension (for `width`, `height`, `min_width`, etc.).
    Scalar(Scalar),
    /// Four-side spacing (for `margin`, `padding`).
    Spacing(Spacing),
    /// Tint value (for `tint`, `background_tint`).
    Tint(Tint),
}

/// Request to animate a CSS property to a target value on a specific node.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleAnimationRequest {
    pub target: NodeId,
    pub property: String,
    pub from: StyleValue,
    pub to: StyleValue,
    pub duration: Duration,
    pub delay: Duration,
    pub ease: AnimationEase,
    pub level: AnimationLevel,
}

impl StyleAnimationRequest {
    pub fn new(
        target: NodeId,
        property: impl Into<String>,
        from: StyleValue,
        to: StyleValue,
        duration: Duration,
    ) -> Self {
        Self {
            target,
            property: property.into(),
            from,
            to,
            duration,
            delay: Duration::ZERO,
            ease: AnimationEase::InOutCubic,
            level: AnimationLevel::Full,
        }
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn with_ease(mut self, ease: AnimationEase) -> Self {
        self.ease = ease;
        self
    }

    pub fn with_level(mut self, level: AnimationLevel) -> Self {
        self.level = level;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationValueEvent {
    pub target: NodeId,
    pub attribute: String,
    pub value: f32,
    pub done: bool,
}

#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEventData),
    Action(Action),
    BindingsChanged(Vec<BindingHint>),
    MouseDown(MouseDownEvent),
    MouseUp(MouseUpEvent),
    MouseMove(MouseMoveEvent),
    MouseScroll(MouseScrollEvent),
    Enter(MouseEnterEvent),
    Leave(MouseLeaveEvent),
    Click(ClickEvent),
    Paste(PasteEvent),
    Mount(MountEvent),
    Unmount(UnmountEvent),
    Ready(ReadyEvent),
    Focus(FocusEvent),
    Blur(BlurEvent),
    AnimationValue(AnimationValueEvent),
    AppFocus(bool),
    Tick(u64),
    Resize(u16, u16),
    /// Sent to the widget tree of a screen when it is no longer the active screen
    /// (another screen has been pushed on top).
    ScreenSuspend,
    /// Sent to the widget tree of a screen when it becomes the active screen again
    /// (the screen above was popped).
    ScreenResume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    FocusNext,
    FocusPrev,
    HelpQuit,
    CopySelectedText,
    ScrollHome,
    ScrollEnd,
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
    ScrollLeft,
    ScrollRight,
    ScrollPageLeft,
    ScrollPageRight,
    Toggle,
    CommandPalette,
}

impl Action {
    pub fn description(self) -> &'static str {
        match self {
            Action::FocusNext => "Focus next",
            Action::FocusPrev => "Focus previous",
            Action::HelpQuit => "Show quit help",
            Action::CopySelectedText => "Copy selected text",
            Action::ScrollHome => "Scroll home",
            Action::ScrollEnd => "Scroll end",
            Action::ScrollUp => "Scroll up",
            Action::ScrollDown => "Scroll down",
            Action::ScrollPageUp => "Page up",
            Action::ScrollPageDown => "Page down",
            Action::ScrollLeft => "Scroll left",
            Action::ScrollRight => "Scroll right",
            Action::ScrollPageLeft => "Page left",
            Action::ScrollPageRight => "Page right",
            Action::Toggle => "Toggle",
            Action::CommandPalette => "Command palette",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyBind {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindingHint {
    pub key: String,
    pub description: String,
    pub tooltip: Option<String>,
    pub namespace: Option<String>,
    pub show: bool,
    pub key_display: Option<String>,
    pub group: Option<String>,
    pub priority: bool,
    pub system: bool,
    /// Action name from the binding declaration (e.g. `"back"`, `"forward"`).
    /// Used by `check_action` to determine enabled/disabled state.
    pub action: Option<String>,
    /// Parsed action name passed to `check_action`.
    ///
    /// For `BindingDecl::action = "app.push_screen('settings')"`, this stores
    /// `"push_screen"`.
    pub action_name: Option<String>,
    /// Parsed positional parameters passed to `check_action`.
    ///
    /// For `BindingDecl::action = "app.push_screen('settings')"`, this stores
    /// `["settings"]`.
    pub action_parameters: Vec<String>,
    /// Result of `check_action` for this binding:
    /// - `Some(true)` — enabled (default, rendered normally)
    /// - `Some(false)` — hidden (not shown in footer)
    /// - `None` — disabled but visible (rendered dimmed in footer)
    pub enabled: Option<bool>,
}

impl BindingHint {
    pub fn new(key: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
            tooltip: None,
            namespace: None,
            show: true,
            key_display: None,
            group: None,
            priority: false,
            system: false,
            action: None,
            action_name: None,
            action_parameters: Vec::new(),
            enabled: Some(true),
        }
    }

    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        let action = action.into();
        self.action = Some(action.clone());
        if let Some(parsed) = crate::action::parse_action(&action) {
            self.action_name = Some(parsed.name);
            self.action_parameters = parsed.arguments;
        } else {
            self.action_name = Some(action);
            self.action_parameters.clear();
        }
        self
    }

    pub fn hidden(mut self, hidden: bool) -> Self {
        self.show = !hidden;
        self
    }

    pub fn with_key_display(mut self, key_display: impl Into<String>) -> Self {
        self.key_display = Some(key_display.into());
        self
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    pub fn with_priority(mut self, priority: bool) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_system(mut self, system: bool) -> Self {
        self.system = system;
        self
    }
}

impl KeyBind {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn from_event(key: &KeyEventData) -> Self {
        Self {
            code: key.code,
            modifiers: key.modifiers,
        }
    }

    pub fn key_name(&self) -> String {
        KeyEventData::from_crossterm(KeyEvent::new(self.code, self.modifiers)).key
    }

    pub fn display_key(&self) -> String {
        format_key_display(&self.key_name())
    }
}

#[derive(Debug, Default)]
pub struct ActionMap {
    bindings: HashMap<KeyBind, Action>,
}

impl ActionMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(&mut self, key: KeyBind, action: Action) {
        self.bindings.insert(key, action);
    }

    pub fn lookup(&self, key: &KeyBind) -> Option<Action> {
        self.bindings.get(key).copied()
    }

    pub fn entries(&self) -> Vec<(KeyBind, Action)> {
        self.bindings
            .iter()
            .map(|(bind, action)| (*bind, *action))
            .collect()
    }
}

/// A class mutation queued by a widget event handler and applied to the
/// arena node record by the runtime after dispatch. Part of the queued-effects
/// pattern alongside `messages` and `recompose_nodes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassOp {
    Add(String),
    Remove(String),
}

#[derive(Debug, Default)]
pub struct EventCtx {
    node_id: NodeId,
    handled: bool,
    repaint_requested: bool,
    invalidation: InvalidationFlags,
    stop_requested: bool,
    messages: Vec<MessageEvent>,
    animation_requests: Vec<AnimationRequest>,
    style_animation_requests: Vec<StyleAnimationRequest>,
    worker_requests: Vec<WorkerRequest>,
    recompose_nodes: Vec<NodeId>,
    class_ops: Vec<(NodeId, ClassOp)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InvalidationFlags {
    pub content: bool,
    pub style: bool,
    pub layout: bool,
}

impl InvalidationFlags {
    pub fn content() -> Self {
        Self {
            content: true,
            style: false,
            layout: false,
        }
    }

    pub fn style() -> Self {
        Self {
            content: true,
            style: true,
            layout: false,
        }
    }

    pub fn layout() -> Self {
        Self {
            content: true,
            style: true,
            layout: true,
        }
    }

    pub fn merge(&mut self, other: Self) {
        self.content |= other.content;
        self.style |= other.style;
        self.layout |= other.layout;
    }
}

impl EventCtx {
    /// The arena node ID for the widget currently being dispatched to.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Set the node ID for the current dispatch context.
    pub fn set_node_id(&mut self, id: NodeId) {
        self.node_id = id;
    }

    pub fn handled(&self) -> bool {
        self.handled
    }

    pub fn set_handled(&mut self) {
        self.handled = true;
    }

    /// Request a repaint after this event dispatch finishes.
    ///
    /// This is useful when a widget updates visual state but does not (or should not)
    /// mark the event as handled.
    pub fn request_repaint(&mut self) {
        self.repaint_requested = true;
        self.invalidation.merge(InvalidationFlags::content());
    }

    pub fn repaint_requested(&self) -> bool {
        self.repaint_requested
    }

    pub fn invalidation(&self) -> InvalidationFlags {
        self.invalidation
    }

    /// Request style recomputation (without forcing a full relayout).
    pub fn request_style_invalidation(&mut self) {
        self.repaint_requested = true;
        self.invalidation.merge(InvalidationFlags::style());
    }

    /// Request a layout/style/content invalidation.
    pub fn request_layout_invalidation(&mut self) {
        self.repaint_requested = true;
        self.invalidation.merge(InvalidationFlags::layout());
    }

    /// Request subtree recomposition for the current widget node.
    pub fn request_recompose(&mut self) {
        self.request_recompose_node(self.node_id);
    }

    /// Request subtree recomposition for a specific node.
    pub fn request_recompose_node(&mut self, node_id: NodeId) {
        if !self.recompose_nodes.contains(&node_id) {
            self.recompose_nodes.push(node_id);
        }
        self.request_layout_invalidation();
    }

    /// Request the runtime event loop to stop after current dispatch finishes.
    pub fn request_stop(&mut self) {
        self.stop_requested = true;
    }

    pub fn stop_requested(&self) -> bool {
        self.stop_requested
    }

    pub fn post_message<M: Message>(&mut self, message: M) {
        self.post_message_boxed(Box::new(message));
    }

    /// Run a string action (`"namespace.name(args)"`).
    ///
    /// Mirrors Python `Widget.run_action` / `MessagePump.run_action`: posts an
    /// [`ActionDispatchRequested`](crate::message::ActionDispatchRequested)
    /// message whose sender is this handler's widget, so the runtime resolves
    /// the action against the `widget → screen → app` namespace chain (with
    /// `check_action` gating) and dispatches it.  Use this from a widget event
    /// handler to trigger an action by name without hard-coding the mutation.
    pub fn run_action(&mut self, action: impl Into<String>) {
        self.post_message(crate::message::ActionDispatchRequested {
            action: action.into(),
        });
    }

    pub fn post_message_boxed(&mut self, message: Box<dyn Message>) {
        debug_message(&format!(
            "[post_message] sender={} payload={message:?}",
            node_id_to_ffi(self.node_id)
        ));
        self.messages
            .push(MessageEvent::from_boxed(self.node_id, message).with_control(self.node_id));
    }

    pub fn spawn_async_task(&mut self, task_id: u64, target: NodeId, request: AsyncTaskRequest) {
        self.post_message(crate::message::AsyncTaskSpawn {
            task_id,
            target,
            request,
        });
    }

    pub fn spawn_async_task_for(&mut self, task_id: u64, request: AsyncTaskRequest) {
        let self_id = self.node_id;
        self.spawn_async_task(task_id, self_id, request);
    }

    pub fn cancel_async_task(&mut self, task_id: u64) {
        self.post_message(crate::message::AsyncTaskCancel { task_id });
    }

    pub fn cancel_async_tasks_for(&mut self, target: NodeId) {
        self.post_message(crate::message::AsyncTaskCancelTarget { target });
    }

    pub fn schedule_timer(&mut self, timer_id: u64, target: NodeId, delay: Duration) {
        self.post_message(crate::message::TimerSchedule {
            timer_id,
            target,
            delay,
        });
    }

    pub fn schedule_timer_for(&mut self, timer_id: u64, delay: Duration) {
        let self_id = self.node_id;
        self.schedule_timer(timer_id, self_id, delay);
    }

    pub fn cancel_timer(&mut self, timer_id: u64) {
        self.post_message(crate::message::TimerCancel { timer_id });
    }

    pub fn set_overlay_visible(&mut self, overlay: NodeId, visible: bool) {
        self.post_message(crate::message::OverlaySetVisible { overlay, visible });
    }

    pub fn show_overlay(&mut self, overlay: NodeId) {
        self.set_overlay_visible(overlay, true);
    }

    pub fn hide_overlay(&mut self, overlay: NodeId) {
        self.set_overlay_visible(overlay, false);
    }

    pub fn toggle_overlay(&mut self, overlay: NodeId) {
        self.post_message(crate::message::OverlayToggle { overlay });
    }

    pub fn dismiss_overlay(&mut self, overlay: Option<NodeId>) {
        self.post_message(crate::message::OverlayDismissRequested { overlay });
    }

    pub fn open_command_palette(&mut self) {
        self.post_message(crate::message::CommandPaletteOpened);
    }

    pub fn close_command_palette(&mut self) {
        self.post_message(crate::message::CommandPaletteClosed);
    }

    pub fn set_command_palette_commands(&mut self, commands: Vec<CommandPaletteCommand>) {
        self.post_message(crate::message::CommandPaletteSetCommands { commands });
    }

    pub fn select_command_palette_command(
        &mut self,
        id: impl Into<String>,
        title: impl Into<String>,
    ) {
        self.post_message(crate::message::CommandPaletteCommandSelected {
            id: id.into(),
            title: title.into(),
        });
    }

    pub fn request_animation(&mut self, request: AnimationRequest) {
        debug_message(&format!(
            "[request_animation] target={} attribute={} start={} end={} duration_ms={} delay_ms={} ease={:?} level={:?}",
            node_id_to_ffi(request.target),
            request.attribute,
            request.start,
            request.end,
            request.duration.as_millis(),
            request.delay.as_millis(),
            request.ease,
            request.level
        ));
        self.animation_requests.push(request);
    }

    /// Request a CSS property animation on a specific node.
    pub fn animate_style(
        &mut self,
        target: NodeId,
        property: impl Into<String>,
        from: StyleValue,
        to: StyleValue,
        duration: Duration,
        ease: AnimationEase,
    ) {
        let request =
            StyleAnimationRequest::new(target, property, from, to, duration).with_ease(ease);
        self.request_style_animation(request);
    }

    /// Enqueue a fully-formed style animation request.
    pub fn request_style_animation(&mut self, request: StyleAnimationRequest) {
        debug_message(&format!(
            "[request_style_animation] target={} property={} duration_ms={} ease={:?}",
            node_id_to_ffi(request.target),
            request.property,
            request.duration.as_millis(),
            request.ease
        ));
        self.style_animation_requests.push(request);
    }

    /// Request a background worker to be spawned by the runtime.
    ///
    /// Returns after recording the request — actual spawning happens in the
    /// runtime event loop after dispatch completes.
    pub fn request_worker(&mut self, name: Option<&str>) {
        self.request_worker_with_payload(name, WorkerRequestPayload::default());
    }

    /// Request a background worker with an explicit payload.
    pub fn request_worker_with_payload(
        &mut self,
        name: Option<&str>,
        payload: WorkerRequestPayload,
    ) {
        self.worker_requests.push(WorkerRequest {
            owner: self.node_id,
            exclusive_key: None,
            name: name.map(|s| s.to_string()),
            payload,
        });
    }

    /// Request an exclusive background worker.
    ///
    /// Any existing worker with the same `key` owned by this widget will be
    /// cancelled before the new one starts.
    pub fn request_exclusive_worker(&mut self, key: &str, name: Option<&str>) {
        self.request_exclusive_worker_with_payload(key, name, WorkerRequestPayload::default());
    }

    /// Request an exclusive background worker with an explicit payload.
    pub fn request_exclusive_worker_with_payload(
        &mut self,
        key: &str,
        name: Option<&str>,
        payload: WorkerRequestPayload,
    ) {
        self.worker_requests.push(WorkerRequest {
            owner: self.node_id,
            exclusive_key: Some(key.to_string()),
            name: name.map(|s| s.to_string()),
            payload,
        });
    }

    /// Request a closure-backed background worker.
    pub fn request_worker_task(
        &mut self,
        name: Option<&str>,
        task: impl FnOnce(CancellationToken) -> Result<(), String> + Send + 'static,
    ) {
        self.request_worker_with_payload(name, WorkerRequestPayload::task(task));
    }

    /// Request a closure-backed exclusive background worker.
    pub fn request_exclusive_worker_task(
        &mut self,
        key: &str,
        name: Option<&str>,
        task: impl FnOnce(CancellationToken) -> Result<(), String> + Send + 'static,
    ) {
        self.request_exclusive_worker_with_payload(key, name, WorkerRequestPayload::task(task));
    }

    /// Take pending worker requests (called by runtime after dispatch).
    pub(crate) fn take_worker_requests(&mut self) -> Vec<WorkerRequest> {
        std::mem::take(&mut self.worker_requests)
    }

    pub(crate) fn take_recompose_nodes(&mut self) -> Vec<NodeId> {
        std::mem::take(&mut self.recompose_nodes)
    }

    /// Queue an `Add` class op on this widget's own node.
    pub fn add_class(&mut self, class: &str) {
        let node_id = self.node_id;
        self.class_ops
            .push((node_id, ClassOp::Add(class.to_string())));
    }

    /// Queue a `Remove` class op on this widget's own node.
    pub fn remove_class(&mut self, class: &str) {
        let node_id = self.node_id;
        self.class_ops
            .push((node_id, ClassOp::Remove(class.to_string())));
    }

    /// Queue an `Add` or `Remove` class op on this widget's own node based on `on`.
    pub fn set_class(&mut self, on: bool, class: &str) {
        if on {
            self.add_class(class);
        } else {
            self.remove_class(class);
        }
    }

    /// Queue an `Add` class op on an arbitrary node.
    pub fn add_class_to(&mut self, node: NodeId, class: &str) {
        self.class_ops.push((node, ClassOp::Add(class.to_string())));
    }

    /// Queue a `Remove` class op on an arbitrary node.
    pub fn remove_class_from(&mut self, node: NodeId, class: &str) {
        self.class_ops
            .push((node, ClassOp::Remove(class.to_string())));
    }

    pub(crate) fn take_class_ops(&mut self) -> Vec<(NodeId, ClassOp)> {
        std::mem::take(&mut self.class_ops)
    }

    pub(crate) fn merge_from(&mut self, mut other: EventCtx) {
        if other.handled {
            self.handled = true;
        }
        if other.repaint_requested {
            self.repaint_requested = true;
        }
        self.invalidation.merge(other.invalidation);
        if other.stop_requested {
            self.stop_requested = true;
        }
        self.messages.append(&mut other.messages);
        self.animation_requests
            .append(&mut other.animation_requests);
        self.style_animation_requests
            .append(&mut other.style_animation_requests);
        self.worker_requests.append(&mut other.worker_requests);
        for node_id in other.recompose_nodes.drain(..) {
            if !self.recompose_nodes.contains(&node_id) {
                self.recompose_nodes.push(node_id);
            }
        }
        self.class_ops.append(&mut other.class_ops);
    }

    pub(crate) fn take_messages(&mut self) -> Vec<MessageEvent> {
        std::mem::take(&mut self.messages)
    }

    pub(crate) fn take_animation_requests(&mut self) -> Vec<AnimationRequest> {
        std::mem::take(&mut self.animation_requests)
    }

    /// Animation infrastructure — will be wired when the animation system
    /// drives CSS transition requests through EventCtx.
    #[allow(dead_code)]
    pub(crate) fn take_style_animation_requests(&mut self) -> Vec<StyleAnimationRequest> {
        std::mem::take(&mut self.style_animation_requests)
    }
}

/// Widget-facing context provided by the runtime during event dispatch and rendering.
///
/// **Key design principle:** Widgets do NOT own or store their canonical identity.
/// The arena (`WidgetTree`) owns node identity; widgets receive it through this
/// context when they need it (event handlers, watchers, render).
///
/// `WidgetCtx` wraps an `EventCtx` plus the caller's `NodeId`, so widgets can
/// post messages, request repaints, and query their own identity without owning
/// an identity field.
///
/// # Lifecycle
///
/// The runtime constructs a `WidgetCtx` before each widget callback (event,
/// render, mount, etc.) and reads side-effects out of it afterwards. Widgets
/// never construct one themselves.
///
/// # Migration path
///
/// `WidgetCtx` will gradually replace direct `EventCtx` parameters in widget
/// trait methods as the arena-tree dispatch matures.
#[derive(Debug)]
pub struct WidgetCtx<'a> {
    node_id: NodeId,
    event_ctx: &'a mut EventCtx,
}

impl<'a> WidgetCtx<'a> {
    /// Create a new widget context. Called by the runtime, not by widgets.
    ///
    /// Public API — documented migration path from EventCtx for tree-aware
    /// event handling. Not yet called from the runtime event loop.
    #[allow(dead_code)]
    pub(crate) fn new(node_id: NodeId, event_ctx: &'a mut EventCtx) -> Self {
        Self { node_id, event_ctx }
    }

    /// The arena-assigned identity of this widget.
    #[inline]
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Access the underlying `EventCtx` for repaint/stop/invalidation requests.
    #[inline]
    pub fn event_ctx(&self) -> &EventCtx {
        self.event_ctx
    }

    /// Mutable access to the underlying `EventCtx`.
    #[inline]
    pub fn event_ctx_mut(&mut self) -> &mut EventCtx {
        self.event_ctx
    }

    // ── Convenience delegates ──────────────────────────────────────────

    /// Mark the event as handled.
    #[inline]
    pub fn set_handled(&mut self) {
        self.event_ctx.set_handled();
    }

    /// Request a repaint after event dispatch.
    #[inline]
    pub fn request_repaint(&mut self) {
        self.event_ctx.request_repaint();
    }

    /// Request the runtime to stop.
    #[inline]
    pub fn request_stop(&mut self) {
        self.event_ctx.request_stop();
    }

    /// Post a message from this widget (sender = self).
    #[inline]
    pub fn post_message<M: Message>(&mut self, message: M) {
        self.event_ctx.set_node_id(self.node_id);
        self.event_ctx.post_message(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{AsyncTaskRequest, CommandPaletteCommand};
    use crate::node_id::node_id_from_ffi;
    use crate::style::{Color, Scalar, Spacing, Tint};
    use std::time::Duration;

    #[test]
    fn helper_methods_emit_runtime_control_messages() {
        let sender_id = node_id_from_ffi(12);
        let mut ctx = EventCtx::default();
        ctx.set_node_id(sender_id);

        ctx.spawn_async_task_for(
            5,
            AsyncTaskRequest::Sleep {
                duration: Duration::from_millis(10),
                label: "work".to_string(),
            },
        );
        ctx.schedule_timer_for(9, Duration::from_millis(25));
        ctx.cancel_async_task(5);
        ctx.cancel_timer(9);

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 4);
        {
            let m = messages[0]
                .downcast_ref::<crate::message::AsyncTaskSpawn>()
                .unwrap();
            assert_eq!(m.task_id, 5);
            assert_eq!(m.target, sender_id);
            assert!(
                matches!(m.request, AsyncTaskRequest::Sleep { ref label, .. } if label == "work")
            );
        }
        {
            let m = messages[1]
                .downcast_ref::<crate::message::TimerSchedule>()
                .unwrap();
            assert_eq!(m.timer_id, 9);
            assert_eq!(m.target, sender_id);
        }
        {
            let m = messages[2]
                .downcast_ref::<crate::message::AsyncTaskCancel>()
                .unwrap();
            assert_eq!(m.task_id, 5);
        }
        {
            let m = messages[3]
                .downcast_ref::<crate::message::TimerCancel>()
                .unwrap();
            assert_eq!(m.timer_id, 9);
        }
    }

    #[test]
    fn overlay_and_command_palette_helpers_emit_messages() {
        let overlay_id = node_id_from_ffi(77);
        let mut ctx = EventCtx::default();
        ctx.set_node_id(node_id_from_ffi(5));

        ctx.show_overlay(overlay_id);
        ctx.hide_overlay(overlay_id);
        ctx.toggle_overlay(overlay_id);
        ctx.dismiss_overlay(Some(overlay_id));
        ctx.open_command_palette();
        ctx.set_command_palette_commands(vec![CommandPaletteCommand {
            id: "open".to_string(),
            title: "Open".to_string(),
            help: "Open file".to_string(),
        }]);
        ctx.select_command_palette_command("open", "Open");
        ctx.close_command_palette();

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 8);
        assert!(
            messages[0]
                .downcast_ref::<crate::message::OverlaySetVisible>()
                .is_some_and(|m| m.overlay == overlay_id && m.visible)
        );
        assert!(
            messages[1]
                .downcast_ref::<crate::message::OverlaySetVisible>()
                .is_some_and(|m| m.overlay == overlay_id && !m.visible)
        );
        assert!(
            messages[2]
                .downcast_ref::<crate::message::OverlayToggle>()
                .is_some_and(|m| m.overlay == overlay_id)
        );
        assert!(
            messages[3]
                .downcast_ref::<crate::message::OverlayDismissRequested>()
                .is_some_and(|m| m.overlay == Some(overlay_id))
        );
        assert!(messages[4].is::<crate::message::CommandPaletteOpened>());
        assert!(
            messages[5]
                .downcast_ref::<crate::message::CommandPaletteSetCommands>()
                .is_some_and(|m| m.commands.len() == 1 && m.commands[0].id == "open")
        );
        assert!(
            messages[6]
                .downcast_ref::<crate::message::CommandPaletteCommandSelected>()
                .is_some_and(|m| m.id == "open" && m.title == "Open")
        );
        assert!(messages[7].is::<crate::message::CommandPaletteClosed>());
    }

    // ── New event struct construction tests ──────────────────────────

    #[test]
    fn mouse_enter_event_construction() {
        let e = MouseEnterEvent {
            x: 5,
            y: 10,
            screen_x: 20,
            screen_y: 30,
        };
        assert_eq!(e.x, 5);
        assert_eq!(e.y, 10);
        assert_eq!(e.screen_x, 20);
        assert_eq!(e.screen_y, 30);
        let ev = Event::Enter(e);
        assert!(matches!(
            ev,
            Event::Enter(MouseEnterEvent { x: 5, y: 10, .. })
        ));
    }

    #[test]
    fn mouse_leave_event_construction() {
        let e = MouseLeaveEvent {
            x: 1,
            y: 2,
            screen_x: 3,
            screen_y: 4,
        };
        let ev = Event::Leave(e);
        assert!(matches!(
            ev,
            Event::Leave(MouseLeaveEvent {
                x: 1,
                y: 2,
                screen_x: 3,
                screen_y: 4
            })
        ));
    }

    #[test]
    fn click_event_construction() {
        let e = ClickEvent {
            x: 10,
            y: 20,
            screen_x: 50,
            screen_y: 60,
            button: 0,
        };
        assert_eq!(e.button, 0);
        let ev = Event::Click(e);
        assert!(matches!(ev, Event::Click(ClickEvent { button: 0, .. })));
    }

    #[test]
    fn click_event_right_button() {
        let e = ClickEvent {
            x: 0,
            y: 0,
            screen_x: 0,
            screen_y: 0,
            button: 2,
        };
        assert_eq!(e.button, 2);
    }

    #[test]
    fn paste_event_construction() {
        let e = PasteEvent {
            text: "hello world".to_string(),
        };
        assert_eq!(e.text, "hello world");
        let ev = Event::Paste(e);
        assert!(matches!(ev, Event::Paste(PasteEvent { .. })));
    }

    #[test]
    fn paste_event_empty_text() {
        let e = PasteEvent {
            text: String::new(),
        };
        assert!(e.text.is_empty());
    }

    #[test]
    fn mount_event_construction() {
        let id = node_id_from_ffi(42);
        let e = MountEvent { node: id };
        assert_eq!(e.node, id);
        let ev = Event::Mount(e);
        assert!(matches!(ev, Event::Mount(MountEvent { node }) if node == id));
    }

    #[test]
    fn unmount_event_construction() {
        let id = node_id_from_ffi(7);
        let e = UnmountEvent { node: id };
        assert_eq!(e.node, id);
        let ev = Event::Unmount(e);
        assert!(matches!(ev, Event::Unmount(UnmountEvent { node }) if node == id));
    }

    #[test]
    fn ready_event_construction() {
        let e = ReadyEvent;
        let ev = Event::Ready(e);
        assert!(matches!(ev, Event::Ready(ReadyEvent)));
    }

    #[test]
    fn focus_event_construction() {
        let id = node_id_from_ffi(99);
        let e = FocusEvent { node: id };
        assert_eq!(e.node, id);
        let ev = Event::Focus(e);
        assert!(matches!(ev, Event::Focus(FocusEvent { node }) if node == id));
    }

    #[test]
    fn blur_event_construction() {
        let id = node_id_from_ffi(55);
        let e = BlurEvent { node: id };
        assert_eq!(e.node, id);
        let ev = Event::Blur(e);
        assert!(matches!(ev, Event::Blur(BlurEvent { node }) if node == id));
    }

    // ── StyleValue / StyleAnimationRequest tests ─────────────────────

    #[test]
    fn style_value_color_construction() {
        let v = StyleValue::Color(Color::rgb(10, 20, 30));
        assert!(matches!(
            v,
            StyleValue::Color(Color {
                r: 10,
                g: 20,
                b: 30,
                ..
            })
        ));
    }

    #[test]
    fn style_value_float_construction() {
        let v = StyleValue::Float(50.0);
        assert!(matches!(v, StyleValue::Float(x) if (x - 50.0).abs() < 0.001));
    }

    #[test]
    fn style_value_scalar_construction() {
        let v = StyleValue::Scalar(Scalar::Cells(42));
        assert!(matches!(v, StyleValue::Scalar(Scalar::Cells(42))));
    }

    #[test]
    fn style_value_spacing_construction() {
        let v = StyleValue::Spacing(Spacing::all(5));
        if let StyleValue::Spacing(s) = v {
            assert_eq!(s.top, 5);
            assert_eq!(s.right, 5);
        } else {
            panic!("expected Spacing");
        }
    }

    #[test]
    fn style_value_tint_construction() {
        let v = StyleValue::Tint(Tint::new(Color::rgb(255, 0, 0), 50));
        if let StyleValue::Tint(t) = v {
            assert_eq!(t.color, Color::rgb(255, 0, 0));
            assert_eq!(t.percent, 50);
        } else {
            panic!("expected Tint");
        }
    }

    #[test]
    fn style_animation_request_builder() {
        let target = node_id_from_ffi(10);
        let req = StyleAnimationRequest::new(
            target,
            "bg",
            StyleValue::Color(Color::rgb(0, 0, 0)),
            StyleValue::Color(Color::rgb(255, 255, 255)),
            Duration::from_millis(300),
        )
        .with_delay(Duration::from_millis(50))
        .with_ease(AnimationEase::Linear)
        .with_level(AnimationLevel::Basic);

        assert_eq!(req.target, target);
        assert_eq!(req.property, "bg");
        assert_eq!(req.duration, Duration::from_millis(300));
        assert_eq!(req.delay, Duration::from_millis(50));
        assert_eq!(req.ease, AnimationEase::Linear);
        assert_eq!(req.level, AnimationLevel::Basic);
    }

    #[test]
    fn event_ctx_animate_style_populates_requests() {
        let target = node_id_from_ffi(20);
        let mut ctx = EventCtx::default();
        ctx.set_node_id(target);

        ctx.animate_style(
            target,
            "opacity",
            StyleValue::Float(0.0),
            StyleValue::Float(100.0),
            Duration::from_millis(500),
            AnimationEase::OutCubic,
        );

        let requests = ctx.take_style_animation_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].property, "opacity");
        assert_eq!(requests[0].ease, AnimationEase::OutCubic);
    }

    #[test]
    fn event_ctx_merge_includes_style_animation_requests() {
        let mut a = EventCtx::default();
        a.set_node_id(node_id_from_ffi(1));
        let mut b = EventCtx::default();
        b.set_node_id(node_id_from_ffi(2));

        let target = node_id_from_ffi(10);
        a.request_style_animation(StyleAnimationRequest::new(
            target,
            "fg",
            StyleValue::Color(Color::rgb(0, 0, 0)),
            StyleValue::Color(Color::rgb(255, 0, 0)),
            Duration::from_millis(200),
        ));
        b.request_style_animation(StyleAnimationRequest::new(
            target,
            "bg",
            StyleValue::Color(Color::rgb(0, 0, 0)),
            StyleValue::Color(Color::rgb(0, 255, 0)),
            Duration::from_millis(300),
        ));

        a.merge_from(b);
        let requests = a.take_style_animation_requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].property, "fg");
        assert_eq!(requests[1].property, "bg");
    }

    #[test]
    fn animation_ease_has_all_variants() {
        let variants = [
            AnimationEase::None,
            AnimationEase::Round,
            AnimationEase::Linear,
            AnimationEase::InOutCubic,
            AnimationEase::OutCubic,
            AnimationEase::InQuad,
            AnimationEase::OutQuad,
            AnimationEase::InOutQuad,
            AnimationEase::InCubic,
            AnimationEase::InQuart,
            AnimationEase::OutQuart,
            AnimationEase::InOutQuart,
            AnimationEase::InQuint,
            AnimationEase::OutQuint,
            AnimationEase::InOutQuint,
            AnimationEase::InExpo,
            AnimationEase::OutExpo,
            AnimationEase::InOutExpo,
            AnimationEase::InCirc,
            AnimationEase::OutCirc,
            AnimationEase::InOutCirc,
            AnimationEase::InBack,
            AnimationEase::OutBack,
            AnimationEase::InOutBack,
            AnimationEase::InBounce,
            AnimationEase::OutBounce,
            AnimationEase::InOutBounce,
            AnimationEase::InElastic,
            AnimationEase::OutElastic,
            AnimationEase::InOutElastic,
        ];
        assert_eq!(variants.len(), 30);
    }

    // ── Worker request tests ───────────────────────────────────────────

    #[test]
    fn event_ctx_request_worker() {
        let owner = node_id_from_ffi(10);
        let mut ctx = EventCtx::default();
        ctx.set_node_id(owner);
        ctx.request_worker(Some("bg-fetch"));

        let reqs = ctx.take_worker_requests();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].owner, owner);
        assert!(reqs[0].exclusive_key.is_none());
        assert_eq!(reqs[0].name.as_deref(), Some("bg-fetch"));
    }

    #[test]
    fn event_ctx_request_exclusive_worker() {
        let owner = node_id_from_ffi(11);
        let mut ctx = EventCtx::default();
        ctx.set_node_id(owner);
        ctx.request_exclusive_worker("search", Some("search-worker"));

        let reqs = ctx.take_worker_requests();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].owner, owner);
        assert_eq!(reqs[0].exclusive_key.as_deref(), Some("search"));
        assert_eq!(reqs[0].name.as_deref(), Some("search-worker"));
    }

    #[test]
    fn event_ctx_request_worker_with_payload() {
        let owner = node_id_from_ffi(12);
        let mut ctx = EventCtx::default();
        ctx.set_node_id(owner);
        ctx.request_worker_with_payload(
            Some("digest"),
            WorkerRequestPayload::ComputeDigest {
                input: "abc".into(),
                rounds: 2,
                delay_per_round_ms: 0,
                fail_with: None,
            },
        );
        let reqs = ctx.take_worker_requests();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].owner, owner);
        assert!(matches!(
            reqs[0].payload,
            WorkerRequestPayload::ComputeDigest { rounds: 2, .. }
        ));
    }

    #[test]
    fn event_ctx_request_worker_task_uses_task_payload() {
        let mut ctx = EventCtx::default();
        ctx.set_node_id(node_id_from_ffi(13));
        ctx.request_worker_task(Some("task"), |_token| Ok(()));
        let reqs = ctx.take_worker_requests();
        assert_eq!(reqs.len(), 1);
        assert!(matches!(reqs[0].payload, WorkerRequestPayload::Task(_)));
    }

    #[test]
    fn event_ctx_take_worker_requests_drains() {
        let mut ctx = EventCtx::default();
        ctx.set_node_id(node_id_from_ffi(1));
        ctx.request_worker(None);
        ctx.request_worker(None);

        let reqs = ctx.take_worker_requests();
        assert_eq!(reqs.len(), 2);
        // Second take should be empty.
        let reqs2 = ctx.take_worker_requests();
        assert!(reqs2.is_empty());
    }

    #[test]
    fn event_ctx_merge_includes_worker_requests() {
        let mut a = EventCtx::default();
        a.set_node_id(node_id_from_ffi(1));
        a.request_worker(Some("a"));

        let mut b = EventCtx::default();
        b.set_node_id(node_id_from_ffi(2));
        b.request_worker(Some("b"));

        a.merge_from(b);
        let reqs = a.take_worker_requests();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].name.as_deref(), Some("a"));
        assert_eq!(reqs[1].name.as_deref(), Some("b"));
    }

    #[test]
    fn post_message_sets_control_to_sender() {
        let sender_id = node_id_from_ffi(42);
        let mut ctx = EventCtx::default();
        ctx.set_node_id(sender_id);

        ctx.post_message(crate::message::ClearRequested);

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, sender_id);
        assert_eq!(
            messages[0].control,
            Some(sender_id),
            "post_message should set control to Some(sender)"
        );
    }
}
