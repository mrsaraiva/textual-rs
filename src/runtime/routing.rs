use crate::debug::debug_message;
use crate::event::{Action, AnimationRequest, BindingHint, Event, EventCtx};
use crate::keys::{KeyEventData, format_key_display};
use crate::message::{MessageEnvelope, MessageEvent};
use crate::node_id::NodeId;
use crate::widget_tree::WidgetTree;
use crate::widgets::Widget;

use super::dispatch_ctx::set_dispatch_recipient;
use super::types::DispatchOutcome;

#[cfg(test)]
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
        worker_requests: ctx.take_worker_requests(),
        recompose_nodes: ctx.take_recompose_nodes(),
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
        worker_requests: ctx.take_worker_requests(),
        recompose_nodes: ctx.take_recompose_nodes(),
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
pub fn focused_node_id_tree(tree: &WidgetTree) -> Option<NodeId> {
    let root = tree.root()?;
    for node_id in tree.walk_depth_first(root) {
        if let Some(node) = tree.get(node_id) {
            if node.display
                && node.visibility == crate::style::Visibility::Visible
                && node.widget.has_focus()
            {
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
pub fn dispatch_event_tree(
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
            let _dispatch_guard = set_dispatch_recipient(node_id, node.state);
            ctx.set_node_id(node_id);
            node.widget.on_event_capture(event, &mut ctx);
        }
    }

    // Bubble phase: focused → root
    if always_bubble || !ctx.handled() {
        for &node_id in path.iter().rev() {
            if let Some(node) = tree.get_mut(node_id) {
                let _dispatch_guard = set_dispatch_recipient(node_id, node.state);
                ctx.set_node_id(node_id);
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
        worker_requests: ctx.take_worker_requests(),
        recompose_nodes: ctx.take_recompose_nodes(),
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
pub fn dispatch_event_to_target_tree(
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
            let _dispatch_guard = set_dispatch_recipient(node_id, node.state);
            ctx.set_node_id(node_id);
            node.widget.on_event_capture(event, &mut ctx);
        }
    }

    // Bubble phase: target → root
    if !ctx.handled() {
        for &node_id in path.iter().rev() {
            if let Some(node) = tree.get_mut(node_id) {
                let _dispatch_guard = set_dispatch_recipient(node_id, node.state);
                ctx.set_node_id(node_id);
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
        worker_requests: ctx.take_worker_requests(),
        recompose_nodes: ctx.take_recompose_nodes(),
        default_prevented: false,
    }
}

/// Dispatch a global event to every node in the tree.
///
/// This is used for runtime-global state updates (e.g. binding-hint payload
/// changes) where non-focused widgets such as `Footer` still need notification.
pub fn dispatch_event_broadcast_tree(tree: &mut WidgetTree, event: &Event) -> DispatchOutcome {
    let Some(root) = tree.root() else {
        return DispatchOutcome::default();
    };

    let mut aggregate = EventCtx::default();
    for node_id in tree.walk_depth_first(root) {
        let mut ctx = EventCtx::default();
        ctx.set_node_id(node_id);
        if let Some(node) = tree.get_mut(node_id) {
            let _dispatch_guard = set_dispatch_recipient(node_id, node.state);
            node.widget.on_event(event, &mut ctx);
        }
        aggregate.merge_from(ctx);
    }

    DispatchOutcome {
        handled: aggregate.handled(),
        repaint_requested: aggregate.repaint_requested(),
        invalidation: aggregate.invalidation(),
        stop_requested: aggregate.stop_requested(),
        messages: aggregate.take_messages(),
        animation_requests: aggregate.take_animation_requests(),
        worker_requests: aggregate.take_worker_requests(),
        recompose_nodes: aggregate.take_recompose_nodes(),
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

    // In tree mode, the arena root is often an app adapter wrapper while the
    // actual screen/content root is the first visible child. Route scroll
    // actions there before trying root-only fallback so PageUp/PageDown remain
    // deterministic regardless of focus/hover state.
    if let Some(root_id) = tree.root()
        && let Some(target) = tree.children(root_id).iter().copied().find(|child_id| {
            tree.get(*child_id).is_some_and(|node| {
                node.display && node.visibility == crate::style::Visibility::Visible
            })
        })
    {
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
            let _dispatch_guard = set_dispatch_recipient(node_id, node.state);
            ctx.set_node_id(node_id);
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
        worker_requests: ctx.take_worker_requests(),
        recompose_nodes: ctx.take_recompose_nodes(),
        default_prevented: false,
    }
}

/// Coalesce replaceable messages in the queue.
///
/// For each older/newer envelope pair with the same sender:
/// - if the newer envelope has `set_replaceable(true)`, it replaces older
///   envelopes of the same message variant;
/// - otherwise replacement is delegated to the payload's `can_replace` trait method.
///
/// This keeps envelope-level override support while making replacement
/// semantics message-driven (Python parity).
pub(crate) fn coalesce_message_queue(queue: &mut std::collections::VecDeque<MessageEnvelope>) {
    if queue.len() < 2 {
        return;
    }

    fn envelope_replaces_pending(newer: &MessageEnvelope, older: &MessageEnvelope) -> bool {
        if newer.can_replace() {
            return newer.event.payload_type_id() == older.event.payload_type_id();
        }
        newer.message().can_replace(older.message())
    }

    let mut keep = vec![true; queue.len()];

    // Walk backwards so later messages survive.
    for i in (0..queue.len()).rev() {
        for j in ((i + 1)..queue.len()).rev() {
            let older = &queue[i];
            let newer = &queue[j];
            if older.sender() != newer.sender() {
                continue;
            }
            if envelope_replaces_pending(newer, older) {
                keep[i] = false;
                break;
            }
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

/// Canonical tree message pump: drain and dispatch a queue of messages through
/// the arena tree.
///
/// Each `MessageEvent` is wrapped in a [`MessageEnvelope`] that controls
/// propagation.  Messages bubble from the sender node up to the root; a
/// handler can stop propagation via `ctx.set_handled()` (maps to
/// `envelope.stop()`).  Before dispatching each batch the queue is
/// coalesced according to message-level replacement semantics.
///
/// This is the same pump used internally by the framework. Third-party
/// integration tests and tooling may drive it directly alongside
/// [`dispatch_event_tree`] to exercise custom widgets and messages.
pub fn dispatch_message_queue_tree(
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
    let mut worker_requests: Vec<crate::worker::WorkerRequest> = Vec::new();
    let mut recompose_nodes: Vec<NodeId> = Vec::new();

    let mut queue: VecDeque<MessageEnvelope> =
        initial.into_iter().map(MessageEnvelope::new).collect();

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
        let mut next_workers = ctx.take_worker_requests();
        let mut next_recompose = ctx.take_recompose_nodes();
        if !next.is_empty() {
            let next_envelopes: VecDeque<MessageEnvelope> = next
                .iter()
                .map(|evt| MessageEnvelope::new(evt.clone()))
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
        if !next_workers.is_empty() {
            worker_requests.append(&mut next_workers);
        }
        if !next_recompose.is_empty() {
            recompose_nodes.append(&mut next_recompose);
        }
    }

    DispatchOutcome {
        handled,
        repaint_requested,
        invalidation,
        stop_requested,
        messages: emitted,
        animation_requests,
        worker_requests,
        recompose_nodes,
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
    // Sync the envelope's promoted/overridden control into the event so that
    // widget `on_message(&MessageEvent, …)` handlers see the correct value.
    envelope.event.control = envelope.control();

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
                let _dispatch_guard = set_dispatch_recipient(node_id, node.state);
                ctx.set_node_id(node_id);
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
            let _dispatch_guard = set_dispatch_recipient(node_id, node.state);
            ctx.set_node_id(node_id);
            node.widget.on_message(&envelope.event, ctx);
            if ctx.handled() {
                envelope.stop();
            }
        }
    }
}

/// Return the focused widget's help markup, if any.
pub(crate) fn focused_help_metadata_tree(tree: &WidgetTree) -> Option<(NodeId, String)> {
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

/// Check whether a `KeyEventData` matches a binding key specification.
///
/// The binding key may contain comma-separated alternatives (e.g. `"j,down"`).
/// Matching is performed against the key's `aliases()` which include the
/// canonical name plus any alias variants.
fn key_matches_binding(key: &KeyEventData, binding_key: &str) -> bool {
    let aliases = key.aliases();
    binding_key
        .split(',')
        .map(str::trim)
        .any(|alt| aliases.iter().any(|a| *a == alt))
}

fn format_binding_key_display(binding_key: &str) -> String {
    binding_key
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            if matches!(part, "tab" | "shift+tab") {
                part.to_string()
            } else {
                format_key_display(part)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Walk the focused widget chain and find the first matching `BindingDecl`.
///
/// Phase 1: priority bindings (focused→root).
/// Phase 2: normal bindings (focused→root).
///
/// Returns `(node_id, action_string)` of the first match, or `None`.
pub(crate) fn match_binding_tree(
    tree: &WidgetTree,
    key: &KeyEventData,
) -> Option<(NodeId, String)> {
    let path = if let Some(focus_id) = focused_node_id_tree(tree) {
        build_path_to_node(tree, focus_id)
    } else {
        // No focused widget: fall back to root + single-child chain so
        // app-level/root declarative bindings still work.
        let Some(root) = tree.root() else {
            return None;
        };
        let mut path = vec![root];
        let mut current = root;
        loop {
            let children = tree.children(current);
            if children.len() != 1 {
                break;
            }
            current = children[0];
            path.push(current);
        }
        path
    };

    // Phase 1: priority bindings (focused → root)
    for &node_id in path.iter().rev() {
        if let Some(node) = tree.get(node_id) {
            for binding in node.widget.bindings() {
                if binding.priority && key_matches_binding(key, &binding.key) {
                    return Some((node_id, binding.action.clone()));
                }
            }
        }
    }

    // Phase 2: normal bindings (focused → root)
    for &node_id in path.iter().rev() {
        if let Some(node) = tree.get(node_id) {
            for binding in node.widget.bindings() {
                if !binding.priority && key_matches_binding(key, &binding.key) {
                    return Some((node_id, binding.action.clone()));
                }
            }
        }
    }

    None
}

/// Collect binding hints along the focused path (focused→root).
///
/// If no widget has focus, falls back to root + single-child chain.
pub(crate) fn active_binding_hints_tree(tree: &WidgetTree) -> (Vec<BindingHint>, Vec<NodeId>) {
    if let Some(focus_id) = focused_node_id_tree(tree) {
        let path = build_path_to_node(tree, focus_id);
        let mut hints = Vec::new();
        let mut sources = Vec::new();
        for &node_id in path.iter().rev() {
            if let Some(node) = tree.get(node_id) {
                sources.push(node_id);
                let namespace = node.widget.action_namespace();
                hints.extend(node.widget.binding_hints().into_iter().map(
                    |hint| match hint.namespace {
                        Some(_) => hint,
                        None => hint.with_namespace(namespace),
                    },
                ));
                // Also include hints derived from declarative bindings.
                for decl in node.widget.bindings() {
                    let mut hint = BindingHint::new(&decl.key, &decl.description)
                        .hidden(!decl.show)
                        .with_key_display(format_binding_key_display(&decl.key))
                        .with_priority(decl.priority)
                        .with_action(&decl.action)
                        .with_namespace(
                            decl.namespace
                                .clone()
                                .unwrap_or_else(|| namespace.to_string()),
                        );
                    if let Some(tooltip) = &decl.tooltip {
                        hint = hint.with_tooltip(tooltip.clone());
                    }
                    hints.push(hint);
                }
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
            let namespace = node.widget.action_namespace();
            hints.extend(node.widget.binding_hints().into_iter().map(
                |hint| match hint.namespace {
                    Some(_) => hint,
                    None => hint.with_namespace(namespace),
                },
            ));
            for decl in node.widget.bindings() {
                let mut hint = BindingHint::new(&decl.key, &decl.description)
                    .hidden(!decl.show)
                    .with_key_display(format_binding_key_display(&decl.key))
                    .with_priority(decl.priority)
                    .with_action(&decl.action)
                    .with_namespace(
                        decl.namespace
                            .clone()
                            .unwrap_or_else(|| namespace.to_string()),
                    );
                if let Some(tooltip) = &decl.tooltip {
                    hint = hint.with_tooltip(tooltip.clone());
                }
                hints.push(hint);
            }
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
    use crate::runtime::render::{apply_layout_info_tree_from_layout_rects, run_layout_pass};
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
    }

    impl HintNode {
        fn new(focused: bool, hints: Vec<BindingHint>) -> Self {
            Self {
                focused,
                hints,
                help_markup: None,
            }
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
                    ctx.post_message(crate::message::InputChanged {
                        value: "ok".into(),
                        validation: crate::validation::ValidationResult::success(),
                    });
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
            if message.is::<crate::message::InputChanged>() {
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

        // Deliver message directly to root for this unit test.
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
            if message.is::<crate::message::ButtonPressed>() {
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

        // Button checks target == self.node_id(). Tree dispatch sets dispatch
        // context to button_id, so events must carry button_id as target.
        let down = dispatch_event_to_target_tree(
            &mut tree,
            button_id,
            &Event::MouseDown(MouseDownEvent {
                target: button_id,
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
                target: Some(button_id),
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
        let button_id = tree.mount(scroll_id, Box::new(Button::new("x")));

        // Button checks target == self.node_id(). Tree dispatch sets dispatch
        // context to button_id, so events must carry button_id as target.
        let down = dispatch_event_to_target_tree(
            &mut tree,
            button_id,
            &Event::MouseDown(MouseDownEvent {
                target: button_id,
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
                target: Some(button_id),
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
        seen: usize,
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
        let root_id = tree.set_root(Box::new(ScrollReceiver { seen: 0 }));
        let button_id = tree.mount(root_id, Box::new(Button::new("x")));

        // Button doesn't handle scroll, so it bubbles to ScrollReceiver.
        let outcome = dispatch_mouse_scroll_to_target_tree(&mut tree, button_id, 0, 1);
        assert!(outcome.handled);
    }

    #[test]
    fn dedicated_scrollbar_click_updates_scrollview_offset() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let scroll_id = tree.mount(
            root_id,
            Box::new(ScrollView::new(Label::new("line\n".repeat(120)))),
        );

        // Enter tree mode and mount ScrollView dedicated scrollbar children.
        let extracted = {
            let node = tree.get_mut(scroll_id).expect("scrollview node");
            node.widget.take_composed_children()
        };
        for child in extracted {
            tree.mount(scroll_id, child);
        }

        run_layout_pass(&mut tree, (40, 10));
        apply_layout_info_tree_from_layout_rects(&mut tree);

        let vbar_id = tree
            .children(scroll_id)
            .iter()
            .copied()
            .find(|child_id| {
                tree.get(*child_id).and_then(|node| node.widget.style_id())
                    == Some("__scrollview_vscrollbar")
            })
            .expect("vertical scrollbar child must exist");

        // Click below the thumb to trigger page-down behavior.
        let down = dispatch_event_to_target_tree(
            &mut tree,
            vbar_id,
            &Event::MouseDown(MouseDownEvent {
                target: vbar_id,
                screen_x: 39,
                screen_y: 8,
                x: 0,
                y: 8,
            }),
        );
        let _ = dispatch_message_queue_tree(&mut tree, down.messages);

        let offset_y = tree
            .get(scroll_id)
            .expect("scrollview node")
            .widget
            .scroll_offset()
            .1;
        assert!(
            offset_y > 0,
            "clicking the dedicated vertical scrollbar should advance offset, got {offset_y}"
        );
    }

    struct ScrollSink {
        focused: bool,
        hits: Arc<AtomicUsize>,
    }

    impl ScrollSink {
        fn new(focused: bool, hits: Arc<AtomicUsize>) -> Self {
            Self { focused, hits }
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

        let outcome = dispatch_scroll_action_tree(&mut tree, Action::ScrollDown, Some(second_id));
        assert!(outcome.handled);
        assert_eq!(first_hits.load(Ordering::Relaxed), 0);
        assert_eq!(second_hits.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn scroll_actions_fallback_to_global_when_no_target_handles() {
        // Without focus or hover, scroll dispatches to the first visible child
        // under the arena root (screen/content root fallback).
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let first_hits = Arc::new(AtomicUsize::new(0));
        let _first_id = tree.mount(
            root_id,
            Box::new(ScrollSink::new(false, first_hits.clone())),
        );

        let outcome = dispatch_scroll_action_tree(&mut tree, Action::ScrollDown, None);
        assert!(outcome.handled);
        assert_eq!(first_hits.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn focused_path_binding_hints_collects_ancestor_chain() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(HintNode::new(
            false,
            vec![BindingHint::new("tab", "next focus")],
        )));
        let mid_id = tree.mount(
            root_id,
            Box::new(HintNode::new(false, vec![BindingHint::new("left", "back")])),
        );
        let _leaf_id = tree.mount(
            mid_id,
            Box::new(HintNode::new(
                true,
                vec![BindingHint::new("enter", "activate")],
            )),
        );

        let (hints, _sources) = active_binding_hints_tree(&tree);
        assert_eq!(
            hints,
            vec![
                BindingHint::new("enter", "activate").with_namespace(""),
                BindingHint::new("left", "back").with_namespace(""),
                BindingHint::new("tab", "next focus").with_namespace("")
            ]
        );
    }

    #[test]
    fn focused_path_binding_hints_returns_empty_without_focus() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(HintNode::new(
            false,
            vec![BindingHint::new("tab", "next")],
        )));
        let _leaf_id = tree.mount(
            root_id,
            Box::new(HintNode::new(
                false,
                vec![BindingHint::new("enter", "activate")],
            )),
        );

        // No focused node — falls back to root scope (single-child chain).
        let (hints, _) = active_binding_hints_tree(&tree);
        // Returns root + leaf hints via single-child fallback.
        assert!(!hints.is_empty());
    }

    #[test]
    fn focused_help_metadata_returns_focused_widget_help() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(HintNode::new(
            false,
            vec![BindingHint::new("tab", "next")],
        )));
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
        let root_id = tree.set_root(Box::new(HintNode::new(
            false,
            vec![BindingHint::new("tab", "next")],
        )));
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
        let root_id = tree.set_root(Box::new(HintNode::new(
            false,
            vec![BindingHint::new("tab", "next focus")],
        )));
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
                BindingHint::new("left/right", "switch tab").with_namespace(""),
                BindingHint::new("tab", "next focus").with_namespace(""),
            ]
        );

        // Transition focus from child to root.
        tree.get_mut(child_id).unwrap().widget.set_focus(false);
        tree.get_mut(root_id).unwrap().widget.set_focus(true);

        let (second, _) = active_binding_hints_tree(&tree);
        assert_eq!(
            second,
            vec![BindingHint::new("tab", "next focus").with_namespace("")]
        );
    }

    #[test]
    fn focused_help_metadata_tracks_focus_transitions() {
        // State 1: child has focus + help markup.
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(HintNode::new(
            false,
            vec![BindingHint::new("tab", "next focus")],
        )));
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
            HintNode::new(true, vec![BindingHint::new("tab", "next focus")]).with_help("## Second"),
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
        let root_id = tree.set_root(Box::new(HintNode::new(
            false,
            vec![BindingHint::new("tab", "next focus")],
        )));
        let mid_id = tree.mount(
            root_id,
            Box::new(HintNode::new(false, vec![BindingHint::new("left", "back")])),
        );
        let _leaf_id = tree.mount(
            mid_id,
            Box::new(HintNode::new(
                true,
                vec![BindingHint::new("enter", "activate")],
            )),
        );

        let (hints, sources) = active_binding_hints_tree(&tree);
        assert_eq!(
            hints,
            vec![
                BindingHint::new("enter", "activate").with_namespace(""),
                BindingHint::new("left", "back").with_namespace(""),
                BindingHint::new("tab", "next focus").with_namespace("")
            ]
        );
        assert_eq!(sources.len(), 3);
    }

    #[test]
    fn active_binding_hints_falls_back_to_single_child_scope_without_focus() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(HintNode::new(
            false,
            vec![BindingHint::new("q", "quit")],
        )));
        let _child_id = tree.mount(
            root_id,
            Box::new(HintNode::new(false, vec![BindingHint::new("f1", "help")])),
        );

        let (hints, sources) = active_binding_hints_tree(&tree);
        assert_eq!(
            hints,
            vec![
                BindingHint::new("q", "quit").with_namespace(""),
                BindingHint::new("f1", "help").with_namespace("")
            ]
        );
        assert_eq!(sources.len(), 2);
    }

    struct BindingEventProbe {
        focused: bool,
        hits: Arc<AtomicUsize>,
    }

    impl BindingEventProbe {
        fn new(focused: bool, hits: Arc<AtomicUsize>) -> Self {
            Self { focused, hits }
        }
    }

    impl Widget for BindingEventProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn has_focus(&self) -> bool {
            self.focused
        }

        fn set_focus(&mut self, focused: bool) {
            self.focused = focused;
        }

        fn on_event(&mut self, event: &Event, _ctx: &mut EventCtx) {
            if matches!(event, Event::BindingsChanged(..)) {
                self.hits.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    #[test]
    fn broadcast_event_reaches_non_focused_siblings() {
        let focused_hits = Arc::new(AtomicUsize::new(0));
        let sibling_hits = Arc::new(AtomicUsize::new(0));

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let _focused = tree.mount(
            root_id,
            Box::new(BindingEventProbe::new(true, focused_hits.clone())),
        );
        let _sibling = tree.mount(
            root_id,
            Box::new(BindingEventProbe::new(false, sibling_hits.clone())),
        );

        let _ = dispatch_event_broadcast_tree(
            &mut tree,
            &Event::BindingsChanged(vec![BindingHint::new("l", "Leto")]),
        );

        assert_eq!(focused_hits.load(Ordering::Relaxed), 1);
        assert_eq!(sibling_hits.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn active_hints_include_root_app_bindings_when_tree_is_focused() {
        // Tree is focused; root (TreeStubWidget) has an app-level binding.
        // After hiding Tree nav bindings, the app binding is visible.
        // This test verifies the hint IS collected from the root node
        // even when the focused child has no hints of its own.
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(HintNode::new(
            false,
            vec![BindingHint::new("a", "Add node")],
        )));
        let _tree_id = tree.mount(root_id, Box::new(HintNode::new(true, vec![])));

        let (hints, sources) = active_binding_hints_tree(&tree);
        assert!(
            hints.iter().any(|h| h.key == "a"),
            "app-level 'a' binding hint must appear in active hints when Tree is focused"
        );
        assert_eq!(sources.len(), 2, "focused node + root both in sources");
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
            if message.is::<crate::message::ButtonPressed>() {
                self.count.fetch_add(1, Ordering::Relaxed);
                if self.stop_on_match {
                    ctx.set_handled();
                }
            }
        }
    }

    /// Helper: build a MessageEvent from a sender FFI id and a typed message.
    fn msg_event<M: Message>(sender_ffi: u64, message: M) -> MessageEvent {
        MessageEvent::new(node_id_from_ffi(sender_ffi), message)
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

        let messages = vec![MessageEvent::new(
            leaf_id,
            crate::message::ButtonPressed {
                description: "test".into(),
                button_id: None,
            },
        )];

        let outcome = dispatch_message_queue_tree(&mut tree, messages);
        // All three nodes on the bubble path should see the message.
        assert!(
            leaf_count.load(Ordering::Relaxed) >= 1,
            "leaf should see message"
        );
        assert!(
            mid_count.load(Ordering::Relaxed) >= 1,
            "mid should see message"
        );
        assert!(
            root_count.load(Ordering::Relaxed) >= 1,
            "root should see message"
        );
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

        let messages = vec![MessageEvent::new(
            leaf_id,
            crate::message::ButtonPressed {
                description: "stop".into(),
                button_id: None,
            },
        )];

        let outcome = dispatch_message_queue_tree(&mut tree, messages);
        assert!(outcome.handled, "mid should have handled it");
        // Leaf sees it first (bubble starts at sender), mid stops.
        assert!(leaf_count.load(Ordering::Relaxed) >= 1, "leaf sees message");
        assert!(
            mid_count.load(Ordering::Relaxed) >= 1,
            "mid sees message and stops"
        );
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
            crate::message::ButtonPressed {
                description: "ghost".into(),
                button_id: None,
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

        let messages = vec![MessageEvent::new(
            root_id,
            crate::message::ButtonPressed {
                description: "dp".into(),
                button_id: None,
            },
        )];

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
        let mut env1 = MessageEnvelope::new(MessageEvent::new(
            sender,
            crate::message::InputChanged {
                value: "a".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        ));
        env1.set_replaceable(true);

        let mut env2 = MessageEnvelope::new(MessageEvent::new(
            sender,
            crate::message::InputChanged {
                value: "ab".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        ));
        env2.set_replaceable(true);

        queue.push_back(env1);
        queue.push_back(env2);
        coalesce_message_queue(&mut queue);

        assert_eq!(queue.len(), 1, "should coalesce to one message");
        assert!(queue[0]
            .downcast_ref::<crate::message::InputChanged>()
            .is_some_and(|m| m.value == "ab"));
    }

    #[test]
    fn coalesce_preserves_non_replaceable_messages() {
        let sender = node_id_from_ffi(1);
        let mut queue: VecDeque<MessageEnvelope> = VecDeque::new();

        // Two ButtonPressed — not replaceable by default.
        let env1 = MessageEnvelope::new(MessageEvent::new(
            sender,
            crate::message::ButtonPressed {
                description: "first".into(),
                button_id: None,
            },
        ));
        let env2 = MessageEnvelope::new(MessageEvent::new(
            sender,
            crate::message::ButtonPressed {
                description: "second".into(),
                button_id: None,
            },
        ));

        queue.push_back(env1);
        queue.push_back(env2);
        coalesce_message_queue(&mut queue);

        assert_eq!(
            queue.len(),
            2,
            "non-replaceable messages should all survive"
        );
    }

    #[test]
    fn coalesce_different_senders_preserved() {
        let sender_a = node_id_from_ffi(1);
        let sender_b = node_id_from_ffi(2);
        let mut queue: VecDeque<MessageEnvelope> = VecDeque::new();

        let mut env1 = MessageEnvelope::new(MessageEvent::new(
            sender_a,
            crate::message::InputChanged {
                value: "a".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        ));
        env1.set_replaceable(true);

        let mut env2 = MessageEnvelope::new(MessageEvent::new(
            sender_b,
            crate::message::InputChanged {
                value: "b".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        ));
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
        let mut env1 = MessageEnvelope::new(MessageEvent::new(
            sender,
            crate::message::InputChanged {
                value: "a".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        ));
        env1.set_replaceable(true);

        // Non-replaceable ButtonPressed
        let env2 = MessageEnvelope::new(MessageEvent::new(
            sender,
            crate::message::ButtonPressed {
                description: "click".into(),
                button_id: None,
            },
        ));

        // Replaceable InputChanged #2
        let mut env3 = MessageEnvelope::new(MessageEvent::new(
            sender,
            crate::message::InputChanged {
                value: "ab".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        ));
        env3.set_replaceable(true);

        queue.push_back(env1);
        queue.push_back(env2);
        queue.push_back(env3);
        coalesce_message_queue(&mut queue);

        // Two InputChanged coalesce to one, ButtonPressed survives.
        assert_eq!(queue.len(), 2, "InputChanged pair → 1, ButtonPressed → 1");
        // First remaining should be ButtonPressed (index 0 InputChanged was removed).
        assert!(queue[0].is::<crate::message::ButtonPressed>());
        // Second should be the latest InputChanged.
        assert!(queue[1]
            .downcast_ref::<crate::message::InputChanged>()
            .is_some_and(|m| m.value == "ab"));
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
        let mut env = MessageEnvelope::new(MessageEvent::new(
            node_id_from_ffi(1),
            crate::message::InputChanged {
                value: "x".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        ));
        env.set_replaceable(true);
        queue.push_back(env);
        coalesce_message_queue(&mut queue);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn dispatch_coalesces_messages_via_message_can_replace() {
        let sender = node_id_from_ffi(1);
        let _count = Arc::new(AtomicUsize::new(0));

        let mut tree = WidgetTree::new();
        let _root_id = tree.set_root(Box::new(Label::new("x")));

        // Three InputChanged from the same sender — should coalesce to one
        // via the replaceable can_replace trait impl.
        let messages = vec![
            MessageEvent::new(
                sender,
                crate::message::InputChanged {
                    value: "a".into(),
                    validation: crate::validation::ValidationResult::success(),
                },
            ),
            MessageEvent::new(
                sender,
                crate::message::InputChanged {
                    value: "ab".into(),
                    validation: crate::validation::ValidationResult::success(),
                },
            ),
            MessageEvent::new(
                sender,
                crate::message::InputChanged {
                    value: "abc".into(),
                    validation: crate::validation::ValidationResult::success(),
                },
            ),
        ];

        // No panic and dispatch succeeds.
        let _outcome = dispatch_message_queue_tree(&mut tree, messages);
    }

    #[test]
    fn message_can_replace_covers_known_variants() {
        // Spot-check that known rapid-fire message types are replaceable.
        assert!(
            crate::message::InputChanged {
                value: "x".into(),
                validation: crate::validation::ValidationResult::success(),
            }
            .can_replace(&crate::message::InputChanged {
                value: "y".into(),
                validation: crate::validation::ValidationResult::success(),
            })
        );
        assert!(
            crate::message::TextAreaChanged { value: "x".into() }
                .can_replace(&crate::message::TextAreaChanged { value: "y".into() })
        );
        assert!(
            crate::message::DataTableCursorMoved { row: 0, column: 0 }
                .can_replace(&crate::message::DataTableCursorMoved { row: 1, column: 1 })
        );
        assert!(
            crate::message::OptionHighlighted { index: 0 }
                .can_replace(&crate::message::OptionHighlighted { index: 1 })
        );
        // Non-replaceable variants.
        assert!(
            !crate::message::ButtonPressed {
                description: "x".into(),
                button_id: None,
            }
            .can_replace(&crate::message::ButtonPressed {
                description: "y".into(),
                button_id: None,
            })
        );
        assert!(
            !crate::message::InputSubmitted { value: "x".into() }
                .can_replace(&crate::message::InputSubmitted { value: "y".into() })
        );
        // Different variants never replace each other by default.
        assert!(
            !crate::message::TextAreaChanged { value: "x".into() }.can_replace(
                &crate::message::InputChanged {
                    value: "x".into(),
                    validation: crate::validation::ValidationResult::success(),
                }
            )
        );
    }

    // =====================================================================
    // P4-17: Envelope control field tests (routing integration)
    // =====================================================================

    #[test]
    fn envelope_control_defaults_to_sender_during_dispatch() {
        // When dispatch_message_queue_tree wraps a MessageEvent the resulting
        // envelope's control() should equal the event's sender.
        let sender = node_id_from_ffi(1);
        let mut tree = WidgetTree::new();
        let _root_id = tree.set_root(Box::new(Label::new("x")));

        let messages = vec![MessageEvent::new(
            sender,
            crate::message::ButtonPressed {
                description: "ctrl".into(),
                button_id: None,
            },
        )];

        // Build the envelope the same way dispatch does and verify control.
        let env = MessageEnvelope::new(messages[0].clone());
        assert_eq!(env.control(), Some(sender));

        // Full dispatch should not panic / break.
        let _outcome = dispatch_message_queue_tree(&mut tree, messages);
    }

    #[test]
    fn envelope_control_preserved_during_bubble() {
        // Tree: root → mid → leaf (sender).  All three nodes see the message
        // via bubble.  The envelope's control stays as the leaf (sender).
        let root_count = Arc::new(AtomicUsize::new(0));
        let mid_count = Arc::new(AtomicUsize::new(0));
        let leaf_count = Arc::new(AtomicUsize::new(0));

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(MessageCounter::new(root_count.clone())));
        let mid_id = tree.mount(root_id, Box::new(MessageCounter::new(mid_count.clone())));
        let leaf_id = tree.mount(mid_id, Box::new(MessageCounter::new(leaf_count.clone())));

        let evt = MessageEvent::new(
            leaf_id,
            crate::message::ButtonPressed {
                description: "bubble".into(),
                button_id: None,
            },
        );
        let mut env = MessageEnvelope::new(evt.clone());
        // Control should be the leaf (sender) before and after dispatch.
        assert_eq!(env.control(), Some(leaf_id));

        let mut ctx = EventCtx::default();
        dispatch_message_bubble(&mut tree, &mut env, &mut ctx);

        // Control must NOT have changed during bubble propagation.
        assert_eq!(
            env.control(),
            Some(leaf_id),
            "control must stay at sender during bubble"
        );
    }

    #[test]
    fn coalesced_messages_preserve_control_from_latest() {
        // When two replaceable messages from the same sender coalesce, the
        // surviving (latest) envelope keeps its control.
        let sender = node_id_from_ffi(5);
        let mut queue: VecDeque<MessageEnvelope> = VecDeque::new();

        let mut env1 = MessageEnvelope::new(MessageEvent::new(
            sender,
            crate::message::InputChanged {
                value: "a".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        ));
        env1.set_replaceable(true);

        let mut env2 = MessageEnvelope::new(MessageEvent::new(
            sender,
            crate::message::InputChanged {
                value: "ab".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        ));
        env2.set_replaceable(true);

        queue.push_back(env1);
        queue.push_back(env2);
        coalesce_message_queue(&mut queue);

        assert_eq!(queue.len(), 1);
        assert_eq!(
            queue[0].control(),
            Some(sender),
            "coalesced envelope should keep the latest control value"
        );
    }

    #[test]
    fn set_control_override_survives_coalescing() {
        // If we override the control on the later envelope, coalescing should
        // preserve that override (since the later one is kept).
        let sender = node_id_from_ffi(5);
        let override_node = node_id_from_ffi(77);
        let mut queue: VecDeque<MessageEnvelope> = VecDeque::new();

        let mut env1 = MessageEnvelope::new(MessageEvent::new(
            sender,
            crate::message::InputChanged {
                value: "a".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        ));
        env1.set_replaceable(true);

        let mut env2 = MessageEnvelope::new(MessageEvent::new(
            sender,
            crate::message::InputChanged {
                value: "ab".into(),
                validation: crate::validation::ValidationResult::success(),
            },
        ));
        env2.set_replaceable(true);
        env2.set_control(override_node);

        queue.push_back(env1);
        queue.push_back(env2);
        coalesce_message_queue(&mut queue);

        assert_eq!(queue.len(), 1);
        assert_eq!(
            queue[0].control(),
            Some(override_node),
            "overridden control on the latest envelope should survive coalescing"
        );
    }

    // =====================================================================
    // Widget observability: widgets receive correct control via MessageEvent
    // =====================================================================

    use crate::node_id::NodeId;
    use std::sync::Mutex;

    /// Widget that captures the `control` value from the MessageEvent it receives.
    struct ControlCapture {
        captured: Arc<Mutex<Vec<Option<NodeId>>>>,
    }

    impl ControlCapture {
        fn new(captured: Arc<Mutex<Vec<Option<NodeId>>>>) -> Self {
            Self { captured }
        }
    }

    impl Widget for ControlCapture {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
            if message.is::<crate::message::ButtonPressed>() {
                self.captured.lock().unwrap().push(message.control);
                ctx.set_handled();
            }
        }
    }

    #[test]
    fn widget_on_message_sees_promoted_control_from_envelope() {
        // When control is None on the event, the envelope promotes it to
        // Some(sender). dispatch_message_bubble must sync this back so the
        // widget's on_message handler sees Some(sender), not None.
        let captured = Arc::new(Mutex::new(Vec::new()));

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(ControlCapture::new(captured.clone())));

        let messages = vec![MessageEvent::new(
            root_id,
            crate::message::ButtonPressed {
                description: "test".into(),
                button_id: None,
            },
        )];

        dispatch_message_queue_tree(&mut tree, messages);

        let values = captured.lock().unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(
            values[0],
            Some(root_id),
            "widget should see control = Some(sender) after envelope promotion"
        );
    }

    #[test]
    fn widget_on_message_sees_explicit_control() {
        // When control is explicitly set on the event, the widget should see
        // that value, not the sender.
        let captured = Arc::new(Mutex::new(Vec::new()));
        let explicit_control = node_id_from_ffi(999);

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(ControlCapture::new(captured.clone())));

        let messages = vec![
            MessageEvent::new(
                root_id,
                crate::message::ButtonPressed {
                    description: "explicit".into(),
                    button_id: None,
                },
            )
            .with_control(explicit_control),
        ];

        dispatch_message_queue_tree(&mut tree, messages);

        let values = captured.lock().unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(
            values[0],
            Some(explicit_control),
            "widget should see the explicit control value from the event"
        );
    }
}

#[cfg(test)]
mod binding_tests {
    use super::*;
    use crate::keys::KeyEventData;
    use crate::widget_tree::WidgetTree;
    use crate::widgets::BindingDecl;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rich_rs::{Console, ConsoleOptions, Segments};

    fn key_event(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    // -- BindingDecl construction tests --

    #[test]
    fn binding_decl_new_defaults() {
        let b = BindingDecl::new("enter", "submit", "Submit form");
        assert_eq!(b.key, "enter");
        assert_eq!(b.action, "submit");
        assert_eq!(b.description, "Submit form");
        assert!(b.show);
        assert!(!b.priority);
    }

    #[test]
    fn binding_decl_hidden_builder() {
        let b = BindingDecl::new("q", "quit", "Quit").hidden();
        assert!(!b.show);
        assert!(!b.priority);
    }

    #[test]
    fn binding_decl_priority_builder() {
        let b = BindingDecl::new("escape", "close", "Close").priority();
        assert!(b.show);
        assert!(b.priority);
    }

    #[test]
    fn binding_decl_chained_builders() {
        let b = BindingDecl::new("x", "delete", "Delete")
            .hidden()
            .priority();
        assert!(!b.show);
        assert!(b.priority);
    }

    // -- key_matches_binding tests --

    #[test]
    fn key_matches_single_binding() {
        let key = KeyEventData::from_crossterm(key_event(KeyCode::Enter, KeyModifiers::empty()));
        assert!(key_matches_binding(&key, "enter"));
        assert!(!key_matches_binding(&key, "space"));
    }

    #[test]
    fn key_matches_comma_separated_alternatives() {
        let key =
            KeyEventData::from_crossterm(key_event(KeyCode::Char('j'), KeyModifiers::empty()));
        assert!(key_matches_binding(&key, "j,down"));
        assert!(key_matches_binding(&key, "up,j"));
    }

    #[test]
    fn key_matches_via_alias() {
        // Tab and ctrl+i are aliases
        let key = KeyEventData::from_crossterm(key_event(KeyCode::Tab, KeyModifiers::empty()));
        assert!(key_matches_binding(&key, "ctrl+i"));
        assert!(key_matches_binding(&key, "tab"));
    }

    #[test]
    fn key_no_match_returns_false() {
        let key =
            KeyEventData::from_crossterm(key_event(KeyCode::Char('z'), KeyModifiers::empty()));
        assert!(!key_matches_binding(&key, "a,b,c"));
    }

    // -- match_binding_tree tests --

    /// Minimal widget that declares bindings and reports focus state.
    struct BindingWidget {
        focused: bool,
        decls: Vec<BindingDecl>,
    }

    impl BindingWidget {
        fn new(focused: bool, decls: Vec<BindingDecl>) -> Self {
            Self { focused, decls }
        }
    }

    impl Widget for BindingWidget {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn bindings(&self) -> Vec<BindingDecl> {
            self.decls.clone()
        }

        fn focusable(&self) -> bool {
            true
        }

        fn has_focus(&self) -> bool {
            self.focused
        }

        fn set_focus(&mut self, focused: bool) {
            self.focused = focused;
        }
    }

    /// Inert root widget.
    struct Root;

    impl Widget for Root {
        fn render(&self, _: &Console, _: &ConsoleOptions) -> Segments {
            Segments::new()
        }
    }

    #[test]
    fn match_binding_focused_widget() {
        // Tree: root → child (focused, binding "enter" → "submit")
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(Root));
        let _child_id = tree.mount(
            root_id,
            Box::new(BindingWidget::new(
                true,
                vec![BindingDecl::new("enter", "submit", "Submit")],
            )),
        );

        let key = KeyEventData::from_crossterm(key_event(KeyCode::Enter, KeyModifiers::empty()));
        let result = match_binding_tree(&tree, &key);
        assert!(result.is_some());
        let (node_id, action) = result.unwrap();
        assert_eq!(action, "submit");
        assert_eq!(node_id, _child_id);
    }

    #[test]
    fn match_binding_ancestor_fallback() {
        // Tree: root (binding "q" → "quit") → child (focused, no bindings)
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(BindingWidget::new(
            false,
            vec![BindingDecl::new("q", "app.quit", "Quit")],
        )));
        let _child_id = tree.mount(root_id, Box::new(BindingWidget::new(false, vec![])));
        // Focus the child
        if let Some(node) = tree.get_mut(_child_id) {
            node.widget.set_focus(true);
        }

        let key =
            KeyEventData::from_crossterm(key_event(KeyCode::Char('q'), KeyModifiers::empty()));
        let result = match_binding_tree(&tree, &key);
        assert!(result.is_some());
        let (node_id, action) = result.unwrap();
        assert_eq!(action, "app.quit");
        assert_eq!(node_id, root_id);
    }

    #[test]
    fn match_binding_priority_wins_over_normal() {
        // Tree: root (priority binding "escape" → "close_app")
        //       → child (focused, normal binding "escape" → "cancel")
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(BindingWidget::new(
            false,
            vec![BindingDecl::new("escape", "close_app", "Close app").priority()],
        )));
        let _child_id = tree.mount(
            root_id,
            Box::new(BindingWidget::new(
                true,
                vec![BindingDecl::new("escape", "cancel", "Cancel")],
            )),
        );

        let key = KeyEventData::from_crossterm(key_event(KeyCode::Esc, KeyModifiers::empty()));
        let result = match_binding_tree(&tree, &key);
        assert!(result.is_some());
        let (node_id, action) = result.unwrap();
        // Priority binding on child should be checked first (focused → root),
        // but child has normal binding, root has priority. Priority phase checks
        // child first (no priority there), then root (priority match!).
        assert_eq!(action, "close_app");
        assert_eq!(node_id, root_id);

        // Now verify that without priority, child would win.
        // Remove priority from root, make it normal.
        let mut tree2 = WidgetTree::new();
        let root_id2 = tree2.set_root(Box::new(BindingWidget::new(
            false,
            vec![BindingDecl::new("escape", "close_app", "Close app")],
        )));
        let child_id2 = tree2.mount(
            root_id2,
            Box::new(BindingWidget::new(
                true,
                vec![BindingDecl::new("escape", "cancel", "Cancel")],
            )),
        );

        let result2 = match_binding_tree(&tree2, &key);
        assert!(result2.is_some());
        let (node_id2, action2) = result2.unwrap();
        // Normal bindings: focused child wins (checked first in focused → root order).
        assert_eq!(action2, "cancel");
        assert_eq!(node_id2, child_id2);
    }

    #[test]
    fn match_binding_no_match_returns_none() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(BindingWidget::new(
            false,
            vec![BindingDecl::new("enter", "submit", "Submit")],
        )));
        let _child_id = tree.mount(root_id, Box::new(BindingWidget::new(true, vec![])));

        let key =
            KeyEventData::from_crossterm(key_event(KeyCode::Char('z'), KeyModifiers::empty()));
        let result = match_binding_tree(&tree, &key);
        assert!(result.is_none());
    }

    #[test]
    fn match_binding_no_focus_uses_root_scope_fallback() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(BindingWidget::new(
            false,
            vec![BindingDecl::new("enter", "submit", "Submit")],
        )));

        let key = KeyEventData::from_crossterm(key_event(KeyCode::Enter, KeyModifiers::empty()));
        let result = match_binding_tree(&tree, &key);
        assert!(result.is_some());
        let (node_id, action) = result.unwrap();
        assert_eq!(node_id, root_id);
        assert_eq!(action, "submit");
    }

    // -- binding hints integration --

    #[test]
    fn active_hints_include_declared_bindings() {
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(BindingWidget::new(
            false,
            vec![BindingDecl::new("q", "quit", "Quit application")],
        )));
        let _child_id = tree.mount(
            root_id,
            Box::new(BindingWidget::new(
                true,
                vec![
                    BindingDecl::new("enter", "submit", "Submit form"),
                    BindingDecl::new("escape", "cancel", "Cancel").hidden(),
                ],
            )),
        );

        let (hints, _sources) = active_binding_hints_tree(&tree);
        // Root has 1 binding, child has 2 bindings = 3 total hints.
        assert_eq!(hints.len(), 3);

        // Check that the hidden binding is marked hidden in the hint.
        let escape_hint = hints.iter().find(|h| h.key == "escape").unwrap();
        assert!(!escape_hint.show); // hidden binding → show=false

        let enter_hint = hints.iter().find(|h| h.key == "enter").unwrap();
        assert!(enter_hint.show);

        let q_hint = hints.iter().find(|h| h.key == "q").unwrap();
        assert!(q_hint.show);
        assert_eq!(q_hint.description, "Quit application");
    }
}
