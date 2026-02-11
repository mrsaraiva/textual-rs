use crate::debug::debug_message;
use crate::keys::KeyEventData;
use crate::keys::format_key_display;
use crate::message::{AsyncTaskRequest, Message, MessageEvent};
use crate::widgets::WidgetId;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseDownEvent {
    pub target: WidgetId,
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates (origin at widget content top-left).
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseUpEvent {
    pub target: Option<WidgetId>,
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates (origin at widget content top-left of `target`, if any).
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseScrollEvent {
    pub target: Option<WidgetId>,
    pub screen_x: u16,
    pub screen_y: u16,
    /// Content-local coordinates (origin at widget content top-left of `target`, if any).
    pub x: u16,
    pub y: u16,
    pub delta_x: i32,
    pub delta_y: i32,
    pub modifiers: KeyModifiers,
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationRequest {
    pub target: WidgetId,
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
        target: WidgetId,
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

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationValueEvent {
    pub target: WidgetId,
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
    MouseScroll(MouseScrollEvent),
    AnimationValue(AnimationValueEvent),
    AppFocus(bool),
    Tick(u64),
    Resize(u16, u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    FocusNext,
    FocusPrev,
    HelpQuit,
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
    pub show: bool,
    pub key_display: Option<String>,
    pub group: Option<String>,
    pub priority: bool,
    pub system: bool,
}

impl BindingHint {
    pub fn new(key: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
            show: true,
            key_display: None,
            group: None,
            priority: false,
            system: false,
        }
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

#[derive(Debug, Default)]
pub struct EventCtx {
    handled: bool,
    repaint_requested: bool,
    invalidation: InvalidationFlags,
    stop_requested: bool,
    messages: Vec<MessageEvent>,
    animation_requests: Vec<AnimationRequest>,
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

    /// Request the runtime event loop to stop after current dispatch finishes.
    pub fn request_stop(&mut self) {
        self.stop_requested = true;
    }

    pub fn stop_requested(&self) -> bool {
        self.stop_requested
    }

    pub fn post_message(&mut self, sender: WidgetId, message: Message) {
        debug_message(&format!(
            "[post_message] sender={} payload={message:?}",
            sender.as_u64()
        ));
        self.messages.push(MessageEvent { sender, message });
    }

    pub fn spawn_async_task(
        &mut self,
        sender: WidgetId,
        task_id: u64,
        target: WidgetId,
        request: AsyncTaskRequest,
    ) {
        self.post_message(
            sender,
            Message::AsyncTaskSpawn {
                task_id,
                target,
                request,
            },
        );
    }

    pub fn spawn_async_task_for(
        &mut self,
        sender: WidgetId,
        task_id: u64,
        request: AsyncTaskRequest,
    ) {
        self.spawn_async_task(sender, task_id, sender, request);
    }

    pub fn cancel_async_task(&mut self, sender: WidgetId, task_id: u64) {
        self.post_message(sender, Message::AsyncTaskCancel { task_id });
    }

    pub fn cancel_async_tasks_for(&mut self, sender: WidgetId, target: WidgetId) {
        self.post_message(sender, Message::AsyncTaskCancelTarget { target });
    }

    pub fn schedule_timer(
        &mut self,
        sender: WidgetId,
        timer_id: u64,
        target: WidgetId,
        delay: Duration,
    ) {
        self.post_message(
            sender,
            Message::TimerSchedule {
                timer_id,
                target,
                delay,
            },
        );
    }

    pub fn schedule_timer_for(&mut self, sender: WidgetId, timer_id: u64, delay: Duration) {
        self.schedule_timer(sender, timer_id, sender, delay);
    }

    pub fn cancel_timer(&mut self, sender: WidgetId, timer_id: u64) {
        self.post_message(sender, Message::TimerCancel { timer_id });
    }

    pub fn request_animation(&mut self, request: AnimationRequest) {
        debug_message(&format!(
            "[request_animation] target={} attribute={} start={} end={} duration_ms={} delay_ms={} ease={:?} level={:?}",
            request.target.as_u64(),
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
    }

    pub(crate) fn take_messages(&mut self) -> Vec<MessageEvent> {
        std::mem::take(&mut self.messages)
    }

    pub(crate) fn take_animation_requests(&mut self) -> Vec<AnimationRequest> {
        std::mem::take(&mut self.animation_requests)
    }
}

#[cfg(test)]
mod tests {
    use super::EventCtx;
    use crate::message::{AsyncTaskRequest, Message};
    use crate::widgets::WidgetId;
    use std::time::Duration;

    #[test]
    fn helper_methods_emit_runtime_control_messages() {
        let sender = WidgetId::from_u64(12);
        let mut ctx = EventCtx::default();

        ctx.spawn_async_task_for(
            sender,
            5,
            AsyncTaskRequest::Sleep {
                duration: Duration::from_millis(10),
                label: "work".to_string(),
            },
        );
        ctx.schedule_timer_for(sender, 9, Duration::from_millis(25));
        ctx.cancel_async_task(sender, 5);
        ctx.cancel_timer(sender, 9);

        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 4);
        assert!(matches!(
            &messages[0].message,
            Message::AsyncTaskSpawn {
                task_id,
                target,
                request: AsyncTaskRequest::Sleep { label, .. },
            } if *task_id == 5 && *target == sender && label == "work"
        ));
        assert!(matches!(
            messages[1].message,
            Message::TimerSchedule {
                timer_id: 9,
                target,
                ..
            } if target == sender
        ));
        assert!(matches!(
            messages[2].message,
            Message::AsyncTaskCancel { task_id: 5 }
        ));
        assert!(matches!(
            messages[3].message,
            Message::TimerCancel { timer_id: 9 }
        ));
    }
}
