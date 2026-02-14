use crate::css::{AppRuntimePseudos, set_app_active, set_app_runtime_pseudos, set_style_context};
use crate::debug::{debug_input, debug_render};
use crate::event::{
    Action, AnimationEase, AnimationRequest, AnimationValueEvent, BlurEvent, Event, EventCtx,
    FocusEvent, MountEvent, MouseDownEvent, MouseScrollEvent, MouseUpEvent, ReadyEvent,
    UnmountEvent,
};
use crate::keys::KeyEventData;
use crate::message::{Message, MessageEvent};
use crate::worker::{WorkerRegistry, WorkerRequest, process_worker_requests};
use crossterm::event::{self, Event as CrosstermEvent, KeyEventKind, MouseEventKind};
use rich_rs::Renderable;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use super::App;
use super::devtools::DevtoolsCommand;
use super::helpers::{
    any_widget_active_tree, call_on_mouse_move_tree, collect_focus_chain_tree,
    generate_enter_leave_events, mouse_scroll_deltas, pointer_shape_for_hover_tree,
    should_quit_key, widget_at_tree_layout,
};
use super::render::apply_layout_info_tree;
use super::routing::{
    active_binding_hints_tree, dispatch_event, dispatch_event_broadcast_tree,
    dispatch_event_to_target_tree, dispatch_event_tree, dispatch_message_queue_tree,
    dispatch_mouse_scroll, dispatch_mouse_scroll_to_target_tree, dispatch_scroll_action_tree,
    focused_help_metadata_tree, focused_node_id_tree, is_priority_action, is_scroll_action,
    match_binding_tree,
};
use super::types::{DispatchOutcome, PendingInvalidation, StylesheetReload};
use crate::node_id::{NodeId, node_id_to_ffi};
use crate::reactive::RuntimeReactiveEntry;
use crate::widgets::Widget;

// ── Worker request accumulator ──────────────────────────────────────
//
// `absorb_outcome` is called from ~37 sites and we cannot add a
// `WorkerRegistry` field to `App` (defined in mod.rs).  Instead, each
// `absorb_outcome` call drains `outcome.worker_requests` into this
// thread-local.  The main loop drains the accumulator once per tick and
// feeds the requests to a function-local `WorkerRegistry`.

thread_local! {
    static WORKER_REQUEST_ACC: RefCell<Vec<WorkerRequest>> = const { RefCell::new(Vec::new()) };
}

/// Drain all worker requests accumulated during this tick.
fn drain_accumulated_worker_requests() -> Vec<WorkerRequest> {
    WORKER_REQUEST_ACC.with(|cell| std::mem::take(&mut *cell.borrow_mut()))
}

/// Push worker requests from an outcome into the thread-local accumulator.
fn accumulate_worker_requests(outcome: &mut DispatchOutcome) {
    let requests = std::mem::take(&mut outcome.worker_requests);
    if !requests.is_empty() {
        WORKER_REQUEST_ACC.with(|cell| cell.borrow_mut().extend(requests));
    }
}

fn should_dispatch_binding_hints(
    last_hints: &[crate::event::BindingHint],
    last_sources: &[NodeId],
    current_hints: &[crate::event::BindingHint],
    current_sources: &[NodeId],
) -> bool {
    last_hints != current_hints || last_sources != current_sources
}

fn should_dispatch_focused_help(
    last_source: Option<NodeId>,
    last_markup: Option<&str>,
    current_source: Option<NodeId>,
    current_markup: Option<&str>,
) -> bool {
    last_source != current_source || last_markup != current_markup
}

fn focused_help_message(current: Option<(NodeId, String)>) -> MessageEvent {
    if let Some((source, markup)) = current {
        MessageEvent {
            sender: source,
            message: Message::HelpPanelFocusedHelpChanged(
                crate::message::HelpPanelFocusedHelpChanged { source, markup },
            ),
            control: Some(source),
        }
    } else {
        let sender = App::runtime_message_sender();
        MessageEvent {
            sender,
            message: Message::HelpPanelFocusedHelpCleared(
                crate::message::HelpPanelFocusedHelpCleared,
            ),
            control: Some(sender),
        }
    }
}

fn worker_state_runtime_messages(
    registry: &WorkerRegistry,
    changes: Vec<crate::worker::WorkerStateChanged>,
) -> Vec<MessageEvent> {
    changes
        .into_iter()
        .map(|change| {
            let sender = registry
                .owner(change.worker_id)
                .unwrap_or_else(App::runtime_message_sender);
            MessageEvent {
                sender,
                message: Message::WorkerStateChanged(crate::message::WorkerStateChanged {
                    worker_id: change.worker_id,
                    state: change.state,
                }),
                control: Some(sender),
            }
        })
        .collect()
}

fn hit_probe_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("TEXTUAL_DEBUG_HIT_TEST_VERBOSE")
            .ok()
            .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
            .unwrap_or(false)
    })
}

fn point_direction(prev: Option<(u16, u16)>, curr: (u16, u16)) -> &'static str {
    let Some((px, py)) = prev else {
        return "start";
    };
    let dx = curr.0 as i32 - px as i32;
    let dy = curr.1 as i32 - py as i32;
    match (dx.signum(), dy.signum()) {
        (0, -1) => "up",
        (0, 1) => "down",
        (-1, 0) => "left",
        (1, 0) => "right",
        (1, -1) => "up-right",
        (-1, -1) => "up-left",
        (1, 1) => "down-right",
        (-1, 1) => "down-left",
        _ => "still",
    }
}

fn fmt_rect(rect: Option<crate::runtime::types::Rect>) -> String {
    match rect {
        Some(r) => format!("[{},{}..{},{}]", r.x0, r.y0, r.x1, r.y1),
        None => "-".to_string(),
    }
}

/// Coalesce consecutive mouse-motion events into the most recent position.
///
/// This prevents hover processing backlog when the pointer moves quickly:
/// we process the latest cursor location and preserve the first non-motion
/// event for normal handling in the next loop step.
fn coalesce_mouse_motion_events(
    mut mouse: crossterm::event::MouseEvent,
    pending_event: &mut Option<CrosstermEvent>,
) -> crate::Result<crossterm::event::MouseEvent> {
    loop {
        if !event::poll(Duration::ZERO)? {
            break;
        }
        match event::read()? {
            CrosstermEvent::Mouse(next)
                if matches!(next.kind, MouseEventKind::Moved | MouseEventKind::Drag(_)) =>
            {
                mouse = next;
            }
            other => {
                *pending_event = Some(other);
                break;
            }
        }
    }
    Ok(mouse)
}

fn collect_clipboard_runtime_messages(
    clipboard: &mut Option<String>,
    messages: &[MessageEvent],
) -> Vec<MessageEvent> {
    let mut system_clipboard = SystemClipboardBackend;
    collect_clipboard_runtime_messages_with_backend(clipboard, messages, &mut system_clipboard)
}

trait ClipboardBackend {
    fn copy(&mut self, text: &str) -> bool;
    fn paste(&mut self) -> Option<String>;
}

struct SystemClipboardBackend;

impl ClipboardBackend for SystemClipboardBackend {
    fn copy(&mut self, text: &str) -> bool {
        copy_to_system_clipboard(text)
    }

    fn paste(&mut self) -> Option<String> {
        paste_from_system_clipboard()
    }
}

fn collect_clipboard_runtime_messages_with_backend(
    clipboard: &mut Option<String>,
    messages: &[MessageEvent],
    backend: &mut impl ClipboardBackend,
) -> Vec<MessageEvent> {
    let mut generated = Vec::new();
    for event in messages {
        match &event.message {
            Message::TextEditClipboardCopyRequested(
                crate::message::TextEditClipboardCopyRequested { text, .. },
            ) => {
                *clipboard = Some(text.clone());
                if !backend.copy(text) {
                    debug_input("[clipboard] system copy unavailable; runtime fallback updated");
                }
            }
            Message::TextEditClipboardPasteRequested(
                crate::message::TextEditClipboardPasteRequested { target },
            ) => {
                let text = if let Some(system_text) = backend.paste() {
                    *clipboard = Some(system_text.clone());
                    Some(system_text)
                } else {
                    if clipboard.is_some() {
                        debug_input("[clipboard] system paste unavailable; using runtime fallback");
                    } else {
                        debug_input(
                            "[clipboard] paste requested with no system data and empty fallback",
                        );
                    }
                    clipboard.clone()
                };
                if let Some(text) = text {
                    generated.push(App::clipboard_message_event(*target, text));
                }
            }
            _ => {}
        }
    }
    generated
}

#[derive(Default)]
struct RuntimeMessagePass {
    deliver: Vec<MessageEvent>,
    generated: Vec<MessageEvent>,
    repaint_requested: bool,
    invalidation: crate::event::InvalidationFlags,
}

fn set_overlay_modal_display_tree(
    tree: &mut crate::widget_tree::WidgetTree,
    overlay: NodeId,
    visible: bool,
) -> bool {
    let modal_root = match tree.children(overlay).get(1).copied() {
        Some(id) => id,
        None => return false,
    };
    let node_ids = tree.walk_depth_first(modal_root);
    let mut changed = false;
    for node_id in node_ids {
        let before = tree.is_displayed(node_id);
        tree.set_runtime_display(node_id, visible);
        if before != tree.is_displayed(node_id) {
            changed = true;
        }
    }
    changed
}

fn sync_widget_controlled_child_display_tree(tree: &mut crate::widget_tree::WidgetTree) -> bool {
    let Some(root) = tree.root() else {
        return false;
    };

    let mut updates: Vec<(NodeId, bool)> = Vec::new();
    for parent_id in tree.walk_depth_first(root) {
        let child_ids = tree.children(parent_id).to_vec();
        if child_ids.is_empty() {
            continue;
        }
        let Some(parent) = tree.get(parent_id) else {
            continue;
        };
        for (idx, child_id) in child_ids.into_iter().enumerate() {
            if let Some(display) = parent.widget.child_display_for_tree(idx) {
                updates.push((child_id, display));
            }
        }
    }

    let mut changed = false;
    for (node_id, display) in updates {
        let before = tree.is_displayed(node_id);
        tree.set_runtime_display(node_id, display);
        if !display && let Some(node) = tree.get_mut(node_id) {
            node.widget.set_focus(false);
        }
        if before != tree.is_displayed(node_id) {
            changed = true;
        }
    }
    changed
}

fn split_runtime_control_messages(app: &mut App, queue: Vec<MessageEvent>) -> RuntimeMessagePass {
    let mut pass = RuntimeMessagePass::default();
    for event in queue {
        match event.message {
            Message::AsyncTaskSpawn(crate::message::AsyncTaskSpawn {
                task_id,
                target,
                request,
            }) => {
                if let Some(cancelled) = app.async_tasks.spawn(task_id, target, request) {
                    pass.generated.push(cancelled);
                }
            }
            Message::AsyncTaskCancel(crate::message::AsyncTaskCancel { task_id }) => {
                if let Some(cancelled) = app.async_tasks.cancel(task_id) {
                    pass.generated.push(cancelled);
                }
            }
            Message::AsyncTaskCancelTarget(crate::message::AsyncTaskCancelTarget { target }) => {
                pass.generated
                    .extend(app.async_tasks.cancel_for_target(target));
            }
            Message::TimerSchedule(crate::message::TimerSchedule {
                timer_id,
                target,
                delay,
            }) => {
                if let Some(cancelled) = app.one_shot_timers.schedule(timer_id, target, delay) {
                    pass.generated.push(cancelled);
                }
            }
            Message::TimerCancel(crate::message::TimerCancel { timer_id }) => {
                if let Some(cancelled) = app.one_shot_timers.cancel(timer_id) {
                    pass.generated.push(cancelled);
                }
            }
            Message::AppAddClass(crate::message::AppAddClass {
                selector,
                class_name,
            }) => match app.action_add_class(&selector, &class_name) {
                Ok(matched) if matched > 0 => {
                    pass.repaint_requested = true;
                    pass.invalidation
                        .merge(crate::event::InvalidationFlags::layout());
                }
                Ok(_) => {}
                Err(err) => {
                    debug_input(&format!(
                        "[runtime] app.add_class ignored selector={selector:?} class={class_name:?} err={err:?}"
                    ));
                }
            },
            Message::AppRemoveClass(crate::message::AppRemoveClass {
                selector,
                class_name,
            }) => match app.action_remove_class(&selector, &class_name) {
                Ok(matched) if matched > 0 => {
                    pass.repaint_requested = true;
                    pass.invalidation
                        .merge(crate::event::InvalidationFlags::layout());
                }
                Ok(_) => {}
                Err(err) => {
                    debug_input(&format!(
                        "[runtime] app.remove_class ignored selector={selector:?} class={class_name:?} err={err:?}"
                    ));
                }
            },
            Message::AppToggleClass(crate::message::AppToggleClass {
                selector,
                class_name,
            }) => match app.action_toggle_class(&selector, &class_name) {
                Ok(matched) if matched > 0 => {
                    pass.repaint_requested = true;
                    pass.invalidation
                        .merge(crate::event::InvalidationFlags::layout());
                }
                Ok(_) => {}
                Err(err) => {
                    debug_input(&format!(
                        "[runtime] app.toggle_class ignored selector={selector:?} class={class_name:?} err={err:?}"
                    ));
                }
            },
            Message::OverlayVisibilityChanged(crate::message::OverlayVisibilityChanged {
                overlay,
                visible,
            }) => {
                if let Some(tree) = app.widget_tree.as_mut()
                    && set_overlay_modal_display_tree(tree, overlay, visible)
                {
                    pass.repaint_requested = true;
                    pass.invalidation
                        .merge(crate::event::InvalidationFlags::layout());
                }
                pass.deliver.push(event);
            }
            _ => pass.deliver.push(event),
        }
    }
    pass.generated
        .extend(app.one_shot_timers.drain_ready(Instant::now()));
    pass.generated.extend(app.async_tasks.drain_completed());
    pass
}

#[derive(Clone)]
struct SelectorSnapshot {
    type_name: String,
    style_id: Option<String>,
    classes: Vec<String>,
    disabled: bool,
    focused: bool,
    hovered: bool,
    active: bool,
    inline: bool,
    ansi: bool,
    nocolor: bool,
}

fn snapshot_for(
    widget: &dyn Widget,
    _node_id: NodeId,
    app_active: bool,
    app_pseudos: AppRuntimePseudos,
) -> SelectorSnapshot {
    SelectorSnapshot {
        type_name: widget.style_type().to_string(),
        style_id: widget.style_id().map(str::to_string),
        classes: widget.style_classes().to_vec(),
        disabled: widget.is_disabled(),
        focused: widget.has_focus() && app_active,
        hovered: widget.is_hovered(),
        active: widget.is_active(),
        inline: app_pseudos.inline,
        ansi: app_pseudos.ansi,
        nocolor: app_pseudos.nocolor,
    }
}

fn selector_matches_snapshot(
    selector: &crate::css::StyleSelector,
    meta: &SelectorSnapshot,
) -> bool {
    if let Some(type_name) = selector.type_name() {
        if meta.type_name != type_name {
            return false;
        }
    }
    if let Some(id) = selector.id_name() {
        if meta.style_id.as_deref() != Some(id) {
            return false;
        }
    }
    if !selector.classes().is_empty() {
        if !selector
            .classes()
            .iter()
            .all(|class| meta.classes.iter().any(|value| value == class))
        {
            return false;
        }
    }
    for pseudo in selector.pseudos() {
        let ok = match pseudo {
            crate::css::PseudoClass::Disabled => meta.disabled,
            crate::css::PseudoClass::Focus => meta.focused,
            crate::css::PseudoClass::Hover => meta.hovered,
            crate::css::PseudoClass::Active => meta.active,
            crate::css::PseudoClass::Blur => !meta.focused,
            crate::css::PseudoClass::Inline => meta.inline,
            crate::css::PseudoClass::Ansi => meta.ansi,
            crate::css::PseudoClass::NoColor => meta.nocolor,
            // Dark/Light/Even/Odd/FirstChild/LastChild are CSS-only pseudo-classes
            // handled by the selector matching engine; in the event_loop quick-check
            // they are treated as non-matching since per-widget state isn't available.
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    true
}

fn rule_matches_snapshot_chain(
    rule: &crate::css::StyleRule,
    current: &SelectorSnapshot,
    ancestors: &[SelectorSnapshot],
) -> bool {
    let chain = rule.selector_chain();
    let parts = chain.parts();
    if parts.is_empty() {
        return false;
    }
    let last = parts.last().expect("parts not empty");
    if !selector_matches_snapshot(last, current) {
        return false;
    }
    if parts.len() == 1 {
        return true;
    }

    let combinators = chain.combinators();
    let mut idx = ancestors.len() as isize - 1;
    if idx < 0 {
        return false;
    }
    for (part_index, selector) in parts[..parts.len() - 1].iter().rev().enumerate() {
        let combinator = combinators[combinators.len() - 1 - part_index];
        match combinator {
            crate::css::Combinator::Child => {
                let meta = &ancestors[idx as usize];
                if !selector_matches_snapshot(selector, meta) {
                    return false;
                }
                idx -= 1;
            }
            crate::css::Combinator::Descendant => {
                let mut found = false;
                let mut current_idx = idx;
                while current_idx >= 0 {
                    let meta = &ancestors[current_idx as usize];
                    if selector_matches_snapshot(selector, meta) {
                        found = true;
                        idx = current_idx - 1;
                        break;
                    }
                    current_idx -= 1;
                }
                if !found {
                    return false;
                }
            }
        }
    }
    true
}

/// Root-only stylesheet invalidation check.
///
/// Only tests the root widget against changed rules. Child widgets are handled
/// by the tree-based version when the arena tree is available.
fn collect_stylesheet_affected_widgets_root(
    root: &dyn Widget,
    changed_rules: &[crate::css::StyleRule],
    app_active: bool,
    app_pseudos: AppRuntimePseudos,
) -> Vec<NodeId> {
    if changed_rules.is_empty() {
        return Vec::new();
    }
    let current = snapshot_for(root, NodeId::default(), app_active, app_pseudos);
    if changed_rules
        .iter()
        .any(|rule| rule_matches_snapshot_chain(rule, &current, &[]))
    {
        vec![NodeId::default()]
    } else {
        Vec::new()
    }
}

/// Tree-based stylesheet invalidation: walk the arena tree depth-first and
/// collect all nodes whose selectors match any of the changed CSS rules.
///
/// Builds an ancestor snapshot chain per node so descendant/child combinators
/// in selectors are evaluated correctly.
fn collect_stylesheet_affected_widgets_tree(
    tree: &crate::widget_tree::WidgetTree,
    changed_rules: &[crate::css::StyleRule],
    app_active: bool,
    app_pseudos: AppRuntimePseudos,
) -> Vec<NodeId> {
    if changed_rules.is_empty() {
        return Vec::new();
    }
    let root = match tree.root() {
        Some(r) => r,
        None => return Vec::new(),
    };

    let mut affected = HashSet::new();
    // Recursive visitor that maintains an ancestor chain for selector matching.
    fn visit(
        tree: &crate::widget_tree::WidgetTree,
        node_id: NodeId,
        rules: &[crate::css::StyleRule],
        app_active: bool,
        app_pseudos: AppRuntimePseudos,
        ancestors: &mut Vec<SelectorSnapshot>,
        affected: &mut HashSet<NodeId>,
    ) {
        let Some(node) = tree.get(node_id) else {
            return;
        };
        let current = snapshot_for(node.widget.as_ref(), node_id, app_active, app_pseudos);
        if rules
            .iter()
            .any(|rule| rule_matches_snapshot_chain(rule, &current, ancestors))
        {
            affected.insert(node_id);
        }
        ancestors.push(current);
        for &child_id in tree.children(node_id) {
            visit(
                tree,
                child_id,
                rules,
                app_active,
                app_pseudos,
                ancestors,
                affected,
            );
        }
        ancestors.pop();
    }

    let mut ancestors = Vec::new();
    visit(
        tree,
        root,
        changed_rules,
        app_active,
        app_pseudos,
        &mut ancestors,
        &mut affected,
    );

    let mut out = affected.into_iter().collect::<Vec<_>>();
    out.sort_by_key(|id| node_id_to_ffi(*id));
    out
}

/// Resolve per-property transition parameters from a CSS [`Style`].
///
/// Checks the `transitions` vec first for a matching property name (or `"all"`).
/// Falls back to the generic `transition-duration / delay / timing` properties.
///
/// Returns `(duration, delay, ease)` if a transition should be applied;
/// `None` if the resolved duration is zero or absent.
pub fn resolve_transition_for_property(
    style: &crate::style::Style,
    property: &str,
) -> Option<(Duration, Duration, AnimationEase)> {
    // Per-property transitions take priority (P2-36).
    if let Some(ref transitions) = style.transitions {
        // Prefer a specific property match over the "all" wildcard.
        if let Some(pt) = transitions
            .iter()
            .find(|t| t.property == property)
            .or_else(|| transitions.iter().find(|t| t.property == "all"))
        {
            if pt.duration.is_zero() {
                return None;
            }
            let ease = transition_timing_to_ease(pt.timing);
            return Some((pt.duration, pt.delay, ease));
        }
    }

    // Fall back to generic transition properties.
    let duration = style.transition_duration?;
    if duration.is_zero() {
        return None;
    }
    let delay = style.transition_delay.unwrap_or(Duration::ZERO);
    let ease = style
        .transition_timing
        .map(transition_timing_to_ease)
        .unwrap_or(AnimationEase::OutCubic);
    Some((duration, delay, ease))
}

fn canonical_transition_property_name(property: &str) -> String {
    property.trim().to_ascii_lowercase().replace('-', "_")
}

fn style_numeric_property(style: &crate::style::Style, property: &str) -> Option<f32> {
    match canonical_transition_property_name(property).as_str() {
        "opacity" => Some(style.opacity.unwrap_or(100) as f32),
        "text_opacity" => Some(style.text_opacity.unwrap_or(100) as f32),
        "offset_x" => style.offset.map(|offset| match offset.x {
            crate::style::OffsetValue::Cells(v) => v as f32,
            crate::style::OffsetValue::Percent(v) => v,
        }),
        "offset_y" => style.offset.map(|offset| match offset.y {
            crate::style::OffsetValue::Cells(v) => v as f32,
            crate::style::OffsetValue::Percent(v) => v,
        }),
        _ => None,
    }
}

fn resolve_transition_for_property_aliases(
    style: &crate::style::Style,
    property: &str,
) -> Option<(Duration, Duration, AnimationEase)> {
    if let Some(found) = resolve_transition_for_property(style, property) {
        return Some(found);
    }
    let canonical = canonical_transition_property_name(property);
    if let Some(found) = resolve_transition_for_property(style, &canonical) {
        return Some(found);
    }
    let dashed = canonical.replace('_', "-");
    if dashed != canonical {
        return resolve_transition_for_property(style, &dashed);
    }
    None
}

fn transition_requests_for_style_change(
    target: NodeId,
    previous: &crate::style::Style,
    current: &crate::style::Style,
) -> Vec<AnimationRequest> {
    if previous == current {
        return Vec::new();
    }

    // Explicitly supported animatable style properties (P2-36 initial runtime scope).
    const ANIMATABLE_PROPERTIES: [&str; 4] = ["opacity", "text_opacity", "offset_x", "offset_y"];

    ANIMATABLE_PROPERTIES
        .iter()
        .filter_map(|property| {
            let from = style_numeric_property(previous, property)?;
            let to = style_numeric_property(current, property)?;
            if (from - to).abs() < f32::EPSILON {
                return None;
            }
            let (duration, delay, ease) =
                resolve_transition_for_property_aliases(current, property)?;
            Some(
                AnimationRequest::new(target, *property, from, to, duration)
                    .with_delay(delay)
                    .with_ease(ease)
                    .with_level(crate::event::AnimationLevel::Basic),
            )
        })
        .collect()
}

fn transition_timing_to_ease(timing: crate::style::TransitionTiming) -> AnimationEase {
    match timing {
        crate::style::TransitionTiming::Linear => AnimationEase::Linear,
        crate::style::TransitionTiming::InOutCubic => AnimationEase::InOutCubic,
        crate::style::TransitionTiming::OutCubic => AnimationEase::OutCubic,
        crate::style::TransitionTiming::Round => AnimationEase::Round,
        crate::style::TransitionTiming::None => AnimationEase::None,
    }
}

fn sanitize_snapshot_field(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\n' | '\r' | '\t' | '|' => ' ',
            _ => ch,
        })
        .collect()
}

fn bool_flag(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

#[derive(Clone, Copy)]
enum InvalidationScope {
    Global,
    Widget(NodeId),
}

fn copy_to_system_clipboard(text: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        return run_copy_command("pbcopy", &[], text);
    }

    #[cfg(target_os = "windows")]
    {
        return run_copy_command(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Set-Clipboard -Value ([Console]::In.ReadToEnd())",
            ],
            text,
        ) || run_copy_command("cmd", &["/C", "clip"], text);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return run_copy_command("wl-copy", &[], text)
            || run_copy_command("xclip", &["-selection", "clipboard"], text)
            || run_copy_command("xsel", &["--clipboard", "--input"], text);
    }

    #[cfg(not(any(unix, target_os = "windows")))]
    {
        let _ = text;
        false
    }
}

fn paste_from_system_clipboard() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        return run_paste_command("pbpaste", &[]);
    }

    #[cfg(target_os = "windows")]
    {
        return run_paste_command(
            "powershell",
            &["-NoProfile", "-Command", "Get-Clipboard -Raw"],
        )
        .or_else(|| run_paste_command("powershell", &["-NoProfile", "-Command", "Get-Clipboard"]));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return run_paste_command("wl-paste", &["-n"])
            .or_else(|| run_paste_command("xclip", &["-selection", "clipboard", "-o"]))
            .or_else(|| run_paste_command("xsel", &["--clipboard", "--output"]));
    }

    #[cfg(not(any(unix, target_os = "windows")))]
    {
        None
    }
}

fn run_copy_command(program: &str, args: &[&str], text: &str) -> bool {
    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };

    let write_ok = match child.stdin.take() {
        Some(mut stdin) => stdin.write_all(text.as_bytes()).is_ok(),
        None => false,
    };
    if !write_ok {
        let _ = child.kill();
        let _ = child.wait();
        return false;
    }

    matches!(child.wait(), Ok(status) if status.success())
}

fn run_paste_command(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    if text.is_empty() {
        return None;
    }
    Some(text)
}

impl App {
    fn apply_devtools_commands(
        &mut self,
        _root: &mut dyn Widget,
        pending_invalidation: &mut PendingInvalidation,
    ) -> bool {
        let Some(devtools) = &self.devtools else {
            return false;
        };
        let commands = devtools.drain_commands();
        if commands.is_empty() {
            return false;
        }
        for command in commands {
            match command {
                DevtoolsCommand::Focus(id) => {
                    // Tree-based focus: set focus on the tree node directly.
                    if let Some(tree) = self.widget_tree.as_mut() {
                        if let Some(node) = tree.get_mut(id) {
                            node.widget.set_focus(true);
                        }
                    }
                    pending_invalidation.request_full_content();
                }
                DevtoolsCommand::SetDebugLayout(enabled) => {
                    self.enable_debug_layout(enabled);
                    pending_invalidation.request_full_content();
                }
                DevtoolsCommand::Quit => {
                    return true;
                }
            }
        }
        false
    }

    fn publish_devtools_snapshot(&mut self, root: &mut dyn Widget) {
        let Some(devtools) = &self.devtools else {
            return;
        };

        fn snapshot_widget_line(
            widget: &dyn Widget,
            id: NodeId,
            depth: usize,
            app_active: bool,
            hovered: Option<NodeId>,
            hit_test: &crate::runtime::types::HitTestMap,
        ) -> (String, bool) {
            let focused = widget.has_focus() && app_active;
            let rect = hit_test.rect(id);
            let rect_field = if let Some(rect) = rect {
                format!("{},{},{},{}", rect.x0, rect.y0, rect.x1, rect.y1)
            } else {
                "-".to_string()
            };
            let style_id = widget
                .style_id()
                .map(sanitize_snapshot_field)
                .unwrap_or_else(|| "-".to_string());
            let classes = widget
                .style_classes()
                .iter()
                .map(|class| sanitize_snapshot_field(class))
                .collect::<Vec<_>>()
                .join(",");
            let line = format!(
                "widget\t{depth}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                node_id_to_ffi(id),
                sanitize_snapshot_field(widget.style_type()),
                style_id,
                classes,
                bool_flag(focused),
                bool_flag(hovered == Some(id)),
                bool_flag(widget.is_active()),
                bool_flag(widget.is_disabled()),
                rect_field
            );
            (line, focused)
        }

        let mut widget_lines = Vec::new();
        let mut focused = None;

        // Tree-based: walk the arena tree depth-first.
        if let Some(tree) = &self.widget_tree {
            if let Some(root_id) = tree.root() {
                let walk = tree.walk_depth_first(root_id);
                for node_id in walk {
                    let Some(node) = tree.get(node_id) else {
                        continue;
                    };
                    let depth = tree.ancestors(node_id).len();
                    let (line, is_focused) = snapshot_widget_line(
                        node.widget.as_ref(),
                        node_id,
                        depth,
                        self.app_active,
                        self.hovered,
                        &self.hit_test,
                    );
                    widget_lines.push(line);
                    if is_focused {
                        focused = Some(node_id);
                    }
                }
            }
        } else {
            // Root-only fallback: just the root widget.
            let (line, is_focused) = snapshot_widget_line(
                root,
                NodeId::default(),
                0,
                self.app_active,
                self.hovered,
                &self.hit_test,
            );
            widget_lines.push(line);
            if is_focused {
                focused = Some(NodeId::default());
            }
        }

        let mut snapshot = String::new();
        snapshot.push_str("version\t1\n");
        snapshot.push_str(&format!("pid\t{}\n", std::process::id()));
        snapshot.push_str(&format!("app_active\t{}\n", bool_flag(self.app_active)));
        snapshot.push_str(&format!(
            "debug_layout\t{}\n",
            bool_flag(self.debug_layout.enabled)
        ));
        snapshot.push_str(&format!(
            "frame\t{}\t{}\n",
            self.frame.width, self.frame.height
        ));
        snapshot.push_str(&format!(
            "hovered\t{}\n",
            self.hovered
                .map(|id| node_id_to_ffi(id).to_string())
                .unwrap_or_else(|| "-".to_string())
        ));
        snapshot.push_str(&format!(
            "focused\t{}\n",
            focused
                .map(|id| node_id_to_ffi(id).to_string())
                .unwrap_or_else(|| "-".to_string())
        ));
        snapshot.push_str(&format!("widget_count\t{}\n", widget_lines.len()));
        for hint in &self.last_binding_hints {
            snapshot.push_str(&format!(
                "hint\t{}\t{}\n",
                sanitize_snapshot_field(&hint.key),
                sanitize_snapshot_field(&hint.description)
            ));
        }
        for line in widget_lines {
            snapshot.push_str(&line);
            snapshot.push('\n');
        }
        devtools.publish_snapshot(snapshot);
    }

    fn dispatch_message_queue_with_runtime(
        &mut self,
        root: &mut dyn Widget,
        initial: Vec<MessageEvent>,
    ) -> DispatchOutcome {
        let mut aggregate = DispatchOutcome::default();
        let mut queue = initial;
        loop {
            let pass = split_runtime_control_messages(self, queue);
            aggregate.repaint_requested |= pass.repaint_requested;
            aggregate.invalidation.merge(pass.invalidation);
            let mut runtime_messages =
                collect_clipboard_runtime_messages(&mut self.clipboard, &pass.deliver);
            runtime_messages.extend(pass.generated);
            let mut outcome = if pass.deliver.is_empty() {
                DispatchOutcome::default()
            } else {
                self.dispatch_message_queue_auto(root, pass.deliver)
            };
            aggregate.handled |= outcome.handled;
            aggregate.repaint_requested |= outcome.repaint_requested;
            aggregate.invalidation.merge(outcome.invalidation);
            aggregate.stop_requested |= outcome.stop_requested;
            aggregate.default_prevented |= outcome.default_prevented;
            aggregate.messages.append(&mut outcome.messages);
            aggregate
                .animation_requests
                .append(&mut outcome.animation_requests);
            aggregate
                .worker_requests
                .append(&mut outcome.worker_requests);

            if aggregate.stop_requested || runtime_messages.is_empty() {
                break;
            }
            queue = runtime_messages;
        }
        aggregate
    }

    fn dispatch_background_runtime_messages(&mut self, root: &mut dyn Widget) -> DispatchOutcome {
        let mut queue = self.one_shot_timers.drain_ready(Instant::now());
        queue.extend(self.async_tasks.drain_completed());
        self.dispatch_message_queue_with_runtime(root, queue)
    }

    pub async fn run_with<F, R>(&mut self, mut render: F) -> crate::Result<()>
    where
        F: FnMut(&mut App, u64) -> R,
        R: Renderable,
    {
        if !self.running {
            return Err(crate::Error::RuntimeStopped);
        }

        self.start()?;

        let mut tick: u64 = 0;
        let tick_rate = Duration::from_millis(100);
        let mut last_render = Instant::now();

        loop {
            let timeout = tick_rate.saturating_sub(last_render.elapsed());
            if event::poll(timeout)? {
                match event::read()? {
                    CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                        if matches!(
                            key.code,
                            crossterm::event::KeyCode::Enter | crossterm::event::KeyCode::Char(' ')
                        ) {
                            debug_input(&format!("[input] key {:?}", key.code));
                        }
                        if should_quit_key(&key, &self.quit_keys) {
                            break;
                        }
                    }
                    CrosstermEvent::Resize(_, _) => {
                        self.refresh_size()?;
                    }
                    _ => {}
                }
            }

            if last_render.elapsed() >= tick_rate {
                let _ = self.poll_stylesheet();
                let renderable = render(self, tick);
                self.render(&renderable)?;
                tick += 1;
                last_render = Instant::now();
            }
        }

        self.finish()?;
        Ok(())
    }

    pub async fn run_widget_tree(&mut self, root: &mut dyn Widget) -> crate::Result<()> {
        if !self.running {
            return Err(crate::Error::RuntimeStopped);
        }

        self.start()?;
        root.on_mount();

        // Build the arena-based widget tree by extracting children from root.
        // If children are found (via take_composed_children or compose),
        // tree mode becomes active; otherwise runtime stays in root-only mode.
        self.build_widget_tree(root);
        if let Some(tree) = self.widget_tree.as_mut() {
            let _ = sync_widget_controlled_child_display_tree(tree);
        }
        self.style_snapshot_cache.clear();

        // Auto-focus the first focusable widget via the arena tree.
        if let Some(tree) = &mut self.widget_tree {
            let focus_chain = collect_focus_chain_tree(tree);
            if let Some(&first) = focus_chain.first() {
                if let Some(node) = tree.get_mut(first) {
                    node.widget.set_focus(true);
                }
            }
        }
        self.publish_devtools_snapshot(root);
        let initial_help_outcome = self.dispatch_focused_help_changed(root);
        if initial_help_outcome.stop_requested {
            root.on_unmount();
            self.finish()?;
            return Ok(());
        }

        let mut tick: u64 = 0;
        let idle_tick_rate = Duration::from_millis(100);
        let active_tick_rate = Duration::from_millis(16);
        let mut worker_registry = WorkerRegistry::new();
        let mut pending_invalidation = PendingInvalidation::default();
        pending_invalidation.request_flags(initial_help_outcome.invalidation);
        if initial_help_outcome.should_repaint() {
            pending_invalidation.request_full_content();
        }
        let mut prev_any_active = false;
        self.render_widget(root)?;
        self.apply_layout_info_to_tree();
        self.publish_devtools_snapshot(root);
        pending_invalidation = PendingInvalidation::default();

        // Dispatch initial Mount events for all tree nodes after first render.
        let initial_mount_nodes: Vec<NodeId> = self
            .widget_tree
            .as_ref()
            .and_then(|tree| tree.root().map(|r| tree.walk_depth_first(r)))
            .unwrap_or_default();
        for node_id in initial_mount_nodes {
            let mut outcome = self.dispatch_event_to_target_auto(
                root,
                node_id,
                &Event::Mount(MountEvent { node: node_id }),
            );
            self.absorb_outcome(
                &mut outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            let mut msg_outcome = self.dispatch_message_queue_with_runtime(root, outcome.messages);
            self.absorb_outcome(
                &mut msg_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
        }

        // Dispatch Ready event once after the first successful render.
        {
            let mut outcome = self.dispatch_event_auto(root, Event::Ready(ReadyEvent));
            self.absorb_outcome(
                &mut outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            let mut msg_outcome = self.dispatch_message_queue_with_runtime(root, outcome.messages);
            self.absorb_outcome(
                &mut msg_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
        }

        // Seed style snapshot cache after startup lifecycle events so initial
        // class/style setup doesn't emit synthetic transition requests.
        self.dispatch_style_transition_requests(root);

        // Track focused widget for Focus/Blur event dispatch.
        let mut previous_focus: Option<NodeId> =
            self.widget_tree.as_ref().and_then(focused_node_id_tree);

        let mut last_render = Instant::now();
        let mut pending_input_event: Option<CrosstermEvent> = None;
        let mut last_mouse_pos: Option<(u16, u16)> = None;

        'event_loop: loop {
            if self.apply_devtools_commands(root, &mut pending_invalidation) {
                break 'event_loop;
            }
            let now = Instant::now();
            let has_runtime_animation = self.animator.has_animations();
            let tick_rate = if has_runtime_animation || prev_any_active {
                active_tick_rate
            } else {
                idle_tick_rate
            };
            let tick_timeout = tick_rate.saturating_sub(last_render.elapsed());
            let timeout = self
                .animator
                .next_timeout(now)
                .map(|anim_timeout| tick_timeout.min(anim_timeout))
                .unwrap_or(tick_timeout);
            let timeout = self
                .one_shot_timers
                .next_timeout(now)
                .map(|timer_timeout| timeout.min(timer_timeout))
                .unwrap_or(timeout);
            let input_event = if let Some(pending) = pending_input_event.take() {
                Some(pending)
            } else if event::poll(timeout)? {
                Some(event::read()?)
            } else {
                None
            };

            if let Some(input_event) = input_event {
                let mut sheet = self.default_stylesheet.clone();
                sheet.extend(&self.stylesheet);
                let _active = set_app_active(self.app_active);
                let _pseudo_state = set_app_runtime_pseudos(AppRuntimePseudos {
                    inline: self.app_inline,
                    ansi: self.app_ansi,
                    nocolor: self.app_nocolor,
                });
                let _guard = set_style_context(sheet);
                match input_event {
                    CrosstermEvent::Key(key) => {
                        debug_input(&format!(
                            "[input] key code={:?} mods={:?} kind={:?}",
                            key.code, key.modifiers, key.kind
                        ));
                        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                            continue;
                        }
                        if should_quit_key(&key, &self.quit_keys) {
                            break;
                        }
                        let key = KeyEventData::from_crossterm(key);

                        // App-level key hook with runtime handle (Textual-style).
                        let mut app_key_ctx = EventCtx::default();
                        root.on_app_key(self, &key, &mut app_key_ctx);
                        if app_key_ctx.repaint_requested() {
                            pending_invalidation.request_full_content();
                        }
                        pending_invalidation.request_flags(app_key_ctx.invalidation());
                        if app_key_ctx.stop_requested() {
                            break 'event_loop;
                        }
                        let app_key_handled = app_key_ctx.handled();
                        let app_key_messages = app_key_ctx.take_messages();
                        if !app_key_messages.is_empty() {
                            let mut msg_outcome =
                                self.dispatch_message_queue_with_runtime(root, app_key_messages);
                            self.absorb_outcome(
                                &mut msg_outcome,
                                &mut pending_invalidation,
                                InvalidationScope::Global,
                            );
                            if msg_outcome.stop_requested {
                                break 'event_loop;
                            }
                        }
                        if app_key_handled {
                            continue;
                        }

                        let bind = crate::event::KeyBind::from_event(&key);
                        let mapped_action = self.action_map.lookup(&bind);

                        // Priority actions (e.g. command palette) run before raw key dispatch.
                        if let Some(action) = mapped_action.filter(|a| is_priority_action(*a)) {
                            debug_input(&format!(
                                "[input] priority action-map {:?} -> {:?}",
                                bind, action
                            ));
                            let mut outcome = self.dispatch_event_auto(root, Event::Action(action));
                            debug_input(&format!(
                                "[input] priority action dispatch action={:?} handled={} repaint={} messages={}",
                                action,
                                outcome.handled,
                                outcome.repaint_requested,
                                outcome.messages.len()
                            ));
                            self.absorb_outcome(
                                &mut outcome,
                                &mut pending_invalidation,
                                InvalidationScope::Global,
                            );
                            let mut msg_outcome =
                                self.dispatch_message_queue_with_runtime(root, outcome.messages);
                            self.absorb_outcome(
                                &mut msg_outcome,
                                &mut pending_invalidation,
                                InvalidationScope::Global,
                            );
                            if outcome.stop_requested || msg_outcome.stop_requested {
                                break 'event_loop;
                            }
                            if outcome.handled {
                                continue;
                            }
                        }

                        // Declarative BINDINGS: check focused widget chain for matching binding.
                        if let Some(tree) = self.widget_tree.as_ref() {
                            if let Some((_node_id, action_str)) = match_binding_tree(tree, &key) {
                                if let Some(parsed) = crate::action::parse_action(&action_str) {
                                    if let Some(tree_mut) = self.widget_tree.as_mut() {
                                        let focused = focused_node_id_tree(tree_mut);
                                        let resolved = {
                                            let tree_ref = &*tree_mut;
                                            focused.and_then(|fid| {
                                                crate::action::resolve_action(
                                                    &parsed,
                                                    tree_ref,
                                                    fid,
                                                    |nid| {
                                                        tree_ref.get(nid).map(|n| {
                                                            (
                                                                n.widget.action_namespace(),
                                                                n.widget.action_registry(),
                                                            )
                                                        })
                                                    },
                                                )
                                            })
                                        };
                                        if let Some(ra) = resolved {
                                            let mut ctx = EventCtx::default();
                                            if let Some(node) = tree_mut.get_mut(ra.node) {
                                                let handled =
                                                    node.widget.execute_action(&parsed, &mut ctx);
                                                debug_input(&format!(
                                                    "[input] binding action={action_str:?} handled={handled}"
                                                ));
                                                if handled || ctx.handled() {
                                                    if ctx.repaint_requested() {
                                                        pending_invalidation.request_full_content();
                                                    }
                                                    let messages = ctx.take_messages();
                                                    if !messages.is_empty() {
                                                        let mut msg_outcome = self
                                                            .dispatch_message_queue_with_runtime(
                                                                root, messages,
                                                            );
                                                        self.absorb_outcome(
                                                            &mut msg_outcome,
                                                            &mut pending_invalidation,
                                                            InvalidationScope::Global,
                                                        );
                                                        if msg_outcome.stop_requested {
                                                            break 'event_loop;
                                                        }
                                                    }
                                                    if ctx.stop_requested() {
                                                        break 'event_loop;
                                                    }
                                                    continue;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Dispatch the raw key so focused widgets (e.g. Input) can consume it.
                        let mut key_outcome =
                            self.dispatch_event_auto(root, Event::Key(key.clone()));
                        debug_input(&format!(
                            "[input] key dispatch handled={} repaint={} messages={}",
                            key_outcome.handled,
                            key_outcome.repaint_requested,
                            key_outcome.messages.len()
                        ));
                        self.absorb_outcome(
                            &mut key_outcome,
                            &mut pending_invalidation,
                            InvalidationScope::Global,
                        );
                        let mut msg_outcome =
                            self.dispatch_message_queue_with_runtime(root, key_outcome.messages);
                        self.absorb_outcome(
                            &mut msg_outcome,
                            &mut pending_invalidation,
                            InvalidationScope::Global,
                        );
                        if key_outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                        if !key_outcome.handled {
                            if let Some(action) = mapped_action.filter(|a| !is_priority_action(*a))
                            {
                                if action == Action::HelpQuit {
                                    self.notify_help_quit();
                                    pending_invalidation.request_full_content();
                                    continue;
                                }
                                if matches!(action, Action::FocusNext | Action::FocusPrev) {
                                    // Give the currently-focused branch a chance to descend
                                    // focus into non-tree descendants (legacy wrappers)
                                    // before falling back to tree-level focus cycling.
                                    let mut focus_outcome =
                                        self.dispatch_event_auto(root, Event::Action(action));
                                    self.absorb_outcome(
                                        &mut focus_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    let mut focus_msg_outcome = self
                                        .dispatch_message_queue_with_runtime(
                                            root,
                                            focus_outcome.messages,
                                        );
                                    self.absorb_outcome(
                                        &mut focus_msg_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    if focus_outcome.stop_requested
                                        || focus_msg_outcome.stop_requested
                                    {
                                        break 'event_loop;
                                    }
                                    if focus_outcome.handled {
                                        continue;
                                    }
                                    if self.move_focus_auto(action) {
                                        pending_invalidation.request_full_content();
                                        continue;
                                    }
                                }
                                debug_input(&format!(
                                    "[input] action-map {:?} -> {:?}",
                                    bind, action
                                ));
                                let mut outcome = if is_scroll_action(action) {
                                    self.dispatch_scroll_action_auto(root, action, self.hovered)
                                } else {
                                    self.dispatch_event_auto(root, Event::Action(action))
                                };
                                debug_input(&format!(
                                    "[input] action dispatch action={:?} handled={} repaint={} messages={}",
                                    action,
                                    outcome.handled,
                                    outcome.repaint_requested,
                                    outcome.messages.len()
                                ));
                                self.absorb_outcome(
                                    &mut outcome,
                                    &mut pending_invalidation,
                                    InvalidationScope::Global,
                                );
                                let mut msg_outcome = self
                                    .dispatch_message_queue_with_runtime(root, outcome.messages);
                                self.absorb_outcome(
                                    &mut msg_outcome,
                                    &mut pending_invalidation,
                                    InvalidationScope::Global,
                                );
                                if outcome.stop_requested || msg_outcome.stop_requested {
                                    break 'event_loop;
                                }
                            } else {
                                debug_input(&format!("[input] action-map {:?} -> none", bind));
                            }
                        }
                    }
                    CrosstermEvent::Mouse(mouse) => {
                        let mouse = if matches!(
                            mouse.kind,
                            MouseEventKind::Moved | MouseEventKind::Drag(_)
                        ) {
                            coalesce_mouse_motion_events(mouse, &mut pending_input_event)?
                        } else {
                            mouse
                        };
                        match mouse.kind {
                            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                                if hit_probe_enabled() {
                                    let curr = (mouse.column, mouse.row);
                                    let dir = point_direction(last_mouse_pos, curr);
                                    let frame_target = self.widget_at(mouse.column, mouse.row);
                                    let tree_target = self.widget_tree.as_ref().and_then(|tree| {
                                        widget_at_tree_layout(tree, mouse.column, mouse.row)
                                    });
                                    let chosen = self
                                        .widget_tree
                                        .as_ref()
                                        .map(|tree| {
                                            super::choose_deeper_target(
                                                tree,
                                                frame_target,
                                                tree_target,
                                            )
                                        })
                                        .unwrap_or(frame_target);
                                    let relation = self
                                        .widget_tree
                                        .as_ref()
                                        .and_then(|tree| match (frame_target, tree_target) {
                                            (Some(frame), Some(tree_hit)) if frame != tree_hit => {
                                                if super::is_ancestor_or_self(tree, frame, tree_hit)
                                                {
                                                    Some("frame->ancestor(tree)")
                                                } else if super::is_ancestor_or_self(
                                                    tree, tree_hit, frame,
                                                ) {
                                                    Some("tree->ancestor(frame)")
                                                } else {
                                                    Some("unrelated")
                                                }
                                            }
                                            (Some(_), Some(_)) => Some("same"),
                                            _ => None,
                                        })
                                        .unwrap_or("-");
                                    let frame_rect =
                                        frame_target.and_then(|id| self.hit_test.rect(id));
                                    let tree_rect =
                                        tree_target.and_then(|id| self.hit_test.rect(id));
                                    debug_input(&format!(
                                        "[hit-probe] pos=({}, {}) dir={} frame={:?} frame_rect={} tree={:?} tree_rect={} relation={} chosen={:?}",
                                        mouse.column,
                                        mouse.row,
                                        dir,
                                        frame_target.map(node_id_to_ffi),
                                        fmt_rect(frame_rect),
                                        tree_target.map(node_id_to_ffi),
                                        fmt_rect(tree_rect),
                                        relation,
                                        chosen.map(node_id_to_ffi)
                                    ));
                                }
                                last_mouse_pos = Some((mouse.column, mouse.row));
                                let before = self.hovered;
                                if self.update_hover_from_frame(mouse.column, mouse.row, root) {
                                    if let Some(id) = before {
                                        pending_invalidation
                                            .request_widget_rect(&self.hit_test, id);
                                    }
                                    if let Some(id) = self.hovered {
                                        pending_invalidation
                                            .request_widget_rect(&self.hit_test, id);
                                    } else {
                                        pending_invalidation.request_full_content();
                                    }

                                    // Dispatch Enter/Leave events on hover change.
                                    let enter_leave = generate_enter_leave_events(
                                        before,
                                        self.hovered,
                                        mouse.column,
                                        mouse.row,
                                        mouse.column,
                                        mouse.row,
                                    );
                                    for (target, event) in enter_leave {
                                        let mut outcome = self
                                            .dispatch_event_to_target_auto(root, target, &event);
                                        self.absorb_outcome(
                                            &mut outcome,
                                            &mut pending_invalidation,
                                            InvalidationScope::Global,
                                        );
                                    }
                                }
                            }
                            MouseEventKind::Down(btn) => {
                                debug_input(&format!(
                                    "[input] mouse down x={} y={} hovered={:?}",
                                    mouse.column,
                                    mouse.row,
                                    self.hovered.map(|id| node_id_to_ffi(id))
                                ));
                                if let Some(target) = self.widget_at_auto(mouse.column, mouse.row) {
                                    let (x, y) = self.content_local_coords_auto(
                                        target,
                                        mouse.column,
                                        mouse.row,
                                    );
                                    debug_input(&format!(
                                        "[input] mouse target id={}",
                                        node_id_to_ffi(target)
                                    ));
                                    // Record click tracker state for click synthesis.
                                    let button = match btn {
                                        crossterm::event::MouseButton::Left => 0,
                                        crossterm::event::MouseButton::Right => 2,
                                        crossterm::event::MouseButton::Middle => 1,
                                    };
                                    self.click_tracker.on_mouse_down(
                                        target,
                                        x,
                                        y,
                                        mouse.column,
                                        mouse.row,
                                        button,
                                    );
                                    let down_event = Event::MouseDown(MouseDownEvent {
                                        target,
                                        screen_x: mouse.column,
                                        screen_y: mouse.row,
                                        x,
                                        y,
                                    });
                                    let mut outcome = self.dispatch_event_to_target_auto(
                                        root,
                                        target,
                                        &down_event,
                                    );
                                    self.absorb_outcome(
                                        &mut outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    let mut msg_outcome = self.dispatch_message_queue_with_runtime(
                                        root,
                                        outcome.messages,
                                    );
                                    self.absorb_outcome(
                                        &mut msg_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    if outcome.stop_requested || msg_outcome.stop_requested {
                                        break 'event_loop;
                                    }
                                } else {
                                    let down_event = Event::MouseDown(MouseDownEvent {
                                        target: NodeId::default(),
                                        screen_x: mouse.column,
                                        screen_y: mouse.row,
                                        x: mouse.column,
                                        y: mouse.row,
                                    });
                                    let mut outcome = self.dispatch_event_auto(root, down_event);
                                    self.absorb_outcome(
                                        &mut outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    let mut msg_outcome = self.dispatch_message_queue_with_runtime(
                                        root,
                                        outcome.messages,
                                    );
                                    self.absorb_outcome(
                                        &mut msg_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    if outcome.stop_requested || msg_outcome.stop_requested {
                                        break 'event_loop;
                                    }
                                }
                            }
                            MouseEventKind::Up(_) => {
                                let target = self.widget_at_auto(mouse.column, mouse.row);
                                let (x, y) = target
                                    .map(|id| {
                                        self.content_local_coords_auto(id, mouse.column, mouse.row)
                                    })
                                    .unwrap_or((mouse.column, mouse.row));
                                let up_event = Event::MouseUp(MouseUpEvent {
                                    target,
                                    screen_x: mouse.column,
                                    screen_y: mouse.row,
                                    x,
                                    y,
                                });
                                let mut outcome = if let Some(target) = target {
                                    self.dispatch_event_to_target_auto(root, target, &up_event)
                                } else {
                                    self.dispatch_event_auto(root, up_event)
                                };
                                self.absorb_outcome(
                                    &mut outcome,
                                    &mut pending_invalidation,
                                    InvalidationScope::Global,
                                );
                                // Synthesize Click if mouseup target matches mousedown target.
                                if let Some((click_target, click_event)) = self
                                    .click_tracker
                                    .on_mouse_up(target, x, y, mouse.column, mouse.row)
                                {
                                    let mut click_outcome = self.dispatch_event_to_target_auto(
                                        root,
                                        click_target,
                                        &click_event,
                                    );
                                    self.absorb_outcome(
                                        &mut click_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                }
                                let mut msg_outcome = self
                                    .dispatch_message_queue_with_runtime(root, outcome.messages);
                                self.absorb_outcome(
                                    &mut msg_outcome,
                                    &mut pending_invalidation,
                                    InvalidationScope::Global,
                                );
                                if outcome.stop_requested || msg_outcome.stop_requested {
                                    break 'event_loop;
                                }
                            }
                            MouseEventKind::ScrollUp
                            | MouseEventKind::ScrollDown
                            | MouseEventKind::ScrollLeft
                            | MouseEventKind::ScrollRight => {
                                debug_input(&format!(
                                    "[input] mouse scroll kind={:?} mods={:?} x={} y={}",
                                    mouse.kind, mouse.modifiers, mouse.column, mouse.row
                                ));
                                let before = self.hovered;
                                if self.update_hover_from_frame(mouse.column, mouse.row, root) {
                                    if let Some(id) = before {
                                        pending_invalidation
                                            .request_widget_rect(&self.hit_test, id);
                                    }
                                    if let Some(id) = self.hovered {
                                        pending_invalidation
                                            .request_widget_rect(&self.hit_test, id);
                                    } else {
                                        pending_invalidation.request_full_content();
                                    }
                                }
                                let (delta_x, delta_y) =
                                    mouse_scroll_deltas(mouse.kind, mouse.modifiers);
                                let target = self.widget_at_auto(mouse.column, mouse.row);
                                let (local_x, local_y) = target
                                    .map(|id| {
                                        self.content_local_coords_auto(id, mouse.column, mouse.row)
                                    })
                                    .unwrap_or((0, 0));
                                debug_input(&format!(
                                    "[input] mouse scroll route target={:?} dx={} dy={}",
                                    target.map(node_id_to_ffi),
                                    delta_x,
                                    delta_y
                                ));
                                let mut diag_outcome = if let Some(target) = target {
                                    self.dispatch_event_to_target_auto(
                                        root,
                                        target,
                                        &Event::MouseScroll(MouseScrollEvent {
                                            target: Some(target),
                                            screen_x: mouse.column,
                                            screen_y: mouse.row,
                                            x: local_x,
                                            y: local_y,
                                            delta_x,
                                            delta_y,
                                            modifiers: mouse.modifiers,
                                        }),
                                    )
                                } else {
                                    self.dispatch_event_auto(
                                        root,
                                        Event::MouseScroll(MouseScrollEvent {
                                            target: None,
                                            screen_x: mouse.column,
                                            screen_y: mouse.row,
                                            x: local_x,
                                            y: local_y,
                                            delta_x,
                                            delta_y,
                                            modifiers: mouse.modifiers,
                                        }),
                                    )
                                };
                                self.absorb_outcome(
                                    &mut diag_outcome,
                                    &mut pending_invalidation,
                                    target
                                        .map(InvalidationScope::Widget)
                                        .unwrap_or(InvalidationScope::Global),
                                );
                                let mut msg_outcome = self.dispatch_message_queue_with_runtime(
                                    root,
                                    diag_outcome.messages,
                                );
                                self.absorb_outcome(
                                    &mut msg_outcome,
                                    &mut pending_invalidation,
                                    InvalidationScope::Global,
                                );
                                let mut outcome = if let Some(target) = target {
                                    self.dispatch_mouse_scroll_to_target_auto(
                                        root, target, delta_x, delta_y,
                                    )
                                } else {
                                    dispatch_mouse_scroll(root, delta_x, delta_y)
                                };
                                debug_input(&format!(
                                    "[input] mouse scroll dispatch handled={} repaint={} messages={}",
                                    outcome.handled,
                                    outcome.repaint_requested,
                                    outcome.messages.len()
                                ));
                                self.absorb_outcome(
                                    &mut outcome,
                                    &mut pending_invalidation,
                                    target
                                        .map(InvalidationScope::Widget)
                                        .unwrap_or(InvalidationScope::Global),
                                );
                                let mut msg_outcome = self
                                    .dispatch_message_queue_with_runtime(root, outcome.messages);
                                self.absorb_outcome(
                                    &mut msg_outcome,
                                    &mut pending_invalidation,
                                    InvalidationScope::Global,
                                );
                                if diag_outcome.stop_requested
                                    || outcome.stop_requested
                                    || msg_outcome.stop_requested
                                {
                                    break 'event_loop;
                                }
                            }
                        }
                    }
                    CrosstermEvent::Resize(_, _) => {
                        let size = self.driver.size();
                        debug_render(&format!("[event] Resize({}x{})", size.width, size.height));
                        self.refresh_size()?;
                        let size = self.driver.size();
                        root.on_resize(size.width, size.height);
                        let mut outcome =
                            self.dispatch_event_auto(root, Event::Resize(size.width, size.height));
                        self.absorb_outcome(
                            &mut outcome,
                            &mut pending_invalidation,
                            InvalidationScope::Global,
                        );
                        let mut msg_outcome =
                            self.dispatch_message_queue_with_runtime(root, outcome.messages);
                        self.absorb_outcome(
                            &mut msg_outcome,
                            &mut pending_invalidation,
                            InvalidationScope::Global,
                        );
                        if outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                    }
                    CrosstermEvent::FocusLost => {
                        self.app_active = false;
                        debug_input("[event] FocusLost");
                        let mut outcome = self.dispatch_event_auto(root, Event::AppFocus(false));
                        self.absorb_outcome(
                            &mut outcome,
                            &mut pending_invalidation,
                            InvalidationScope::Global,
                        );
                        let mut msg_outcome =
                            self.dispatch_message_queue_with_runtime(root, outcome.messages);
                        self.absorb_outcome(
                            &mut msg_outcome,
                            &mut pending_invalidation,
                            InvalidationScope::Global,
                        );
                        if outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                    }
                    CrosstermEvent::FocusGained => {
                        self.app_active = true;
                        debug_input("[event] FocusGained");
                        let mut outcome = self.dispatch_event_auto(root, Event::AppFocus(true));
                        self.absorb_outcome(
                            &mut outcome,
                            &mut pending_invalidation,
                            InvalidationScope::Global,
                        );
                        let mut msg_outcome =
                            self.dispatch_message_queue_with_runtime(root, outcome.messages);
                        self.absorb_outcome(
                            &mut msg_outcome,
                            &mut pending_invalidation,
                            InvalidationScope::Global,
                        );
                        if outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                    }
                    _ => {}
                }
            }

            let mut background_outcome = self.dispatch_background_runtime_messages(root);
            self.absorb_outcome(
                &mut background_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            if background_outcome.stop_requested {
                break 'event_loop;
            }

            let mut focused_help_outcome = self.dispatch_focused_help_changed(root);
            self.absorb_outcome(
                &mut focused_help_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            if focused_help_outcome.stop_requested {
                break 'event_loop;
            }

            // Drain pending lifecycle events from the tree and dispatch
            // Mount/Unmount events to affected widgets.
            let lifecycle_events: Vec<(NodeId, bool)> = self
                .widget_tree
                .as_mut()
                .map(|tree| {
                    tree.drain_lifecycle()
                        .into_iter()
                        .map(|evt| match evt {
                            crate::widget_tree::LifecycleEvent::Mount { node } => (node, true),
                            crate::widget_tree::LifecycleEvent::Unmount { node } => (node, false),
                        })
                        .collect()
                })
                .unwrap_or_default();
            for (node_id, is_mount) in lifecycle_events {
                let event = if is_mount {
                    Event::Mount(MountEvent { node: node_id })
                } else {
                    Event::Unmount(UnmountEvent { node: node_id })
                };
                let mut outcome = self.dispatch_event_to_target_auto(root, node_id, &event);
                self.absorb_outcome(
                    &mut outcome,
                    &mut pending_invalidation,
                    InvalidationScope::Global,
                );
                let mut msg_outcome =
                    self.dispatch_message_queue_with_runtime(root, outcome.messages);
                self.absorb_outcome(
                    &mut msg_outcome,
                    &mut pending_invalidation,
                    InvalidationScope::Global,
                );
                if outcome.stop_requested || msg_outcome.stop_requested {
                    break 'event_loop;
                }
            }

            // ── Reactive phase ────────────────────────────────────────
            // Run the reactive phase for widgets that accumulated changes
            // during event dispatch. This drains ReactiveCtx changes, calls
            // watchers/computed recomputation, and detects cycles.
            //
            // Currently a no-op until widgets are migrated to carry
            // ReactiveCtx (P3-10..P3-13). The infrastructure is ready:
            // when a widget implements ReactiveWidget and has a ReactiveCtx,
            // the runtime will call `run_reactive_phase()` here and feed
            // its result into `pending_invalidation`.
            self.run_event_loop_reactive_phase(root, &mut pending_invalidation);

            // Detect focus transitions and dispatch Focus/Blur events.
            let current_focus: Option<NodeId> =
                self.widget_tree.as_ref().and_then(focused_node_id_tree);
            if current_focus != previous_focus {
                if let Some(old_id) = previous_focus {
                    let mut blur_outcome = self.dispatch_event_to_target_auto(
                        root,
                        old_id,
                        &Event::Blur(BlurEvent { node: old_id }),
                    );
                    self.absorb_outcome(
                        &mut blur_outcome,
                        &mut pending_invalidation,
                        InvalidationScope::Global,
                    );
                    let mut msg_outcome =
                        self.dispatch_message_queue_with_runtime(root, blur_outcome.messages);
                    self.absorb_outcome(
                        &mut msg_outcome,
                        &mut pending_invalidation,
                        InvalidationScope::Global,
                    );
                    if blur_outcome.stop_requested || msg_outcome.stop_requested {
                        break 'event_loop;
                    }
                }
                if let Some(new_id) = current_focus {
                    let mut focus_outcome = self.dispatch_event_to_target_auto(
                        root,
                        new_id,
                        &Event::Focus(FocusEvent { node: new_id }),
                    );
                    self.absorb_outcome(
                        &mut focus_outcome,
                        &mut pending_invalidation,
                        InvalidationScope::Global,
                    );
                    let mut msg_outcome =
                        self.dispatch_message_queue_with_runtime(root, focus_outcome.messages);
                    self.absorb_outcome(
                        &mut msg_outcome,
                        &mut pending_invalidation,
                        InvalidationScope::Global,
                    );
                    if focus_outcome.stop_requested || msg_outcome.stop_requested {
                        break 'event_loop;
                    }
                }
                previous_focus = current_focus;
            }

            let mut binding_outcome = self.dispatch_binding_hints_changed(root);
            self.absorb_outcome(
                &mut binding_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            if binding_outcome.stop_requested {
                break 'event_loop;
            }

            let mut animation_outcome = self.dispatch_animation_frame(root);
            self.absorb_outcome(
                &mut animation_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            if animation_outcome.stop_requested {
                break 'event_loop;
            }

            // ── Process accumulated worker requests for this tick ────
            {
                let pending_workers = drain_accumulated_worker_requests();
                let changes = process_worker_requests(&mut worker_registry, pending_workers);
                if !changes.is_empty() {
                    let worker_messages = worker_state_runtime_messages(&worker_registry, changes);
                    let mut worker_outcome =
                        self.dispatch_message_queue_with_runtime(root, worker_messages);
                    self.absorb_outcome(
                        &mut worker_outcome,
                        &mut pending_invalidation,
                        InvalidationScope::Global,
                    );
                    if worker_outcome.stop_requested {
                        break 'event_loop;
                    }
                }
                worker_registry.cleanup();
            }

            self.dispatch_style_transition_requests(root);

            if let Some(tree) = self.widget_tree.as_mut()
                && sync_widget_controlled_child_display_tree(tree)
            {
                pending_invalidation.request_flags(crate::event::InvalidationFlags::layout());
                pending_invalidation.request_full_content();
            }

            if pending_invalidation.is_dirty() || self.resized_since_last_render {
                let regions = pending_invalidation
                    .content_regions
                    .as_render_regions(self.frame.width, self.frame.height);
                let layout_invalidation = pending_invalidation.flags.layout
                    || pending_invalidation.flags.style
                    || self.resized_since_last_render;
                self.render_widget_with_regions(root, regions.as_deref(), layout_invalidation)?;
                self.apply_layout_info_to_tree();
                self.publish_devtools_snapshot(root);
                pending_invalidation = PendingInvalidation::default();
                last_render = Instant::now();
            }

            if last_render.elapsed() >= tick_rate {
                let mut sheet = self.default_stylesheet.clone();
                sheet.extend(&self.stylesheet);
                let _active = set_app_active(self.app_active);
                let _pseudo_state = set_app_runtime_pseudos(AppRuntimePseudos {
                    inline: self.app_inline,
                    ansi: self.app_ansi,
                    nocolor: self.app_nocolor,
                });
                let _guard = set_style_context(sheet);
                if let Some(reload) = self.poll_stylesheet() {
                    self.absorb_stylesheet_reload(root, reload, &mut pending_invalidation);
                }
                root.on_tick(tick);
                // `on_tick` mutates widget state without an `EventCtx`, so request a repaint
                // for this frame to keep tick-driven widgets (e.g. counters/cursors) in sync.
                pending_invalidation.request_full_content();
                let mut outcome = self.dispatch_event_auto(root, Event::Tick(tick));
                self.absorb_outcome(
                    &mut outcome,
                    &mut pending_invalidation,
                    InvalidationScope::Global,
                );
                let mut msg_outcome =
                    self.dispatch_message_queue_with_runtime(root, outcome.messages);
                self.absorb_outcome(
                    &mut msg_outcome,
                    &mut pending_invalidation,
                    InvalidationScope::Global,
                );
                let notifications_before = self.notifications.len();
                let now = Instant::now();
                self.notifications.retain(|note| note.expires_at > now);
                if self.notifications.len() != notifications_before {
                    pending_invalidation.request_full_content();
                }
                self.dispatch_style_transition_requests(root);
                if outcome.stop_requested || msg_outcome.stop_requested {
                    break 'event_loop;
                }

                let any_active = self.any_widget_active_auto(root);
                if let Some(tree) = self.widget_tree.as_mut()
                    && sync_widget_controlled_child_display_tree(tree)
                {
                    pending_invalidation.request_flags(crate::event::InvalidationFlags::layout());
                    pending_invalidation.request_full_content();
                }
                if pending_invalidation.is_dirty()
                    || self.resized_since_last_render
                    || any_active
                    || prev_any_active
                {
                    let regions = pending_invalidation
                        .content_regions
                        .as_render_regions(self.frame.width, self.frame.height);
                    let layout_invalidation = pending_invalidation.flags.layout
                        || pending_invalidation.flags.style
                        || self.resized_since_last_render;
                    self.render_widget_with_regions(root, regions.as_deref(), layout_invalidation)?;
                    self.apply_layout_info_to_tree();
                    self.publish_devtools_snapshot(root);
                    pending_invalidation = PendingInvalidation::default();
                    last_render = Instant::now();
                }
                prev_any_active = any_active;
                tick += 1;
            }
        }

        root.on_unmount();
        self.finish()?;
        Ok(())
    }

    pub(super) fn dispatch_binding_hints_changed(
        &mut self,
        root: &mut dyn Widget,
    ) -> DispatchOutcome {
        let (widget_hints, current_sources) = self.active_binding_hints_auto(root);
        let mut current = self.binding_hints();
        current.extend(widget_hints);
        let current = self.normalize_binding_hints(current);
        if !should_dispatch_binding_hints(
            &self.last_binding_hints,
            &self.last_binding_hint_sources,
            &current,
            &current_sources,
        ) {
            return DispatchOutcome::default();
        }
        self.last_binding_hints = current.clone();
        self.last_binding_hint_sources = current_sources;
        let outcome = if let Some(tree) = self.widget_tree.as_mut() {
            dispatch_event_broadcast_tree(tree, &Event::BindingsChanged(current))
        } else {
            self.dispatch_event_auto(root, Event::BindingsChanged(current))
        };
        let msg_outcome = self.dispatch_message_queue_with_runtime(root, outcome.messages);
        let mut invalidation = outcome.invalidation;
        invalidation.merge(msg_outcome.invalidation);
        DispatchOutcome {
            handled: outcome.handled || msg_outcome.handled,
            repaint_requested: outcome.repaint_requested || msg_outcome.repaint_requested,
            invalidation,
            stop_requested: outcome.stop_requested || msg_outcome.stop_requested,
            messages: msg_outcome.messages,
            animation_requests: {
                let mut requests = outcome.animation_requests;
                requests.extend(msg_outcome.animation_requests);
                requests
            },
            worker_requests: {
                let mut requests = outcome.worker_requests;
                requests.extend(msg_outcome.worker_requests);
                requests
            },
            default_prevented: outcome.default_prevented || msg_outcome.default_prevented,
        }
    }

    pub(super) fn dispatch_focused_help_changed(
        &mut self,
        root: &mut dyn Widget,
    ) -> DispatchOutcome {
        let current = self.focused_help_metadata_auto(root);
        let current_source = current.as_ref().map(|(source, _)| *source);
        let current_markup = current.as_ref().map(|(_, markup)| markup.as_str());
        if !should_dispatch_focused_help(
            self.last_focused_help_source,
            self.last_focused_help_markup.as_deref(),
            current_source,
            current_markup,
        ) {
            return DispatchOutcome::default();
        }

        self.last_focused_help_source = current_source;
        self.last_focused_help_markup = current.as_ref().map(|(_, markup)| markup.clone());

        let event = focused_help_message(current);
        self.dispatch_message_queue_with_runtime(root, vec![event])
    }

    pub(super) fn enqueue_animation_requests(&mut self, requests: Vec<AnimationRequest>) {
        if requests.is_empty() {
            return;
        }
        self.animator.enqueue_many(requests, Instant::now());
    }

    fn absorb_outcome(
        &mut self,
        outcome: &mut DispatchOutcome,
        pending: &mut PendingInvalidation,
        scope: InvalidationScope,
    ) {
        pending.request_flags(outcome.invalidation);
        if outcome.should_repaint() {
            match scope {
                InvalidationScope::Global => pending.request_full_content(),
                InvalidationScope::Widget(id) => pending.request_widget_rect(&self.hit_test, id),
            }
        }
        let requests = std::mem::take(&mut outcome.animation_requests);
        self.enqueue_animation_requests(requests);
        accumulate_worker_requests(outcome);
    }

    fn absorb_stylesheet_reload(
        &mut self,
        _root: &mut dyn Widget,
        reload: StylesheetReload,
        pending: &mut PendingInvalidation,
    ) {
        if reload.previous == reload.next {
            return;
        }
        let affected = if let Some(tree) = &self.widget_tree {
            collect_stylesheet_affected_widgets_tree(
                tree,
                &reload.changed_rules,
                self.app_active,
                AppRuntimePseudos {
                    inline: self.app_inline,
                    ansi: self.app_ansi,
                    nocolor: self.app_nocolor,
                },
            )
        } else {
            collect_stylesheet_affected_widgets_root(
                _root,
                &reload.changed_rules,
                self.app_active,
                AppRuntimePseudos {
                    inline: self.app_inline,
                    ansi: self.app_ansi,
                    nocolor: self.app_nocolor,
                },
            )
        };
        if affected.is_empty() {
            return;
        }

        pending.request_flags(if reload.layout_affected {
            crate::event::InvalidationFlags::layout()
        } else {
            crate::event::InvalidationFlags::style()
        });
        if reload.layout_affected || affected.len() > 128 {
            pending.request_full_content();
            return;
        }
        for id in affected {
            pending.request_widget_rect(&self.hit_test, id);
        }
    }

    fn collect_current_resolved_styles(
        &self,
        root: &dyn Widget,
    ) -> HashMap<NodeId, crate::style::Style> {
        let mut out = HashMap::new();
        if let Some(tree) = &self.widget_tree {
            if let Some(root_id) = tree.root() {
                for node_id in tree.walk_depth_first(root_id) {
                    let Some(node) = tree.get(node_id) else {
                        continue;
                    };
                    let meta = crate::css::selector_meta_generic(node.widget.as_ref());
                    out.insert(
                        node_id,
                        crate::css::resolve_style(node.widget.as_ref(), &meta),
                    );
                }
            }
            return out;
        }
        let meta = crate::css::selector_meta_generic(root);
        out.insert(NodeId::default(), crate::css::resolve_style(root, &meta));
        out
    }

    fn dispatch_style_transition_requests(&mut self, root: &dyn Widget) {
        let mut sheet = self.default_stylesheet.clone();
        sheet.extend(&self.stylesheet);
        let _active = set_app_active(self.app_active);
        let _pseudo_state = set_app_runtime_pseudos(AppRuntimePseudos {
            inline: self.app_inline,
            ansi: self.app_ansi,
            nocolor: self.app_nocolor,
        });
        let _guard = set_style_context(sheet);

        let current_styles = self.collect_current_resolved_styles(root);
        let mut requests = Vec::new();
        for (node_id, current_style) in &current_styles {
            if let Some(previous_style) = self.style_snapshot_cache.get(node_id) {
                requests.extend(transition_requests_for_style_change(
                    *node_id,
                    previous_style,
                    current_style,
                ));
            }
        }
        self.style_snapshot_cache = current_styles;
        self.enqueue_animation_requests(requests);
    }

    pub(super) fn dispatch_animation_frame(&mut self, root: &mut dyn Widget) -> DispatchOutcome {
        let updates = self.animator.step(Instant::now(), self.animation_level);
        if updates.is_empty() {
            return DispatchOutcome::default();
        }

        let mut aggregate = DispatchOutcome::default();
        for update in updates {
            let mut outcome = self.dispatch_event_to_target_auto(
                root,
                update.target,
                &Event::AnimationValue(AnimationValueEvent {
                    target: update.target,
                    attribute: update.attribute,
                    value: update.value,
                    done: update.done,
                }),
            );
            aggregate.handled |= outcome.handled;
            aggregate.repaint_requested |= outcome.repaint_requested;
            aggregate.invalidation.merge(outcome.invalidation);
            let mut msg_outcome = self.dispatch_message_queue_with_runtime(root, outcome.messages);
            aggregate.handled |= msg_outcome.handled;
            aggregate.repaint_requested |= msg_outcome.repaint_requested;
            aggregate.invalidation.merge(msg_outcome.invalidation);
            let requests = std::mem::take(&mut outcome.animation_requests);
            aggregate.animation_requests.extend(requests);
            let msg_requests = std::mem::take(&mut msg_outcome.animation_requests);
            aggregate.animation_requests.extend(msg_requests);
            aggregate
                .worker_requests
                .extend(std::mem::take(&mut outcome.worker_requests));
            aggregate
                .worker_requests
                .extend(std::mem::take(&mut msg_outcome.worker_requests));

            aggregate.stop_requested |= outcome.stop_requested || msg_outcome.stop_requested;
            aggregate.default_prevented |=
                outcome.default_prevented || msg_outcome.default_prevented;
            aggregate.messages.extend(msg_outcome.messages);
        }
        aggregate.repaint_requested = true;
        aggregate
            .invalidation
            .merge(crate::event::InvalidationFlags::content());
        aggregate
    }

    // ===================================================================
    // Reactive phase
    // ===================================================================

    /// Run the reactive phase for all widgets that accumulated changes
    /// during event dispatch.
    ///
    /// Iterates over tree nodes that have a `ReactiveCtx` with pending changes,
    /// calls `run_reactive_phase()` for each, and feeds repaint/layout results
    /// into `pending_invalidation`.
    fn run_event_loop_reactive_phase(
        &mut self,
        _root: &mut dyn Widget,
        pending: &mut PendingInvalidation,
    ) {
        let queued = crate::reactive::take_runtime_reactive_entries();
        if queued.is_empty() {
            return;
        }

        let mut by_node: std::collections::HashMap<NodeId, Vec<RuntimeReactiveEntry>> =
            std::collections::HashMap::new();
        for entry in queued {
            by_node.entry(entry.node_id()).or_default().push(entry);
        }

        if let Some(tree) = self.widget_tree.as_ref() {
            if let Some(root_id) = tree.root() {
                for node_id in tree.walk_depth_first(root_id) {
                    if let Some(entries) = by_node.remove(&node_id) {
                        self.process_reactive_entries_for_node(node_id, entries, pending);
                    }
                }
            }
        }

        let mut remaining: Vec<(NodeId, Vec<RuntimeReactiveEntry>)> = by_node.into_iter().collect();
        remaining.sort_by_key(|(node_id, _)| node_id_to_ffi(*node_id));
        for (node_id, entries) in remaining {
            self.process_reactive_entries_for_node(node_id, entries, pending);
        }
    }

    fn process_reactive_entries_for_node(
        &mut self,
        node_id: NodeId,
        entries: Vec<RuntimeReactiveEntry>,
        pending: &mut PendingInvalidation,
    ) {
        let mut repaint_requested = false;
        let mut layout_requested = false;
        for mut entry in entries {
            let result = if let Some(tree) = self.widget_tree.as_mut() {
                if let Some(node) = tree.get_mut(node_id) {
                    entry.run_with_dispatch(|changes, ctx| {
                        if let Some(reactive_widget) = node.widget.reactive_widget() {
                            reactive_widget.reactive_dispatch(changes, ctx);
                        }
                    })
                } else {
                    entry.run_without_dispatch()
                }
            } else {
                entry.run_without_dispatch()
            };
            repaint_requested |= result.needs_repaint;
            layout_requested |= result.needs_layout;
        }

        if layout_requested {
            pending.request_flags(crate::event::InvalidationFlags::layout());
        }
        if repaint_requested {
            pending.request_widget_rect(&self.hit_test, node_id);
        }
    }

    // ===================================================================
    // Arena-tree bridge methods
    //
    // The runtime has two explicit modes:
    // - Tree mode: a composed widget tree exists (`self.widget_tree.is_some()`).
    // - Root-only mode: no composed children were extracted; only the root
    //   widget participates in dispatch.
    //
    // Root-only fallbacks below are intentional compatibility for root-only
    // apps, not migration-only behavior.
    // ===================================================================

    /// Move focus forward/backward in the tree focus chain.
    ///
    /// Returns `true` when focus changed.
    fn move_focus_auto(&mut self, action: Action) -> bool {
        let Some(tree) = self.widget_tree.as_mut() else {
            return false;
        };
        let focus_chain = collect_focus_chain_tree(tree);
        if focus_chain.is_empty() {
            return false;
        }

        let current = focused_node_id_tree(tree);
        let current_index =
            current.and_then(|id| focus_chain.iter().position(|candidate| *candidate == id));
        let next_index = match (action, current_index) {
            (Action::FocusNext, Some(idx)) => (idx + 1) % focus_chain.len(),
            (Action::FocusPrev, Some(0)) => focus_chain.len() - 1,
            (Action::FocusPrev, Some(idx)) => idx - 1,
            (Action::FocusNext, None) => 0,
            (Action::FocusPrev, None) => focus_chain.len() - 1,
            _ => return false,
        };

        let next = focus_chain[next_index];
        if current == Some(next) {
            return false;
        }

        if let Some(current) = current {
            if let Some(node) = tree.get_mut(current) {
                node.widget.set_focus(false);
            }
        }
        if let Some(node) = tree.get_mut(next) {
            node.widget.set_focus(true);
            return true;
        }
        false
    }

    /// Dispatch an event via tree mode, or root-only mode when no tree exists.
    fn dispatch_event_auto(&mut self, root: &mut dyn Widget, event: Event) -> DispatchOutcome {
        if let Some(tree) = self.widget_tree.as_mut() {
            // In tree mode, the root widget (e.g. TextualAppAdapter) is not mounted
            // in the arena, so app-level hooks on root would otherwise be skipped.
            // Run key capture on root first, then route through tree.
            let mut root_capture_ctx = EventCtx::default();
            if matches!(&event, Event::Key(..)) {
                root.on_event_capture(&event, &mut root_capture_ctx);
                if root_capture_ctx.handled() {
                    return DispatchOutcome {
                        handled: root_capture_ctx.handled(),
                        repaint_requested: root_capture_ctx.repaint_requested(),
                        invalidation: root_capture_ctx.invalidation(),
                        stop_requested: root_capture_ctx.stop_requested(),
                        messages: root_capture_ctx.take_messages(),
                        animation_requests: root_capture_ctx.take_animation_requests(),
                        worker_requests: root_capture_ctx.take_worker_requests(),
                        default_prevented: false,
                    };
                }
            }

            let focused = focused_node_id_tree(tree);
            let mut outcome = dispatch_event_tree(tree, focused, &event);

            // Merge root key-capture side effects (if any) while preserving
            // ordering: root-capture emissions happen before tree dispatch.
            if matches!(&event, Event::Key(..)) {
                outcome.handled |= root_capture_ctx.handled();
                outcome.repaint_requested |= root_capture_ctx.repaint_requested();
                outcome.invalidation.merge(root_capture_ctx.invalidation());
                outcome.stop_requested |= root_capture_ctx.stop_requested();

                let mut root_messages = root_capture_ctx.take_messages();
                if !root_messages.is_empty() {
                    root_messages.extend(outcome.messages);
                    outcome.messages = root_messages;
                }

                let mut root_animation_requests = root_capture_ctx.take_animation_requests();
                if !root_animation_requests.is_empty() {
                    root_animation_requests.extend(outcome.animation_requests);
                    outcome.animation_requests = root_animation_requests;
                }

                let mut root_worker_requests = root_capture_ctx.take_worker_requests();
                if !root_worker_requests.is_empty() {
                    root_worker_requests.extend(outcome.worker_requests);
                    outcome.worker_requests = root_worker_requests;
                }
            }

            // Preserve adapter-style app action fallback in tree mode:
            // run root.on_event only when tree dispatch didn't handle Action.
            if !outcome.handled && matches!(&event, Event::Action(..)) {
                let mut root_action_ctx = EventCtx::default();
                root.on_event(&event, &mut root_action_ctx);
                outcome.handled |= root_action_ctx.handled();
                outcome.repaint_requested |= root_action_ctx.repaint_requested();
                outcome.invalidation.merge(root_action_ctx.invalidation());
                outcome.stop_requested |= root_action_ctx.stop_requested();
                outcome.messages.extend(root_action_ctx.take_messages());
                outcome
                    .animation_requests
                    .extend(root_action_ctx.take_animation_requests());
                outcome
                    .worker_requests
                    .extend(root_action_ctx.take_worker_requests());
            }

            outcome
        } else {
            dispatch_event(root, event)
        }
    }

    /// Dispatch an event to a specific target via the arena tree.
    ///
    /// Falls back to root-only dispatch when no tree exists.
    fn dispatch_event_to_target_auto(
        &mut self,
        root: &mut dyn Widget,
        _target: NodeId,
        event: &Event,
    ) -> DispatchOutcome {
        if let Some(tree) = self.widget_tree.as_mut() {
            dispatch_event_to_target_tree(tree, _target, event)
        } else {
            dispatch_event(root, event.clone())
        }
    }

    /// Dispatch a scroll action via the arena tree.
    ///
    /// Falls back to root-only dispatch when no tree exists.
    fn dispatch_scroll_action_auto(
        &mut self,
        root: &mut dyn Widget,
        action: Action,
        hovered: Option<NodeId>,
    ) -> DispatchOutcome {
        if let Some(tree) = self.widget_tree.as_mut() {
            dispatch_scroll_action_tree(tree, action, hovered)
        } else {
            dispatch_event(root, Event::Action(action))
        }
    }

    /// Dispatch mouse scroll to a specific target via the arena tree.
    ///
    /// Falls back to root-only scroll when no tree exists.
    fn dispatch_mouse_scroll_to_target_auto(
        &mut self,
        root: &mut dyn Widget,
        _target: NodeId,
        delta_x: i32,
        delta_y: i32,
    ) -> DispatchOutcome {
        if let Some(tree) = self.widget_tree.as_mut() {
            dispatch_mouse_scroll_to_target_tree(tree, _target, delta_x, delta_y)
        } else {
            dispatch_mouse_scroll(root, delta_x, delta_y)
        }
    }

    /// Dispatch a message queue via the arena tree.
    ///
    /// Falls back to root-only message delivery when no tree exists.
    fn dispatch_message_queue_auto(
        &mut self,
        root: &mut dyn Widget,
        initial: Vec<MessageEvent>,
    ) -> DispatchOutcome {
        if let Some(tree) = self.widget_tree.as_mut() {
            let mut outcome = dispatch_message_queue_tree(tree, initial.clone());

            // Tree routing delivers to arena nodes, but the TextualApp adapter root
            // also hosts typed hooks (e.g. on_button_pressed). Forward top-level
            // messages to root so app hooks still run in tree mode.
            for message in initial {
                let mut ctx = EventCtx::default();
                root.on_message(&message, &mut ctx);
                outcome.handled |= ctx.handled();
                outcome.repaint_requested |= ctx.repaint_requested();
                outcome.invalidation.merge(ctx.invalidation());
                outcome.stop_requested |= ctx.stop_requested();
                outcome.messages.extend(ctx.take_messages());
                outcome
                    .animation_requests
                    .extend(ctx.take_animation_requests());
                outcome.worker_requests.extend(ctx.take_worker_requests());
            }

            outcome
        } else {
            // Root-only fallback: deliver each message to root.on_message,
            // re-queuing follow-up messages like the tree-based version.
            use std::collections::VecDeque;
            let mut handled = false;
            let mut repaint_requested = false;
            let mut invalidation = crate::event::InvalidationFlags::default();
            let mut stop_requested = false;
            let mut emitted = Vec::new();
            let mut animation_requests = Vec::new();
            let mut worker_requests = Vec::new();
            let mut queue: VecDeque<MessageEvent> = initial.into();
            const LIMIT: usize = 1024;
            let mut processed = 0usize;
            while let Some(message) = queue.pop_front() {
                processed += 1;
                if processed > LIMIT {
                    break;
                }
                let mut ctx = EventCtx::default();
                root.on_message(&message, &mut ctx);
                handled |= ctx.handled();
                repaint_requested |= ctx.repaint_requested();
                invalidation.merge(ctx.invalidation());
                stop_requested |= ctx.stop_requested();
                let next = ctx.take_messages();
                animation_requests.extend(ctx.take_animation_requests());
                worker_requests.extend(ctx.take_worker_requests());
                if !next.is_empty() {
                    queue.extend(next.clone());
                    emitted.extend(next);
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
                default_prevented: false,
            }
        }
    }

    /// Check whether any widget is active, using tree mode or root-only mode.
    fn any_widget_active_auto(&self, root: &mut dyn Widget) -> bool {
        if let Some(tree) = &self.widget_tree {
            any_widget_active_tree(tree)
        } else {
            root.is_active()
        }
    }

    /// Collect active binding hints via tree mode or root-only mode.
    fn active_binding_hints_auto(
        &self,
        root: &mut dyn Widget,
    ) -> (Vec<crate::event::BindingHint>, Vec<NodeId>) {
        if let Some(tree) = &self.widget_tree {
            active_binding_hints_tree(tree)
        } else {
            (root.binding_hints(), vec![])
        }
    }

    /// Get focused help metadata via tree mode or root-only mode.
    fn focused_help_metadata_auto(&self, root: &mut dyn Widget) -> Option<(NodeId, String)> {
        if let Some(tree) = &self.widget_tree {
            focused_help_metadata_tree(tree)
        } else {
            if root.has_focus() {
                let help = root.help_markup().map(str::trim).unwrap_or_default();
                if !help.is_empty() {
                    return Some((NodeId::default(), help.to_string()));
                }
            }
            None
        }
    }

    /// Forward `on_mouse_move` via tree mode or root-only mode.
    pub(super) fn call_on_mouse_move_auto(
        &mut self,
        root: &mut dyn Widget,
        _target: NodeId,
        x: u16,
        y: u16,
    ) -> bool {
        if let Some(tree) = self.widget_tree.as_mut() {
            call_on_mouse_move_tree(tree, _target, x, y)
        } else {
            root.on_mouse_move(x, y)
        }
    }

    /// Determine pointer shape for hover via tree or default fallback.
    pub(super) fn pointer_shape_for_hover_auto(
        &self,
        _root: &mut dyn Widget,
        hovered: Option<NodeId>,
    ) -> crate::driver::PointerShape {
        if let Some(tree) = &self.widget_tree {
            pointer_shape_for_hover_tree(tree, hovered)
        } else {
            crate::driver::PointerShape::Default
        }
    }

    /// Distribute layout info to the arena tree after rendering.
    ///
    /// The legacy `apply_layout_info` is already called inside the render
    /// pipeline (`render.rs`). This method handles only the tree-based path
    /// so compose()-created widgets also receive layout geometry.
    fn apply_layout_info_to_tree(&mut self) {
        if let Some(tree) = self.widget_tree.as_mut() {
            let node_hit_test = super::types::NodeHitTestMap::from_frame(&self.frame);
            apply_layout_info_tree(tree, &node_hit_test);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClipboardBackend, collect_clipboard_runtime_messages_with_backend,
        collect_stylesheet_affected_widgets_root, focused_help_message,
        set_overlay_modal_display_tree, should_dispatch_binding_hints,
        should_dispatch_focused_help, transition_requests_for_style_change,
    };
    use crate::action::{ActionDecl, ParsedAction, parse_action, resolve_action};
    use crate::css::StyleSheet;
    use crate::event::{Action, BindingHint, Event, EventCtx, MountEvent};
    use crate::keys::KeyEventData;
    use crate::message::{Message, MessageEvent};
    use crate::node_id::{NodeId, node_id_from_ffi};
    use crate::reactive::{
        ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget, enqueue_runtime_reactive_entry,
        take_runtime_reactive_entries,
    };
    use crate::style::{Offset, OffsetValue, PropertyTransition, Style, TransitionTiming};
    use crate::widgets::{AppRoot, Widget};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rich_rs::{Console, ConsoleOptions, Segments};
    use std::collections::VecDeque;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Default)]
    struct StubClipboardBackend {
        copy_results: VecDeque<bool>,
        paste_results: VecDeque<Option<String>>,
        copied: Vec<String>,
    }

    impl ClipboardBackend for StubClipboardBackend {
        fn copy(&mut self, text: &str) -> bool {
            self.copied.push(text.to_string());
            self.copy_results.pop_front().unwrap_or(false)
        }

        fn paste(&mut self) -> Option<String> {
            self.paste_results.pop_front().unwrap_or(None)
        }
    }

    struct RootHookProbe {
        key_hits: Arc<AtomicUsize>,
        action_hits: Arc<AtomicUsize>,
        handle_key: bool,
    }

    impl Widget for RootHookProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
            if matches!(event, Event::Key(..)) {
                self.key_hits.fetch_add(1, Ordering::SeqCst);
                if self.handle_key {
                    ctx.set_handled();
                }
            }
        }

        fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
            if matches!(event, Event::Action(..)) {
                self.action_hits.fetch_add(1, Ordering::SeqCst);
                ctx.set_handled();
            }
        }
    }

    struct TreeEventProbe {
        focused: bool,
        capture_hits: Arc<AtomicUsize>,
    }

    impl Widget for TreeEventProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn has_focus(&self) -> bool {
            self.focused
        }

        fn on_event_capture(&mut self, event: &Event, _ctx: &mut EventCtx) {
            if matches!(event, Event::Key(..)) {
                self.capture_hits.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    #[test]
    fn binding_hints_dispatch_when_hint_payload_changes() {
        let last_hints = vec![BindingHint::new("tab", "next")];
        let current_hints = vec![
            BindingHint::new("tab", "next"),
            BindingHint::new("q", "quit"),
        ];
        let last_sources = vec![node_id_from_ffi(1)];
        let current_sources = vec![node_id_from_ffi(1)];

        assert!(should_dispatch_binding_hints(
            &last_hints,
            &last_sources,
            &current_hints,
            &current_sources,
        ));
    }

    #[test]
    fn dispatch_event_auto_tree_runs_root_key_capture_and_tree_dispatch() {
        let root_key_hits = Arc::new(AtomicUsize::new(0));
        let root_action_hits = Arc::new(AtomicUsize::new(0));
        let tree_root_capture_hits = Arc::new(AtomicUsize::new(0));
        let tree_focused_capture_hits = Arc::new(AtomicUsize::new(0));

        let mut tree = crate::widget_tree::WidgetTree::new();
        let tree_root = tree.set_root(Box::new(TreeEventProbe {
            focused: false,
            capture_hits: Arc::clone(&tree_root_capture_hits),
        }));
        tree.mount(
            tree_root,
            Box::new(TreeEventProbe {
                focused: true,
                capture_hits: Arc::clone(&tree_focused_capture_hits),
            }),
        );

        let mut app = test_app_with_tree(tree);
        let mut runtime_root = RootHookProbe {
            key_hits: Arc::clone(&root_key_hits),
            action_hits: Arc::clone(&root_action_hits),
            handle_key: false,
        };

        let outcome = app.dispatch_event_auto(
            &mut runtime_root,
            Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('k'),
                KeyModifiers::NONE,
            ))),
        );

        assert_eq!(root_key_hits.load(Ordering::SeqCst), 1);
        assert_eq!(tree_root_capture_hits.load(Ordering::SeqCst), 1);
        assert_eq!(tree_focused_capture_hits.load(Ordering::SeqCst), 1);
        assert!(!outcome.handled);
        assert_eq!(root_action_hits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn dispatch_event_auto_tree_root_key_handle_short_circuits_tree_dispatch() {
        let root_key_hits = Arc::new(AtomicUsize::new(0));
        let root_action_hits = Arc::new(AtomicUsize::new(0));
        let tree_root_capture_hits = Arc::new(AtomicUsize::new(0));
        let tree_focused_capture_hits = Arc::new(AtomicUsize::new(0));

        let mut tree = crate::widget_tree::WidgetTree::new();
        let tree_root = tree.set_root(Box::new(TreeEventProbe {
            focused: false,
            capture_hits: Arc::clone(&tree_root_capture_hits),
        }));
        tree.mount(
            tree_root,
            Box::new(TreeEventProbe {
                focused: true,
                capture_hits: Arc::clone(&tree_focused_capture_hits),
            }),
        );

        let mut app = test_app_with_tree(tree);
        let mut runtime_root = RootHookProbe {
            key_hits: Arc::clone(&root_key_hits),
            action_hits: Arc::clone(&root_action_hits),
            handle_key: true,
        };

        let outcome = app.dispatch_event_auto(
            &mut runtime_root,
            Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('k'),
                KeyModifiers::NONE,
            ))),
        );

        assert_eq!(root_key_hits.load(Ordering::SeqCst), 1);
        assert_eq!(tree_root_capture_hits.load(Ordering::SeqCst), 0);
        assert_eq!(tree_focused_capture_hits.load(Ordering::SeqCst), 0);
        assert!(outcome.handled);
        assert_eq!(root_action_hits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn dispatch_event_auto_tree_runs_root_action_fallback_when_unhandled() {
        let root_key_hits = Arc::new(AtomicUsize::new(0));
        let root_action_hits = Arc::new(AtomicUsize::new(0));
        let tree_capture_hits = Arc::new(AtomicUsize::new(0));

        let mut tree = crate::widget_tree::WidgetTree::new();
        tree.set_root(Box::new(TreeEventProbe {
            focused: true,
            capture_hits: Arc::clone(&tree_capture_hits),
        }));

        let mut app = test_app_with_tree(tree);
        let mut runtime_root = RootHookProbe {
            key_hits: Arc::clone(&root_key_hits),
            action_hits: Arc::clone(&root_action_hits),
            handle_key: false,
        };

        let outcome = app.dispatch_event_auto(&mut runtime_root, Event::Action(Action::HelpQuit));

        assert_eq!(root_action_hits.load(Ordering::SeqCst), 1);
        assert!(outcome.handled);
        assert_eq!(root_key_hits.load(Ordering::SeqCst), 0);
        assert_eq!(tree_capture_hits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn binding_hints_dispatch_when_sources_change_with_same_hints() {
        let hints = vec![BindingHint::new("tab", "next")];
        let last_sources = vec![node_id_from_ffi(1)];
        let current_sources = vec![node_id_from_ffi(2)];

        assert!(should_dispatch_binding_hints(
            &hints,
            &last_sources,
            &hints,
            &current_sources,
        ));
    }

    #[test]
    fn binding_hints_skip_when_hints_and_sources_are_stable() {
        let hints = vec![BindingHint::new("tab", "next")];
        let sources = vec![node_id_from_ffi(1)];

        assert!(!should_dispatch_binding_hints(
            &hints, &sources, &hints, &sources,
        ));
    }

    #[test]
    fn focused_help_dispatches_when_focus_source_changes() {
        assert!(should_dispatch_focused_help(
            Some(node_id_from_ffi(1)),
            Some("## First"),
            Some(node_id_from_ffi(2)),
            Some("## Second"),
        ));
    }

    #[test]
    fn focused_help_dispatches_when_help_clears() {
        assert!(should_dispatch_focused_help(
            Some(node_id_from_ffi(1)),
            Some("## First"),
            None,
            None,
        ));
    }

    #[test]
    fn focused_help_skips_when_source_and_markup_stable() {
        assert!(!should_dispatch_focused_help(
            Some(node_id_from_ffi(1)),
            Some("## Stable"),
            Some(node_id_from_ffi(1)),
            Some("## Stable"),
        ));
    }

    #[test]
    fn focused_help_message_emits_set_payload() {
        let source = node_id_from_ffi(7);
        let event = focused_help_message(Some((source, "## Source help".to_string())));
        assert_eq!(event.sender, source);
        assert!(matches!(
            event.message,
            Message::HelpPanelFocusedHelpChanged(crate::message::HelpPanelFocusedHelpChanged {
                source: msg_source,
                markup,
            }) if msg_source == source && markup == "## Source help"
        ));
    }

    #[test]
    fn focused_help_message_emits_clear_payload() {
        let event = focused_help_message(None);
        assert_eq!(event.sender, node_id_from_ffi(0));
        assert!(matches!(
            event.message,
            Message::HelpPanelFocusedHelpCleared(_)
        ));
    }

    #[test]
    fn clipboard_runtime_handles_copy_then_paste_request() {
        let target = node_id_from_ffi(42);
        let mut clipboard = None;
        let mut backend = StubClipboardBackend {
            copy_results: VecDeque::from(vec![true]),
            paste_results: VecDeque::from(vec![None]),
            copied: Vec::new(),
        };
        let generated = collect_clipboard_runtime_messages_with_backend(
            &mut clipboard,
            &[
                MessageEvent {
                    sender: node_id_from_ffi(1),
                    message: Message::TextEditClipboardCopyRequested(
                        crate::message::TextEditClipboardCopyRequested {
                            text: "hello".to_string(),
                            cut: false,
                        },
                    ),
                    control: None,
                },
                MessageEvent {
                    sender: node_id_from_ffi(2),
                    message: Message::TextEditClipboardPasteRequested(
                        crate::message::TextEditClipboardPasteRequested { target },
                    ),
                    control: None,
                },
            ],
            &mut backend,
        );
        assert_eq!(clipboard.as_deref(), Some("hello"));
        assert_eq!(backend.copied, vec!["hello".to_string()]);
        assert_eq!(generated.len(), 1);
        assert!(matches!(
            &generated[0].message,
            Message::TextEditClipboardPaste(crate::message::TextEditClipboardPaste {
                target: t,
                text
            }) if *t == target && text == "hello"
        ));
    }

    #[test]
    fn clipboard_runtime_ignores_paste_without_buffered_text() {
        let target = node_id_from_ffi(7);
        let mut clipboard = None;
        let mut backend = StubClipboardBackend::default();
        let generated = collect_clipboard_runtime_messages_with_backend(
            &mut clipboard,
            &[MessageEvent {
                sender: node_id_from_ffi(2),
                message: Message::TextEditClipboardPasteRequested(
                    crate::message::TextEditClipboardPasteRequested { target },
                ),
                control: None,
            }],
            &mut backend,
        );
        assert!(generated.is_empty());
    }

    #[test]
    fn clipboard_runtime_prefers_system_clipboard_on_paste() {
        let target = node_id_from_ffi(9);
        let mut clipboard = Some("fallback".to_string());
        let mut backend = StubClipboardBackend {
            copy_results: VecDeque::new(),
            paste_results: VecDeque::from(vec![Some("system".to_string())]),
            copied: Vec::new(),
        };

        let generated = collect_clipboard_runtime_messages_with_backend(
            &mut clipboard,
            &[MessageEvent {
                sender: node_id_from_ffi(2),
                message: Message::TextEditClipboardPasteRequested(
                    crate::message::TextEditClipboardPasteRequested { target },
                ),
                control: None,
            }],
            &mut backend,
        );

        assert_eq!(clipboard.as_deref(), Some("system"));
        assert_eq!(generated.len(), 1);
        assert!(matches!(
            &generated[0].message,
            Message::TextEditClipboardPaste(crate::message::TextEditClipboardPaste { target: t, text }) if *t == target && text == "system"
        ));
    }

    struct StyleNode {
        _node_id: NodeId,
        type_name: &'static str,
        style_id: Option<String>,
        classes: Vec<String>,
        focused: bool,
        children: Vec<StyleNode>,
    }

    impl StyleNode {
        fn new(type_name: &'static str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Self {
                _node_id: node_id_from_ffi(n),
                type_name,
                style_id: None,
                classes: Vec::new(),
                focused: false,
                children: Vec::new(),
            }
        }

        fn with_class(mut self, class: &str) -> Self {
            self.classes.push(class.to_string());
            self
        }

        fn with_focus(mut self, focused: bool) -> Self {
            self.focused = focused;
            self
        }

        fn with_child(mut self, child: StyleNode) -> Self {
            self.children.push(child);
            self
        }
    }

    impl Widget for StyleNode {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            self.type_name
        }

        fn style_id(&self) -> Option<&str> {
            self.style_id.as_deref()
        }

        fn style_classes(&self) -> &[String] {
            &self.classes
        }

        fn has_focus(&self) -> bool {
            self.focused
        }
    }

    /// Build a `WidgetTree` from a `StyleNode` hierarchy for testing.
    fn build_tree_from_style_node(node: StyleNode) -> (crate::widget_tree::WidgetTree, NodeId) {
        let mut tree = crate::widget_tree::WidgetTree::new();
        fn insert(
            tree: &mut crate::widget_tree::WidgetTree,
            mut node: StyleNode,
            parent: Option<NodeId>,
        ) -> NodeId {
            let children = std::mem::take(&mut node.children);
            let id = if let Some(p) = parent {
                tree.mount(p, Box::new(node))
            } else {
                tree.set_root(Box::new(node))
            };
            for child in children {
                insert(tree, child, Some(id));
            }
            id
        }
        let root_id = insert(&mut tree, node, None);
        (tree, root_id)
    }

    #[test]
    fn stylesheet_invalidation_matches_selectively_via_tree() {
        let button = StyleNode::new("Button").with_class("special");
        let root_node = StyleNode::new("Container")
            .with_class("panel")
            .with_child(button)
            .with_child(StyleNode::new("Label"));

        let (tree, _root_id) = build_tree_from_style_node(root_node);

        // Descendant combinator: Container.panel > Button.special
        let changed = StyleSheet::parse("Container.panel > Button.special { bg: #334455; }");
        let affected = collect_stylesheet_affected_widgets_root(
            tree.get(_root_id).unwrap().widget.as_ref(),
            changed.rules(),
            true,
            crate::css::AppRuntimePseudos::default(),
        );
        // Root-only check: root is "Container.panel" which doesn't match "Button.special"
        assert!(affected.is_empty());

        // Tree-based check: walks children with ancestor context and finds the Button
        let affected_tree = super::collect_stylesheet_affected_widgets_tree(
            &tree,
            changed.rules(),
            true,
            crate::css::AppRuntimePseudos::default(),
        );
        // The tree should find exactly the Button node (child of Container.panel)
        assert_eq!(affected_tree.len(), 1);
    }

    #[test]
    fn stylesheet_invalidation_respects_focus_pseudo_state_via_tree() {
        let button = StyleNode::new("Button").with_focus(true);
        let root_node = StyleNode::new("Container").with_child(button);

        let (tree, _root_id) = build_tree_from_style_node(root_node);

        let changed = StyleSheet::parse("Button:focus { fg: #ffffff; }");
        let affected_active = super::collect_stylesheet_affected_widgets_tree(
            &tree,
            changed.rules(),
            true,
            crate::css::AppRuntimePseudos::default(),
        );
        let affected_inactive = super::collect_stylesheet_affected_widgets_tree(
            &tree,
            changed.rules(),
            false,
            crate::css::AppRuntimePseudos::default(),
        );

        assert!(!affected_active.is_empty());
        assert!(affected_inactive.is_empty());
    }

    #[test]
    fn overlay_visibility_hides_modal_subtree_display_in_tree_mode() {
        let root_node = StyleNode::new("Container").with_child(
            StyleNode::new("Overlay")
                .with_child(StyleNode::new("Base"))
                .with_child(StyleNode::new("Modal").with_child(StyleNode::new("ModalBody"))),
        );
        let (mut tree, root_id) = build_tree_from_style_node(root_node);
        let overlay_id = tree.children(root_id)[0];
        let base_id = tree.children(overlay_id)[0];
        let modal_id = tree.children(overlay_id)[1];
        let modal_body_id = tree.children(modal_id)[0];

        assert!(set_overlay_modal_display_tree(&mut tree, overlay_id, false));
        assert!(
            tree.get(base_id).unwrap().display,
            "base child stays displayed"
        );
        assert!(!tree.get(modal_id).unwrap().display, "modal root hidden");
        assert!(
            !tree.get(modal_body_id).unwrap().display,
            "modal descendants hidden"
        );
    }

    #[test]
    fn overlay_visibility_show_restores_modal_subtree_display_in_tree_mode() {
        let root_node = StyleNode::new("Container").with_child(
            StyleNode::new("Overlay")
                .with_child(StyleNode::new("Base"))
                .with_child(StyleNode::new("Modal").with_child(StyleNode::new("ModalBody"))),
        );
        let (mut tree, root_id) = build_tree_from_style_node(root_node);
        let overlay_id = tree.children(root_id)[0];
        let modal_id = tree.children(overlay_id)[1];
        let modal_body_id = tree.children(modal_id)[0];

        assert!(set_overlay_modal_display_tree(&mut tree, overlay_id, false));
        assert!(set_overlay_modal_display_tree(&mut tree, overlay_id, true));
        assert!(tree.get(modal_id).unwrap().display, "modal root shown");
        assert!(
            tree.get(modal_body_id).unwrap().display,
            "modal descendants shown"
        );
    }

    #[test]
    fn p2g36_runtime_transition_dispatch_matches_changed_properties() {
        let target = node_id_from_ffi(99);
        let old = Style::new().opacity(10).text_opacity(30);
        let mut new = Style::new().opacity(80).text_opacity(30);
        new.transitions = Some(vec![
            PropertyTransition {
                property: "opacity".to_string(),
                duration: std::time::Duration::from_millis(250),
                timing: TransitionTiming::Linear,
                delay: std::time::Duration::from_millis(20),
            },
            PropertyTransition {
                property: "offset_y".to_string(),
                duration: std::time::Duration::from_millis(500),
                timing: TransitionTiming::InOutCubic,
                delay: std::time::Duration::ZERO,
            },
        ]);

        let requests = transition_requests_for_style_change(target, &old, &new);
        assert_eq!(
            requests.len(),
            1,
            "only changed+transitioned property should animate"
        );
        assert_eq!(requests[0].target, target);
        assert_eq!(requests[0].attribute, "opacity");
        assert_eq!(requests[0].start, 10.0);
        assert_eq!(requests[0].end, 80.0);
        assert_eq!(requests[0].duration, std::time::Duration::from_millis(250));
        assert_eq!(requests[0].delay, std::time::Duration::from_millis(20));
    }

    #[test]
    fn p2g36_runtime_transition_dispatch_handles_css_hyphen_names() {
        let target = node_id_from_ffi(101);
        let mut old = Style::new();
        old.offset = Some(Offset {
            x: OffsetValue::Cells(0),
            y: OffsetValue::Cells(0),
        });
        let mut new = Style::new();
        new.offset = Some(Offset {
            x: OffsetValue::Cells(0),
            y: OffsetValue::Cells(6),
        });
        new.transitions = Some(vec![PropertyTransition {
            property: "offset-y".to_string(),
            duration: std::time::Duration::from_millis(120),
            timing: TransitionTiming::OutCubic,
            delay: std::time::Duration::ZERO,
        }]);

        let requests = transition_requests_for_style_change(target, &old, &new);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].attribute, "offset_y");
        assert_eq!(requests[0].start, 0.0);
        assert_eq!(requests[0].end, 6.0);
        assert_eq!(requests[0].duration, std::time::Duration::from_millis(120));
    }

    // ── Worker request accumulator tests ─────────────────────────────

    #[test]
    fn worker_accumulator_drain_empty() {
        // Ensure thread-local is empty before starting.
        let _ = super::drain_accumulated_worker_requests();
        let drained = super::drain_accumulated_worker_requests();
        assert!(drained.is_empty());
    }

    #[test]
    fn worker_accumulator_collects_from_outcome() {
        use crate::worker::{WorkerRequest, WorkerRequestPayload};
        // Clear any leftovers.
        let _ = super::drain_accumulated_worker_requests();

        let mut outcome = super::DispatchOutcome {
            worker_requests: vec![
                WorkerRequest {
                    owner: node_id_from_ffi(1),
                    exclusive_key: None,
                    name: Some("w1".into()),
                    payload: WorkerRequestPayload::default(),
                },
                WorkerRequest {
                    owner: node_id_from_ffi(2),
                    exclusive_key: Some("exc".into()),
                    name: None,
                    payload: WorkerRequestPayload::default(),
                },
            ],
            ..Default::default()
        };
        super::accumulate_worker_requests(&mut outcome);
        assert!(
            outcome.worker_requests.is_empty(),
            "should drain from outcome"
        );

        let drained = super::drain_accumulated_worker_requests();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].name.as_deref(), Some("w1"));
        assert_eq!(drained[1].exclusive_key.as_deref(), Some("exc"));

        // Second drain should be empty.
        let drained2 = super::drain_accumulated_worker_requests();
        assert!(drained2.is_empty());
    }

    #[test]
    fn worker_accumulator_multiple_outcomes() {
        use crate::worker::{WorkerRequest, WorkerRequestPayload};
        let _ = super::drain_accumulated_worker_requests();

        let mut o1 = super::DispatchOutcome {
            worker_requests: vec![WorkerRequest {
                owner: node_id_from_ffi(1),
                exclusive_key: None,
                name: Some("a".into()),
                payload: WorkerRequestPayload::default(),
            }],
            ..Default::default()
        };
        let mut o2 = super::DispatchOutcome {
            worker_requests: vec![WorkerRequest {
                owner: node_id_from_ffi(2),
                exclusive_key: None,
                name: Some("b".into()),
                payload: WorkerRequestPayload::default(),
            }],
            ..Default::default()
        };
        super::accumulate_worker_requests(&mut o1);
        super::accumulate_worker_requests(&mut o2);

        let drained = super::drain_accumulated_worker_requests();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].name.as_deref(), Some("a"));
        assert_eq!(drained[1].name.as_deref(), Some("b"));
    }

    #[test]
    fn worker_accumulator_empty_outcome_is_noop() {
        let _ = super::drain_accumulated_worker_requests();

        let mut outcome = super::DispatchOutcome::default();
        super::accumulate_worker_requests(&mut outcome);

        let drained = super::drain_accumulated_worker_requests();
        assert!(drained.is_empty());
    }

    #[test]
    fn worker_full_pipeline_ctx_to_registry() {
        use crate::event::EventCtx;
        use crate::worker::{WorkerRegistry, WorkerState, process_worker_requests};
        let _ = super::drain_accumulated_worker_requests();

        // 1. Widget creates worker requests via EventCtx.
        let owner = node_id_from_ffi(42);
        let mut ctx = EventCtx::default();
        ctx.set_node_id(owner);
        ctx.request_worker(Some("bg-job"));
        ctx.request_exclusive_worker("search", Some("searcher"));

        // 2. Runtime drains ctx (simulating what routing.rs does).
        let requests = ctx.take_worker_requests();
        assert_eq!(requests.len(), 2);

        // 3. Feed into an outcome (simulating DispatchOutcome construction).
        let mut outcome = super::DispatchOutcome {
            worker_requests: requests,
            ..Default::default()
        };

        // 4. accumulate_worker_requests drains outcome into thread-local.
        super::accumulate_worker_requests(&mut outcome);
        assert!(outcome.worker_requests.is_empty());

        // 5. At end of tick, drain and process.
        let pending = super::drain_accumulated_worker_requests();
        assert_eq!(pending.len(), 2);

        let mut registry = WorkerRegistry::new();
        let mut changes = process_worker_requests(&mut registry, pending);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(150);
        while changes.len() < 2 && std::time::Instant::now() < deadline {
            let mut batch = process_worker_requests(&mut registry, Vec::new());
            if batch.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(1));
                continue;
            }
            changes.append(&mut batch);
        }
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].state, WorkerState::Success);
        assert_eq!(changes[1].state, WorkerState::Success);

        // 6. Cleanup removes finished workers.
        registry.cleanup();
        assert!(registry.active_workers().is_empty());
    }

    #[test]
    fn worker_request_processing_in_runtime_hot_path_is_non_blocking() {
        use crate::worker::{WorkerRegistry, WorkerRequest, WorkerRequestPayload, WorkerState};

        let owner = node_id_from_ffi(90);
        let mut registry = WorkerRegistry::new();
        let delayed_request = WorkerRequest {
            owner,
            exclusive_key: None,
            name: Some("delayed".into()),
            payload: WorkerRequestPayload::ComputeDigest {
                input: "payload".into(),
                rounds: 1,
                delay_per_round_ms: 80,
                fail_with: None,
            },
        };

        let start = std::time::Instant::now();
        let first = crate::worker::process_worker_requests(&mut registry, vec![delayed_request]);
        let elapsed = start.elapsed();

        assert!(
            elapsed < std::time::Duration::from_millis(40),
            "worker processing should not block waiting for completion; elapsed={elapsed:?}"
        );
        assert!(
            first.is_empty(),
            "delayed worker completion should not be synchronously delivered"
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(250);
        let mut completed = Vec::new();
        while completed.is_empty() && std::time::Instant::now() < deadline {
            completed = crate::worker::process_worker_requests(&mut registry, Vec::new());
            if completed.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].state, WorkerState::Success);
    }

    struct WorkerDeliveryProbe {
        success_hits: Arc<AtomicUsize>,
        error_hits: Arc<AtomicUsize>,
    }

    impl Widget for WorkerDeliveryProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
            if let Message::WorkerStateChanged(crate::message::WorkerStateChanged {
                state, ..
            }) = &message.message
            {
                match state {
                    crate::worker::WorkerState::Success => {
                        self.success_hits.fetch_add(1, Ordering::Relaxed);
                        ctx.set_handled();
                    }
                    crate::worker::WorkerState::Error(_) => {
                        self.error_hits.fetch_add(1, Ordering::Relaxed);
                        ctx.set_handled();
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn worker_state_changes_route_to_owning_widgets_via_message_pipeline() {
        use crate::worker::{
            WorkerRegistry, WorkerRequest, WorkerRequestPayload, WorkerState,
            process_worker_requests,
        };

        let success_hits = Arc::new(AtomicUsize::new(0));
        let error_hits = Arc::new(AtomicUsize::new(0));
        let bystander_hits = Arc::new(AtomicUsize::new(0));

        let mut tree = crate::widget_tree::WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let owner_success = tree.mount(
            root_id,
            Box::new(WorkerDeliveryProbe {
                success_hits: Arc::clone(&success_hits),
                error_hits: Arc::new(AtomicUsize::new(0)),
            }),
        );
        let owner_error = tree.mount(
            root_id,
            Box::new(WorkerDeliveryProbe {
                success_hits: Arc::new(AtomicUsize::new(0)),
                error_hits: Arc::clone(&error_hits),
            }),
        );
        let _bystander = tree.mount(
            root_id,
            Box::new(WorkerDeliveryProbe {
                success_hits: Arc::clone(&bystander_hits),
                error_hits: Arc::clone(&bystander_hits),
            }),
        );

        let mut registry = WorkerRegistry::new();
        let (errored_worker, _) = registry.register(owner_error, None, Some("errored".into()));
        registry.set_running(errored_worker);
        registry.complete(errored_worker, Err("boom".into()));

        let requests = vec![WorkerRequest {
            owner: owner_success,
            exclusive_key: None,
            name: Some("ok".into()),
            payload: WorkerRequestPayload::default(),
        }];
        let mut changes = process_worker_requests(&mut registry, requests);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(150);
        while changes.len() < 2 && std::time::Instant::now() < deadline {
            let mut batch = process_worker_requests(&mut registry, Vec::new());
            if batch.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(1));
                continue;
            }
            changes.append(&mut batch);
        }
        assert_eq!(changes.len(), 2);
        assert!(
            changes
                .iter()
                .any(|c| c.worker_id == errored_worker
                    && c.state == WorkerState::Error("boom".into()))
        );
        assert!(changes.iter().any(|c| c.state == WorkerState::Success));

        let messages = super::worker_state_runtime_messages(&registry, changes);
        assert_eq!(messages.len(), 2);
        assert!(
            messages
                .iter()
                .all(|event| event.control == Some(event.sender))
        );
        assert!(messages.iter().any(|event| event.sender == owner_success
            && matches!(
                event.message,
                Message::WorkerStateChanged(crate::message::WorkerStateChanged {
                    state: WorkerState::Success,
                    ..
                })
            )));
        assert!(messages.iter().any(|event| event.sender == owner_error
            && matches!(
                event.message,
                Message::WorkerStateChanged(crate::message::WorkerStateChanged {
                    state: WorkerState::Error(ref err),
                    ..
                }) if err == "boom"
            )));

        let mut app = test_app_with_tree(tree);
        let mut runtime_root = StyleNode::new("RuntimeRoot");
        let routed = app.dispatch_message_queue_with_runtime(&mut runtime_root, messages);
        assert!(routed.handled);
        assert_eq!(success_hits.load(Ordering::Relaxed), 1);
        assert_eq!(error_hits.load(Ordering::Relaxed), 1);
        assert_eq!(bystander_hits.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn worker_state_runtime_messages_fallback_to_runtime_sender_when_owner_missing() {
        let registry = crate::worker::WorkerRegistry::new();
        let orphan_change = crate::worker::WorkerStateChanged {
            worker_id: crate::worker::WorkerId::new(),
            state: crate::worker::WorkerState::Cancelled,
        };

        let messages = super::worker_state_runtime_messages(&registry, vec![orphan_change]);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, crate::node_id::node_id_from_ffi(0));
        assert_eq!(
            messages[0].control,
            Some(crate::node_id::node_id_from_ffi(0))
        );
        assert!(matches!(
            messages[0].message,
            Message::WorkerStateChanged(crate::message::WorkerStateChanged {
                state: crate::worker::WorkerState::Cancelled,
                ..
            })
        ));
    }

    #[test]
    fn runtime_app_selector_messages_mutate_tree_and_request_layout_invalidation() {
        let mut tree = crate::widget_tree::WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        tree.mount(root_id, Box::new(crate::widgets::Button::new("go")));
        let mut app = test_app_with_tree(tree);
        let mut runtime_root = StyleNode::new("RuntimeRoot");

        let messages = vec![MessageEvent {
            sender: node_id_from_ffi(1),
            message: Message::AppAddClass(crate::message::AppAddClass {
                selector: "Button".to_string(),
                class_name: "highlight".to_string(),
            }),
            control: Some(node_id_from_ffi(1)),
        }];
        let outcome = app.dispatch_message_queue_with_runtime(&mut runtime_root, messages);
        assert!(outcome.repaint_requested);
        assert!(outcome.invalidation.layout);
        let highlighted = app.query(".highlight").expect("selector parses");
        assert_eq!(highlighted.len(), 1);
    }

    struct AppActionHost;

    impl Widget for AppActionHost {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn action_namespace(&self) -> &str {
            "app"
        }

        fn action_registry(&self) -> &[ActionDecl] {
            const ACTIONS: &[ActionDecl] = &[
                ActionDecl {
                    name: "add_class",
                    namespace: "app",
                    description: "Add class",
                    default_binding: None,
                },
                ActionDecl {
                    name: "remove_class",
                    namespace: "app",
                    description: "Remove class",
                    default_binding: None,
                },
                ActionDecl {
                    name: "toggle_class",
                    namespace: "app",
                    description: "Toggle class",
                    default_binding: None,
                },
            ];
            ACTIONS
        }

        fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
            if action.name != "add_class" || action.arguments.len() != 2 {
                return false;
            }
            ctx.post_message(Message::AppAddClass(crate::message::AppAddClass {
                selector: action.arguments[0].clone(),
                class_name: action.arguments[1].clone(),
            }));
            ctx.set_handled();
            true
        }
    }

    #[test]
    fn action_routing_app_add_class_uses_runtime_pipeline() {
        let mut tree = crate::widget_tree::WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let app_node = tree.mount(root, Box::new(AppActionHost));
        let button = tree.mount(app_node, Box::new(crate::widgets::Button::new("ok")));
        if let Some(node) = tree.get_mut(button) {
            node.widget.set_focus(true);
        }

        let mut app = test_app_with_tree(tree);
        let parsed =
            parse_action("app.add_class('Button', 'from-action')").expect("action should parse");
        let resolved = {
            let tree_ref = app.widget_tree.as_ref().expect("tree exists");
            resolve_action(&parsed, tree_ref, button, |nid| {
                tree_ref.get(nid).map(|node| {
                    (
                        node.widget.action_namespace(),
                        node.widget.action_registry(),
                    )
                })
            })
        }
        .expect("action should resolve");
        assert_eq!(resolved.node, app_node);

        let mut ctx = EventCtx::default();
        if let Some(tree_mut) = app.widget_tree.as_mut()
            && let Some(node) = tree_mut.get_mut(resolved.node)
        {
            assert!(node.widget.execute_action(&parsed, &mut ctx));
        }
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);

        let mut runtime_root = StyleNode::new("RuntimeRoot");
        let outcome = app.dispatch_message_queue_with_runtime(&mut runtime_root, messages);
        assert!(outcome.repaint_requested);
        assert!(outcome.invalidation.layout);
        let mutated = app.query(".from-action").expect("selector should parse");
        assert_eq!(mutated.len(), 1);
    }

    struct ReactivePhaseProbeWidget {
        value: i32,
        watch_calls: Arc<AtomicUsize>,
        init_calls: Arc<AtomicUsize>,
        emit_init: bool,
        init_enabled: bool,
    }

    impl ReactivePhaseProbeWidget {
        fn new(
            watch_calls: Arc<AtomicUsize>,
            init_calls: Arc<AtomicUsize>,
            emit_init: bool,
            init_enabled: bool,
        ) -> Self {
            Self {
                value: 0,
                watch_calls,
                init_calls,
                emit_init,
                init_enabled,
            }
        }

        fn set_value(&mut self, value: i32) {
            if self.value == value {
                return;
            }

            let old = self.value;
            self.value = value;
            let node_id = self.node_id();
            let mut rctx = ReactiveCtx::new(node_id);
            rctx.record_change(
                "value",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
            enqueue_runtime_reactive_entry(crate::reactive::RuntimeReactiveEntry::new(
                node_id, rctx,
            ));
        }

        fn enqueue_init_watcher(&mut self) {
            if !self.emit_init || !self.init_enabled {
                return;
            }
            let node_id = self.node_id();
            let mut rctx = ReactiveCtx::new(node_id);
            rctx.record_change(
                "value",
                ReactiveFlags::reactive(),
                Box::new(self.value),
                Box::new(self.value),
            );
            enqueue_runtime_reactive_entry(crate::reactive::RuntimeReactiveEntry::new(
                node_id, rctx,
            ));
        }
    }

    impl ReactiveWidget for ReactivePhaseProbeWidget {
        fn reactive_dispatch(&mut self, changes: &[ReactiveChange], _ctx: &mut ReactiveCtx) {
            for change in changes {
                self.watch_calls.fetch_add(1, Ordering::SeqCst);
                if let (Some(old), Some(new)) = (
                    change.old_value.downcast_ref::<i32>(),
                    change.new_value.downcast_ref::<i32>(),
                ) {
                    if old == new {
                        self.init_calls.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }
    }

    impl Widget for ReactivePhaseProbeWidget {
        fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
            Some(self)
        }

        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_event(&mut self, event: &Event, _ctx: &mut EventCtx) {
            match event {
                Event::Action(Action::Toggle) => self.set_value(1),
                Event::Mount(_mount) => self.enqueue_init_watcher(),
                _ => {}
            }
        }
    }

    fn test_app_with_tree(tree: crate::widget_tree::WidgetTree) -> crate::runtime::App {
        let mut app = super::App::new().expect("app should initialize for runtime tests");
        app.widget_tree = Some(tree);
        app
    }

    #[test]
    fn reactive_phase_in_event_loop_runs_setter_watcher_and_repaint_invalidation() {
        let _ = take_runtime_reactive_entries();
        let watch_calls = Arc::new(AtomicUsize::new(0));
        let init_calls = Arc::new(AtomicUsize::new(0));

        let mut tree = crate::widget_tree::WidgetTree::new();
        let target = tree.set_root(Box::new(ReactivePhaseProbeWidget::new(
            Arc::clone(&watch_calls),
            Arc::clone(&init_calls),
            false,
            false,
        )));
        let _ =
            super::dispatch_event_to_target_tree(&mut tree, target, &Event::Action(Action::Toggle));

        let mut app = test_app_with_tree(tree);
        let mut pending = super::PendingInvalidation::default();
        let mut root = StyleNode::new("Root");
        app.run_event_loop_reactive_phase(&mut root, &mut pending);

        assert_eq!(watch_calls.load(Ordering::SeqCst), 1);
        assert!(pending.flags.content);
        assert!(pending.is_dirty());
    }

    #[test]
    fn reactive_phase_mount_init_watcher_respects_init_flag() {
        let _ = take_runtime_reactive_entries();
        let init_true_calls = Arc::new(AtomicUsize::new(0));
        let init_false_calls = Arc::new(AtomicUsize::new(0));
        let watch_calls = Arc::new(AtomicUsize::new(0));

        let mut tree = crate::widget_tree::WidgetTree::new();
        let root = tree.set_root(Box::new(StyleNode::new("Root")));
        let init_true = tree.mount(
            root,
            Box::new(ReactivePhaseProbeWidget::new(
                Arc::clone(&watch_calls),
                Arc::clone(&init_true_calls),
                true,
                true,
            )),
        );
        let init_false = tree.mount(
            root,
            Box::new(ReactivePhaseProbeWidget::new(
                Arc::clone(&watch_calls),
                Arc::clone(&init_false_calls),
                true,
                false,
            )),
        );

        let _ = super::dispatch_event_to_target_tree(
            &mut tree,
            init_true,
            &Event::Mount(MountEvent { node: init_true }),
        );
        let _ = super::dispatch_event_to_target_tree(
            &mut tree,
            init_false,
            &Event::Mount(MountEvent { node: init_false }),
        );

        let mut app = test_app_with_tree(tree);
        let mut pending = super::PendingInvalidation::default();
        let mut runtime_root = StyleNode::new("RuntimeRoot");
        app.run_event_loop_reactive_phase(&mut runtime_root, &mut pending);

        assert_eq!(init_true_calls.load(Ordering::SeqCst), 1);
        assert_eq!(init_false_calls.load(Ordering::SeqCst), 0);
        assert!(pending.flags.content);
    }
}
