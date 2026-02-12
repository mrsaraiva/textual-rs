use crate::debug::debug_message;
use crate::event::{Action, AnimationRequest, BindingHint, Event, EventCtx};
use crate::message::MessageEvent;
use crate::node_id::{NodeId, node_id_from_ffi, node_id_to_ffi};
use crate::widget_tree::WidgetTree;
use crate::widgets::Widget;

use super::types::DispatchOutcome;

/// Legacy bridge: deprecated `Widget::id()` → `NodeId` for migration code.
#[allow(deprecated)]
fn widget_node_id(w: &dyn Widget) -> NodeId {
    node_id_from_ffi(w.id().as_u64())
}

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

pub(crate) fn focused_widget_id(root: &mut dyn Widget) -> Option<NodeId> {
    fn visit(widget: &mut dyn Widget, out: &mut Option<NodeId>) {
        if out.is_some() {
            return;
        }
        if widget.has_focus() {
            *out = Some(widget_node_id(widget));
            return;
        }
        #[allow(deprecated)]
        widget.visit_children_mut(&mut |child| visit(child, out));
    }

    let mut out = None;
    visit(root, &mut out);
    out
}

pub(crate) fn focused_help_metadata(root: &mut dyn Widget) -> Option<(NodeId, String)> {
    fn visit(widget: &mut dyn Widget, out: &mut Option<(NodeId, String)>) {
        if out.is_some() {
            return;
        }
        if widget.has_focus() {
            let help = widget.help_markup().map(str::trim).unwrap_or_default();
            if !help.is_empty() {
                *out = Some((widget_node_id(widget), help.to_string()));
            }
            return;
        }
        widget.visit_children_mut(&mut |child| visit(child, out));
    }

    let mut out = None;
    visit(root, &mut out);
    out
}

#[cfg(test)]
pub(crate) fn focused_path_binding_hints(root: &mut dyn Widget) -> Vec<BindingHint> {
    fn walk(widget: &mut dyn Widget, out: &mut Vec<BindingHint>) -> bool {
        let mut child_hints = Vec::new();
        let mut found_in_child = false;
        widget.visit_children_mut(&mut |child| {
            if found_in_child {
                return;
            }
            if walk(child, &mut child_hints) {
                found_in_child = true;
            }
        });
        if found_in_child {
            out.extend(widget.binding_hints());
            out.extend(child_hints);
            return true;
        }

        if widget.has_focus() {
            out.extend(widget.binding_hints());
            return true;
        }

        false
    }

    let mut out = Vec::new();
    let _ = walk(root, &mut out);
    out
}

pub(crate) fn active_binding_hints(root: &mut dyn Widget) -> (Vec<BindingHint>, Vec<NodeId>) {
    fn walk(
        widget: &mut dyn Widget,
        hints_out: &mut Vec<BindingHint>,
        sources_out: &mut Vec<NodeId>,
    ) -> bool {
        let mut child_hints = Vec::new();
        let mut child_sources = Vec::new();
        let mut found_in_child = false;
        widget.visit_children_mut(&mut |child| {
            if found_in_child {
                return;
            }
            if walk(child, &mut child_hints, &mut child_sources) {
                found_in_child = true;
            }
        });

        if found_in_child {
            sources_out.push(widget_node_id(widget));
            hints_out.extend(widget.binding_hints());
            sources_out.extend(child_sources);
            hints_out.extend(child_hints);
            return true;
        }

        if widget.has_focus() {
            sources_out.push(widget_node_id(widget));
            hints_out.extend(widget.binding_hints());
            return true;
        }

        false
    }

    fn collect_no_focus_scope(
        widget: &mut dyn Widget,
        hints_out: &mut Vec<BindingHint>,
        sources_out: &mut Vec<NodeId>,
    ) {
        sources_out.push(widget_node_id(widget));
        hints_out.extend(widget.binding_hints());

        let mut child_count = 0usize;
        widget.visit_children_mut(&mut |_| {
            child_count += 1;
        });
        if child_count != 1 {
            return;
        }

        let mut descended = false;
        widget.visit_children_mut(&mut |child| {
            if descended {
                return;
            }
            descended = true;
            collect_no_focus_scope(child, hints_out, sources_out);
        });
    }

    let mut hints = Vec::new();
    let mut sources = Vec::new();
    if walk(root, &mut hints, &mut sources) {
        return (hints, sources);
    }

    collect_no_focus_scope(root, &mut hints, &mut sources);
    (hints, sources)
}

pub(crate) fn dispatch_event_to_target(
    root: &mut dyn Widget,
    target: NodeId,
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
        node_id_to_ffi(target),
        handled,
        repaint_requested,
        messages.len()
    ));
    DispatchOutcome {
        handled,
        repaint_requested,
        invalidation: ctx.invalidation(),
        stop_requested: ctx.stop_requested(),
        messages,
        animation_requests,
    }
}

fn dispatch_event_bubble(
    widget: &mut dyn Widget,
    target: NodeId,
    event: &Event,
    ctx: &mut EventCtx,
) -> bool {
    if widget_node_id(widget) == target {
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
    hovered: Option<NodeId>,
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
        invalidation: ctx.invalidation(),
        stop_requested: ctx.stop_requested(),
        messages: ctx.take_messages(),
        animation_requests: ctx.take_animation_requests(),
    }
}

pub(crate) fn dispatch_mouse_scroll_to_target(
    root: &mut dyn Widget,
    target: NodeId,
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
        node_id_to_ffi(target),
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
        invalidation: ctx.invalidation(),
        stop_requested: ctx.stop_requested(),
        messages,
        animation_requests,
    }
}

fn dispatch_mouse_scroll_bubble(
    widget: &mut dyn Widget,
    target: NodeId,
    delta_x: i32,
    delta_y: i32,
    ctx: &mut EventCtx,
) -> bool {
    if widget_node_id(widget) == target {
        widget.on_mouse_scroll(delta_x, delta_y, ctx);
        return true;
    }

    let mut found_in_child = false;
    #[allow(deprecated)]
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
    let mut invalidation = crate::event::InvalidationFlags::default();
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
            node_id_to_ffi(message.sender),
            message.message
        ));
        let mut ctx = EventCtx::default();
        dispatch_message_tree(root, &message, &mut ctx);
        handled |= ctx.handled();

        repaint_requested |= ctx.repaint_requested();
        invalidation.merge(ctx.invalidation());
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
        invalidation,
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
        "[dispatch_message_tree] visit widget={} sender={} payload={:?}",
        root.style_type(),
        node_id_to_ffi(message.sender),
        message.message
    ));
    root.on_message(message, ctx);
    if ctx.handled() {
        debug_message(&format!(
            "[dispatch_message_tree] handled by {}",
            root.style_type()
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

// ===========================================================================
// P1-11: Arena-tree-based event routing (scaffold)
//
// These functions replace the recursive `visit_children_mut` dispatch with
// explicit `Vec<NodeId>` path traversal over the `WidgetTree` arena. They
// are added alongside the old functions; the runtime will switch to these
// once the main event loop is wired to the arena tree (P1-05).
// ===========================================================================

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
    }
}

/// Drain and dispatch a queue of messages through the arena tree.
///
/// Each message is broadcast to every node in the tree (depth-first order)
/// until a handler sets `ctx.handled()`. Newly emitted messages are appended
/// to the queue (bounded by a safety limit).
pub(crate) fn dispatch_message_queue_tree(
    tree: &mut WidgetTree,
    initial: Vec<MessageEvent>,
) -> DispatchOutcome {
    use std::collections::VecDeque;

    let mut handled = false;
    let mut repaint_requested = false;
    let mut invalidation = crate::event::InvalidationFlags::default();
    let mut stop_requested = false;
    let mut queue: VecDeque<MessageEvent> = initial.into();
    let mut emitted: Vec<MessageEvent> = Vec::new();
    let mut animation_requests: Vec<AnimationRequest> = Vec::new();

    const LIMIT: usize = 1024;
    let mut processed = 0usize;

    while let Some(message) = queue.pop_front() {
        processed += 1;
        if processed > LIMIT {
            debug_message("[dispatch_message_queue_tree] limit reached, dropping remaining");
            break;
        }

        let mut ctx = EventCtx::default();
        dispatch_message_tree_walk(tree, &message, &mut ctx);
        handled |= ctx.handled();
        repaint_requested |= ctx.repaint_requested();
        invalidation.merge(ctx.invalidation());
        stop_requested |= ctx.stop_requested();
        let next = ctx.take_messages();
        let mut next_anims = ctx.take_animation_requests();
        if !next.is_empty() {
            queue.extend(next.clone());
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
    }
}

/// Broadcast a single message to all nodes in the tree (depth-first).
fn dispatch_message_tree_walk(
    tree: &mut WidgetTree,
    message: &MessageEvent,
    ctx: &mut EventCtx,
) {
    let root = match tree.root() {
        Some(r) => r,
        None => return,
    };
    // Collect node IDs first to avoid borrow conflicts.
    let node_ids = tree.walk_depth_first(root);
    for node_id in node_ids {
        if ctx.handled() {
            return;
        }
        if let Some(node) = tree.get_mut(node_id) {
            node.widget.on_message(message, ctx);
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
    use crate::widgets::{AppRoot, Button, ScrollView};
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

        let msg_outcome = dispatch_message_queue(&mut root, outcome.messages);
        assert!(msg_outcome.handled);
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
        let button = Button::new("x");
        #[allow(deprecated)]
        let button_id = node_id_from_ffi(button.id().as_u64());
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
        #[allow(deprecated)]
        let button_id = node_id_from_ffi(button.id().as_u64());
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
        let button = Button::new("x");
        #[allow(deprecated)]
        let button_id = node_id_from_ffi(button.id().as_u64());
        let mut root = ScrollReceiver::new(button);

        let outcome = dispatch_mouse_scroll_to_target(&mut root, button_id, 0, 1);
        assert!(outcome.handled);
        assert_eq!(root.seen, 1);
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
        #[allow(deprecated)]
        let second_id = node_id_from_ffi(second.id().as_u64());
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

    #[test]
    fn focused_path_binding_hints_collects_ancestor_chain() {
        let leaf = HintNode::new(true, vec![BindingHint::new("enter", "activate")]);
        let mid = HintNode::new(false, vec![BindingHint::new("left", "back")]).with_child(leaf);
        let mut root =
            HintNode::new(false, vec![BindingHint::new("tab", "next focus")]).with_child(mid);

        let hints = focused_path_binding_hints(&mut root);
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
        let leaf = HintNode::new(false, vec![BindingHint::new("enter", "activate")]);
        let mut root = HintNode::new(false, vec![BindingHint::new("tab", "next")]).with_child(leaf);

        assert!(focused_path_binding_hints(&mut root).is_empty());
    }

    #[test]
    fn focused_help_metadata_returns_focused_widget_help() {
        let mut root = HintNode::new(false, vec![BindingHint::new("tab", "next")]).with_child(
            HintNode::new(true, vec![BindingHint::new("enter", "activate")])
                .with_help("## Focused help\nUse enter"),
        );

        let focused = focused_help_metadata(&mut root);
        assert!(matches!(
            focused.as_ref(),
            Some((_, markup)) if markup == "## Focused help\nUse enter"
        ));
    }

    #[test]
    fn focused_help_metadata_returns_none_without_focus() {
        let mut root = HintNode::new(false, vec![BindingHint::new("tab", "next")]).with_child(
            HintNode::new(false, vec![BindingHint::new("enter", "activate")])
                .with_help("## Focused help"),
        );

        assert!(focused_help_metadata(&mut root).is_none());
    }

    #[test]
    fn focused_path_binding_hints_tracks_focus_transitions() {
        let mut root =
            HintNode::new(false, vec![BindingHint::new("tab", "next focus")]).with_child(
                HintNode::new(true, vec![BindingHint::new("left/right", "switch tab")]),
            );

        let first = focused_path_binding_hints(&mut root);
        assert_eq!(
            first,
            vec![
                BindingHint::new("tab", "next focus"),
                BindingHint::new("left/right", "switch tab"),
            ]
        );

        if let Some(child) = root.child.as_mut() {
            child.set_focus(false);
        }
        root.set_focus(true);

        let second = focused_path_binding_hints(&mut root);
        assert_eq!(second, vec![BindingHint::new("tab", "next focus")]);
    }

    #[test]
    fn focused_help_metadata_tracks_focus_transitions() {
        let mut root = HintNode::new(false, vec![BindingHint::new("tab", "next focus")])
            .with_child(
                HintNode::new(true, vec![BindingHint::new("left/right", "switch tab")])
                    .with_help("## First"),
            );

        let first = focused_help_metadata(&mut root);
        assert!(matches!(
            first.as_ref(),
            Some((_, markup)) if markup == "## First"
        ));

        if let Some(child) = root.child.as_mut() {
            child.set_focus(false);
        }
        root.set_focus(true);
        root.help_markup = Some("## Second".to_string());

        let second = focused_help_metadata(&mut root);
        assert!(matches!(
            second.as_ref(),
            Some((_, markup)) if markup == "## Second"
        ));
    }

    #[test]
    fn active_binding_hints_returns_focused_chain_and_sources() {
        let leaf = HintNode::new(true, vec![BindingHint::new("enter", "activate")]);
        let mid = HintNode::new(false, vec![BindingHint::new("left", "back")]).with_child(leaf);
        let mut root =
            HintNode::new(false, vec![BindingHint::new("tab", "next focus")]).with_child(mid);

        let (hints, sources) = active_binding_hints(&mut root);
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
        let child = HintNode::new(false, vec![BindingHint::new("f1", "help")]);
        let mut root = HintNode::new(false, vec![BindingHint::new("q", "quit")]).with_child(child);

        let (hints, sources) = active_binding_hints(&mut root);
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
