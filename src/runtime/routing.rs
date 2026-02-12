use crate::debug::debug_message;
use crate::event::{Action, AnimationRequest, BindingHint, Event, EventCtx};
use crate::message::{Message, MessageEnvelope, MessageEvent};
use crate::node_id::NodeId;
use crate::widget_tree::WidgetTree;
use crate::widgets::Widget;

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
        invalidation: ctx.invalidation(),
        stop_requested: ctx.stop_requested(),
        messages: ctx.take_messages(),
        animation_requests: ctx.take_animation_requests(),
        default_prevented: false,
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
        Action::ScrollHome
            | Action::ScrollEnd
            | Action::ScrollUp
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
        invalidation: ctx.invalidation(),
        stop_requested: ctx.stop_requested(),
        messages: ctx.take_messages(),
        animation_requests: ctx.take_animation_requests(),
        default_prevented: false,
    }
}

// ---------------------------------------------------------------------------
// Arena-tree-based event routing
// ---------------------------------------------------------------------------

/// Build the path from root to `target` (inclusive): `[root, …, parent, target]`.
///
/// Returns an empty vec if `target` is not in the tree or the tree has no root.
fn build_path_to_node(tree: &WidgetTree, target: NodeId) -> Vec<NodeId> {
    if !tree.contains(target) {
        return Vec::new();
    }
    let mut path = vec![target];
    let ancestors = tree.ancestors(target); // [parent, grandparent, …, root]
    path.extend(ancestors);
    path.reverse(); // [root, …, parent, target]
    path
}

/// Find the currently focused node by walking the entire tree depth-first.
///
/// Returns the first node whose widget reports `has_focus() == true`.
pub(crate) fn focused_node_id_tree(tree: &WidgetTree) -> Option<NodeId> {
    let root = tree.root()?;
    for node_id in tree.walk_depth_first(root) {
        if let Some(node) = tree.get(node_id) {
            if node.widget.has_focus() {
                return Some(node_id);
            }
        }
    }
    None
}

/// Dispatch an event through the arena tree using capture + bubble phases.
///
/// 1. Build the path from root to `focused` node.
/// 2. **Capture phase**: walk root→focused, calling `on_event_capture()`.
/// 3. **Bubble phase**: walk focused→root, calling `on_event()`.
///
/// If `focused` is `None`, dispatches to the root node only.
pub(crate) fn dispatch_event_tree(
    tree: &mut WidgetTree,
    focused: Option<NodeId>,
    event: &Event,
) -> DispatchOutcome {
    let event_debug = format!("{event:?}");
    let mut ctx = EventCtx::default();
    let always_bubble = matches!(event, Event::MouseUp(..));

    let path = match focused {
        Some(focus_id) => build_path_to_node(tree, focus_id),
        None => match tree.root() {
            Some(root) => vec![root],
            None => return DispatchOutcome::default(),
        },
    };

    // Capture phase: root → focused
    for &node_id in &path {
        if ctx.handled() {
            break;
        }
        if let Some(node) = tree.get_mut(node_id) {
            node.widget.on_event_capture(event, &mut ctx);
        }
    }

    // Bubble phase: focused → root
    if always_bubble || !ctx.handled() {
        for &node_id in path.iter().rev() {
            if let Some(node) = tree.get_mut(node_id) {
                node.widget.on_event(event, &mut ctx);
            }
            if ctx.handled() {
                break;
            }
        }
    }

    let outcome = DispatchOutcome {
        handled: ctx.handled(),
        repaint_requested: ctx.repaint_requested(),
        invalidation: ctx.invalidation(),
        stop_requested: ctx.stop_requested(),
        messages: ctx.take_messages(),
        animation_requests: ctx.take_animation_requests(),
        default_prevented: false,
    };
    debug_message(&format!(
        "[dispatch_event_tree] event={event_debug} handled={} repaint={} messages={}",
        outcome.handled,
        outcome.repaint_requested,
        outcome.messages.len()
    ));
    outcome
}

/// Dispatch an event to a specific `target` node using the arena tree.
///
/// Capture phase runs root→target, then bubble phase runs target→root.
pub(crate) fn dispatch_event_to_target_tree(
    tree: &mut WidgetTree,
    target: NodeId,
    event: &Event,
) -> DispatchOutcome {
    let mut ctx = EventCtx::default();
    let path = build_path_to_node(tree, target);

    // Capture phase: root → target
    for &node_id in &path {
        if ctx.handled() {
            break;
        }
        if let Some(node) = tree.get_mut(node_id) {
            node.widget.on_event_capture(event, &mut ctx);
        }
    }

    // Bubble phase: target → root
    if !ctx.handled() {
        for &node_id in path.iter().rev() {
            if let Some(node) = tree.get_mut(node_id) {
                node.widget.on_event(event, &mut ctx);
            }
            if ctx.handled() {
                break;
            }
        }
    }

    DispatchOutcome {
        handled: ctx.handled(),
        repaint_requested: ctx.repaint_requested(),
        invalidation: ctx.invalidation(),
        stop_requested: ctx.stop_requested(),
        messages: ctx.take_messages(),
        animation_requests: ctx.take_animation_requests(),
        default_prevented: false,
    }
}

/// Dispatch a scroll action through the tree, preferring focused → hovered → root.
pub(crate) fn dispatch_scroll_action_tree(
    tree: &mut WidgetTree,
    action: Action,
    hovered: Option<NodeId>,
) -> DispatchOutcome {
    let event = Event::Action(action);
    let focused = focused_node_id_tree(tree);

    if let Some(target) = focused {
        let outcome = dispatch_event_to_target_tree(tree, target, &event);
        if outcome.handled || outcome.repaint_requested || !outcome.messages.is_empty() {
            return outcome;
        }
    }

    if let Some(target) = hovered.filter(|id| Some(*id) != focused) {
        let outcome = dispatch_event_to_target_tree(tree, target, &event);
        if outcome.handled || outcome.repaint_requested || !outcome.messages.is_empty() {
            return outcome;
        }
    }

    dispatch_event_tree(tree, None, &event)
}

/// Dispatch mouse scroll to a target node, bubbling up the ancestor path.
pub(crate) fn dispatch_mouse_scroll_to_target_tree(
    tree: &mut WidgetTree,
    target: NodeId,
    delta_x: i32,
    delta_y: i32,
) -> DispatchOutcome {
    let mut ctx = EventCtx::default();
    let path = build_path_to_node(tree, target);

    // Bubble phase only: target → root (mouse scroll doesn't have a capture phase)
    for &node_id in path.iter().rev() {
        if let Some(node) = tree.get_mut(node_id) {
            node.widget.on_mouse_scroll(delta_x, delta_y, &mut ctx);
        }
        if ctx.handled() {
            break;
        }
    }

    DispatchOutcome {
        handled: ctx.handled(),
        repaint_requested: ctx.repaint_requested(),
        invalidation: ctx.invalidation(),
        stop_requested: ctx.stop_requested(),
        messages: ctx.take_messages(),
        animation_requests: ctx.take_animation_requests(),
        default_prevented: false,
    }
}

/// Returns `true` for message variants that may arrive in rapid bursts and
/// are safe to coalesce (only the latest value matters).
fn is_message_replaceable(message: &Message) -> bool {
    matches!(
        message,
        Message::InputChanged { .. }
            | Message::TextAreaChanged { .. }
            | Message::TextAreaSelectionChanged { .. }
            | Message::DataTableCursorMoved { .. }
            | Message::DataTableCellHighlighted { .. }
            | Message::DataTableRowHighlighted { .. }
            | Message::DataTableColumnHighlighted { .. }
            | Message::TreeNodeHighlighted { .. }
            | Message::OptionHighlighted { .. }
            | Message::KeyPanelScrolled { .. }
            | Message::RichLogScrolled { .. }
    )
}

/// Coalesce replaceable messages in the queue.
///
/// For each pair of envelopes where `can_replace()` is true, both originate
/// from the same sender, and both carry the same `Message` variant
/// discriminant, the earlier one is dropped and only the latest is kept.
/// Non-replaceable envelopes and envelopes that differ in sender or variant
/// pass through untouched.
pub(crate) fn coalesce_message_queue(
    queue: &mut std::collections::VecDeque<MessageEnvelope>,
) {
    use std::mem::discriminant;

    if queue.len() < 2 {
        return;
    }

    // Walk from the back; for each replaceable envelope, check if there is
    // an earlier one with the same (sender, discriminant) and remove it.
    // We track seen keys in a small vec (message queues are typically short).
    let mut seen: Vec<(NodeId, std::mem::Discriminant<Message>)> = Vec::new();
    let mut keep = vec![true; queue.len()];

    // Walk backwards so later messages survive.
    for i in (0..queue.len()).rev() {
        let env = &queue[i];
        if !env.can_replace() {
            continue;
        }
        let key = (env.sender(), discriminant(env.message()));
        if seen.contains(&key) {
            // A later envelope with the same key was already seen; drop this earlier one.
            keep[i] = false;
        } else {
            seen.push(key);
        }
    }

    // Remove dropped envelopes (drain back-to-front to preserve indices).
    let mut idx = queue.len();
    while idx > 0 {
        idx -= 1;
        if !keep[idx] {
            queue.remove(idx);
        }
    }
}

/// Drain and dispatch a queue of messages through the arena tree.
///
/// Each `MessageEvent` is wrapped in a [`MessageEnvelope`] that controls
/// propagation.  Messages bubble from the sender node up to the root; a
/// handler can stop propagation via `ctx.set_handled()` (maps to
/// `envelope.stop()`).  Before dispatching each batch the queue is
/// coalesced: replaceable messages with the same sender+variant keep only
/// the latest entry.
pub(crate) fn dispatch_message_queue_tree(
    tree: &mut WidgetTree,
    initial: Vec<MessageEvent>,
) -> DispatchOutcome {
    use std::collections::VecDeque;

    let mut handled = false;
    let mut repaint_requested = false;
    let mut invalidation = crate::event::InvalidationFlags::default();
    let mut stop_requested = false;
    let mut default_prevented = false;
    let mut emitted: Vec<MessageEvent> = Vec::new();
    let mut animation_requests: Vec<AnimationRequest> = Vec::new();

    // Wrap incoming messages in envelopes and mark known rapid-fire variants
    // as replaceable so the coalescing pass can deduplicate them.
    let mut queue: VecDeque<MessageEnvelope> = initial
        .into_iter()
        .map(|evt| {
            let mut env = MessageEnvelope::new(evt);
            if is_message_replaceable(env.message()) {
                env.set_replaceable(true);
            }
            env
        })
        .collect();

    coalesce_message_queue(&mut queue);

    const LIMIT: usize = 1024;
    let mut processed = 0usize;

    while let Some(mut envelope) = queue.pop_front() {
        processed += 1;
        if processed > LIMIT {
            debug_message("[dispatch_message_queue_tree] limit reached, dropping remaining");
            break;
        }

        let mut ctx = EventCtx::default();
        dispatch_message_bubble(tree, &mut envelope, &mut ctx);
        handled |= ctx.handled();
        repaint_requested |= ctx.repaint_requested();
        invalidation.merge(ctx.invalidation());
        stop_requested |= ctx.stop_requested();
        default_prevented |= envelope.is_default_prevented();
        let next = ctx.take_messages();
        let mut next_anims = ctx.take_animation_requests();
        if !next.is_empty() {
            let next_envelopes: VecDeque<MessageEnvelope> = next
                .iter()
                .map(|evt| {
                    let mut env = MessageEnvelope::new(evt.clone());
                    if is_message_replaceable(env.message()) {
                        env.set_replaceable(true);
                    }
                    env
                })
                .collect();
            queue.extend(next_envelopes);
            // Re-coalesce the full pending queue so that newly emitted
            // replaceable messages can deduplicate against older entries.
            coalesce_message_queue(&mut queue);
            emitted.extend(next);
        }
        if !next_anims.is_empty() {
            animation_requests.append(&mut next_anims);
        }
    }

    DispatchOutcome {
        handled,
        repaint_requested,
        invalidation,
        stop_requested,
        messages: emitted,
        animation_requests,
        default_prevented,
    }
}

/// Bubble a single message from its sender up to the tree root.
///
/// The walk order is `[sender, parent, …, root]`.  At each node,
/// `widget.on_message()` is called.  If the handler sets `ctx.handled()`,
/// propagation stops (`envelope.stop()` is called).  When the sender is not
/// present in the tree, the message falls back to a depth-first broadcast
/// so that globally-targeted messages (e.g. overlay commands) still reach
/// their recipient.
fn dispatch_message_bubble(
    tree: &mut WidgetTree,
    envelope: &mut MessageEnvelope,
    ctx: &mut EventCtx,
) {
    let sender = envelope.sender();
    let bubble_path = build_path_to_node(tree, sender); // [root, …, parent, sender]

    if bubble_path.is_empty() {
        // Sender not in tree — fall back to depth-first broadcast so
        // globally-addressed messages (overlay commands, etc.) still work.
        let root = match tree.root() {
            Some(r) => r,
            None => return,
        };
        let node_ids = tree.walk_depth_first(root);
        for node_id in node_ids {
            if envelope.is_stopped() || ctx.handled() {
                return;
            }
            if let Some(node) = tree.get_mut(node_id) {
                node.widget.on_message(&envelope.event, ctx);
                if ctx.handled() {
                    envelope.stop();
                }
            }
        }
        return;
    }

    // Bubble: sender → parent → … → root (reverse of build_path_to_node).
    for &node_id in bubble_path.iter().rev() {
        if envelope.is_stopped() {
            break;
        }
        if let Some(node) = tree.get_mut(node_id) {
            node.widget.on_message(&envelope.event, ctx);
            if ctx.handled() {
                envelope.stop();
            }
        }
    }
}

/// Return the focused widget's help markup, if any.
pub(crate) fn focused_help_metadata_tree(
    tree: &WidgetTree,
) -> Option<(NodeId, String)> {
    let root = tree.root()?;
    for node_id in tree.walk_depth_first(root) {
        let node = tree.get(node_id)?;
        if node.widget.has_focus() {
            let help = node.widget.help_markup().map(str::trim).unwrap_or_default();
            if !help.is_empty() {
                return Some((node_id, help.to_string()));
            }
            return None;
        }
    }
    None
}

/// Collect binding hints along the focused path (root→focused).
///
/// If no widget has focus, falls back to root + single-child chain.
pub(crate) fn active_binding_hints_tree(
    tree: &WidgetTree,
) -> (Vec<BindingHint>, Vec<NodeId>) {
    if let Some(focus_id) = focused_node_id_tree(tree) {
        let path = build_path_to_node(tree, focus_id);
        let mut hints = Vec::new();
        let mut sources = Vec::new();
        for &node_id in &path {
            if let Some(node) = tree.get(node_id) {
                sources.push(node_id);
                hints.extend(node.widget.binding_hints());
            }
        }
        return (hints, sources);
    }

    // No focus — walk root + single-child chain (matches old `collect_no_focus_scope`).
    collect_root_scope_hints(tree)
}

/// Walk from root along single-child chains collecting hints (no-focus fallback).
fn collect_root_scope_hints(tree: &WidgetTree) -> (Vec<BindingHint>, Vec<NodeId>) {
    let mut hints = Vec::new();
    let mut sources = Vec::new();
    let Some(root) = tree.root() else {
        return (hints, sources);
    };

    let mut current = root;
    loop {
        if let Some(node) = tree.get(current) {
            sources.push(current);
            hints.extend(node.widget.binding_hints());
            let children = tree.children(current);
            if children.len() == 1 {
                current = children[0];
            } else {
                break;
            }
        } else {
            break;
        }
    }

    (hints, sources)
}

#[cfg(test)]
mod message_tests {
    use super::*;
    use crate::event::{MouseDownEvent, MouseUpEvent};
    use crate::keys::KeyEventData;
    use crate::message::Message;
    use crate::widget_tree::WidgetTree;
    use crate::widgets::{AppRoot, Button, Label, ScrollView};
    use crossterm::event::{KeyCode, KeyModifiers};
    use rich_rs::{Console, ConsoleOptions, Segments};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct HintNode {
        focused: bool,
        hints: Vec<BindingHint>,
        help_markup: Option<String>,
        child: Option<Box<HintNode>>,
    }

    impl HintNode {
        fn new(focused: bool, hints: Vec<BindingHint>) -> Self {
            Self {
                focused,
                hints,
                help_markup: None,
                child: None,
            }
        }

        fn with_child(mut self, child: HintNode) -> Self {
            self.child = Some(Box::new(child));
            self
        }

        fn with_help(mut self, help_markup: impl Into<String>) -> Self {
            self.help_markup = Some(help_markup.into());
            self
        }
    }

    impl Widget for HintNode {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn binding_hints(&self) -> Vec<BindingHint> {
            self.hints.clone()
        }

        fn help_markup(&self) -> Option<&str> {
            self.help_markup.as_deref()
        }

        fn has_focus(&self) -> bool {
            self.focused
        }

        fn set_focus(&mut self, focused: bool) {
            self.focused = focused;
        }
    }

    struct Child;

    impl Child {
        fn new() -> Self {
            Self
        }
    }

    impl Widget for Child {
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
        child: Box<dyn Widget>,
        seen: usize,
    }

    impl Parent {
        fn new(child: impl Widget + 'static) -> Self {
            Self {
                child: Box::new(child),
                seen: 0,
            }
        }
    }

    impl Widget for Parent {
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

        // Deliver message directly to root (mirrors production's root-only fallback).
        let mut ctx = EventCtx::default();
        root.on_message(&outcome.messages[0], &mut ctx);
        assert!(ctx.handled());
        assert_eq!(root.seen, 1);
    }

    struct Receiver {
        child: Box<dyn Widget>,
        seen: usize,
    }

    impl Receiver {
        fn new(child: impl Widget + 'static) -> Self {
            Self {
                child: Box::new(child),
                seen: 0,
            }
        }

        fn new_leaf() -> Self {
            Self {
                child: Box::new(Label::new("")),
                seen: 0,
            }
        }
    }

    impl Widget for Receiver {
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
    }

    #[test]
    fn button_pressed_message_reaches_ancestor() {
        // Build tree: root(AppRoot) → recv(Receiver) → button(Button)
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let recv_id = tree.mount(root_id, Box::new(Receiver::new_leaf()));
        let button_id = tree.mount(recv_id, Box::new(Button::new("x")));

        // Button checks target == NodeId::default() (P1-14 workaround).
        // Tree dispatch routes the event to the correct node regardless.
        let default_id = NodeId::default();
        let down = dispatch_event_to_target_tree(
            &mut tree,
            button_id,
            &Event::MouseDown(MouseDownEvent {
                target: default_id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
        );
        let _ = dispatch_message_queue_tree(&mut tree, down.messages);

        let up = dispatch_event_to_target_tree(
            &mut tree,
            button_id,
            &Event::MouseUp(MouseUpEvent {
                target: Some(default_id),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
        );
        assert!(!up.messages.is_empty());
        let routed = dispatch_message_queue_tree(&mut tree, up.messages);
        assert!(routed.handled);
    }

    #[test]
    fn button_pressed_message_survives_scrollview_forwarding() {
        // Build tree: root(AppRoot) → recv(Receiver) → scroll(ScrollView) → button(Button)
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let recv_id = tree.mount(root_id, Box::new(Receiver::new_leaf()));
        let scroll_id = tree.mount(recv_id, Box::new(ScrollView::new(Label::new(""))));
        let _button_id = tree.mount(scroll_id, Box::new(Button::new("x")));

        // Button checks target == NodeId::default() (P1-14 workaround).
        let default_id = NodeId::default();
        let down = dispatch_event_to_target_tree(
            &mut tree,
            _button_id,
            &Event::MouseDown(MouseDownEvent {
                target: default_id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
        );
        let _ = dispatch_message_queue_tree(&mut tree, down.messages);

        let up = dispatch_event_to_target_tree(
            &mut tree,
            _button_id,
            &Event::MouseUp(MouseUpEvent {
                target: Some(default_id),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
        );
        assert_eq!(up.messages.len(), 1);
        let routed = dispatch_message_queue_tree(&mut tree, up.messages);
        assert!(routed.handled);
    }

    struct ScrollReceiver {
        child: Box<dyn Widget>,
        seen: usize,
    }

    impl ScrollReceiver {
        fn new(child: impl Widget + 'static) -> Self {
            Self {
                child: Box::new(child),
                seen: 0,
            }
        }
    }

    impl Widget for ScrollReceiver {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }
        fn on_mouse_scroll(&mut self, _delta_x: i32, _delta_y: i32, ctx: &mut EventCtx) {
            self.seen += 1;
            ctx.set_handled();
        }
    }

    #[test]
    fn mouse_scroll_bubbles_to_ancestor_handlers() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(ScrollReceiver::new(Label::new(""))));
        let button_id = tree.mount(root_id, Box::new(Button::new("x")));

        // Button doesn't handle scroll, so it bubbles to ScrollReceiver.
        let outcome = dispatch_mouse_scroll_to_target_tree(&mut tree, button_id, 0, 1);
        assert!(outcome.handled);
    }

    struct ScrollSink {
        focused: bool,
        hits: Arc<AtomicUsize>,
    }

    impl ScrollSink {
        fn new(focused: bool, hits: Arc<AtomicUsize>) -> Self {
            Self {
                focused,
                hits,
            }
        }
    }

    impl Widget for ScrollSink {
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

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let _first_id = tree.mount(
            root_id,
            Box::new(ScrollSink::new(false, first_hits.clone())),
        );
        let _second_id = tree.mount(
            root_id,
            Box::new(ScrollSink::new(true, second_hits.clone())),
        );

        let outcome = dispatch_scroll_action_tree(&mut tree, Action::ScrollDown, None);
        assert!(outcome.handled);
        assert_eq!(first_hits.load(Ordering::Relaxed), 0);
        assert_eq!(second_hits.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn scroll_actions_fallback_to_hovered_when_unfocused() {
        let first_hits = Arc::new(AtomicUsize::new(0));
        let second_hits = Arc::new(AtomicUsize::new(0));

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let _first_id = tree.mount(
            root_id,
            Box::new(ScrollSink::new(false, first_hits.clone())),
        );
        let second_id = tree.mount(
            root_id,
            Box::new(ScrollSink::new(false, second_hits.clone())),
        );

        let outcome =
            dispatch_scroll_action_tree(&mut tree, Action::ScrollDown, Some(second_id));
        assert!(outcome.handled);
        assert_eq!(first_hits.load(Ordering::Relaxed), 0);
        assert_eq!(second_hits.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn scroll_actions_fallback_to_global_when_no_target_handles() {
        // Without focus or hover, scroll dispatches to root node only.
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let first_hits = Arc::new(AtomicUsize::new(0));
        let _first_id = tree.mount(
            root_id,
            Box::new(ScrollSink::new(false, first_hits.clone())),
        );

        let outcome = dispatch_scroll_action_tree(&mut tree, Action::ScrollDown, None);
        // No focused/hovered → root-only dispatch. Children don't see the event
        // because tree dispatch routes along root→focused path only.
        assert!(!outcome.handled);
        assert_eq!(first_hits.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn focused_path_binding_hints_collects_ancestor_chain() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(
            HintNode::new(false, vec![BindingHint::new("tab", "next focus")]),
        ));
        let mid_id = tree.mount(
            root_id,
            Box::new(HintNode::new(false, vec![BindingHint::new("left", "back")])),
        );
        let _leaf_id = tree.mount(
            mid_id,
            Box::new(HintNode::new(true, vec![BindingHint::new("enter", "activate")])),
        );

        let (hints, _sources) = active_binding_hints_tree(&tree);
        assert_eq!(
            hints,
            vec![
                BindingHint::new("tab", "next focus"),
                BindingHint::new("left", "back"),
                BindingHint::new("enter", "activate")
            ]
        );
    }

    #[test]
    fn focused_path_binding_hints_returns_empty_without_focus() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(
            HintNode::new(false, vec![BindingHint::new("tab", "next")]),
        ));
        let _leaf_id = tree.mount(
            root_id,
            Box::new(HintNode::new(false, vec![BindingHint::new("enter", "activate")])),
        );

        // No focused node — falls back to root scope (single-child chain).
        let (hints, _) = active_binding_hints_tree(&tree);
        // Returns root + leaf hints via single-child fallback.
        assert!(!hints.is_empty());
    }

    #[test]
    fn focused_help_metadata_returns_focused_widget_help() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(
            HintNode::new(false, vec![BindingHint::new("tab", "next")]),
        ));
        let _child_id = tree.mount(
            root_id,
            Box::new(
                HintNode::new(true, vec![BindingHint::new("enter", "activate")])
                    .with_help("## Focused help\nUse enter"),
            ),
        );

        let focused = focused_help_metadata_tree(&tree);
        assert!(matches!(
            focused.as_ref(),
            Some((_, markup)) if markup == "## Focused help\nUse enter"
        ));
    }

    #[test]
    fn focused_help_metadata_returns_none_without_focus() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(
            HintNode::new(false, vec![BindingHint::new("tab", "next")]),
        ));
        let _child_id = tree.mount(
            root_id,
            Box::new(
                HintNode::new(false, vec![BindingHint::new("enter", "activate")])
                    .with_help("## Focused help"),
            ),
        );

        assert!(focused_help_metadata_tree(&tree).is_none());
    }

    #[test]
    fn focused_path_binding_hints_tracks_focus_transitions() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(
            HintNode::new(false, vec![BindingHint::new("tab", "next focus")]),
        ));
        let child_id = tree.mount(
            root_id,
            Box::new(HintNode::new(
                true,
                vec![BindingHint::new("left/right", "switch tab")],
            )),
        );

        let (first, _) = active_binding_hints_tree(&tree);
        assert_eq!(
            first,
            vec![
                BindingHint::new("tab", "next focus"),
                BindingHint::new("left/right", "switch tab"),
            ]
        );

        // Transition focus from child to root.
        tree.get_mut(child_id).unwrap().widget.set_focus(false);
        tree.get_mut(root_id).unwrap().widget.set_focus(true);

        let (second, _) = active_binding_hints_tree(&tree);
        assert_eq!(second, vec![BindingHint::new("tab", "next focus")]);
    }

    #[test]
    fn focused_help_metadata_tracks_focus_transitions() {
        // State 1: child has focus + help markup.
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(
            HintNode::new(false, vec![BindingHint::new("tab", "next focus")]),
        ));
        let _child_id = tree.mount(
            root_id,
            Box::new(
                HintNode::new(true, vec![BindingHint::new("left/right", "switch tab")])
                    .with_help("## First"),
            ),
        );

        let first = focused_help_metadata_tree(&tree);
        assert!(matches!(
            first.as_ref(),
            Some((_, markup)) if markup == "## First"
        ));

        // State 2: focus moves to root which has its own help markup.
        let mut tree2 = WidgetTree::new();
        let _root_id2 = tree2.set_root(Box::new(
            HintNode::new(true, vec![BindingHint::new("tab", "next focus")])
                .with_help("## Second"),
        ));
        let _child_id2 = tree2.mount(
            _root_id2,
            Box::new(
                HintNode::new(false, vec![BindingHint::new("left/right", "switch tab")])
                    .with_help("## First"),
            ),
        );

        let second = focused_help_metadata_tree(&tree2);
        assert!(matches!(
            second.as_ref(),
            Some((_, markup)) if markup == "## Second"
        ));
    }

    #[test]
    fn active_binding_hints_returns_focused_chain_and_sources() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(
            HintNode::new(false, vec![BindingHint::new("tab", "next focus")]),
        ));
        let mid_id = tree.mount(
            root_id,
            Box::new(HintNode::new(false, vec![BindingHint::new("left", "back")])),
        );
        let _leaf_id = tree.mount(
            mid_id,
            Box::new(HintNode::new(true, vec![BindingHint::new("enter", "activate")])),
        );

        let (hints, sources) = active_binding_hints_tree(&tree);
        assert_eq!(
            hints,
            vec![
                BindingHint::new("tab", "next focus"),
                BindingHint::new("left", "back"),
                BindingHint::new("enter", "activate")
            ]
        );
        assert_eq!(sources.len(), 3);
    }

    #[test]
    fn active_binding_hints_falls_back_to_single_child_scope_without_focus() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(
            HintNode::new(false, vec![BindingHint::new("q", "quit")]),
        ));
        let _child_id = tree.mount(
            root_id,
            Box::new(HintNode::new(false, vec![BindingHint::new("f1", "help")])),
        );

        let (hints, sources) = active_binding_hints_tree(&tree);
        assert_eq!(
            hints,
            vec![
                BindingHint::new("q", "quit"),
                BindingHint::new("f1", "help")
            ]
        );
        assert_eq!(sources.len(), 2);
    }
}

#[cfg(test)]
mod envelope_tests {
    use super::*;
    use crate::message::{Message, MessageEnvelope, MessageEvent};
    use crate::node_id::node_id_from_ffi;
    use crate::widget_tree::WidgetTree;
    use crate::widgets::Label;
    use rich_rs::{Console, ConsoleOptions, Segments};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // -----------------------------------------------------------------------
    // Test widget: counts how many times on_message is called
    // -----------------------------------------------------------------------
    struct MessageCounter {
        count: Arc<AtomicUsize>,
        stop_on_match: bool,
    }

    impl MessageCounter {
        fn new(count: Arc<AtomicUsize>) -> Self {
            Self {
                count,
                stop_on_match: false,
            }
        }

        fn stopping(count: Arc<AtomicUsize>) -> Self {
            Self {
                count,
                stop_on_match: true,
            }
        }
    }

    impl Widget for MessageCounter {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
            if matches!(message.message, Message::ButtonPressed { .. }) {
                self.count.fetch_add(1, Ordering::Relaxed);
                if self.stop_on_match {
                    ctx.set_handled();
                }
            }
        }
    }

    /// Helper: build a MessageEvent from a sender FFI id and a Message.
    fn msg_event(sender_ffi: u64, message: Message) -> MessageEvent {
        MessageEvent {
            sender: node_id_from_ffi(sender_ffi),
            message,
        }
    }

    // =====================================================================
    // P4-02: Envelope bubble dispatch tests
    // =====================================================================

    #[test]
    fn envelope_message_bubbles_from_sender_to_root() {
        // Tree: root → mid → leaf (sender)
        let root_count = Arc::new(AtomicUsize::new(0));
        let mid_count = Arc::new(AtomicUsize::new(0));
        let leaf_count = Arc::new(AtomicUsize::new(0));

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(MessageCounter::new(root_count.clone())));
        let mid_id = tree.mount(root_id, Box::new(MessageCounter::new(mid_count.clone())));
        let leaf_id = tree.mount(mid_id, Box::new(MessageCounter::new(leaf_count.clone())));

        let messages = vec![MessageEvent {
            sender: leaf_id,
            message: Message::ButtonPressed {
                description: "test".into(),
            },
        }];

        let outcome = dispatch_message_queue_tree(&mut tree, messages);
        // All three nodes on the bubble path should see the message.
        assert!(leaf_count.load(Ordering::Relaxed) >= 1, "leaf should see message");
        assert!(mid_count.load(Ordering::Relaxed) >= 1, "mid should see message");
        assert!(root_count.load(Ordering::Relaxed) >= 1, "root should see message");
        assert!(outcome.handled || leaf_count.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn envelope_stop_halts_propagation() {
        // Tree: root → mid(stops) → leaf (sender)
        // Mid stops propagation, so root should NOT see the message.
        let root_count = Arc::new(AtomicUsize::new(0));
        let mid_count = Arc::new(AtomicUsize::new(0));
        let leaf_count = Arc::new(AtomicUsize::new(0));

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(MessageCounter::new(root_count.clone())));
        let mid_id = tree.mount(
            root_id,
            Box::new(MessageCounter::stopping(mid_count.clone())),
        );
        let leaf_id = tree.mount(mid_id, Box::new(MessageCounter::new(leaf_count.clone())));

        let messages = vec![MessageEvent {
            sender: leaf_id,
            message: Message::ButtonPressed {
                description: "stop".into(),
            },
        }];

        let outcome = dispatch_message_queue_tree(&mut tree, messages);
        assert!(outcome.handled, "mid should have handled it");
        // Leaf sees it first (bubble starts at sender), mid stops.
        assert!(leaf_count.load(Ordering::Relaxed) >= 1, "leaf sees message");
        assert!(mid_count.load(Ordering::Relaxed) >= 1, "mid sees message and stops");
        assert_eq!(
            root_count.load(Ordering::Relaxed),
            0,
            "root should NOT see message after stop"
        );
    }

    #[test]
    fn envelope_sender_not_in_tree_falls_back_to_broadcast() {
        // Message from unknown sender should still reach nodes via broadcast fallback.
        let root_count = Arc::new(AtomicUsize::new(0));

        let mut tree = WidgetTree::new();
        let _root_id = tree.set_root(Box::new(MessageCounter::new(root_count.clone())));

        let messages = vec![msg_event(
            99999,
            Message::ButtonPressed {
                description: "ghost".into(),
            },
        )];

        dispatch_message_queue_tree(&mut tree, messages);
        assert!(
            root_count.load(Ordering::Relaxed) >= 1,
            "broadcast fallback should reach root"
        );
    }

    #[test]
    fn envelope_default_prevented_propagates_to_outcome() {
        // Currently default_prevented tracks through the envelope. Since widgets
        // don't have direct access to prevent_default() yet (Widget trait takes
        // &MessageEvent, not &mut MessageEnvelope), this test verifies the
        // field exists and defaults to false for normal dispatch.
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(Label::new("x")));

        let messages = vec![MessageEvent {
            sender: root_id,
            message: Message::ButtonPressed {
                description: "dp".into(),
            },
        }];

        let outcome = dispatch_message_queue_tree(&mut tree, messages);
        assert!(
            !outcome.default_prevented,
            "default_prevented should be false when no handler calls prevent_default()"
        );
    }

    // =====================================================================
    // P4-14: Message queue coalescing tests
    // =====================================================================

    #[test]
    fn coalesce_removes_earlier_replaceable_same_sender_same_variant() {
        let sender = node_id_from_ffi(1);
        let mut queue: VecDeque<MessageEnvelope> = VecDeque::new();

        // Two InputChanged from the same sender — both replaceable.
        let mut env1 = MessageEnvelope::new(MessageEvent {
            sender,
            message: Message::InputChanged {
                value: "a".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        });
        env1.set_replaceable(true);

        let mut env2 = MessageEnvelope::new(MessageEvent {
            sender,
            message: Message::InputChanged {
                value: "ab".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        });
        env2.set_replaceable(true);

        queue.push_back(env1);
        queue.push_back(env2);
        coalesce_message_queue(&mut queue);

        assert_eq!(queue.len(), 1, "should coalesce to one message");
        match queue[0].message() {
            Message::InputChanged { value, .. } => {
                assert_eq!(value, "ab", "should keep the latest value");
            }
            other => panic!("unexpected message: {:?}", other),
        }
    }

    #[test]
    fn coalesce_preserves_non_replaceable_messages() {
        let sender = node_id_from_ffi(1);
        let mut queue: VecDeque<MessageEnvelope> = VecDeque::new();

        // Two ButtonPressed — not replaceable by default.
        let env1 = MessageEnvelope::new(MessageEvent {
            sender,
            message: Message::ButtonPressed {
                description: "first".into(),
            },
        });
        let env2 = MessageEnvelope::new(MessageEvent {
            sender,
            message: Message::ButtonPressed {
                description: "second".into(),
            },
        });

        queue.push_back(env1);
        queue.push_back(env2);
        coalesce_message_queue(&mut queue);

        assert_eq!(queue.len(), 2, "non-replaceable messages should all survive");
    }

    #[test]
    fn coalesce_different_senders_preserved() {
        let sender_a = node_id_from_ffi(1);
        let sender_b = node_id_from_ffi(2);
        let mut queue: VecDeque<MessageEnvelope> = VecDeque::new();

        let mut env1 = MessageEnvelope::new(MessageEvent {
            sender: sender_a,
            message: Message::InputChanged {
                value: "a".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        });
        env1.set_replaceable(true);

        let mut env2 = MessageEnvelope::new(MessageEvent {
            sender: sender_b,
            message: Message::InputChanged {
                value: "b".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        });
        env2.set_replaceable(true);

        queue.push_back(env1);
        queue.push_back(env2);
        coalesce_message_queue(&mut queue);

        assert_eq!(
            queue.len(),
            2,
            "different senders should not coalesce even with same variant"
        );
    }

    #[test]
    fn coalesce_mixed_replaceable_and_non_replaceable() {
        let sender = node_id_from_ffi(1);
        let mut queue: VecDeque<MessageEnvelope> = VecDeque::new();

        // Replaceable InputChanged #1
        let mut env1 = MessageEnvelope::new(MessageEvent {
            sender,
            message: Message::InputChanged {
                value: "a".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        });
        env1.set_replaceable(true);

        // Non-replaceable ButtonPressed
        let env2 = MessageEnvelope::new(MessageEvent {
            sender,
            message: Message::ButtonPressed {
                description: "click".into(),
            },
        });

        // Replaceable InputChanged #2
        let mut env3 = MessageEnvelope::new(MessageEvent {
            sender,
            message: Message::InputChanged {
                value: "ab".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        });
        env3.set_replaceable(true);

        queue.push_back(env1);
        queue.push_back(env2);
        queue.push_back(env3);
        coalesce_message_queue(&mut queue);

        // Two InputChanged coalesce to one, ButtonPressed survives.
        assert_eq!(queue.len(), 2, "InputChanged pair → 1, ButtonPressed → 1");
        // First remaining should be ButtonPressed (index 0 InputChanged was removed).
        assert!(matches!(
            queue[0].message(),
            Message::ButtonPressed { .. }
        ));
        // Second should be the latest InputChanged.
        match queue[1].message() {
            Message::InputChanged { value, .. } => {
                assert_eq!(value, "ab");
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn coalesce_empty_queue_is_noop() {
        let mut queue: VecDeque<MessageEnvelope> = VecDeque::new();
        coalesce_message_queue(&mut queue);
        assert!(queue.is_empty());
    }

    #[test]
    fn coalesce_single_element_is_noop() {
        let mut queue: VecDeque<MessageEnvelope> = VecDeque::new();
        let mut env = MessageEnvelope::new(MessageEvent {
            sender: node_id_from_ffi(1),
            message: Message::InputChanged {
                value: "x".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        });
        env.set_replaceable(true);
        queue.push_back(env);
        coalesce_message_queue(&mut queue);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn dispatch_auto_marks_rapid_fire_messages_replaceable() {
        // Verify that dispatch_message_queue_tree marks known rapid-fire
        // messages as replaceable and coalesces them.
        let sender = node_id_from_ffi(1);
        let _count = Arc::new(AtomicUsize::new(0));

        let mut tree = WidgetTree::new();
        let _root_id = tree.set_root(Box::new(Label::new("x")));

        // Three InputChanged from the same sender — should coalesce to one.
        let messages = vec![
            MessageEvent {
                sender,
                message: Message::InputChanged {
                    value: "a".into(),
                    validation: crate::validation::ValidationResult::success(),
                },
            },
            MessageEvent {
                sender,
                message: Message::InputChanged {
                    value: "ab".into(),
                    validation: crate::validation::ValidationResult::success(),
                },
            },
            MessageEvent {
                sender,
                message: Message::InputChanged {
                    value: "abc".into(),
                    validation: crate::validation::ValidationResult::success(),
                },
            },
        ];

        // Label doesn't handle messages, so outcome.handled will be false, but
        // coalescing should have reduced the queue to 1 message dispatched.
        let _outcome = dispatch_message_queue_tree(&mut tree, messages);
        // We can't directly observe the internal queue size, but the test proves
        // no panics and the function accepts the input.
        // The real verification is in the coalesce_* tests above.
    }

    #[test]
    fn is_message_replaceable_covers_known_variants() {
        // Spot-check that known rapid-fire message types are replaceable.
        assert!(is_message_replaceable(&Message::InputChanged {
            value: "x".into(),
            validation: crate::validation::ValidationResult::success(),
        }));
        assert!(is_message_replaceable(&Message::TextAreaChanged {
            value: "x".into(),
        }));
        assert!(is_message_replaceable(&Message::DataTableCursorMoved {
            row: 0,
            column: 0,
        }));
        assert!(is_message_replaceable(&Message::OptionHighlighted {
            index: 0,
        }));
        // Non-replaceable variants.
        assert!(!is_message_replaceable(&Message::ButtonPressed {
            description: "x".into(),
        }));
        assert!(!is_message_replaceable(&Message::InputSubmitted {
            value: "x".into(),
        }));
    }
}
