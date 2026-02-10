use crate::debug::debug_message;
use crate::event::{Action, AnimationRequest, Event, EventCtx};
use crate::message::MessageEvent;
use crate::widgets::{Widget, WidgetId};

use super::types::DispatchOutcome;

pub(crate) fn dispatch_event(root: &mut dyn Widget, event: Event) -> DispatchOutcome {
    let event_debug = format!("{event:?}");
    let mut ctx = EventCtx::default();
    let always_bubble = matches!(&event, Event::MouseUp(..));
    root.on_event_capture(&event, &mut ctx);
    if always_bubble || !ctx.handled() {
        root.on_event(&event, &mut ctx);
    }
    let outcome = DispatchOutcome {
        handled: ctx.handled(),
        repaint_requested: ctx.repaint_requested(),
        stop_requested: ctx.stop_requested(),
        messages: ctx.take_messages(),
        animation_requests: ctx.take_animation_requests(),
    };
    debug_message(&format!(
        "[dispatch_event] event={event_debug} handled={} repaint={} messages={}",
        outcome.handled,
        outcome.repaint_requested,
        outcome.messages.len()
    ));
    outcome
}

pub(crate) fn is_scroll_action(action: Action) -> bool {
    matches!(
        action,
        Action::ScrollUp
            | Action::ScrollDown
            | Action::ScrollPageUp
            | Action::ScrollPageDown
            | Action::ScrollLeft
            | Action::ScrollRight
            | Action::ScrollPageLeft
            | Action::ScrollPageRight
    )
}

pub(crate) fn is_priority_action(action: Action) -> bool {
    matches!(action, Action::CommandPalette)
}

pub(crate) fn focused_widget_id(root: &mut dyn Widget) -> Option<WidgetId> {
    fn visit(widget: &mut dyn Widget, out: &mut Option<WidgetId>) {
        if out.is_some() {
            return;
        }
        if widget.has_focus() {
            *out = Some(widget.id());
            return;
        }
        widget.visit_children_mut(&mut |child| visit(child, out));
    }

    let mut out = None;
    visit(root, &mut out);
    out
}

pub(crate) fn dispatch_event_to_target(
    root: &mut dyn Widget,
    target: WidgetId,
    event: &Event,
) -> DispatchOutcome {
    let mut ctx = EventCtx::default();
    root.on_event_capture(event, &mut ctx);
    if !ctx.handled() {
        let found = dispatch_event_bubble(root, target, event, &mut ctx);
        if !found {
            root.on_event(event, &mut ctx);
        }
    }
    let handled = ctx.handled();
    let repaint_requested = ctx.repaint_requested();
    let messages = ctx.take_messages();
    let animation_requests = ctx.take_animation_requests();
    debug_message(&format!(
        "[dispatch_event_to_target] target={} event={event:?} handled={} repaint={} messages={}",
        target.as_u64(),
        handled,
        repaint_requested,
        messages.len()
    ));
    DispatchOutcome {
        handled,
        repaint_requested,
        stop_requested: ctx.stop_requested(),
        messages,
        animation_requests,
    }
}

fn dispatch_event_bubble(
    widget: &mut dyn Widget,
    target: WidgetId,
    event: &Event,
    ctx: &mut EventCtx,
) -> bool {
    if widget.id() == target {
        widget.on_event(event, ctx);
        return true;
    }

    let mut found_in_child = false;
    widget.visit_children_mut(&mut |child| {
        if found_in_child {
            return;
        }
        found_in_child = dispatch_event_bubble(child, target, event, ctx);
    });

    if found_in_child && !ctx.handled() {
        widget.on_event(event, ctx);
    }

    found_in_child
}

pub(crate) fn dispatch_scroll_action(
    root: &mut dyn Widget,
    action: Action,
    hovered: Option<WidgetId>,
) -> DispatchOutcome {
    let event = Event::Action(action);
    let focused = focused_widget_id(root);

    if let Some(target) = focused {
        let outcome = dispatch_event_to_target(root, target, &event);
        if outcome.handled || outcome.repaint_requested || !outcome.messages.is_empty() {
            return outcome;
        }
    }

    if let Some(target) = hovered.filter(|id| Some(*id) != focused) {
        let outcome = dispatch_event_to_target(root, target, &event);
        if outcome.handled || outcome.repaint_requested || !outcome.messages.is_empty() {
            return outcome;
        }
    }

    dispatch_event(root, event)
}

pub(crate) fn dispatch_mouse_scroll(
    root: &mut dyn Widget,
    delta_x: i32,
    delta_y: i32,
) -> DispatchOutcome {
    let mut ctx = EventCtx::default();
    root.on_mouse_scroll(delta_x, delta_y, &mut ctx);
    DispatchOutcome {
        handled: ctx.handled(),
        repaint_requested: ctx.repaint_requested(),
        stop_requested: ctx.stop_requested(),
        messages: ctx.take_messages(),
        animation_requests: ctx.take_animation_requests(),
    }
}

pub(crate) fn dispatch_mouse_scroll_to_target(
    root: &mut dyn Widget,
    target: WidgetId,
    delta_x: i32,
    delta_y: i32,
) -> DispatchOutcome {
    let mut ctx = EventCtx::default();
    let found = dispatch_mouse_scroll_bubble(root, target, delta_x, delta_y, &mut ctx);
    if !found {
        root.on_mouse_scroll(delta_x, delta_y, &mut ctx);
    }
    let handled = ctx.handled();
    let repaint_requested = ctx.repaint_requested();
    let messages = ctx.take_messages();
    let animation_requests = ctx.take_animation_requests();
    debug_message(&format!(
        "[dispatch_mouse_scroll] target={} found={} dx={} dy={} handled={} repaint={} messages={}",
        target.as_u64(),
        found,
        delta_x,
        delta_y,
        handled,
        repaint_requested,
        messages.len()
    ));
    DispatchOutcome {
        handled,
        repaint_requested,
        stop_requested: ctx.stop_requested(),
        messages,
        animation_requests,
    }
}

fn dispatch_mouse_scroll_bubble(
    widget: &mut dyn Widget,
    target: WidgetId,
    delta_x: i32,
    delta_y: i32,
    ctx: &mut EventCtx,
) -> bool {
    if widget.id() == target {
        widget.on_mouse_scroll(delta_x, delta_y, ctx);
        return true;
    }

    let mut found_in_child = false;
    widget.visit_children_mut(&mut |child| {
        if found_in_child {
            return;
        }
        found_in_child = dispatch_mouse_scroll_bubble(child, target, delta_x, delta_y, ctx);
    });

    if found_in_child && !ctx.handled() {
        widget.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    found_in_child
}

pub(crate) fn dispatch_message_queue(
    root: &mut dyn Widget,
    initial: Vec<MessageEvent>,
) -> DispatchOutcome {
    use std::collections::VecDeque;

    let mut handled = false;
    let mut repaint_requested = false;
    let mut stop_requested = false;
    let mut queue: VecDeque<MessageEvent> = initial.into();
    let mut emitted: Vec<MessageEvent> = Vec::new();
    let mut animation_requests: Vec<AnimationRequest> = Vec::new();
    debug_message(&format!(
        "[dispatch_message_queue] start initial={}",
        queue.len()
    ));

    // Prevent message storms from hanging the runtime.
    const LIMIT: usize = 1024;
    let mut processed = 0usize;

    while let Some(message) = queue.pop_front() {
        processed += 1;
        if processed > LIMIT {
            debug_message("[dispatch_message_queue] limit reached, dropping remaining messages");
            break;
        }

        debug_message(&format!(
            "[dispatch_message_queue] pop idx={} sender={} payload={:?}",
            processed,
            message.sender.as_u64(),
            message.message
        ));
        let mut ctx = EventCtx::default();
        dispatch_message_tree(root, &message, &mut ctx);
        handled |= ctx.handled();

        repaint_requested |= ctx.repaint_requested();
        stop_requested |= ctx.stop_requested();
        let next = ctx.take_messages();
        let mut next_animation_requests = ctx.take_animation_requests();
        debug_message(&format!(
            "[dispatch_message_queue] delivered idx={} handled={} repaint={} emitted_now={}",
            processed,
            ctx.handled(),
            ctx.repaint_requested(),
            next.len()
        ));
        if !next.is_empty() {
            queue.extend(next.clone());
            emitted.extend(next);
        }
        if !next_animation_requests.is_empty() {
            animation_requests.append(&mut next_animation_requests);
        }
    }

    let outcome = DispatchOutcome {
        handled,
        repaint_requested,
        stop_requested,
        messages: emitted,
        animation_requests,
    };
    debug_message(&format!(
        "[dispatch_message_queue] end handled={} repaint={} emitted_total={} processed={}",
        outcome.handled,
        outcome.repaint_requested,
        outcome.messages.len(),
        processed
    ));
    outcome
}

fn dispatch_message_tree(root: &mut dyn Widget, message: &MessageEvent, ctx: &mut EventCtx) {
    debug_message(&format!(
        "[dispatch_message_tree] visit widget={}#{} sender={} payload={:?}",
        root.style_type(),
        root.id().as_u64(),
        message.sender.as_u64(),
        message.message
    ));
    root.on_message(message, ctx);
    if ctx.handled() {
        debug_message(&format!(
            "[dispatch_message_tree] handled by {}#{}",
            root.style_type(),
            root.id().as_u64()
        ));
        return;
    }
    root.visit_children_mut(&mut |child| {
        if ctx.handled() {
            return;
        }
        dispatch_message_tree(child, message, ctx);
    });
}

#[cfg(test)]
mod message_tests {
    use super::*;
    use crate::event::{MouseDownEvent, MouseUpEvent};
    use crate::keys::KeyEventData;
    use crate::message::Message;
    use crate::widgets::{AppRoot, Button, ScrollView};
    use crossterm::event::{KeyCode, KeyModifiers};
    use rich_rs::{Console, ConsoleOptions};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Child {
        id: WidgetId,
    }

    impl Child {
        fn new() -> Self {
            Self {
                id: WidgetId::new(),
            }
        }
    }

    impl Widget for Child {
        fn id(&self) -> WidgetId {
            self.id
        }

        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }

        fn focusable(&self) -> bool {
            true
        }

        fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
            if let Event::Key(key) = event {
                if matches!(key.code, KeyCode::Char('x')) {
                    ctx.post_message(
                        self.id,
                        Message::InputChanged {
                            value: "ok".into(),
                            validation: crate::validation::ValidationResult::success(),
                        },
                    );
                    ctx.set_handled();
                }
            }
        }
    }

    struct Parent {
        id: WidgetId,
        child: Box<dyn Widget>,
        seen: usize,
    }

    impl Parent {
        fn new(child: impl Widget + 'static) -> Self {
            Self {
                id: WidgetId::new(),
                child: Box::new(child),
                seen: 0,
            }
        }
    }

    impl Widget for Parent {
        fn id(&self) -> WidgetId {
            self.id
        }

        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }

        fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
            self.child.on_event_capture(event, ctx);
        }

        fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
            self.child.on_event(event, ctx);
        }

        fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
            if matches!(message.message, Message::InputChanged { .. }) {
                self.seen += 1;
                ctx.set_handled();
            }
        }

        fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
            f(self.child.as_mut());
        }
    }

    #[test]
    fn messages_bubble_to_ancestor_handlers() {
        let mut root = Parent::new(Child::new());
        let key = KeyEventData::from_crossterm(crossterm::event::KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::empty(),
        ));
        let outcome = dispatch_event(&mut root, Event::Key(key));
        assert_eq!(outcome.messages.len(), 1);

        let msg_outcome = dispatch_message_queue(&mut root, outcome.messages);
        assert!(msg_outcome.handled);
        assert_eq!(root.seen, 1);
    }

    struct Receiver {
        id: WidgetId,
        child: Box<dyn Widget>,
        seen: usize,
    }

    impl Receiver {
        fn new(child: impl Widget + 'static) -> Self {
            Self {
                id: WidgetId::new(),
                child: Box::new(child),
                seen: 0,
            }
        }
    }

    impl Widget for Receiver {
        fn id(&self) -> WidgetId {
            self.id
        }
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }
        fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
            self.child.on_event_capture(event, ctx);
        }
        fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
            self.child.on_event(event, ctx);
        }
        fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
            if matches!(message.message, Message::ButtonPressed { .. }) {
                self.seen += 1;
                ctx.set_handled();
            }
        }
        fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
            f(self.child.as_mut());
        }
    }

    #[test]
    fn button_pressed_message_reaches_ancestor() {
        let button = Button::new("x");
        let button_id = button.id();
        let mut root = AppRoot::new().with_child(Receiver::new(button));

        let down = dispatch_event(
            &mut root,
            Event::MouseDown(MouseDownEvent {
                target: button_id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
        );
        let _ = dispatch_message_queue(&mut root, down.messages);

        let up = dispatch_event(
            &mut root,
            Event::MouseUp(MouseUpEvent {
                target: Some(button_id),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
        );
        assert!(!up.messages.is_empty());
        let routed = dispatch_message_queue(&mut root, up.messages);
        assert!(routed.handled);
    }

    #[test]
    fn button_pressed_message_survives_scrollview_forwarding() {
        let button = Button::new("x");
        let button_id = button.id();
        let scroll = ScrollView::new(button);
        let mut root = AppRoot::new().with_child(Receiver::new(scroll));

        let down = dispatch_event(
            &mut root,
            Event::MouseDown(MouseDownEvent {
                target: button_id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
        );
        let _ = dispatch_message_queue(&mut root, down.messages);

        let up = dispatch_event(
            &mut root,
            Event::MouseUp(MouseUpEvent {
                target: Some(button_id),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
        );
        assert_eq!(up.messages.len(), 1);
        let routed = dispatch_message_queue(&mut root, up.messages);
        assert!(routed.handled);
    }

    struct ScrollReceiver {
        id: WidgetId,
        child: Box<dyn Widget>,
        seen: usize,
    }

    impl ScrollReceiver {
        fn new(child: impl Widget + 'static) -> Self {
            Self {
                id: WidgetId::new(),
                child: Box::new(child),
                seen: 0,
            }
        }
    }

    impl Widget for ScrollReceiver {
        fn id(&self) -> WidgetId {
            self.id
        }
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }
        fn on_mouse_scroll(&mut self, _delta_x: i32, _delta_y: i32, ctx: &mut EventCtx) {
            self.seen += 1;
            ctx.set_handled();
        }
        fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
            f(self.child.as_mut());
        }
    }

    #[test]
    fn mouse_scroll_bubbles_to_ancestor_handlers() {
        let button = Button::new("x");
        let button_id = button.id();
        let mut root = ScrollReceiver::new(button);

        let outcome = dispatch_mouse_scroll_to_target(&mut root, button_id, 0, 1);
        assert!(outcome.handled);
        assert_eq!(root.seen, 1);
    }

    struct ScrollSink {
        id: WidgetId,
        focused: bool,
        hits: Arc<AtomicUsize>,
    }

    impl ScrollSink {
        fn new(focused: bool, hits: Arc<AtomicUsize>) -> Self {
            Self {
                id: WidgetId::new(),
                focused,
                hits,
            }
        }
    }

    impl Widget for ScrollSink {
        fn id(&self) -> WidgetId {
            self.id
        }

        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }

        fn focusable(&self) -> bool {
            true
        }

        fn set_focus(&mut self, focused: bool) {
            self.focused = focused;
        }

        fn has_focus(&self) -> bool {
            self.focused
        }

        fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
            if matches!(event, Event::Action(Action::ScrollDown)) {
                self.hits.fetch_add(1, Ordering::Relaxed);
                ctx.set_handled();
            }
        }
    }

    #[test]
    fn scroll_actions_prefer_focused_target() {
        let first_hits = Arc::new(AtomicUsize::new(0));
        let second_hits = Arc::new(AtomicUsize::new(0));

        let first = ScrollSink::new(false, first_hits.clone());
        let second = ScrollSink::new(true, second_hits.clone());
        let mut root = AppRoot::new().with_child(first).with_child(second);

        let outcome = dispatch_scroll_action(&mut root, Action::ScrollDown, None);
        assert!(outcome.handled);
        assert_eq!(first_hits.load(Ordering::Relaxed), 0);
        assert_eq!(second_hits.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn scroll_actions_fallback_to_hovered_when_unfocused() {
        let first_hits = Arc::new(AtomicUsize::new(0));
        let second_hits = Arc::new(AtomicUsize::new(0));

        let first = ScrollSink::new(false, first_hits.clone());
        let second = ScrollSink::new(false, second_hits.clone());
        let second_id = second.id();
        let mut root = AppRoot::new().with_child(first).with_child(second);

        let outcome = dispatch_scroll_action(&mut root, Action::ScrollDown, Some(second_id));
        assert!(outcome.handled);
        assert_eq!(first_hits.load(Ordering::Relaxed), 0);
        assert_eq!(second_hits.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn scroll_actions_fallback_to_global_when_no_target_handles() {
        let first_hits = Arc::new(AtomicUsize::new(0));
        let second_hits = Arc::new(AtomicUsize::new(0));

        let first = ScrollSink::new(false, first_hits.clone());
        let second = ScrollSink::new(false, second_hits.clone());
        let mut root = AppRoot::new().with_child(first).with_child(second);

        let outcome = dispatch_scroll_action(&mut root, Action::ScrollDown, None);
        assert!(outcome.handled);
        assert_eq!(first_hits.load(Ordering::Relaxed), 1);
        assert_eq!(second_hits.load(Ordering::Relaxed), 0);
    }
}
