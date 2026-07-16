use crate::css::{AppRuntimePseudos, set_app_active, set_app_runtime_pseudos, set_style_context};
use crate::debug::{debug_input, debug_render, debug_timing, timing_enabled};
use crate::event::{
    Action, AnimationEase, AnimationRequest, AnimationValueEvent, BlurEvent, ClassOp, Event,
    EventCtx, FocusEvent, MountEvent, MouseDownEvent, MouseScrollEvent, MouseUpEvent, ReadyEvent,
    StyleAnimationRequest, StyleValue, UnmountEvent, WidgetCtx,
};
use crate::keys::KeyEventData;
use crate::message::MessageEvent;
use crate::worker::{WorkerRegistry, WorkerRequest, process_worker_requests};
use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};
use rich_rs::Renderable;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use super::App;
use super::devtools::DevtoolsCommand;
use super::dispatch_ctx::set_dispatch_recipient;
use super::helpers::{
    any_widget_active_tree, call_on_mouse_move_tree, collect_focus_chain_tree,
    generate_enter_leave_events, mouse_scroll_deltas, pointer_shape_for_hover_tree,
    should_quit_key, tree_content_local_coords, widget_at_tree_layout,
};
use super::render::apply_layout_info_tree_from_layout_rects;
use super::routing::{
    active_binding_hints_tree, dispatch_event_broadcast_tree, dispatch_event_to_target_tree,
    dispatch_event_tree, dispatch_message_queue_tree, dispatch_mouse_scroll,
    dispatch_mouse_scroll_to_target_tree, dispatch_scroll_action_tree, focused_help_metadata_tree,
    focused_node_id_tree, is_priority_action, is_scroll_action, match_binding_chain,
    BindingSource,
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

/// RAII guard that registers the current thread as the UI thread for
/// `App::call_from_thread` and unregisters it (draining pending jobs) on drop,
/// covering every event-loop exit path.
struct CallFromThreadGuard;

impl CallFromThreadGuard {
    fn register() -> Self {
        crate::runtime::tasks::register_ui_thread();
        Self
    }
}

impl Drop for CallFromThreadGuard {
    fn drop(&mut self) {
        crate::runtime::tasks::unregister_ui_thread();
    }
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
        MessageEvent::new(
            source,
            crate::message::HelpPanelFocusedHelpChanged { source, markup },
        )
        .with_control(source)
    } else {
        let sender = App::runtime_message_sender();
        MessageEvent::new(sender, crate::message::HelpPanelFocusedHelpCleared).with_control(sender)
    }
}

fn parse_simulated_key(spec: &str) -> Option<KeyEventData> {
    let spec = spec.trim().to_ascii_lowercase();
    if spec.is_empty() {
        return None;
    }

    let (modifiers, key_token) = if let Some(chord) = spec.strip_prefix('^') {
        if chord.chars().count() == 1 {
            (KeyModifiers::CONTROL, chord.to_string())
        } else {
            return None;
        }
    } else {
        let mut modifiers = KeyModifiers::NONE;
        let mut key_token = None::<String>;
        for token in spec
            .split('+')
            .map(str::trim)
            .filter(|token| !token.is_empty())
        {
            match token {
                "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
                "alt" => modifiers |= KeyModifiers::ALT,
                "shift" => modifiers |= KeyModifiers::SHIFT,
                "super" | "meta" => modifiers |= KeyModifiers::SUPER,
                other => key_token = Some(other.to_string()),
            }
        }
        (modifiers, key_token?)
    };

    let code = match key_token.as_str() {
        "enter" | "return" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "escape" | "esc" => KeyCode::Esc,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "page_up" => KeyCode::PageUp,
        "pagedown" | "page_down" => KeyCode::PageDown,
        "insert" => KeyCode::Insert,
        "space" => KeyCode::Char(' '),
        token if token.starts_with('f') && token.len() > 1 => {
            let number = token[1..].parse::<u8>().ok()?;
            KeyCode::F(number)
        }
        token if token.chars().count() == 1 => KeyCode::Char(token.chars().next().unwrap()),
        _ => return None,
    };

    Some(KeyEventData::from_crossterm(KeyEvent::new(code, modifiers)))
}

fn input_event_kind(event: &CrosstermEvent) -> &'static str {
    match event {
        CrosstermEvent::Key(_) => "key",
        CrosstermEvent::Mouse(_) => "mouse",
        CrosstermEvent::Resize(_, _) => "resize",
        CrosstermEvent::FocusLost => "focus_lost",
        CrosstermEvent::FocusGained => "focus_gained",
        CrosstermEvent::Paste(_) => "paste",
    }
}

fn scrollbar_drag_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("TEXTUAL_DEBUG_SCROLLBAR_DRAG_TRACE")
            .ok()
            .map(|value| {
                let normalized = value.trim().to_ascii_lowercase();
                !(normalized.is_empty()
                    || normalized == "0"
                    || normalized == "false"
                    || normalized == "off"
                    || normalized == "no")
            })
            .unwrap_or(false)
    })
}

fn merge_outcome_into_runtime_pass(pass: &mut RuntimeMessagePass, outcome: &mut DispatchOutcome) {
    pass.repaint_requested |= outcome.repaint_requested;
    pass.invalidation.merge(outcome.invalidation);
    pass.stop_requested |= outcome.stop_requested;
    pass.animation_requests
        .append(&mut outcome.animation_requests);
    pass.style_animation_requests
        .append(&mut outcome.style_animation_requests);
    pass.worker_requests.append(&mut outcome.worker_requests);
    pass.recompose_nodes.append(&mut outcome.recompose_nodes);
    pass.class_ops.append(&mut outcome.class_ops);
    pass.generated.append(&mut outcome.messages);
}

fn execute_action_with_dispatch_target(
    widget: &mut dyn Widget,
    action: &crate::action::ParsedAction,
    ctx: &mut EventCtx,
    target: NodeId,
) -> bool {
    let _dispatch_guard = set_dispatch_recipient(target, crate::widgets::NodeState::default());
    let mut wctx = WidgetCtx::__from_dispatch(target, ctx);
    let handled = widget.execute_action(action, &mut wctx);
    wctx.__enqueue_reactive_if_dirty();
    handled
}

thread_local! {
    /// Bounded record of matched-but-unhandled binding actions (newest last),
    /// observable via [`take_unhandled_binding_reports`]. UI-thread local, so
    /// tests and tooling drain exactly what their own dispatches produced.
    static UNHANDLED_BINDING_REPORTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Cap on the retained unhandled-binding reports (oldest dropped first), so a
/// long-lived app repeatedly pressing an unwired binding cannot grow the
/// buffer unboundedly when nothing drains it.
const UNHANDLED_BINDING_REPORTS_CAP: usize = 32;

/// Report a matched `BindingDecl` whose action fell through every dispatch
/// layer unhandled: the source-node `execute_action`, the app-root
/// `execute_action`, and the `on_app_unhandled_action` fallback all declined.
///
/// Without this, such a binding is a silent no-op: the author believes the key
/// is wired but nothing happens. Python surfaces the same situation via
/// `App._dispatch_action`'s `log.system("<action> ... has no target. Could not
/// find methods ...")` (`app.py`). The warning goes to the input debug channel
/// (`TEXTUAL_DEBUG_INPUT_FILE`) and to a bounded, thread-local buffer for
/// tests/tooling. Default behavior for the key (falling through to raw key
/// dispatch) is unchanged.
fn report_unhandled_binding_action(source_node: NodeId, action_str: &str) {
    let line = format!(
        "[input] WARNING: binding matched on node {} but action {:?} was not handled by any \
         node; expected an `execute_action` arm / `action_registry` entry on the target widget \
         or an app-level `on_app_action_str` handler",
        node_id_to_ffi(source_node),
        action_str,
    );
    debug_input(&line);
    UNHANDLED_BINDING_REPORTS.with(|reports| {
        let mut reports = reports.borrow_mut();
        if reports.len() >= UNHANDLED_BINDING_REPORTS_CAP {
            reports.remove(0);
        }
        reports.push(line);
    });
}

/// Drain the recorded matched-but-unhandled binding reports (see
/// [`report_unhandled_binding_action`]). Observability hook for regression
/// tests and tooling; the runtime never reads it back.
#[doc(hidden)]
pub fn take_unhandled_binding_reports() -> Vec<String> {
    UNHANDLED_BINDING_REPORTS.with(|reports| std::mem::take(&mut *reports.borrow_mut()))
}

/// Run a string action through the full Python-faithful dispatch chain and merge
/// the resulting effects into `pass`.
///
/// This is the single shared entry point for *every* string-action source:
/// `[@click=...]` span clicks, `App::run_action(...)`, and the
/// [`ActionDispatchRequested`](crate::message::ActionDispatchRequested) message
/// posted by widgets (links, buttons with `action=`, etc.).
///
/// Resolution mirrors `App.run_action` / `_dispatch_action` in Python:
/// 1. Parse the action string (`namespace.name(args)`).
/// 2. Resolve the target by walking `sender → ancestors → root` against each
///    node's `action_namespace` / `action_registry`
///    ([`crate::action::resolve_action`]).
/// 3. Gate the resolved target with `check_action` (skip on `Some(false)`/`None`).
/// 4. Dispatch to the resolved widget; if nothing resolved, fall back to the app
///    root, then to the app's custom-action hook (`on_app_unhandled_action`),
///    which is how user-defined `action_<name>` methods run.
///
/// `default_namespace` is the node that owns the action when no explicit
/// namespace is given (the clicked widget for `@click`, the message sender for
/// `ActionDispatchRequested`).
///
/// Returns `true` if the action was handled.
fn dispatch_action_string(
    app: &mut App,
    root: &mut dyn Widget,
    action_str: &str,
    default_namespace: NodeId,
    pass: &mut RuntimeMessagePass,
) -> bool {
    let parsed = match crate::action::parse_action(action_str) {
        Ok(parsed) => parsed,
        Err(err) => {
            debug_input(&format!(
                "[runtime] action dispatch ignored invalid action={action_str:?} error={err}"
            ));
            return false;
        }
    };

    // --- Widget-tree resolution (focused/sender → root) ---
    if let Some(tree_mut) = app.active_widget_tree_mut() {
        let resolve_from = if tree_mut.contains(default_namespace) {
            Some(default_namespace)
        } else {
            focused_node_id_tree(tree_mut).or_else(|| tree_mut.root())
        };

        let resolved = {
            let tree_ref = &*tree_mut;
            resolve_from.and_then(|start| {
                crate::action::resolve_action(&parsed, tree_ref, start, |nid| {
                    tree_ref
                        .get(nid)
                        .map(|node| (node.widget.action_namespace(), node.widget.action_registry()))
                })
            })
        };

        if let Some(ra) = resolved
            && let Some(node) = tree_mut.get_mut(ra.node)
        {
            // check_action gating (Python `action_target.check_action`).
            let gate = node.widget.check_action(&parsed.name, &parsed.arguments);
            if gate != Some(true) {
                debug_input(&format!(
                    "[runtime] action gated by check_action action={action_str:?} gate={gate:?}"
                ));
                return false;
            }
            let mut ctx = EventCtx::default();
            let handled =
                execute_action_with_dispatch_target(&mut *node.widget, &parsed, &mut ctx, ra.node);
            let mut outcome = DispatchOutcome {
                handled: handled || ctx.handled(),
                repaint_requested: ctx.repaint_requested(),
                invalidation: ctx.invalidation(),
                stop_requested: ctx.stop_requested(),
                messages: ctx.take_messages(),
                animation_requests: ctx.take_animation_requests(),
                style_animation_requests: ctx.take_style_animation_requests(),
                worker_requests: ctx.take_worker_requests(),
                recompose_nodes: ctx.take_recompose_nodes(),
                default_prevented: false,
                class_ops: ctx.take_class_ops(),
            };
            let handled = outcome.handled;
            merge_outcome_into_runtime_pass(pass, &mut outcome);
            if handled {
                return true;
            }
        }
    }

    // --- App-root dispatch (built-in app actions) ---
    {
        let mut ctx = EventCtx::default();
        let handled =
            execute_action_with_dispatch_target(root, &parsed, &mut ctx, NodeId::default());
        let mut outcome = DispatchOutcome {
            handled: handled || ctx.handled(),
            repaint_requested: ctx.repaint_requested(),
            invalidation: ctx.invalidation(),
            stop_requested: ctx.stop_requested(),
            messages: ctx.take_messages(),
            animation_requests: ctx.take_animation_requests(),
            style_animation_requests: ctx.take_style_animation_requests(),
            worker_requests: ctx.take_worker_requests(),
            recompose_nodes: ctx.take_recompose_nodes(),
            default_prevented: false,
            class_ops: ctx.take_class_ops(),
        };
        let handled = outcome.handled;
        merge_outcome_into_runtime_pass(pass, &mut outcome);
        if handled {
            return true;
        }
    }

    // --- App custom-action fallback (user `action_<name>` methods) ---
    {
        let mut ctx = EventCtx::default();
        {
            let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
            root.on_app_unhandled_action(app, action_str, &mut __wctx);
            __wctx.__enqueue_reactive_if_dirty();
        }
        if ctx.handled() {
            let mut outcome = DispatchOutcome {
                handled: true,
                repaint_requested: ctx.repaint_requested(),
                invalidation: ctx.invalidation(),
                stop_requested: ctx.stop_requested(),
                messages: ctx.take_messages(),
                animation_requests: ctx.take_animation_requests(),
                style_animation_requests: ctx.take_style_animation_requests(),
                worker_requests: ctx.take_worker_requests(),
                recompose_nodes: ctx.take_recompose_nodes(),
                default_prevented: false,
                class_ops: ctx.take_class_ops(),
            };
            merge_outcome_into_runtime_pass(pass, &mut outcome);
            return true;
        }
    }

    debug_input(&format!(
        "[runtime] action dispatch unresolved action={action_str:?}"
    ));
    false
}

fn dispatch_simulated_key_like_input(
    app: &mut App,
    root: &mut dyn Widget,
    key: KeyEventData,
    pass: &mut RuntimeMessagePass,
) {
    // App-level key hook.
    let mut app_key_ctx = EventCtx::default();
    {
        let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut app_key_ctx);
        root.on_app_key(app, &key, &mut __wctx);
        __wctx.__enqueue_reactive_if_dirty();
    }
    pass.repaint_requested |= app_key_ctx.repaint_requested();
    pass.invalidation.merge(app_key_ctx.invalidation());
    pass.stop_requested |= app_key_ctx.stop_requested();
    pass.animation_requests
        .extend(app_key_ctx.take_animation_requests());
    pass.style_animation_requests
        .extend(app_key_ctx.take_style_animation_requests());
    pass.worker_requests
        .extend(app_key_ctx.take_worker_requests());
    pass.recompose_nodes
        .extend(app_key_ctx.take_recompose_nodes());
    pass.class_ops.extend(app_key_ctx.take_class_ops());
    pass.generated.extend(app_key_ctx.take_messages());
    if pass.stop_requested || app_key_ctx.handled() {
        return;
    }

    let bind = crate::event::KeyBind::from_event(&key);
    let mapped_action = app.action_map.lookup(&bind);

    // Priority actions first (e.g. command palette).
    if let Some(action) = mapped_action.filter(|a| is_priority_action(*a)) {
        // Wave 1: ctrl+p opens the composed CommandPaletteScreen via the adapter,
        // NOT via Action::CommandPalette to the legacy host.
        let mut outcome = if matches!(action, Action::CommandPalette) {
            app.dispatch_command_palette_open(root)
        } else {
            app.dispatch_event_auto(root, Event::Action(action))
        };
        let handled = outcome.handled || matches!(action, Action::CommandPalette);
        merge_outcome_into_runtime_pass(pass, &mut outcome);
        if handled {
            return;
        }
    }

    // Declarative bindings before raw key dispatch.
    let mut binding_clashes = Vec::new();
    let binding_match = app.active_widget_tree().and_then(|tree| {
        match_binding_chain(
            tree,
            app.app_root_tree_when_screen_active(),
            &key,
            app.check_action_fn.as_deref(),
            &app.keymap,
            Some(&mut binding_clashes),
        )
    });
    // Deliver keymap clash reports after the tree borrow ends (Python calls
    // handle_bindings_clash per chain build, i.e. per clashing keypress).
    app.deliver_binding_clashes(&binding_clashes);
    if let Some((binding_node_id, action_str, binding_source)) = binding_match
        && let Ok(parsed) = crate::action::parse_action(&action_str)
    {
        // CLUSTER 7: a binding declared on a node whose action is served only
        // by `execute_action` (no `action_registry()` entry) must still run on
        // that source node — the binding source IS the target. Only applies
        // when the binding came from the active tree (the source node lives
        // there); app-root bindings are dispatched on the `root` adapter below.
        if binding_source == BindingSource::Active
            && let Some(tree_mut) = app.active_widget_tree_mut()
        {
            let focused = focused_node_id_tree(tree_mut);
            let resolved = {
                let tree_ref = &*tree_mut;
                focused.and_then(|fid| {
                    crate::action::resolve_action(&parsed, tree_ref, fid, |nid| {
                        tree_ref
                            .get(nid)
                            .map(|n| (n.widget.action_namespace(), n.widget.action_registry()))
                    })
                })
            };
            // Prefer the registry-resolved owner; otherwise fall back to the
            // binding's own source node.
            let target = resolved.map(|ra| ra.node).unwrap_or(binding_node_id);
            if let Some(node) = tree_mut.get_mut(target) {
                let mut ctx = EventCtx::default();
                if execute_action_with_dispatch_target(
                    &mut *node.widget,
                    &parsed,
                    &mut ctx,
                    target,
                ) || ctx.handled()
                {
                    pass.repaint_requested |= ctx.repaint_requested();
                    pass.invalidation.merge(ctx.invalidation());
                    pass.stop_requested |= ctx.stop_requested();
                    pass.animation_requests
                        .extend(ctx.take_animation_requests());
                    pass.style_animation_requests
                        .extend(ctx.take_style_animation_requests());
                    pass.worker_requests.extend(ctx.take_worker_requests());
                    pass.recompose_nodes.extend(ctx.take_recompose_nodes());
                    pass.generated.extend(ctx.take_messages());
                    return;
                }
            }
        }

        let mut root_ctx = EventCtx::default();
        if execute_action_with_dispatch_target(root, &parsed, &mut root_ctx, NodeId::default())
            || root_ctx.handled()
        {
            pass.repaint_requested |= root_ctx.repaint_requested();
            pass.invalidation.merge(root_ctx.invalidation());
            pass.stop_requested |= root_ctx.stop_requested();
            pass.animation_requests
                .extend(root_ctx.take_animation_requests());
            pass.style_animation_requests
                .extend(root_ctx.take_style_animation_requests());
            pass.worker_requests.extend(root_ctx.take_worker_requests());
            pass.recompose_nodes.extend(root_ctx.take_recompose_nodes());
            pass.generated.extend(root_ctx.take_messages());
            return;
        }

        // Fallback: app-defined custom action (e.g. "add", "clear").
        // Called when no action_registry handler exists and execute_action declined.
        {
            let mut fallback_ctx = EventCtx::default();
            {
                let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut fallback_ctx);
                root.on_app_unhandled_action(app, &action_str, &mut __wctx);
                __wctx.__enqueue_reactive_if_dirty();
            }
            if fallback_ctx.handled() {
                pass.repaint_requested |= fallback_ctx.repaint_requested();
                pass.invalidation.merge(fallback_ctx.invalidation());
                pass.stop_requested |= fallback_ctx.stop_requested();
                pass.animation_requests
                    .extend(fallback_ctx.take_animation_requests());
                pass.style_animation_requests
                    .extend(fallback_ctx.take_style_animation_requests());
                pass.worker_requests
                    .extend(fallback_ctx.take_worker_requests());
                pass.recompose_nodes
                    .extend(fallback_ctx.take_recompose_nodes());
                pass.generated.extend(fallback_ctx.take_messages());
                return;
            }
        }

        // The binding matched but no layer handled its action: report the
        // silent no-op (debug channel + test-observable buffer) before the key
        // falls through to raw dispatch.
        report_unhandled_binding_action(binding_node_id, &action_str);
    }

    // Raw key dispatch.
    let mut key_outcome = app.dispatch_event_auto(root, Event::Key(key.clone()));
    let key_handled = key_outcome.handled;
    merge_outcome_into_runtime_pass(pass, &mut key_outcome);
    if key_handled {
        return;
    }

    // Fallback action-map behavior.
    if let Some(action) = mapped_action.filter(|a| !is_priority_action(*a)) {
        if action == Action::CopySelectedText {
            if let Some(text) = app.action_copy_selected_text() {
                let sender = App::runtime_message_sender();
                pass.generated.push(
                    MessageEvent::new(
                        sender,
                        crate::message::TextEditClipboardCopyRequested { text, cut: false },
                    )
                    .with_control(sender),
                );
            } else {
                app.notify_help_quit();
                pass.repaint_requested = true;
            }
            return;
        }
        if action == Action::HelpQuit {
            app.notify_help_quit();
            pass.repaint_requested = true;
            return;
        }
        if matches!(action, Action::FocusNext | Action::FocusPrev) {
            let mut focus_outcome = app.dispatch_event_auto(root, Event::Action(action));
            let focus_handled = focus_outcome.handled;
            merge_outcome_into_runtime_pass(pass, &mut focus_outcome);
            if focus_handled {
                return;
            }
            if app.move_focus_auto(action) {
                pass.repaint_requested = true;
                return;
            }
        }
        let mut outcome = if is_scroll_action(action) {
            app.dispatch_scroll_action_auto(root, action, app.hovered)
        } else {
            app.dispatch_event_auto(root, Event::Action(action))
        };
        merge_outcome_into_runtime_pass(pass, &mut outcome);
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
            MessageEvent::new(
                sender,
                crate::message::WorkerStateChanged {
                    worker_id: change.worker_id,
                    state: change.state,
                },
            )
            .with_control(sender)
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
        if let Some(m) = event.downcast_ref::<crate::message::TextEditClipboardCopyRequested>() {
            *clipboard = Some(m.text.clone());
            if !backend.copy(&m.text) {
                debug_input("[clipboard] system copy unavailable; runtime fallback updated");
            }
        } else if let Some(m) =
            event.downcast_ref::<crate::message::TextEditClipboardPasteRequested>()
        {
            let target = m.target;
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
                generated.push(App::clipboard_message_event(target, text));
            }
        }
    }
    generated
}

/// Result of [`App::drain_tree_lifecycle_events`]: whether any Mount/Unmount
/// event was drained (progress for the headless pump's idle check) and whether
/// a lifecycle handler requested app exit (the live loop breaks on it).
#[derive(Default)]
struct LifecycleDrainOutcome {
    progressed: bool,
    stop_requested: bool,
}

#[derive(Default)]
struct RuntimeMessagePass {
    deliver: Vec<MessageEvent>,
    generated: Vec<MessageEvent>,
    repaint_requested: bool,
    invalidation: crate::event::InvalidationFlags,
    animation_requests: Vec<AnimationRequest>,
    style_animation_requests: Vec<crate::event::StyleAnimationRequest>,
    worker_requests: Vec<WorkerRequest>,
    recompose_nodes: Vec<NodeId>,
    stop_requested: bool,
    class_ops: Vec<(crate::node_id::NodeId, ClassOp)>,
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

fn sync_widget_controlled_child_display_tree(
    tree: &mut crate::widget_tree::WidgetTree,
    root_widget: &dyn Widget,
) -> bool {
    let Some(root) = tree.root() else {
        return false;
    };

    let mut updates: Vec<(NodeId, bool)> = Vec::new();
    // Per-child class overrides driven by the parent's state (e.g. ListView's
    // `-highlight` / `-hovered`). Collected alongside display so the same sync
    // pass mirrors both onto the child node records.
    let mut class_updates: Vec<(NodeId, &'static str, bool)> = Vec::new();
    for (idx, child_id) in tree.children(root).iter().copied().enumerate() {
        if let Some(display) = root_widget.child_display_for_tree(idx) {
            updates.push((child_id, display));
        }
        for (class, on) in root_widget.child_classes_for_tree(idx) {
            class_updates.push((child_id, class, on));
        }
    }
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
            for (class, on) in parent.widget.child_classes_for_tree(idx) {
                class_updates.push((child_id, class, on));
            }
        }
    }

    let mut changed = false;
    for (node_id, display) in updates {
        let before = tree.is_displayed(node_id);
        tree.set_runtime_display(node_id, display);
        if !display {
            tree.set_focus_state(node_id, false);
        }
        if before != tree.is_displayed(node_id) {
            changed = true;
        }
    }
    for (node_id, class, on) in class_updates {
        let before = tree.has_class(node_id, class);
        if on && !before {
            tree.add_class(node_id, class);
            changed = true;
        } else if !on && before {
            tree.remove_class(node_id, class);
            changed = true;
        }
    }
    changed
}

/// Rebuild a node's arena subtree from its `compose()` (RA2.1).
///
/// # Recompose invariant (RA2.1 trap 1)
///
/// This is only ever driven for a node that requested recompose via
/// `ctx.request_recompose()` — i.e. a generative, state-pure widget (typically
/// `#[reactive(recompose)]`) whose `compose()` rebuilds its whole subtree from
/// current state on every call. Plain containers MUST NOT self-request
/// recompose: their children were drained once at initial mount and now live in
/// the arena, so re-draining yields nothing and `remove_children` below would
/// silently delete them. The debug diagnostic here catches exactly that vanish
/// bug (recompose produced no children while the node still had some).
pub(crate) fn recompose_node_subtree(tree: &mut crate::widget_tree::WidgetTree, node_id: NodeId) {
    let Some(node) = tree.get_mut(node_id) else {
        return;
    };
    let declarations = node.widget.compose();

    // Vanish diagnostic (trap 1): a recompose that produces NO children for a
    // node that still HAS arena children means either a container was wrongly
    // asked to self-recompose (its drained `compose()` is empty) or a generative
    // widget's `compose()` is not state-pure. This is almost always a bug — the
    // children are about to be removed and never re-mounted.
    if declarations.is_empty() && !tree.children(node_id).is_empty() {
        crate::debug::debug_render(&format!(
            "recompose-vanish: node {node_id:?} recompose produced no children \
             while it still had {} arena child(ren); a node that requests \
             recompose must have a generative (state-pure) compose — plain \
             containers must never self-request recompose",
            tree.children(node_id).len()
        ));
    }

    tree.remove_children(node_id);
    App::mount_declarations(tree, node_id, declarations);
}

fn split_runtime_control_messages(
    app: &mut App,
    root: &mut dyn Widget,
    queue: Vec<MessageEvent>,
) -> RuntimeMessagePass {
    let mut pass = RuntimeMessagePass::default();
    for event in queue {
        if let Some(m) = event.downcast_ref::<crate::message::AsyncTaskSpawn>() {
            let m = m.clone();
            if let Some(cancelled) = app.async_tasks.spawn(m.task_id, m.target, m.request) {
                pass.generated.push(cancelled);
            }
            continue;
        }
        if let Some(m) = event.downcast_ref::<crate::message::AsyncTaskCancel>() {
            let task_id = m.task_id;
            if let Some(cancelled) = app.async_tasks.cancel(task_id) {
                pass.generated.push(cancelled);
            }
            continue;
        }
        if let Some(m) = event.downcast_ref::<crate::message::AsyncTaskCancelTarget>() {
            let target = m.target;
            pass.generated
                .extend(app.async_tasks.cancel_for_target(target));
            continue;
        }
        if let Some(m) = event.downcast_ref::<crate::message::TimerSchedule>() {
            let m = m.clone();
            if let Some(cancelled) = app.timers.schedule(m.timer_id, m.target, m.delay) {
                pass.generated.push(cancelled);
            }
            continue;
        }
        if let Some(m) = event.downcast_ref::<crate::message::TimerCancel>() {
            let timer_id = m.timer_id;
            if let Some(cancelled) = app.timers.cancel(timer_id) {
                pass.generated.push(cancelled);
            }
            continue;
        }
        if let Some(m) = event.downcast_ref::<crate::message::OverlayVisibilityChanged>() {
            let overlay = m.overlay;
            let visible = m.visible;
            if let Some(tree) = app.active_widget_tree_mut()
                && set_overlay_modal_display_tree(tree, overlay, visible)
            {
                pass.repaint_requested = true;
                pass.invalidation
                    .merge(crate::event::InvalidationFlags::layout());
            }
            pass.deliver.push(event);
            continue;
        }
        if let Some(m) = event.downcast_ref::<crate::message::NotificationExpired>() {
            // An auto-dismiss timer elapsed (or a toast was clicked): drop the
            // notification from the store. The event loop re-syncs the rack on the
            // next iteration, unmounting the toast node. Consumed here (not
            // delivered onward).
            app.remove_notification(m.id);
            pass.repaint_requested = true;
            continue;
        }
        if let Some(m) = event.downcast_ref::<crate::message::AppAddClass>() {
            let selector = m.selector.clone();
            let class_name = m.class_name.clone();
            match app.action_add_class(&selector, &class_name) {
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
            }
        } else if let Some(m) = event.downcast_ref::<crate::message::AppRemoveClass>() {
            let selector = m.selector.clone();
            let class_name = m.class_name.clone();
            match app.action_remove_class(&selector, &class_name) {
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
            }
        } else if let Some(m) = event.downcast_ref::<crate::message::AppToggleClass>() {
            let selector = m.selector.clone();
            let class_name = m.class_name.clone();
            match app.action_toggle_class(&selector, &class_name) {
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
            }
        } else if let Some(m) = event.downcast_ref::<crate::message::AppSetDisabled>() {
            let selector = m.selector.clone();
            let disabled = m.disabled;
            match app.query_mut(&selector) {
                Ok(query) => {
                    let matched = query.len();
                    query.set(None, None, Some(disabled), None);
                    if matched > 0 {
                        pass.repaint_requested = true;
                        pass.invalidation
                            .merge(crate::event::InvalidationFlags::layout());
                    }
                }
                Err(err) => {
                    debug_input(&format!(
                        "[runtime] app.set_disabled ignored selector={selector:?} disabled={disabled:?} err={err:?}"
                    ));
                }
            }
        } else if event.is::<crate::message::AppBack>() {
            if app.action_back() {
                pass.repaint_requested = true;
                pass.invalidation
                    .merge(crate::event::InvalidationFlags::layout());
            }
        } else if event.is::<crate::message::AppBell>() {
            let _ = app.action_bell();
        } else if event.is::<crate::message::AppChangeTheme>() {
            if app.action_change_theme() {
                pass.repaint_requested = true;
            }
        } else if event.is::<crate::message::AppCycleTheme>() {
            if app.action_cycle_theme() {
                pass.repaint_requested = true;
                pass.invalidation
                    .merge(crate::event::InvalidationFlags::layout());
            }
        } else if let Some(m) = event.downcast_ref::<crate::message::AppSetTheme>() {
            let name = m.name.clone();
            if app.set_theme_by_name(&name) {
                pass.repaint_requested = true;
                pass.invalidation
                    .merge(crate::event::InvalidationFlags::layout());
            } else {
                debug_input(&format!("[runtime] app.set_theme unknown theme={name:?}"));
            }
        } else if event.is::<crate::message::AppCommandPalette>() {
            // Wave 1: deliver `AppCommandPalette` to the adapter's `on_app_message`
            // (which owns the providers + `&mut App`) so it can push the composed
            // `CommandPaletteScreen`. This SEAM gates the legacy always-mounted
            // `CommandPalette` host: it no longer receives `Action::CommandPalette`,
            // so it stays inert. Wave 2 deletes the old host + this delivery hop.
            pass.deliver.push(event);
        } else if let Some(m) = event.downcast_ref::<crate::message::AppFocus>() {
            let widget_id = m.widget_id.clone();
            match app.action_focus(&widget_id) {
                Ok(true) => {
                    // A focus landed — that is the newest intent, so drop any
                    // earlier deferred request (last-writer wins).
                    app.pending_focus = None;
                    pass.repaint_requested = true;
                }
                Ok(false) => {
                    // Not focused. If the target EXISTS (it just isn't displayed
                    // yet — a sibling handler flipped its `display` via a deferred
                    // class op the post-dispatch flush + layout applies AFTER this
                    // message routes), defer the request so it lands once the
                    // same-frame display resolution runs (`retry_pending_focus`).
                    // A genuine no-match is dropped (never displays → never
                    // deferred), so a target that can never show cannot spin.
                    if app.query_one(&format!("#{widget_id}")).is_ok() {
                        app.pending_focus = Some(widget_id);
                    }
                }
                Err(err) => {
                    debug_input(&format!(
                        "[runtime] app.focus ignored widget_id={widget_id:?} err={err:?}"
                    ));
                }
            }
        } else if event.is::<crate::message::AppFocusNext>() {
            // An explicit tab is a newer focus intent — drop any deferred request.
            app.pending_focus = None;
            if app.action_focus_next() {
                pass.repaint_requested = true;
            }
        } else if event.is::<crate::message::AppFocusPrevious>() {
            app.pending_focus = None;
            if app.action_focus_previous() {
                pass.repaint_requested = true;
            }
        } else if event.is::<crate::message::AppHelpQuit>() {
            app.action_help_quit();
            pass.repaint_requested = true;
        } else if event.is::<crate::message::AppCopySelectedText>() {
            if let Some(text) = app.action_copy_selected_text() {
                let sender = App::runtime_message_sender();
                pass.generated.push(
                    MessageEvent::new(
                        sender,
                        crate::message::TextEditClipboardCopyRequested { text, cut: false },
                    )
                    .with_control(sender),
                );
            } else {
                app.notify_help_quit();
                pass.repaint_requested = true;
            }
        } else if event.is::<crate::message::AppHideHelpPanel>() {
            match app.action_hide_help_panel() {
                Ok(changed) => {
                    if changed {
                        pass.repaint_requested = true;
                        pass.invalidation
                            .merge(crate::event::InvalidationFlags::layout());
                    }
                }
                Err(err) => {
                    debug_input(&format!(
                        "[runtime] app.hide_help_panel ignored err={err:?}"
                    ));
                }
            }
            // Keep lifecycle/control visibility messages observable by
            // widgets (e.g. CommandPalette/TextualAppAdapter) after runtime
            // applies the state change.
            pass.deliver.push(event);
        } else if let Some(m) = event.downcast_ref::<crate::message::AppNotify>() {
            let message = m.message.clone();
            let title = m.title.clone();
            let severity = m.severity.clone();
            app.action_notify(&message, &title, &severity);
            pass.repaint_requested = true;
        } else if event.is::<crate::message::AppPopScreen>() {
            if app.action_pop_screen() {
                pass.repaint_requested = true;
                pass.invalidation
                    .merge(crate::event::InvalidationFlags::layout());
            }
        } else if let Some(m) = event.downcast_ref::<crate::message::AppPushScreen>() {
            let screen = m.screen.clone();
            if app.action_push_screen(&screen) {
                pass.repaint_requested = true;
                pass.invalidation
                    .merge(crate::event::InvalidationFlags::layout());
            } else {
                debug_input(&format!(
                    "[runtime] app.push_screen ignored missing screen={screen:?}"
                ));
            }
        } else if let Some(m) = event.downcast_ref::<crate::message::AppScreenshot>() {
            let filename = m.filename.clone();
            let path = m.path.clone();
            pass.repaint_requested |= app.action_screenshot(filename.as_deref(), path.as_deref());
        } else if event.is::<crate::message::AppShowHelpPanel>() {
            match app.action_show_help_panel() {
                Ok(changed) => {
                    if changed {
                        pass.repaint_requested = true;
                        pass.invalidation
                            .merge(crate::event::InvalidationFlags::layout());
                    }
                }
                Err(err) => {
                    debug_input(&format!(
                        "[runtime] app.show_help_panel ignored err={err:?}"
                    ));
                }
            }
            // Keep lifecycle/control visibility messages observable by
            // widgets (e.g. CommandPalette/TextualAppAdapter) after runtime
            // applies the state change.
            pass.deliver.push(event);
        } else if let Some(m) = event.downcast_ref::<crate::message::AppSimulateKey>() {
            let key = m.key.clone();
            if let Some(synthetic) = parse_simulated_key(&key) {
                dispatch_simulated_key_like_input(app, root, synthetic, &mut pass);
            } else {
                debug_input(&format!(
                    "[runtime] app.simulate_key ignored invalid key spec {:?}",
                    key
                ));
            }
        } else if event.is::<crate::message::AppSuspendProcess>() {
            pass.repaint_requested |= app.action_suspend_process();
        } else if let Some(m) = event.downcast_ref::<crate::message::AppSwitchMode>() {
            let mode = m.mode.clone();
            if app.switch_mode(&mode) {
                pass.repaint_requested = true;
                pass.invalidation
                    .merge(crate::event::InvalidationFlags::layout());
            } else {
                debug_input(&format!("[runtime] app.switch_mode ignored mode={mode:?}"));
            }
        } else if let Some(m) = event.downcast_ref::<crate::message::AppSwitchScreen>() {
            let screen = m.screen.clone();
            if app.action_switch_screen(&screen) {
                pass.repaint_requested = true;
                pass.invalidation
                    .merge(crate::event::InvalidationFlags::layout());
            } else {
                debug_input(&format!(
                    "[runtime] app.switch_screen ignored screen={screen:?}"
                ));
            }
        } else if event.is::<crate::message::AppToggleDark>() {
            if app.action_toggle_dark() {
                // Toggling dark switches the active *registered* theme, which
                // swaps the design-token map. Token-styled surfaces (Header /
                // Footer / Screen) bake their blank surface from a seed style
                // resolved at build/layout time, so a bare repaint keeps the
                // stale colours. Request the same style/layout invalidation as
                // `AppSetTheme`/`AppCycleTheme` so every widget re-seeds against
                // the new tokens and the frame actually recolours (Python's
                // `_watch_theme` -> `_invalidate_css` + `refresh_css`).
                pass.repaint_requested = true;
                pass.invalidation
                    .merge(crate::event::InvalidationFlags::layout());
            }
        } else if let Some(m) = event.downcast_ref::<crate::message::ActionDispatchRequested>() {
            let action = m.action.clone();
            dispatch_action_string(app, root, &action, event.sender, &mut pass);
        } else {
            pass.deliver.push(event);
        }
    }
    pass.generated.extend(app.drain_ready_timers());
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
    let is_screen = widget.style_type() == "Screen"
        || widget
            .style_type_aliases().contains(&"Screen");
    SelectorSnapshot {
        type_name: widget.style_type().to_string(),
        // Step 6: identity/state now lives on the node record; this off-tree
        // path (root-only fallback when no arena tree exists) has no node record,
        // so use safe defaults.
        style_id: None,
        classes: Vec::new(),
        disabled: false,
        focused: is_screen && app_active,
        hovered: false,
        active: widget.is_active(),
        inline: app_pseudos.inline,
        ansi: app_pseudos.ansi,
        nocolor: app_pseudos.nocolor,
    }
}

/// Node-record-based variant of [`snapshot_for`] for tree-mode paths.
///
/// Reads css_id, classes, and interaction state exclusively from the
/// `WidgetNode` record (Step 6: legacy widget getters deleted).
fn snapshot_for_node(
    node: &crate::widget_tree::WidgetNode,
    _node_id: NodeId,
    app_active: bool,
    app_pseudos: AppRuntimePseudos,
) -> SelectorSnapshot {
    let widget = node.widget.as_ref();
    let is_screen = widget.style_type() == "Screen"
        || widget
            .style_type_aliases().contains(&"Screen");
    let style_id = node.css_id.clone();
    let classes: Vec<String> = node.classes.iter().cloned().collect();
    SelectorSnapshot {
        type_name: widget.style_type().to_string(),
        style_id,
        classes,
        disabled: node.state.disabled,
        focused: (node.state.focused || is_screen) && app_active,
        hovered: node.state.hovered,
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
    if !selector.classes().is_empty()
        && !selector
            .classes()
            .iter()
            .all(|class| meta.classes.iter().any(|value| value == class))
        {
            return false;
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
        let current = snapshot_for_node(node, node_id, app_active, app_pseudos);
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

/// Map semantic CSS property names to internal property names.
///
/// Python Textual uses `color`/`background` for transition targets; Rust uses
/// `fg`/`bg` internally.  This mapping lets CSS authored with Python names
/// (`transition: color 300ms`) work correctly in the Rust runtime.
fn semantic_transition_alias(property: &str) -> Option<&'static str> {
    match property {
        "color" | "foreground" => Some("fg"),
        "background" => Some("bg"),
        "background_tint" | "background-tint" => Some("background_tint"),
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
        if let Some(found) = resolve_transition_for_property(style, &dashed) {
            return Some(found);
        }
    }
    // Try semantic aliases (e.g. "color" → "fg", "background" → "bg") so that
    // CSS authored with Python Textual property names resolves correctly.
    if let Some(alias) = semantic_transition_alias(&canonical) {
        return resolve_transition_for_property(style, alias);
    }
    None
}

/// Returns `(numeric_requests, style_requests)` for all animatable property changes.
///
/// - Numeric/float properties (`opacity`, `text_opacity`, `offset_x`, `offset_y`) produce
///   `AnimationRequest` entries that are dispatched as `Event::AnimationValue` to widgets.
/// - Style-value properties (color, scalar, spacing, tint) produce `StyleAnimationRequest`
///   entries that are applied directly to widget inline styles via `step_style()`.
fn transition_requests_for_style_change(
    target: NodeId,
    previous: &crate::style::Style,
    current: &crate::style::Style,
) -> (Vec<AnimationRequest>, Vec<StyleAnimationRequest>) {
    if previous == current {
        return (Vec::new(), Vec::new());
    }

    // Float/scalar properties dispatched as Event::AnimationValue (existing path).
    const NUMERIC_ANIMATABLE: [&str; 4] = ["opacity", "text_opacity", "offset_x", "offset_y"];
    let numeric: Vec<AnimationRequest> = NUMERIC_ANIMATABLE
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
        .collect();

    // StyleValue properties applied directly to widget inline styles.
    const STYLE_ANIMATABLE: [&str; 12] = [
        "fg",
        "bg",
        "width",
        "height",
        "min_width",
        "max_width",
        "min_height",
        "max_height",
        "margin",
        "padding",
        "tint",
        "background_tint",
    ];
    let style: Vec<StyleAnimationRequest> = STYLE_ANIMATABLE
        .iter()
        .filter_map(|property| {
            let from = style_property_as_style_value(previous, property)?;
            let to = style_property_as_style_value(current, property)?;
            if from == to {
                return None;
            }
            let (duration, delay, ease) =
                resolve_transition_for_property_aliases(current, property)?;
            Some(
                StyleAnimationRequest::new(target, *property, from, to, duration)
                    .with_delay(delay)
                    .with_ease(ease)
                    .with_level(crate::event::AnimationLevel::Full),
            )
        })
        .collect();

    (numeric, style)
}

/// Extract the `StyleValue` for a style-animatable property from a `Style`.
/// Returns `None` if the property is not set in the style.
fn style_property_as_style_value(
    style: &crate::style::Style,
    property: &str,
) -> Option<StyleValue> {
    match property {
        "fg" => Some(StyleValue::Color(style.fg?)),
        "bg" => Some(StyleValue::Color(style.bg?)),
        "width" => Some(StyleValue::Scalar(style.width?)),
        "height" => Some(StyleValue::Scalar(style.height?)),
        "min_width" => Some(StyleValue::Scalar(style.min_width?)),
        "max_width" => Some(StyleValue::Scalar(style.max_width?)),
        "min_height" => Some(StyleValue::Scalar(style.min_height?)),
        "max_height" => Some(StyleValue::Scalar(style.max_height?)),
        "margin" => Some(StyleValue::Spacing(*style.margin.as_ref()?)),
        "padding" => Some(StyleValue::Spacing(*style.padding.as_ref()?)),
        "tint" => Some(StyleValue::Tint(*style.tint.as_ref()?)),
        "background_tint" => Some(StyleValue::Tint(*style.background_tint.as_ref()?)),
        _ => None,
    }
}

/// Apply a `StyleValue` animation result to the corresponding field of a `Style`.
fn apply_style_value_to_property(
    style: &mut crate::style::Style,
    property: &str,
    value: &StyleValue,
) {
    match (property, value) {
        ("fg", StyleValue::Color(c)) => style.fg = Some(*c),
        ("bg", StyleValue::Color(c)) => style.bg = Some(*c),
        ("width", StyleValue::Scalar(s)) => style.width = Some(*s),
        ("height", StyleValue::Scalar(s)) => style.height = Some(*s),
        ("min_width", StyleValue::Scalar(s)) => style.min_width = Some(*s),
        ("max_width", StyleValue::Scalar(s)) => style.max_width = Some(*s),
        ("min_height", StyleValue::Scalar(s)) => style.min_height = Some(*s),
        ("max_height", StyleValue::Scalar(s)) => style.max_height = Some(*s),
        ("margin", StyleValue::Spacing(sp)) => style.margin = Some(*sp),
        ("padding", StyleValue::Spacing(sp)) => style.padding = Some(*sp),
        ("tint", StyleValue::Tint(t)) => style.tint = Some(*t),
        ("background_tint", StyleValue::Tint(t)) => style.background_tint = Some(*t),
        // `opacity`/`text_opacity` animate as `StyleValue::Float` on a 0.0–100.0
        // scale (mirroring Python's `Styles.opacity` 0–1 fraction expressed as a
        // percent here). Round to the `u8` percent the resolved style and the
        // render-time opacity compositor consume. Without this arm the animator's
        // per-tick computed opacity would be silently dropped and the rendered
        // frame would never change.
        ("opacity", StyleValue::Float(v)) => {
            style.opacity = Some(v.round().clamp(0.0, 100.0) as u8);
        }
        ("text_opacity", StyleValue::Float(v)) => {
            style.text_opacity = Some(v.round().clamp(0.0, 100.0) as u8);
        }
        _ => {}
    }
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

/// Deliver a frame tick to every active arena widget of `tree` (and any cover
/// widget, the `loading` overlay). Shared by the live loop's active-tree
/// tick, the headless `advance_ticks` analogue, and the opt-in
/// inactive-screen tick path.
fn deliver_frame_tick(tree: &mut crate::widget_tree::WidgetTree, tick: u64) {
    let Some(tree_root) = tree.root() else {
        return;
    };
    for node in tree.walk_depth_first(tree_root) {
        if let Some(widget_node) = tree.get_mut(node) {
            if widget_node.widget.is_active() {
                widget_node.widget.on_tick(tick);
            }
            if let Some(cover) = widget_node.cover_widget.as_mut()
                && cover.is_active()
            {
                cover.on_tick(tick);
            }
        }
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
pub(crate) enum InvalidationScope {
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
        run_copy_command("wl-copy", &[], text)
            || run_copy_command("xclip", &["-selection", "clipboard"], text)
            || run_copy_command("xsel", &["--clipboard", "--input"], text)
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
        run_paste_command("wl-paste", &["-n"])
            .or_else(|| run_paste_command("xclip", &["-selection", "clipboard", "-o"]))
            .or_else(|| run_paste_command("xsel", &["--clipboard", "--output"]))
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
    /// The app-root widget tree, but only when a *separate* screen/mode tree is
    /// active on top of it. Returns `None` when the app-root tree is itself the
    /// active tree (no screen pushed), so callers don't double-walk it.
    ///
    /// Used by binding resolution so `App::BINDINGS` stay in the chain beneath
    /// an active screen (Python `App._check_bindings` always appends the App's
    /// own bindings after the screen chain).
    pub(crate) fn app_root_tree_when_screen_active(&self) -> Option<&crate::widget_tree::WidgetTree> {
        if self.screen_stack.top().is_some() {
            self.widget_tree.as_ref()
        } else {
            None
        }
    }

    fn apply_app_blur_focus_state(&mut self) {
        self.app_active = false;
        let focused = self.active_widget_tree().and_then(focused_node_id_tree);
        self.last_focused_on_app_blur = focused;
        if let Some(focused_id) = focused
            && let Some(tree) = self.active_widget_tree_mut()
        {
            tree.set_focus_state(focused_id, false);
        }
    }

    fn apply_app_focus_restore_state(&mut self) {
        self.app_active = true;
        if let Some(focused_id) = self.last_focused_on_app_blur.take()
            && let Some(tree) = self.active_widget_tree_mut()
            && focused_node_id_tree(tree).is_none()
            && tree.contains(focused_id)
            && tree.is_displayed(focused_id)
        {
            tree.set_focus_state(focused_id, true);
        }
    }

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
                    if let Some(tree) = self.active_widget_tree_mut() {
                        tree.set_focus_state(id, true);
                    }
                    pending_invalidation.request_full_content();
                }
                DevtoolsCommand::SetDebugLayout(enabled) => {
                    self.enable_debug_layout(enabled);
                    pending_invalidation.request_full_content();
                }
                DevtoolsCommand::ToggleDisplay(id) => {
                    if let Some(tree) = self.active_widget_tree_mut() {
                        let current = tree.is_displayed(id);
                        tree.set_runtime_display(id, !current);
                    }
                    pending_invalidation.request_full_content();
                }
                DevtoolsCommand::Highlight(id) => {
                    if let Some(tree) = self.active_widget_tree_mut() {
                        tree.add_class(id, "-devtools-highlight");
                    }
                    pending_invalidation.request_full_content();
                    // Schedule removal after ~500ms via a pending highlight clear.
                    self.pending_highlight_clear = Some((
                        id,
                        std::time::Instant::now() + std::time::Duration::from_millis(500),
                    ));
                }
                DevtoolsCommand::AddClass(id, class) => {
                    if let Some(tree) = self.active_widget_tree_mut() {
                        tree.add_class(id, &class);
                    }
                    pending_invalidation.request_full_content();
                }
                DevtoolsCommand::RemoveClass(id, class) => {
                    if let Some(tree) = self.active_widget_tree_mut() {
                        tree.remove_class(id, &class);
                    }
                    pending_invalidation.request_full_content();
                }
                DevtoolsCommand::Quit => {
                    return true;
                }
            }
        }
        // Check pending highlight clear.
        if let Some((id, clear_at)) = self.pending_highlight_clear {
            if std::time::Instant::now() >= clear_at {
                if let Some(tree) = self.active_widget_tree_mut() {
                    tree.remove_class(id, "-devtools-highlight");
                }
                self.pending_highlight_clear = None;
                pending_invalidation.request_full_content();
            }
        }
        false
    }

    /// Deliver the frame tick to background (inactive) trees when
    /// [`App::set_tick_inactive_screens`] is enabled: the app-root tree under
    /// a pushed screen stack plus every stacked screen below the top. No-op
    /// while the screen stack is empty (the app-root tree IS the active tree)
    /// or when the opt-in is off.
    fn deliver_background_screen_ticks(&mut self, tick: u64) {
        if !self.tick_inactive_screens || self.screen_stack.is_empty() {
            return;
        }
        if let Some(tree) = self.widget_tree.as_mut() {
            deliver_frame_tick(tree, tick);
        }
        let top = self.screen_stack.len() - 1;
        for index in 0..top {
            if let Some(entry) = self.screen_stack.get_mut(index) {
                deliver_frame_tick(&mut entry.widget_tree, tick);
            }
        }
    }

    fn publish_devtools_snapshot(&mut self, root: &mut dyn Widget) {
        let Some(devtools) = &self.devtools else {
            return;
        };

        let mut widget_lines = Vec::new();
        let mut focused = None;

        // Tree-based: walk the arena tree depth-first.
        if let Some(tree) = self.active_widget_tree() {
            if let Some(root_id) = tree.root() {
                let walk = tree.walk_depth_first(root_id);
                for node_id in walk {
                    let Some(node) = tree.get(node_id) else {
                        continue;
                    };
                    let depth = tree.ancestors(node_id).len();
                    let widget = node.widget.as_ref();
                    // Step 6: read focus, id, classes, disabled from node record only.
                    let is_focused = node.state.focused && self.app_active;

                    // Layout rect from hit-test map.
                    let layout_rect = self.hit_test.rect(node_id);
                    let layout_rect_field = if let Some(r) = layout_rect {
                        format!("{},{},{},{}", r.x0, r.y0, r.x1, r.y1)
                    } else {
                        "-".to_string()
                    };
                    // Content rect from tree node.
                    let cr = &node.content_rect;
                    let content_rect_field = if cr.x0 == 0 && cr.y0 == 0 && cr.x1 == 0 && cr.y1 == 0
                    {
                        "-".to_string()
                    } else {
                        format!("{},{},{},{}", cr.x0, cr.y0, cr.x1, cr.y1)
                    };

                    let style_id = node
                        .css_id
                        .as_deref()
                        .map(sanitize_snapshot_field)
                        .unwrap_or_else(|| "-".to_string());

                    let classes_field = node
                        .classes
                        .iter()
                        .map(|c| sanitize_snapshot_field(c))
                        .collect::<Vec<_>>()
                        .join(",");

                    // Parent / children IDs.
                    let parent_field = node
                        .parent
                        .map(|p| node_id_to_ffi(p).to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let children_field = if node.children.is_empty() {
                        "-".to_string()
                    } else {
                        node.children
                            .iter()
                            .map(|c| node_id_to_ffi(*c).to_string())
                            .collect::<Vec<_>>()
                            .join(",")
                    };

                    // Visibility.
                    let visibility_field = match node.visibility {
                        crate::style::Visibility::Visible => "visible",
                        crate::style::Visibility::Hidden => "hidden",
                    };

                    let line = format!(
                        "widget\t{depth}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                        node_id_to_ffi(node_id),
                        sanitize_snapshot_field(widget.style_type()),
                        style_id,
                        classes_field,
                        bool_flag(is_focused),
                        bool_flag(self.hovered == Some(node_id)),
                        bool_flag(widget.is_active()),
                        bool_flag(node.state.disabled),
                        layout_rect_field,
                        content_rect_field,
                        bool_flag(node.display),
                        visibility_field,
                        bool_flag(node.css_display),
                        bool_flag(node.runtime_display),
                        bool_flag(node.mounted),
                        parent_field,
                        children_field,
                    );
                    widget_lines.push(line);
                    if is_focused {
                        focused = Some(node_id);
                    }
                }
            }
        } else {
            // Root-only fallback: just the root widget (limited info).
            let widget = root as &dyn Widget;
            // Step 6: no node record for off-tree root; identity/state defaults to empty/false.
            let is_focused = false;
            let rect = self.hit_test.rect(NodeId::default());
            let rect_field = if let Some(r) = rect {
                format!("{},{},{},{}", r.x0, r.y0, r.x1, r.y1)
            } else {
                "-".to_string()
            };
            let line = format!(
                "widget\t0\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t-\t1\tvisible\t1\t1\t1\t-\t-",
                node_id_to_ffi(NodeId::default()),
                sanitize_snapshot_field(widget.style_type()),
                "-",
                "",
                bool_flag(is_focused),
                bool_flag(self.hovered == Some(NodeId::default())),
                bool_flag(widget.is_active()),
                bool_flag(false),
                rect_field,
            );
            widget_lines.push(line);
            if is_focused {
                focused = Some(NodeId::default());
            }
        }

        let mut snapshot = String::new();
        snapshot.push_str("version\t2\n");
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
        // Emit resolved CSS style lines from the snapshot cache.
        for (node_id, style) in &self.style_snapshot_cache {
            let ffi_id = node_id_to_ffi(*node_id);
            for (prop, value) in style.debug_properties() {
                snapshot.push_str(&format!(
                    "style\t{ffi_id}\t{}\t{}\n",
                    sanitize_snapshot_field(prop),
                    sanitize_snapshot_field(&value)
                ));
            }
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
            let pass = split_runtime_control_messages(self, root, queue);
            aggregate.repaint_requested |= pass.repaint_requested;
            aggregate.invalidation.merge(pass.invalidation);
            aggregate.stop_requested |= pass.stop_requested;
            aggregate
                .animation_requests
                .extend(pass.animation_requests);
            aggregate
                .style_animation_requests
                .extend(pass.style_animation_requests);
            aggregate
                .worker_requests
                .extend(pass.worker_requests);
            aggregate
                .recompose_nodes
                .extend(pass.recompose_nodes);
            aggregate.class_ops.extend(pass.class_ops);
            let mut next_queue =
                collect_clipboard_runtime_messages(&mut self.clipboard, &pass.deliver);
            next_queue.extend(pass.generated);
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
            aggregate
                .animation_requests
                .append(&mut outcome.animation_requests);
            aggregate
                .style_animation_requests
                .append(&mut outcome.style_animation_requests);
            aggregate
                .worker_requests
                .append(&mut outcome.worker_requests);
            aggregate
                .recompose_nodes
                .append(&mut outcome.recompose_nodes);
            aggregate.class_ops.append(&mut outcome.class_ops);
            let emitted = std::mem::take(&mut outcome.messages);
            if !emitted.is_empty() {
                aggregate.messages.extend(emitted.iter().cloned());
                next_queue.extend(emitted);
            }

            if aggregate.stop_requested || next_queue.is_empty() {
                break;
            }
            queue = next_queue;
        }
        aggregate
    }

    /// Wave 1: open the composed `CommandPaletteScreen` by dispatching an
    /// `AppCommandPalette` app message through the normal pipeline to the adapter
    /// root's `on_app_message` (which owns providers + `push_screen`), rather than
    /// dispatching `Action::CommandPalette` into the tree where the legacy
    /// always-mounted host would catch it. Runs inline (same pass) so `ctrl+p`
    /// opens immediately. This is the single seam that retires the old host's open
    /// path across all three key loops; Wave 2 deletes the host + this hop.
    fn dispatch_command_palette_open(&mut self, root: &mut dyn Widget) -> DispatchOutcome {
        let sender = Self::runtime_message_sender();
        let msg = MessageEvent::new(sender, crate::message::AppCommandPalette);
        self.dispatch_message_queue_with_runtime(root, vec![msg])
    }

    fn dispatch_background_runtime_messages(&mut self, root: &mut dyn Widget) -> DispatchOutcome {
        // Drain app-level messages first (set_title/set_sub_title broadcasts).
        let mut queue = self.drain_pending_app_messages();
        queue.extend(self.drain_ready_timers());
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
        let mut last_tick = Instant::now();

        loop {
            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
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

            if last_tick.elapsed() >= tick_rate {
                let _ = self.poll_stylesheet();
                let renderable = render(self, tick);
                self.render(&renderable)?;
                tick += 1;
                last_tick = Instant::now();
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

        // Register this thread as the UI/event-loop thread so worker threads can
        // post callables via `App::call_from_thread`. The guard unregisters on
        // every exit path (early return, break, or `?`), draining any pending
        // jobs so blocked workers unblock.
        let _call_from_thread_guard = CallFromThreadGuard::register();

        let mut root_mount_outcome = {
            // RA2.2: the app root's own mount hook now takes a `&mut WidgetCtx`.
            // The arena tree does not exist yet (built just below), so synthesize
            // a throwaway ctx rooted at `NodeId::default()`. The ctx's outcome is
            // captured here and absorbed after the tree is built (Gap 6 drop
            // site B): worker requests are position-free so the deferral is
            // unobservable, and messages are staged for the first flush with
            // sender `NodeId::default()`, which the message router already
            // treats as app-level.
            let mut synth = EventCtx::default();
            let mut wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut synth);
            root.on_mount(&mut wctx);
            wctx.__enqueue_reactive_if_dirty();
            DispatchOutcome::from_event_ctx(&mut synth)
        };

        // Build the arena-based widget tree by extracting children from root.
        // Runtime dispatch stays tree-driven even when only the synthetic root
        // node exists.
        self.build_widget_tree(root);
        if let Some(tree) = self.active_widget_tree_mut() {
            let _ = sync_widget_controlled_child_display_tree(tree, root);
        }
        self.style_snapshot_cache.clear();

        // Auto-focus the first focusable widget via the arena tree.
        if let Some(tree) = self.active_widget_tree_mut() {
            let focus_chain = collect_focus_chain_tree(tree);
            if let Some(&first) = focus_chain.first() {
                tree.set_focus_state(first, true);
            }
        }
        let mut pending_invalidation = PendingInvalidation::default();

        // Absorb the root's own mount-ctx outcome now that the tree exists
        // (mirrors the `on_app_mount` absorption below). Without this, worker
        // requests, animations and messages staged in a raw root widget's
        // `on_mount` were silently dropped (only the reactive enqueue survived).
        // Messages are dispatched inline (not merely staged): the shared flush
        // only drains `pending_widget_posts` when the command/reactive queue
        // ran, and nothing here enqueues a command, so inline dispatch — exactly
        // as the adjacent `on_app_mount` block does — guarantees delivery.
        if !root_mount_outcome.is_empty() {
            let messages = std::mem::take(&mut root_mount_outcome.messages);
            self.absorb_outcome(
                &mut root_mount_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            let mut msg_outcome = self.dispatch_message_queue_with_runtime(root, messages);
            self.absorb_outcome(
                &mut msg_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            if root_mount_outcome.stop_requested || msg_outcome.stop_requested {
                root.on_unmount();
                self.finish()?;
                return Ok(());
            }
        }

        // Dispatch app-level reactive init phase.
        //
        // Called after the widget tree is built so that init-watcher dispatch
        // (triggered by reactive setters inside `on_mount_with_app`) can reach
        // existing tree nodes via `query_one` / `query_mut`.
        {
            let mut mount_ctx = crate::event::EventCtx::default();
            {
                let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut mount_ctx);
                root.on_app_mount(self, &mut __wctx);
                __wctx.__enqueue_reactive_if_dirty();
            }
            // Absorb the mount ctx so its outcome (worker requests, messages,
            // invalidation, recompositions, animations) flows into the runtime.
            // Without this the live loop dropped everything staged on the ctx —
            // in particular worker requests issued from `on_mount_with_app`
            // (e.g. questions01's `@work`-decorated `on_mount` that calls
            // `push_screen_wait`) were silently discarded, so the QuestionScreen
            // was never pushed and the app rendered blank. The headless startup
            // (`headless_startup`) already absorbs the mount ctx the same way;
            // the live loop must match so worker-driven screen pushes land.
            let mut mount_outcome = DispatchOutcome::from_event_ctx(&mut mount_ctx);
            self.absorb_outcome(
                &mut mount_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            let mut msg_outcome =
                self.dispatch_message_queue_with_runtime(root, mount_outcome.messages);
            self.absorb_outcome(
                &mut msg_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            if mount_outcome.stop_requested || msg_outcome.stop_requested {
                root.on_unmount();
                self.finish()?;
                return Ok(());
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
            .active_widget_tree()
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
            // Mount-time messages (e.g. Select/ListView initial selection) are
            // posted by the widget's `on_mount` (fired at tree build) and routed
            // through the command queue, bubbled by the shared reactive flush.
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
            self.active_widget_tree().and_then(focused_node_id_tree);

        let mut last_tick = Instant::now();
        let mut pending_input_event: Option<CrosstermEvent> = None;
        let mut last_mouse_pos: Option<(u16, u16)> = None;

        'event_loop: loop {
            let timing_on = timing_enabled();
            let loop_started = Instant::now();
            let mut input_kind = "none";
            self.validate_active_selection_owner();
            let mut input_dispatch_us: u128 = 0;
            let mut background_us: Option<u128> = None;
            let mut focused_help_us: Option<u128> = None;
            let mut lifecycle_us: Option<u128> = None;
            let mut reactive_us: Option<u128> = None;
            let mut focus_transition_us: Option<u128> = None;
            let mut binding_us: Option<u128> = None;
            let mut animation_us: Option<u128> = None;
            let mut worker_us: Option<u128> = None;
            let mut style_transition_us: u128 = 0;
            let mut immediate_render_us: u128 = 0;
            let mut normal_render_us: u128 = 0;
            let mut tick_render_us: u128 = 0;
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
            let tick_timeout = tick_rate.saturating_sub(last_tick.elapsed());
            let timeout = self
                .animator
                .next_timeout(now)
                .map(|anim_timeout| tick_timeout.min(anim_timeout))
                .unwrap_or(tick_timeout);
            let timeout = self
                .timers
                .next_timeout(self.timers.now())
                .map(|timer_timeout| timeout.min(timer_timeout))
                .unwrap_or(timeout);
            let poll_started = Instant::now();
            let input_event = if let Some(pending) = pending_input_event.take() {
                Some(pending)
            } else if event::poll(timeout)? {
                Some(event::read()?)
            } else {
                None
            };
            let poll_wait_us = poll_started.elapsed().as_micros();
            if let Some(ref event) = input_event {
                input_kind = input_event_kind(event);
            }
            if timing_on
                && input_event.is_none()
                && pending_invalidation.is_dirty()
                && poll_wait_us > 1_000
            {
                debug_timing(&format!(
                    "[timing] wait_for_input kind=none timeout_us={} waited_us={} dirty=true flags(c={} s={} l={})",
                    timeout.as_micros(),
                    poll_wait_us,
                    pending_invalidation.flags.content,
                    pending_invalidation.flags.style,
                    pending_invalidation.flags.layout
                ));
            }

            let mut handled_input_this_loop = false;
            if let Some(input_event) = input_event {
                let input_started = Instant::now();
                handled_input_this_loop = true;
                let mut sheet = self.default_stylesheet.clone();
                sheet.extend(&self.stylesheet);
                if let Some(screen_sheet) = self.active_screen_stylesheet() {
                    sheet.extend(screen_sheet);
                }
                let _active = set_app_active(self.app_active);
                let _pseudo_state = set_app_runtime_pseudos(AppRuntimePseudos {
                    dark: self.dark_mode,
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
                            input_dispatch_us = input_started.elapsed().as_micros();
                            if timing_on {
                                debug_timing(&format!(
                                    "[timing] early_continue reason=non_press_key input={} dispatch_us={} loop_us={} dirty={} flags(c={} s={} l={})",
                                    input_kind,
                                    input_dispatch_us,
                                    loop_started.elapsed().as_micros(),
                                    pending_invalidation.is_dirty(),
                                    pending_invalidation.flags.content,
                                    pending_invalidation.flags.style,
                                    pending_invalidation.flags.layout
                                ));
                            }
                            continue;
                        }
                        if should_quit_key(&key, &self.quit_keys) {
                            break;
                        }
                        let key = KeyEventData::from_crossterm(key);

                        // App-level key hook with runtime handle (Textual-style).
                        let mut app_key_ctx = EventCtx::default();
                        {
                            let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut app_key_ctx);
                            root.on_app_key(self, &key, &mut __wctx);
                            __wctx.__enqueue_reactive_if_dirty();
                        }
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
                        // Apply any class ops queued by on_app_key handlers (e.g. via
                        // widget methods that stage ClassOps for the next event turn).
                        let app_key_class_ops = app_key_ctx.take_class_ops();
                        if !app_key_class_ops.is_empty() {
                            if let Some(tree) = self.active_widget_tree_mut() {
                                for (node, op) in app_key_class_ops {
                                    match op {
                                        crate::event::ClassOp::Add(c) => tree.add_class(node, &c),
                                        crate::event::ClassOp::Remove(c) => {
                                            tree.remove_class(node, &c)
                                        }
                                    }
                                }
                            }
                            // Class change may flip descendant display/visibility;
                            // relayout so the affected subtree re-resolves CSS.
                            pending_invalidation
                                .request_flags(crate::event::InvalidationFlags::layout());
                        }
                        if app_key_handled {
                            input_dispatch_us = input_started.elapsed().as_micros();
                            if timing_on {
                                debug_timing(&format!(
                                    "[timing] early_continue reason=app_key_handled input={} dispatch_us={} loop_us={} dirty={} flags(c={} s={} l={})",
                                    input_kind,
                                    input_dispatch_us,
                                    loop_started.elapsed().as_micros(),
                                    pending_invalidation.is_dirty(),
                                    pending_invalidation.flags.content,
                                    pending_invalidation.flags.style,
                                    pending_invalidation.flags.layout
                                ));
                            }
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
                            // Wave 1: ctrl+p opens the composed CommandPaletteScreen
                            // via the adapter (on_app_message), NOT by dispatching
                            // Action::CommandPalette to the legacy host.
                            let mut outcome = if matches!(action, Action::CommandPalette) {
                                self.dispatch_command_palette_open(root)
                            } else {
                                self.dispatch_event_auto(root, Event::Action(action))
                            };
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
                            if outcome.handled || matches!(action, Action::CommandPalette) {
                                input_dispatch_us = input_started.elapsed().as_micros();
                                if timing_on {
                                    debug_timing(&format!(
                                        "[timing] early_continue reason=priority_action_handled input={} dispatch_us={} loop_us={} dirty={} flags(c={} s={} l={})",
                                        input_kind,
                                        input_dispatch_us,
                                        loop_started.elapsed().as_micros(),
                                        pending_invalidation.is_dirty(),
                                        pending_invalidation.flags.content,
                                        pending_invalidation.flags.style,
                                        pending_invalidation.flags.layout
                                    ));
                                }
                                continue;
                            }
                        }

                        // Declarative BINDINGS: walk the active chain (focused→root,
                        // or screen-body root when unfocused) plus App::BINDINGS
                        // beneath an active screen.
                        let mut binding_clashes = Vec::new();
                        let binding_match = self.active_widget_tree().and_then(|tree| {
                            let root_target = tree.root().unwrap_or_default();
                            match_binding_chain(
                                tree,
                                self.app_root_tree_when_screen_active(),
                                &key,
                                self.check_action_fn.as_deref(),
                                &self.keymap,
                                Some(&mut binding_clashes),
                            )
                            .map(|(node_id, action_str, source)| {
                                (node_id, action_str, source, root_target)
                            })
                        });
                        // Deliver keymap clash reports after the tree borrow
                        // ends (per clashing keypress, Python cadence).
                        self.deliver_binding_clashes(&binding_clashes);
                        if let Some((binding_node_id, action_str, binding_source, root_target)) =
                            binding_match
                            && let Ok(parsed) = crate::action::parse_action(&action_str)
                        {
                            // CLUSTER 7: execute the binding on its source node when
                            // no registry owner resolves (binding source IS target).
                            if binding_source == BindingSource::Active
                                && let Some(tree_mut) = self.active_widget_tree_mut()
                            {
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
                                let target =
                                    resolved.map(|ra| ra.node).unwrap_or(binding_node_id);
                                if let Some(node) = tree_mut.get_mut(target) {
                                    let mut ctx = EventCtx::default();
                                    let handled = execute_action_with_dispatch_target(
                                        &mut *node.widget,
                                        &parsed,
                                        &mut ctx,
                                        target,
                                    );
                                    debug_input(&format!(
                                        "[input] binding action={action_str:?} handled={handled}"
                                    ));
                                    if handled || ctx.handled() {
                                        let mut binding_outcome = DispatchOutcome {
                                            handled: handled || ctx.handled(),
                                            repaint_requested: ctx.repaint_requested(),
                                            invalidation: ctx.invalidation(),
                                            stop_requested: ctx.stop_requested(),
                                            messages: ctx.take_messages(),
                                            animation_requests: ctx.take_animation_requests(),
                                            style_animation_requests: ctx.take_style_animation_requests(),
                                            worker_requests: ctx.take_worker_requests(),
                                            recompose_nodes: ctx.take_recompose_nodes(),
                                            default_prevented: false,
                                            class_ops: ctx.take_class_ops(),
                                        };
                                        self.absorb_outcome(
                                            &mut binding_outcome,
                                            &mut pending_invalidation,
                                            InvalidationScope::Global,
                                        );
                                        let messages = binding_outcome.messages;
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
                                        if binding_outcome.stop_requested {
                                            break 'event_loop;
                                        }
                                        input_dispatch_us = input_started.elapsed().as_micros();
                                        if timing_on {
                                            debug_timing(&format!(
                                                "[timing] early_continue reason=binding_widget_action input={} dispatch_us={} loop_us={} dirty={} flags(c={} s={} l={})",
                                                input_kind,
                                                input_dispatch_us,
                                                loop_started.elapsed().as_micros(),
                                                pending_invalidation.is_dirty(),
                                                pending_invalidation.flags.content,
                                                pending_invalidation.flags.style,
                                                pending_invalidation.flags.layout
                                            ));
                                        }
                                        continue;
                                    }
                                }
                            }

                            let mut root_ctx = EventCtx::default();
                            let handled = execute_action_with_dispatch_target(
                                root,
                                &parsed,
                                &mut root_ctx,
                                root_target,
                            );
                            debug_input(&format!(
                                "[input] binding action={action_str:?} root_handled={handled}"
                            ));
                            if handled || root_ctx.handled() {
                                let mut root_binding_outcome = DispatchOutcome {
                                    handled: handled || root_ctx.handled(),
                                    repaint_requested: root_ctx.repaint_requested(),
                                    invalidation: root_ctx.invalidation(),
                                    stop_requested: root_ctx.stop_requested(),
                                    messages: root_ctx.take_messages(),
                                    animation_requests: root_ctx.take_animation_requests(),
                                    style_animation_requests: root_ctx.take_style_animation_requests(),
                                    worker_requests: root_ctx.take_worker_requests(),
                                    recompose_nodes: root_ctx.take_recompose_nodes(),
                                    default_prevented: false,
                                    class_ops: root_ctx.take_class_ops(),
                                };
                                self.absorb_outcome(
                                    &mut root_binding_outcome,
                                    &mut pending_invalidation,
                                    InvalidationScope::Global,
                                );
                                let messages = root_binding_outcome.messages;
                                if !messages.is_empty() {
                                    let mut msg_outcome =
                                        self.dispatch_message_queue_with_runtime(root, messages);
                                    self.absorb_outcome(
                                        &mut msg_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    if msg_outcome.stop_requested {
                                        break 'event_loop;
                                    }
                                }
                                if root_binding_outcome.stop_requested {
                                    break 'event_loop;
                                }
                                input_dispatch_us = input_started.elapsed().as_micros();
                                if timing_on {
                                    debug_timing(&format!(
                                        "[timing] early_continue reason=binding_root_action input={} dispatch_us={} loop_us={} dirty={} flags(c={} s={} l={})",
                                        input_kind,
                                        input_dispatch_us,
                                        loop_started.elapsed().as_micros(),
                                        pending_invalidation.is_dirty(),
                                        pending_invalidation.flags.content,
                                        pending_invalidation.flags.style,
                                        pending_invalidation.flags.layout
                                    ));
                                }
                                continue;
                            }

                            // Fallback: app-defined custom action (e.g. "add", "clear").
                            // Called when no action_registry handler exists and execute_action declined.
                            {
                                let mut fallback_ctx = EventCtx::default();
                                {
                                    let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut fallback_ctx);
                                    root.on_app_unhandled_action(self, &action_str, &mut __wctx);
                                    __wctx.__enqueue_reactive_if_dirty();
                                }
                                if fallback_ctx.handled() {
                                    let mut fallback_outcome = DispatchOutcome {
                                        handled: true,
                                        repaint_requested: fallback_ctx.repaint_requested(),
                                        invalidation: fallback_ctx.invalidation(),
                                        stop_requested: fallback_ctx.stop_requested(),
                                        messages: fallback_ctx.take_messages(),
                                        animation_requests: fallback_ctx.take_animation_requests(),
                                        style_animation_requests: fallback_ctx.take_style_animation_requests(),
                                        worker_requests: fallback_ctx.take_worker_requests(),
                                        recompose_nodes: fallback_ctx.take_recompose_nodes(),
                                        default_prevented: false,
                                        class_ops: fallback_ctx.take_class_ops(),
                                    };
                                    self.absorb_outcome(
                                        &mut fallback_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    let messages = fallback_outcome.messages;
                                    if !messages.is_empty() {
                                        let mut msg_outcome = self
                                            .dispatch_message_queue_with_runtime(root, messages);
                                        self.absorb_outcome(
                                            &mut msg_outcome,
                                            &mut pending_invalidation,
                                            InvalidationScope::Global,
                                        );
                                        if msg_outcome.stop_requested {
                                            break 'event_loop;
                                        }
                                    }
                                    if fallback_outcome.stop_requested {
                                        break 'event_loop;
                                    }
                                    continue;
                                }
                            }

                            // The binding matched but no layer handled its
                            // action: report the silent no-op (debug channel +
                            // test-observable buffer) before the key falls
                            // through to raw dispatch.
                            report_unhandled_binding_action(binding_node_id, &action_str);
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
                                if action == Action::CopySelectedText {
                                    if let Some(text) = self.action_copy_selected_text() {
                                        let sender = App::runtime_message_sender();
                                        let mut msg_outcome = self
                                            .dispatch_message_queue_with_runtime(
                                                root,
                                                vec![MessageEvent::new(
                                                    sender,
                                                    crate::message::TextEditClipboardCopyRequested {
                                                        text,
                                                        cut: false,
                                                    },
                                                )
                                                .with_control(sender)],
                                            );
                                        self.absorb_outcome(
                                            &mut msg_outcome,
                                            &mut pending_invalidation,
                                            InvalidationScope::Global,
                                        );
                                        input_dispatch_us = input_started.elapsed().as_micros();
                                        if timing_on {
                                            debug_timing(&format!(
                                                "[timing] early_continue reason=copy_selected_text input={} dispatch_us={} loop_us={} dirty={} flags(c={} s={} l={})",
                                                input_kind,
                                                input_dispatch_us,
                                                loop_started.elapsed().as_micros(),
                                                pending_invalidation.is_dirty(),
                                                pending_invalidation.flags.content,
                                                pending_invalidation.flags.style,
                                                pending_invalidation.flags.layout
                                            ));
                                        }
                                    } else {
                                        self.notify_help_quit();
                                        pending_invalidation.request_full_content();
                                        input_dispatch_us = input_started.elapsed().as_micros();
                                        if timing_on {
                                            debug_timing(&format!(
                                                "[timing] early_continue reason=help_quit input={} dispatch_us={} loop_us={} dirty={} flags(c={} s={} l={})",
                                                input_kind,
                                                input_dispatch_us,
                                                loop_started.elapsed().as_micros(),
                                                pending_invalidation.is_dirty(),
                                                pending_invalidation.flags.content,
                                                pending_invalidation.flags.style,
                                                pending_invalidation.flags.layout
                                            ));
                                        }
                                    }
                                    continue;
                                }
                                if action == Action::HelpQuit {
                                    self.notify_help_quit();
                                    pending_invalidation.request_full_content();
                                    input_dispatch_us = input_started.elapsed().as_micros();
                                    if timing_on {
                                        debug_timing(&format!(
                                            "[timing] early_continue reason=help_quit input={} dispatch_us={} loop_us={} dirty={} flags(c={} s={} l={})",
                                            input_kind,
                                            input_dispatch_us,
                                            loop_started.elapsed().as_micros(),
                                            pending_invalidation.is_dirty(),
                                            pending_invalidation.flags.content,
                                            pending_invalidation.flags.style,
                                            pending_invalidation.flags.layout
                                        ));
                                    }
                                    continue;
                                }
                                if matches!(action, Action::FocusNext | Action::FocusPrev) {
                                    // Give the currently-focused branch a chance to descend
                                    // focus before falling back to tree-level focus cycling.
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
                                        input_dispatch_us = input_started.elapsed().as_micros();
                                        if timing_on {
                                            debug_timing(&format!(
                                                "[timing] early_continue reason=focus_action_handled input={} dispatch_us={} loop_us={} dirty={} flags(c={} s={} l={})",
                                                input_kind,
                                                input_dispatch_us,
                                                loop_started.elapsed().as_micros(),
                                                pending_invalidation.is_dirty(),
                                                pending_invalidation.flags.content,
                                                pending_invalidation.flags.style,
                                                pending_invalidation.flags.layout
                                            ));
                                        }
                                        continue;
                                    }
                                    if self.move_focus_auto(action) {
                                        pending_invalidation.request_full_content();
                                        input_dispatch_us = input_started.elapsed().as_micros();
                                        if timing_on {
                                            debug_timing(&format!(
                                                "[timing] early_continue reason=focus_moved input={} dispatch_us={} loop_us={} dirty={} flags(c={} s={} l={})",
                                                input_kind,
                                                input_dispatch_us,
                                                loop_started.elapsed().as_micros(),
                                                pending_invalidation.is_dirty(),
                                                pending_invalidation.flags.content,
                                                pending_invalidation.flags.style,
                                                pending_invalidation.flags.layout
                                            ));
                                        }
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
                                    let tree_target = self.active_widget_tree().and_then(|tree| {
                                        widget_at_tree_layout(tree, mouse.column, mouse.row)
                                    });
                                    let chosen = self
                                        .active_widget_tree()
                                        .map(|tree| {
                                            super::choose_deeper_target(
                                                tree,
                                                frame_target,
                                                tree_target,
                                            )
                                        })
                                        .unwrap_or(frame_target);
                                    let relation = self
                                        .active_widget_tree()
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
                                if let Some(owner) = self.active_selection_owner
                                    && self.selection_drag_active
                                {
                                    let (sx, sy) = self.content_local_coords_auto(
                                        owner,
                                        mouse.column,
                                        mouse.row,
                                    );
                                    if self.update_selection_drag(owner, sx, sy).unwrap_or(false) {
                                        pending_invalidation
                                            .request_widget_rect(&self.hit_test, owner);
                                    }
                                }
                                if self.update_hover_tooltip(mouse.column, mouse.row) {
                                    pending_invalidation
                                        .request_flags(crate::event::InvalidationFlags::layout());
                                    pending_invalidation.request_full_content();
                                }
                                let is_drag = matches!(mouse.kind, MouseEventKind::Drag(_));
                                let down_target = self.click_tracker.down_target();
                                let move_target = if is_drag {
                                    down_target
                                        .or_else(|| self.widget_at_auto(mouse.column, mouse.row))
                                } else {
                                    self.widget_at_auto(mouse.column, mouse.row)
                                };

                                if is_drag && scrollbar_drag_trace_enabled() {
                                    debug_input(&format!(
                                        "[scrollbar-drag] move screen=({}, {}) down_target={:?} hovered={:?} chosen_target={:?}",
                                        mouse.column,
                                        mouse.row,
                                        down_target.map(node_id_to_ffi),
                                        self.hovered.map(node_id_to_ffi),
                                        move_target.map(node_id_to_ffi),
                                    ));
                                }

                                if let Some(target) = move_target {
                                    let changed = self.call_on_mouse_move_auto(
                                        root,
                                        target,
                                        mouse.column,
                                        mouse.row,
                                        is_drag && down_target.is_some(),
                                    );

                                    let (local_x, local_y) = self.content_local_coords_auto(
                                        target,
                                        mouse.column,
                                        mouse.row,
                                    );
                                    let move_event =
                                        Event::MouseMove(crate::event::MouseMoveEvent {
                                            target,
                                            screen_x: mouse.column,
                                            screen_y: mouse.row,
                                            x: local_x,
                                            y: local_y,
                                        });
                                    let mut move_outcome = self.dispatch_event_to_target_auto(
                                        root,
                                        target,
                                        &move_event,
                                    );
                                    self.absorb_outcome(
                                        &mut move_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    let mut move_msg_outcome = self
                                        .dispatch_message_queue_with_runtime(
                                            root,
                                            move_outcome.messages,
                                        );
                                    self.absorb_outcome(
                                        &mut move_msg_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    if move_outcome.stop_requested
                                        || move_msg_outcome.stop_requested
                                    {
                                        break 'event_loop;
                                    }

                                    if changed {
                                        if matches!(mouse.kind, MouseEventKind::Drag(_)) {
                                            // Dragging scrollbar thumbs can shift large composed
                                            // regions and produce stale strip artifacts if only
                                            // partial regions are updated. Force full content
                                            // invalidation per drag update.
                                            pending_invalidation.request_flags(
                                                crate::event::InvalidationFlags::layout(),
                                            );
                                            pending_invalidation.request_full_content();
                                        } else {
                                            pending_invalidation.request_full_content();
                                        }
                                    }

                                    // `call_on_mouse_move_auto` bubbles target -> root, so root
                                    // drag handlers participate without a second explicit root
                                    // dispatch.
                                }
                            }
                            MouseEventKind::Down(btn) => {
                                debug_input(&format!(
                                    "[input] mouse down x={} y={} hovered={:?}",
                                    mouse.column,
                                    mouse.row,
                                    self.hovered.map(node_id_to_ffi)
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
                                    if scrollbar_drag_trace_enabled() {
                                        debug_input(&format!(
                                            "[scrollbar-drag] down target={} local=({}, {}) screen=({}, {}) button={}",
                                            node_id_to_ffi(target),
                                            x,
                                            y,
                                            mouse.column,
                                            mouse.row,
                                            button
                                        ));
                                    }
                                    let down_event = Event::MouseDown(MouseDownEvent {
                                        target,
                                        screen_x: mouse.column,
                                        screen_y: mouse.row,
                                        x,
                                        y,
                                    });
                                    if matches!(
                                        btn,
                                        crossterm::event::MouseButton::Left
                                            | crossterm::event::MouseButton::Right
                                    ) {
                                        let previous_owner = self.active_selection_owner;
                                        let click_count = self.register_selection_click(
                                            target,
                                            button,
                                            mouse.column,
                                            mouse.row,
                                        );
                                        let changed = match click_count {
                                            1 => self
                                                .begin_selection_drag(target, x, y)
                                                .or_else(|| Some(self.clear_active_selection()))
                                                .unwrap_or(false),
                                            2 => self
                                                .select_word_at(target, x, y)
                                                .or_else(|| Some(self.clear_active_selection()))
                                                .unwrap_or(false),
                                            _ => self
                                                .select_all_at_target(target)
                                                .or_else(|| Some(self.clear_active_selection()))
                                                .unwrap_or(false),
                                        };
                                        if changed {
                                            if let Some(id) = previous_owner {
                                                pending_invalidation
                                                    .request_widget_rect(&self.hit_test, id);
                                            }
                                            if let Some(id) = self.active_selection_owner {
                                                pending_invalidation
                                                    .request_widget_rect(&self.hit_test, id);
                                            }
                                        }
                                    } else {
                                        self.clear_selection_click_streak();
                                    }
                                    // Python `Screen._forward_event` (MouseDown):
                                    // focus the nearest focusable widget under the
                                    // pointer BEFORE forwarding the event
                                    // (`get_focusable_widget_at` + `set_focus`).
                                    let focus_target =
                                        self.active_widget_tree().and_then(|tree| {
                                            crate::runtime::helpers::focusable_node_for_click(
                                                tree, target,
                                            )
                                        });
                                    if let Some(focus_target) = focus_target
                                        && self.set_focus_node(focus_target)
                                    {
                                        pending_invalidation.request_full_content();
                                    }
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
                                    if matches!(
                                        btn,
                                        crossterm::event::MouseButton::Left
                                            | crossterm::event::MouseButton::Right
                                    ) {
                                        self.clear_selection_click_streak();
                                        let previous_owner = self.active_selection_owner;
                                        if self.clear_active_selection() {
                                            if let Some(id) = previous_owner {
                                                pending_invalidation
                                                    .request_widget_rect(&self.hit_test, id);
                                            } else {
                                                pending_invalidation.request_full_content();
                                            }
                                        }
                                    }
                                    // No widget cell under the press: Python
                                    // treats this as a press on the Screen
                                    // (not focusable) — focus is untouched.
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
                                self.end_selection_drag();
                                let down_target = self.click_tracker.down_target();
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
                                if scrollbar_drag_trace_enabled() {
                                    debug_input(&format!(
                                        "[scrollbar-drag] up target={:?} local=({}, {}) screen=({}, {}) down_target_before_clear={:?}",
                                        target.map(node_id_to_ffi),
                                        x,
                                        y,
                                        mouse.column,
                                        mouse.row,
                                        self.click_tracker.down_target().map(node_id_to_ffi)
                                    ));
                                }
                                // Mouse-up must be delivered to the original mouse-down owner
                                // (capture-style semantics), even if pointer has drifted.
                                if let Some(capture_target) =
                                    down_target.filter(|id| Some(*id) != target)
                                {
                                    let (cx, cy) = self.content_local_coords_auto(
                                        capture_target,
                                        mouse.column,
                                        mouse.row,
                                    );
                                    let capture_up = Event::MouseUp(MouseUpEvent {
                                        target: Some(capture_target),
                                        screen_x: mouse.column,
                                        screen_y: mouse.row,
                                        x: cx,
                                        y: cy,
                                    });
                                    let mut capture_outcome = self.dispatch_event_to_target_auto(
                                        root,
                                        capture_target,
                                        &capture_up,
                                    );
                                    self.absorb_outcome(
                                        &mut capture_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    let mut capture_msg_outcome = self
                                        .dispatch_message_queue_with_runtime(
                                            root,
                                            capture_outcome.messages,
                                        );
                                    self.absorb_outcome(
                                        &mut capture_msg_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    if capture_outcome.stop_requested
                                        || capture_msg_outcome.stop_requested
                                    {
                                        break 'event_loop;
                                    }
                                }

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
                                    let click_stopped = click_outcome.stop_requested;
                                    self.absorb_outcome(
                                        &mut click_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );

                                    // `@click` action-link routing (Python
                                    // `widget._on_click` → `app._broker_event`):
                                    // consult the style meta at the clicked cell.
                                    // If a `[@click=...]` span baked an action
                                    // string there, dispatch it with the clicked
                                    // widget as the default action namespace.
                                    if !click_stopped {
                                        if let Some(action) =
                                            self.click_action_at(mouse.column, mouse.row)
                                        {
                                            let msg = MessageEvent::new(
                                                click_target,
                                                crate::message::ActionDispatchRequested { action },
                                            );
                                            let mut action_outcome = self
                                                .dispatch_message_queue_with_runtime(
                                                    root,
                                                    vec![msg],
                                                );
                                            self.absorb_outcome(
                                                &mut action_outcome,
                                                &mut pending_invalidation,
                                                InvalidationScope::Global,
                                            );
                                        }
                                    }

                                    // Messages posted by widgets while handling
                                    // the synthesized Click (e.g. a custom
                                    // `on_event(Click)` posting a demo message)
                                    // must reach the app, exactly as the
                                    // headless click path dispatches them.
                                    // Dropping them here left live clicks inert
                                    // where `pilot.click` worked (custom01).
                                    let mut click_msg_outcome = self
                                        .dispatch_message_queue_with_runtime(
                                            root,
                                            click_outcome.messages,
                                        );
                                    self.absorb_outcome(
                                        &mut click_msg_outcome,
                                        &mut pending_invalidation,
                                        InvalidationScope::Global,
                                    );
                                    if click_msg_outcome.stop_requested {
                                        break 'event_loop;
                                    }
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
                                // Scroll bubbling may be handled by an ancestor (including the
                                // root screen), which can shift large portions of the composed
                                // frame. Use global invalidation for the hook-path outcome to
                                // avoid stale region artifacts.
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
                        self.apply_app_blur_focus_state();
                        if self.clear_hover_tooltip() {
                            pending_invalidation
                                .request_flags(crate::event::InvalidationFlags::layout());
                        }
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
                        pending_invalidation.request_full_content();
                        if outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                    }
                    CrosstermEvent::FocusGained => {
                        self.apply_app_focus_restore_state();
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
                        pending_invalidation.request_full_content();
                        if outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                    }
                    _ => {}
                }
                if input_dispatch_us == 0 {
                    input_dispatch_us = input_started.elapsed().as_micros();
                }
            }

            let phase_started = Instant::now();
            let mut background_outcome = self.dispatch_background_runtime_messages(root);
            background_us = Some(
                background_us
                    .unwrap_or(0)
                    .saturating_add(phase_started.elapsed().as_micros()),
            );
            self.absorb_outcome(
                &mut background_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            if background_outcome.stop_requested {
                break 'event_loop;
            }

            // App-level timer callbacks (set_interval / set_timer). The
            // background drain above stashed any due app-timer ids; invoke their
            // callbacks now through the adapter's `on_app_timer` hook so reactive
            // mutations fire their watchers in the same turn.
            if self.has_pending_timer_fires() {
                let mut timer_ctx = EventCtx::default();
                {
                    let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut timer_ctx);
                    root.on_app_timer(self, &mut __wctx);
                    __wctx.__enqueue_reactive_if_dirty();
                }
                let mut timer_outcome = DispatchOutcome::from_event_ctx(&mut timer_ctx);
                self.absorb_outcome(
                    &mut timer_outcome,
                    &mut pending_invalidation,
                    InvalidationScope::Global,
                );
                if timer_outcome.stop_requested {
                    break 'event_loop;
                }
                let mut timer_msg_outcome =
                    self.dispatch_message_queue_with_runtime(root, timer_outcome.messages);
                self.absorb_outcome(
                    &mut timer_msg_outcome,
                    &mut pending_invalidation,
                    InvalidationScope::Global,
                );
                if timer_msg_outcome.stop_requested {
                    break 'event_loop;
                }
            }

            // Widget-owned interval callbacks (WidgetCtx::set_interval). Same
            // TimerRuntime as app timers; the background drain above stashed any
            // due widget-timer ids. Fires run against each node's widget with a
            // fresh WidgetCtx (reactive mutations flow to watchers via the flush).
            if self.has_pending_widget_timer_fires() {
                self.run_due_widget_timer_callbacks(&mut pending_invalidation);
            }

            let phase_started = Instant::now();
            let mut focused_help_outcome = self.dispatch_focused_help_changed(root);
            focused_help_us = Some(
                focused_help_us
                    .unwrap_or(0)
                    .saturating_add(phase_started.elapsed().as_micros()),
            );
            self.absorb_outcome(
                &mut focused_help_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            if focused_help_outcome.stop_requested {
                break 'event_loop;
            }

            // Drain pending lifecycle events from the tree and dispatch
            // Mount/Unmount events to affected widgets (this fires the
            // widget-owned `on_mount_ctx` hook — set_interval registration).
            // Shared with the headless pump so a subtree mounted via dynamic
            // recompose registers its timers identically in both loops.
            let phase_started = Instant::now();
            let lifecycle_outcome =
                self.drain_tree_lifecycle_events(root, &mut pending_invalidation);
            lifecycle_us = Some(
                lifecycle_us
                    .unwrap_or(0)
                    .saturating_add(phase_started.elapsed().as_micros()),
            );
            if lifecycle_outcome.stop_requested {
                break 'event_loop;
            }

            // ── Reactive phase ────────────────────────────────────────
            // Run the reactive phase for widgets that accumulated changes
            // during event dispatch. This drains ReactiveCtx changes, calls
            // watchers/computed recomputation, and detects cycles.
            let phase_started = Instant::now();
            self.run_event_loop_reactive_phase(root, &mut pending_invalidation);
            reactive_us = Some(
                reactive_us
                    .unwrap_or(0)
                    .saturating_add(phase_started.elapsed().as_micros()),
            );

            // Detect focus transitions and dispatch Focus/Blur events.
            let phase_started = Instant::now();
            let current_focus: Option<NodeId> =
                self.active_widget_tree().and_then(focused_node_id_tree);
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
            focus_transition_us = Some(
                focus_transition_us
                    .unwrap_or(0)
                    .saturating_add(phase_started.elapsed().as_micros()),
            );

            // Input-priority fast path: render immediately after input-driven
            // invalidation so visible state updates (selection/caret/list focus)
            // land before slower per-loop housekeeping.
            let mut rendered_immediately_for_input = false;
            if handled_input_this_loop
                && (pending_invalidation.is_dirty() || self.resized_since_last_render)
            {
                let render_started = Instant::now();
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
                rendered_immediately_for_input = true;
                immediate_render_us = render_started.elapsed().as_micros();
            }

            // If more input is already queued after an immediate render, keep
            // draining input first to avoid visible backlog.
            if rendered_immediately_for_input && event::poll(Duration::ZERO)? {
                pending_input_event = Some(event::read()?);
                // Fairness guard: keep low-latency input draining, but do not
                // starve Tick delivery under sustained keyboard input.
                let tick_due = last_tick.elapsed() >= tick_rate;
                if !tick_due {
                    if timing_on {
                        debug_timing(&format!(
                            "[timing] early_continue reason=input_priority_drain input={} dispatch_us={} immediate_render_us={} loop_us={} dirty_end={} flags(c={} s={} l={})",
                            input_kind,
                            input_dispatch_us,
                            immediate_render_us,
                            loop_started.elapsed().as_micros(),
                            pending_invalidation.is_dirty(),
                            pending_invalidation.flags.content,
                            pending_invalidation.flags.style,
                            pending_invalidation.flags.layout
                        ));
                    }
                    continue;
                }
                if timing_on {
                    debug_timing(&format!(
                        "[timing] fairness_break reason=tick_due_after_input_drain input={} dispatch_us={} immediate_render_us={} loop_us={}",
                        input_kind,
                        input_dispatch_us,
                        immediate_render_us,
                        loop_started.elapsed().as_micros()
                    ));
                }
            }

            let phase_started = Instant::now();
            let mut binding_outcome = self.dispatch_binding_hints_changed(root);
            binding_us = Some(
                binding_us
                    .unwrap_or(0)
                    .saturating_add(phase_started.elapsed().as_micros()),
            );
            self.absorb_outcome(
                &mut binding_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            if binding_outcome.stop_requested {
                break 'event_loop;
            }

            let phase_started = Instant::now();
            let mut animation_outcome = self.dispatch_animation_frame(root);
            animation_us = Some(
                animation_us
                    .unwrap_or(0)
                    .saturating_add(phase_started.elapsed().as_micros()),
            );
            self.absorb_outcome(
                &mut animation_outcome,
                &mut pending_invalidation,
                InvalidationScope::Global,
            );
            if animation_outcome.stop_requested {
                break 'event_loop;
            }

            // ── Run pending call_from_thread jobs for this tick ──────
            //
            // Worker threads posted these via `App::call_from_thread` and are
            // blocked waiting for them to run. Execute each with `&mut App`
            // (the closure ships its return value back to the worker). Run
            // before worker-request processing so a callable that mutates the
            // tree is reflected in the same tick's invalidation handling.
            for job in crate::runtime::tasks::drain_call_from_thread_jobs() {
                job(self);
                pending_invalidation.request_full_content();
            }

            // ── Process accumulated worker requests for this tick ────
            let phase_started = Instant::now();
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
            worker_us = Some(
                worker_us
                    .unwrap_or(0)
                    .saturating_add(phase_started.elapsed().as_micros()),
            );

            if pending_invalidation.flags.style
                || pending_invalidation.flags.layout
                || self.style_snapshot_cache.is_empty()
            {
                let phase_started = Instant::now();
                self.dispatch_style_transition_requests(root);
                style_transition_us += phase_started.elapsed().as_micros();
            }

            if let Some(tree) = self.active_widget_tree_mut()
                && sync_widget_controlled_child_display_tree(tree, root)
            {
                pending_invalidation.request_flags(crate::event::InvalidationFlags::layout());
                pending_invalidation.request_full_content();
            }
            self.absorb_pending_recompositions(&mut pending_invalidation);
            self.absorb_pending_query_refreshes(&mut pending_invalidation);
            if self.take_pending_force_relayout() {
                pending_invalidation.request_flags(crate::event::InvalidationFlags::layout());
                pending_invalidation.request_full_content();
            }

            if pending_invalidation.is_dirty() || self.resized_since_last_render {
                let render_started = Instant::now();
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
                normal_render_us = render_started.elapsed().as_micros();
            }

            if last_tick.elapsed() >= tick_rate {
                let mut sheet = self.default_stylesheet.clone();
                sheet.extend(&self.stylesheet);
                if let Some(screen_sheet) = self.active_screen_stylesheet() {
                    sheet.extend(screen_sheet);
                }
                let _active = set_app_active(self.app_active);
                let _pseudo_state = set_app_runtime_pseudos(AppRuntimePseudos {
                    dark: self.dark_mode,
                    inline: self.app_inline,
                    ansi: self.app_ansi,
                    nocolor: self.app_nocolor,
                });
                let _guard = set_style_context(sheet);
                if let Some(reload) = self.poll_stylesheet() {
                    self.absorb_stylesheet_reload(root, reload, &mut pending_invalidation);
                }
                root.on_tick(tick);
                // `root.on_tick` only reaches the app adapter — its composed
                // children were extracted into the arena at tree build, so the
                // widgets that actually animate (LoadingIndicator, …) never see
                // the frame tick through it. Deliver the tick to every ACTIVE
                // arena widget (and any cover widget — the `loading` overlay),
                // mirroring `headless_advance_ticks`.
                if let Some(tree) = self.active_widget_tree_mut() {
                    deliver_frame_tick(tree, tick);
                }
                // Opt-in: inactive screens tick too (background animation).
                self.deliver_background_screen_ticks(tick);

                let mut app_tick_ctx = EventCtx::default();
                {
                    let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut app_tick_ctx);
                    root.on_app_tick(self, tick, &mut __wctx);
                    __wctx.__enqueue_reactive_if_dirty();
                }
                let mut app_tick_outcome = DispatchOutcome::from_event_ctx(&mut app_tick_ctx);
                self.absorb_outcome(
                    &mut app_tick_outcome,
                    &mut pending_invalidation,
                    InvalidationScope::Global,
                );
                if app_tick_outcome.stop_requested {
                    break 'event_loop;
                }
                let mut app_tick_msg_outcome =
                    self.dispatch_message_queue_with_runtime(root, app_tick_outcome.messages);
                self.absorb_outcome(
                    &mut app_tick_msg_outcome,
                    &mut pending_invalidation,
                    InvalidationScope::Global,
                );
                if app_tick_msg_outcome.stop_requested {
                    break 'event_loop;
                }

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
                // Re-sync the docked ToastRack from the notification store when it
                // changed (a `notify`, or a `NotificationExpired` removal from an
                // elapsed rack timer / toast click). Auto-dismiss timing is owned
                // by the rack node's widget timers, not a runtime prune.
                if self.notifications_dirty && self.refresh_toast_rack() {
                    pending_invalidation.request_full_content();
                }
                if pending_invalidation.flags.style
                    || pending_invalidation.flags.layout
                    || self.style_snapshot_cache.is_empty()
                {
                    let phase_started = Instant::now();
                    self.dispatch_style_transition_requests(root);
                    style_transition_us += phase_started.elapsed().as_micros();
                }
                if outcome.stop_requested || msg_outcome.stop_requested {
                    break 'event_loop;
                }

                let any_active = self.any_widget_active_auto(root);
                if let Some(tree) = self.active_widget_tree_mut()
                    && sync_widget_controlled_child_display_tree(tree, root)
                {
                    pending_invalidation.request_flags(crate::event::InvalidationFlags::layout());
                    pending_invalidation.request_full_content();
                }
                self.absorb_pending_recompositions(&mut pending_invalidation);
                self.absorb_pending_query_refreshes(&mut pending_invalidation);
                if pending_invalidation.is_dirty()
                    || self.resized_since_last_render
                    || any_active
                    || prev_any_active
                {
                    let render_started = Instant::now();
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
                    tick_render_us = render_started.elapsed().as_micros();
                }
                prev_any_active = any_active;
                last_tick = Instant::now();
                tick += 1;
            }

            if timing_on {
                let total_us = loop_started.elapsed().as_micros();
                if handled_input_this_loop
                    || immediate_render_us > 0
                    || normal_render_us > 0
                    || tick_render_us > 0
                    || total_us > 2_000
                {
                    debug_timing(&format!(
                        "[timing] loop input={} poll_wait_us={} input_dispatch_us={} phases_us(bg={} help={} lifecycle={} reactive={} focus={} binding={} anim={} worker={} style={}) render_us(immediate={} normal={} tick={}) total_us={} dirty_end={} flags_end(c={} s={} l={})",
                        input_kind,
                        poll_wait_us,
                        input_dispatch_us,
                        background_us.unwrap_or(0),
                        focused_help_us.unwrap_or(0),
                        lifecycle_us.unwrap_or(0),
                        reactive_us.unwrap_or(0),
                        focus_transition_us.unwrap_or(0),
                        binding_us.unwrap_or(0),
                        animation_us.unwrap_or(0),
                        worker_us.unwrap_or(0),
                        style_transition_us,
                        immediate_render_us,
                        normal_render_us,
                        tick_render_us,
                        total_us,
                        pending_invalidation.is_dirty(),
                        pending_invalidation.flags.content,
                        pending_invalidation.flags.style,
                        pending_invalidation.flags.layout
                    ));
                }
            }
        }

        root.on_unmount();
        self.finish()?;
        Ok(())
    }

    // ── Headless test harness (App::run_test / Pilot) ────────────────────────
    //
    // These methods power the in-process Pilot (see `src/runtime/pilot.rs`).
    // They drive the same dispatch primitives the live `run_widget_tree` loop
    // uses (`dispatch_event_auto`, `dispatch_message_queue_with_runtime`,
    // declarative BINDINGS resolution, action-map, focus/scroll, animation and
    // timer pumps, `render_widget_with_regions`) but read input from injected
    // events instead of a real terminal, and render into the in-memory
    // `FrameBuffer` (see `App::headless`). This mirrors Python Textual's
    // headless driver + `pilot.py`.

    /// Set up the headless style context (default + app + screen stylesheets and
    /// runtime pseudo-state) for the duration of a closure that mutates state.
    fn with_headless_style_context<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut sheet = self.default_stylesheet.clone();
        sheet.extend(&self.stylesheet);
        if let Some(screen_sheet) = self.active_screen_stylesheet() {
            sheet.extend(screen_sheet);
        }
        let _active = set_app_active(self.app_active);
        let _pseudo_state = set_app_runtime_pseudos(AppRuntimePseudos {
            dark: self.dark_mode,
            inline: self.app_inline,
            ansi: self.app_ansi,
            nocolor: self.app_nocolor,
        });
        let _guard = set_style_context(sheet);
        f(self)
    }

    /// Headless equivalent of the `run_widget_tree` startup sequence: mount the
    /// root, build the arena tree, auto-focus, dispatch the initial Mount/Ready
    /// lifecycle, and produce the first render into the in-memory frame.
    pub(crate) fn headless_startup(&mut self, root: &mut dyn Widget) -> crate::Result<()> {
        self.headless = true;
        self.start()?;

        let mut root_mount_outcome = {
            // RA2.2: the app root's own mount hook now takes a `&mut WidgetCtx`.
            // The arena tree does not exist yet (built just below), so synthesize
            // a throwaway ctx rooted at `NodeId::default()`. As in the live
            // loop, the ctx's outcome is captured and absorbed after the tree
            // is built (Gap 6 drop site B).
            let mut synth = EventCtx::default();
            let mut wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut synth);
            root.on_mount(&mut wctx);
            wctx.__enqueue_reactive_if_dirty();
            DispatchOutcome::from_event_ctx(&mut synth)
        };
        self.build_widget_tree(root);
        if let Some(tree) = self.active_widget_tree_mut() {
            let _ = sync_widget_controlled_child_display_tree(tree, root);
        }
        self.style_snapshot_cache.clear();

        if let Some(tree) = self.active_widget_tree_mut() {
            let focus_chain = collect_focus_chain_tree(tree);
            if let Some(&first) = focus_chain.first() {
                tree.set_focus_state(first, true);
            }
        }

        let mut pending = PendingInvalidation::default();

        // Absorb the root's own mount-ctx outcome now that the tree exists
        // (mirrors the `on_app_mount` absorption below and the live loop).
        // Messages are dispatched inline (the shared flush only drains
        // `pending_widget_posts` when a command/reactive entry ran); a stop
        // request is recorded stickily by `absorb_outcome` so Pilot tests
        // observe it via `headless_stop_requested()`.
        if !root_mount_outcome.is_empty() {
            let messages = std::mem::take(&mut root_mount_outcome.messages);
            self.absorb_outcome(&mut root_mount_outcome, &mut pending, InvalidationScope::Global);
            let mut msg_outcome = self.dispatch_message_queue_with_runtime(root, messages);
            self.absorb_outcome(&mut msg_outcome, &mut pending, InvalidationScope::Global);
        }
        {
            let mut mount_ctx = crate::event::EventCtx::default();
            {
                let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut mount_ctx);
                root.on_app_mount(self, &mut __wctx);
                __wctx.__enqueue_reactive_if_dirty();
            }
            // Absorb the mount ctx so its outcome (worker requests, messages,
            // invalidation, recompositions) flows into the runtime — in
            // particular, worker requests issued from `on_mount_with_app`
            // (e.g. questions01's `push_screen_wait` worker) reach the headless
            // worker phase via the accumulator instead of being dropped.
            let mut mount_outcome = DispatchOutcome::from_event_ctx(&mut mount_ctx);
            self.absorb_outcome(&mut mount_outcome, &mut pending, InvalidationScope::Global);
            let mut msg_outcome =
                self.dispatch_message_queue_with_runtime(root, mount_outcome.messages);
            self.absorb_outcome(&mut msg_outcome, &mut pending, InvalidationScope::Global);
        }

        self.render_widget(root)?;
        self.apply_layout_info_to_tree();

        // Dispatch initial Mount events for all tree nodes.
        let initial_mount_nodes: Vec<NodeId> = self
            .active_widget_tree()
            .and_then(|tree| tree.root().map(|r| tree.walk_depth_first(r)))
            .unwrap_or_default();
        for node_id in initial_mount_nodes {
            let mut outcome = self.dispatch_event_to_target_auto(
                root,
                node_id,
                &Event::Mount(MountEvent { node: node_id }),
            );
            self.absorb_outcome(&mut outcome, &mut pending, InvalidationScope::Global);
            let mut msg_outcome =
                self.dispatch_message_queue_with_runtime(root, outcome.messages);
            self.absorb_outcome(&mut msg_outcome, &mut pending, InvalidationScope::Global);
            // RA2.2: the merged `on_mount(ctx)` for these initial nodes already
            // fired during tree build (`WidgetTree::fire_mount_callbacks`), so it
            // is NOT re-fired here (that would double-mount). Its mount-time
            // messages were routed through the command queue there. Dynamic
            // recompose mounts are handled by the shared flush's lifecycle drain.
        }

        // Ready event after first render.
        {
            let mut outcome = self.dispatch_event_auto(root, Event::Ready(ReadyEvent));
            self.absorb_outcome(&mut outcome, &mut pending, InvalidationScope::Global);
            let mut msg_outcome =
                self.dispatch_message_queue_with_runtime(root, outcome.messages);
            self.absorb_outcome(&mut msg_outcome, &mut pending, InvalidationScope::Global);
        }

        self.dispatch_style_transition_requests(root);
        self.headless_pump(root, &mut pending)?;
        Ok(())
    }

    /// Run housekeeping (background/app messages, ready timers, animation frame,
    /// style transitions, recompositions) and render until the app is idle.
    ///
    /// "Idle" = no pending invalidation, no runtime animations, and no timers
    /// whose deadline has elapsed. Mirrors `pilot._wait_for_screen` + `pause`.
    pub(crate) fn headless_pump(
        &mut self,
        root: &mut dyn Widget,
        pending: &mut PendingInvalidation,
    ) -> crate::Result<()> {
        const MAX_ITERATIONS: usize = 10_000;
        for _ in 0..MAX_ITERATIONS {
            let mut progressed = false;

            // `call_from_thread` jobs posted by worker threads. Run each with
            // `&mut App` so the worker (blocked on its result channel) unblocks,
            // exactly as the live loop does once per tick. Only when THIS app
            // registered the (process-global) UI thread — i.e. it actually
            // spawned a worker — so a worker-free `run_test` does not drain jobs
            // belonging to a concurrent test sharing the singleton bridge.
            if self.headless_ui_thread_registered {
                for job in crate::runtime::tasks::drain_call_from_thread_jobs() {
                    progressed = true;
                    job(self);
                    pending.request_full_content();
                }
            }

            // App-level / broadcast messages (set_title etc.).
            let app_messages = self.drain_pending_app_messages();
            if !app_messages.is_empty() {
                progressed = true;
                let mut outcome = self.dispatch_message_queue_with_runtime(root, app_messages);
                self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
            }

            // Ready timers (deadlines that have already elapsed). App-level timer
            // callbacks are run via run_due_timer_callbacks.
            let timer_messages = self.drain_ready_timers();
            if !timer_messages.is_empty() {
                progressed = true;
                let mut outcome =
                    self.dispatch_message_queue_with_runtime(root, timer_messages);
                self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
            }
            if self.has_pending_timer_fires() {
                progressed = true;
                let mut ctx = EventCtx::default();
                // Route through the root's `on_app_timer` hook (NOT a bare
                // `run_due_timer_callbacks`) so app-struct reactive mutations made
                // inside timer callbacks (e.g. `app.reactive_ctx()` setters in a
                // `set_interval` closure) flush their watchers via
                // `dispatch_app_reactive`, exactly as the live event loop does
                // (see the `root.on_app_timer(...)` call in `run_with`). Without
                // this, headless/Pilot timer ticks fire the callback but never the
                // app-level `watch_*`, so time-driven clock demos looked dead.
                {
                    let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
                    root.on_app_timer(self, &mut __wctx);
                    __wctx.__enqueue_reactive_if_dirty();
                }
                let mut outcome = DispatchOutcome::from_event_ctx(&mut ctx);
                self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
                let mut msg_outcome =
                    self.dispatch_message_queue_with_runtime(root, outcome.messages);
                self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
            }

            // Widget-owned interval callbacks (WidgetCtx::set_interval). Same
            // TimerRuntime as app timers; the drain above stashed due ids. Under
            // the manual clock, Pilot::advance_clock drives these deterministically.
            if self.has_pending_widget_timer_fires() {
                progressed = true;
                self.run_due_widget_timer_callbacks(pending);
            }

            // Async task completions.
            let task_messages = self.async_tasks.drain_completed();
            if !task_messages.is_empty() {
                progressed = true;
                let mut outcome =
                    self.dispatch_message_queue_with_runtime(root, task_messages);
                self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
            }

            // Worker requests + completions. The live loop owns a function-local
            // `WorkerRegistry` and runs this phase each pass (see `run_with`);
            // headless has no such loop-local state, so it keeps its registry on
            // the `App` and processes it here. We spawn newly-requested workers
            // and, while any worker is still active, block briefly for its
            // completion — so a "type a city → background fetch → update Static"
            // demo reaches a settled, deterministic result by the time the pump
            // returns to idle, instead of leaving the request un-run.
            if self.headless_process_workers(root, pending) {
                progressed = true;
            }

            // Animation frame (advances active animations toward completion).
            //
            // Only count it as progress when the frame actually produced an
            // update. Under the manual clock (run_test) time does not advance
            // within a single pump, so a still-active animation that yields no
            // new value this instant must NOT keep the pump spinning — otherwise
            // the loop would burn MAX_ITERATIONS until `advance_clock` moves time
            // forward. (On the wall clock, `clock_now()` always advances, so an
            // active animation keeps producing frames and progress, as before.)
            if self.animator.has_animations() {
                let mut anim_outcome = self.dispatch_animation_frame(root);
                if anim_outcome.repaint_requested
                    || anim_outcome.invalidation.layout
                    || anim_outcome.invalidation.style
                    || anim_outcome.invalidation.content
                    || !anim_outcome.messages.is_empty()
                {
                    progressed = true;
                }
                self.absorb_outcome(&mut anim_outcome, pending, InvalidationScope::Global);
            }

            // Binding-hints changed (footer/help refresh).
            let mut binding_outcome = self.dispatch_binding_hints_changed(root);
            if binding_outcome.invalidation.layout
                || binding_outcome.invalidation.style
                || binding_outcome.repaint_requested
                || !binding_outcome.messages.is_empty()
            {
                progressed = true;
            }
            self.absorb_outcome(&mut binding_outcome, pending, InvalidationScope::Global);

            // Reactive phase — drain widget-level runtime reactive entries
            // (those enqueued via `enqueue_runtime_reactive_entry`, e.g. a custom
            // widget bumping its own reactive in `on_message`/`on_button_pressed`),
            // running each node's watchers/recompose. The live event loop runs
            // this every pass (see `run_event_loop_reactive_phase` in `run_with`);
            // without it here, headless/Pilot dispatch enqueues the entry but
            // never fires the widget's `watch_*`, so widget-reactive demos looked
            // dead.
            if crate::reactive::runtime_reactive_queue_is_nonempty()
                || crate::runtime::commands::command_queue_is_nonempty()
            {
                progressed = true;
                self.run_event_loop_reactive_phase(root, pending);
            }

            // Re-sync the docked ToastRack when the notification store changed —
            // a `notify`, or a `NotificationExpired` removal (from an elapsed
            // rack-owned auto-dismiss timer or a toast click) just dispatched in
            // the reactive phase above. Mirrors the live loop's sweep so headless/
            // Pilot runs converge identically. `refresh_toast_rack` only reports
            // progress when a rack exists and it consumed the store, so this
            // cannot spin when no rack is mounted.
            if self.notifications_dirty && self.refresh_toast_rack() {
                progressed = true;
                pending.request_full_content();
            }

            // Style transitions + display-tree sync + recompositions.
            if pending.flags.style || pending.flags.layout || self.style_snapshot_cache.is_empty() {
                self.dispatch_style_transition_requests(root);
            }
            if let Some(tree) = self.active_widget_tree_mut()
                && sync_widget_controlled_child_display_tree(tree, root)
            {
                progressed = true;
                pending.request_flags(crate::event::InvalidationFlags::layout());
                pending.request_full_content();
            }
            self.absorb_pending_recompositions(pending);

            // Drain tree lifecycle events produced by the recompose above (and
            // any mount commands): dispatch Mount/Unmount and fire the
            // widget-owned `on_mount_ctx` hook, via the SAME shared drain the
            // live loop runs each tick. Without this, a widget mounted through a
            // DYNAMIC RECOMPOSE never gets `on_mount_ctx` headlessly, so its
            // `set_interval` timers never register (the last live-vs-headless
            // mount divergence). A fresh mount counts as progress so the pump
            // iterates again, letting the just-registered timer fire on the next
            // `advance_clock`. `absorb_outcome` records the sticky headless stop
            // flag internally, so an exit-on-mount handler is still observed.
            if self.drain_tree_lifecycle_events(root, pending).progressed {
                progressed = true;
            }

            self.absorb_pending_query_refreshes(pending);
            if self.take_pending_force_relayout() {
                pending.request_flags(crate::event::InvalidationFlags::layout());
                pending.request_full_content();
            }

            // Python parity: focus must leave a widget this pass just hid (see
            // `reset_focus_on_hidden`). The live loop gets this at the end of
            // its per-iteration reactive phase; the pump only runs that phase
            // when queues are non-empty (a class op is applied inline by
            // `absorb_outcome`), so sweep explicitly.
            self.reset_focus_on_hidden(pending);

            // Render if dirty.
            if pending.is_dirty() || self.resized_since_last_render {
                progressed = true;
                let regions = pending
                    .content_regions
                    .as_render_regions(self.frame.width, self.frame.height);
                let layout_invalidation = pending.flags.layout
                    || pending.flags.style
                    || self.resized_since_last_render;
                self.render_widget_with_regions(root, regions.as_deref(), layout_invalidation)?;
                self.apply_layout_info_to_tree();
                *pending = PendingInvalidation::default();
            }

            if !progressed {
                break;
            }
        }
        Ok(())
    }

    /// Headless worker phase: spawn newly-requested workers into the App-owned
    /// [`WorkerRegistry`], deterministically await any still-running worker, and
    /// route the resulting [`WorkerStateChanged`] messages through the runtime.
    ///
    /// Returns `true` if any work progressed (a request was spawned or a state
    /// change was routed), so the pump loop keeps iterating until idle.
    ///
    /// Determinism: worker jobs run on OS threads (as in production), but a
    /// headless test must reach a settled frame before the pump returns. So
    /// while any worker remains active we block — bounded by a wall-clock
    /// deadline — polling for completions. This is the one place the headless
    /// harness touches the wall clock; it is confined to "wait for the
    /// background thread I just spawned to finish," which a real `pilot.pause`
    /// would also observe in Python (its event loop yields until the worker's
    /// awaitable resolves).
    fn headless_process_workers(
        &mut self,
        root: &mut dyn Widget,
        pending: &mut PendingInvalidation,
    ) -> bool {
        let pending_workers = drain_accumulated_worker_requests();
        let has_new = !pending_workers.is_empty();

        // Lazily create the registry the first time a worker is requested, and
        // register this (the test) thread as the UI thread so the spawned
        // worker's `App::call_from_thread` posts are serviced. Done here (not at
        // startup) so worker-free `run_test` runs never touch the process-global
        // `call_from_thread` bridge — avoiding contention with other tests that
        // share the singleton.
        if has_new && self.headless_worker_registry.is_none() {
            self.headless_worker_registry = Some(WorkerRegistry::new());
            if !self.headless_ui_thread_registered {
                crate::runtime::tasks::register_ui_thread();
                self.headless_ui_thread_registered = true;
            }
        }

        let Some(mut registry) = self.headless_worker_registry.take() else {
            // No registry and nothing requested → nothing to do.
            return false;
        };

        let mut progressed = false;

        // Spawn newly-requested workers and collect any synchronous changes
        // (e.g. exclusive-mode cancellations) plus completions already ready.
        let mut changes = process_worker_requests(&mut registry, pending_workers);
        if has_new {
            progressed = true;
        }

        // Deterministically drive workers toward quiescence: while any worker is
        // still active, run `call_from_thread` jobs it posts and collect its
        // state changes. We do NOT block indefinitely for a worker — a worker
        // can deliberately *park* (e.g. `push_screen_wait`, which suspends the
        // worker on the UI until a screen is dismissed). So we stop waiting once
        // the worker has gone quiescent: no `call_from_thread` job ran and no
        // state change landed within a short grace window while the worker stays
        // active. Fast workers (weather) complete within the window; parked
        // workers (questions01) yield control back to the pump so the test body
        // can drive the next interaction (the click that dismisses the screen).
        const WORKER_WAIT_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);
        const QUIESCENCE_GRACE: std::time::Duration = std::time::Duration::from_millis(25);
        let deadline = Instant::now() + WORKER_WAIT_BUDGET;
        let mut last_activity = Instant::now();
        while !registry.active_workers().is_empty() && Instant::now() < deadline {
            let mut activity = false;

            // Run any `call_from_thread` jobs the worker posted while we wait —
            // otherwise a worker that calls `App::call_from_thread` (weather04/05,
            // and the screen push in questions01) would deadlock against this
            // spin, each waiting on the other.
            for job in crate::runtime::tasks::drain_call_from_thread_jobs() {
                job(self);
                pending.request_full_content();
                activity = true;
            }

            let mut batch = registry.drain_state_changes();
            if !batch.is_empty() {
                changes.append(&mut batch);
                activity = true;
            }

            if activity {
                last_activity = Instant::now();
            } else if last_activity.elapsed() >= QUIESCENCE_GRACE {
                // Worker still active but idle (parked waiting on the UI, or
                // simply slow between posts): hand control back to the pump.
                break;
            } else {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        // Final non-blocking sweep for any completions that landed last.
        changes.extend(registry.drain_state_changes());

        if !changes.is_empty() {
            progressed = true;
            let worker_messages = worker_state_runtime_messages(&registry, changes);
            let mut worker_outcome =
                self.dispatch_message_queue_with_runtime(root, worker_messages);
            // `absorb_outcome` records a sticky `headless_stop_requested` if the
            // worker-driven handler requested app exit.
            self.absorb_outcome(&mut worker_outcome, pending, InvalidationScope::Global);
        }

        registry.cleanup();

        // Keep the registry only if it still tracks workers; otherwise drop it
        // so a fully-idle app does not carry empty worker state.
        if !registry.active_workers().is_empty() {
            self.headless_worker_registry = Some(registry);
        }

        progressed
    }

    /// Deliver `count` frame ticks to the widget tree, then pump to idle.
    ///
    /// Mirrors the live loop's per-frame `root.on_tick(tick)` / `on_app_tick`
    /// (which the headless pump otherwise never fires). Each tick uses a
    /// strictly-increasing counter (`headless_tick`), so on-tick-driven
    /// animations (LoadingIndicator's spinner phase, button flash, …) advance
    /// frame-by-frame deterministically. This is the headless analogue of the
    /// real loop ticking once per `tick_rate`.
    pub(crate) fn headless_advance_ticks(
        &mut self,
        root: &mut dyn Widget,
        count: u64,
    ) -> crate::Result<()> {
        let mut pending = PendingInvalidation::default();
        for _ in 0..count {
            self.headless_tick = self.headless_tick.wrapping_add(1);
            let tick = self.headless_tick;

            // Widget-level tick. `root.on_tick` reaches the AppRoot wrapper, but
            // the widgets that actually animate (LoadingIndicator, …) live in the
            // arena tree, so deliver the tick to every active arena node too.
            root.on_tick(tick);
            if let Some(tree) = self.active_widget_tree_mut() {
                deliver_frame_tick(tree, tick);
            }
            // Opt-in: inactive screens tick too (mirrors the live loop).
            self.deliver_background_screen_ticks(tick);
            // Any on-tick widget repaint is content-level; request it so the
            // pump re-renders and the rendered frame reflects the new phase.
            pending.request_full_content();

            // App-level tick hook (mirrors the live loop's `on_app_tick`).
            let mut app_tick_ctx = EventCtx::default();
            {
                let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut app_tick_ctx);
                root.on_app_tick(self, tick, &mut __wctx);
                __wctx.__enqueue_reactive_if_dirty();
            }
            let mut app_tick_outcome = DispatchOutcome::from_event_ctx(&mut app_tick_ctx);
            self.absorb_outcome(&mut app_tick_outcome, &mut pending, InvalidationScope::Global);
            let mut msg_outcome =
                self.dispatch_message_queue_with_runtime(root, app_tick_outcome.messages);
            self.absorb_outcome(&mut msg_outcome, &mut pending, InvalidationScope::Global);
        }
        self.headless_pump(root, &mut pending)
    }

    /// Inject a single key press through the same dispatch cascade the live
    /// event loop uses, then pump to idle. Mirrors `pilot.press`.
    pub(crate) fn headless_inject_key(
        &mut self,
        root: &mut dyn Widget,
        key: KeyEvent,
    ) -> crate::Result<()> {
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return Ok(());
        }
        let mut pending = PendingInvalidation::default();
        self.with_headless_style_context(|app| {
            app.headless_process_key(root, key, &mut pending)
        });
        self.headless_pump(root, &mut pending)
    }

    /// Full key cascade (app key hook → priority action → command palette →
    /// declarative BINDINGS → raw key dispatch → action-map), mirroring the
    /// `CrosstermEvent::Key` arm of `run_widget_tree` without instrumentation.
    fn headless_process_key(
        &mut self,
        root: &mut dyn Widget,
        key: KeyEvent,
        pending: &mut PendingInvalidation,
    ) {
        let key = KeyEventData::from_crossterm(key);

        // App-level key hook.
        let mut app_key_ctx = EventCtx::default();
        {
            let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut app_key_ctx);
            root.on_app_key(self, &key, &mut __wctx);
            __wctx.__enqueue_reactive_if_dirty();
        }
        if app_key_ctx.repaint_requested() {
            pending.request_full_content();
        }
        pending.request_flags(app_key_ctx.invalidation());
        let app_key_handled = app_key_ctx.handled();
        let app_key_messages = app_key_ctx.take_messages();
        if !app_key_messages.is_empty() {
            let mut msg_outcome = self.dispatch_message_queue_with_runtime(root, app_key_messages);
            self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
        }
        let app_key_class_ops = app_key_ctx.take_class_ops();
        if !app_key_class_ops.is_empty() {
            if let Some(tree) = self.active_widget_tree_mut() {
                for (node, op) in app_key_class_ops {
                    match op {
                        crate::event::ClassOp::Add(c) => tree.add_class(node, &c),
                        crate::event::ClassOp::Remove(c) => tree.remove_class(node, &c),
                    }
                }
            }
            // Class change may flip descendant display/visibility; relayout so
            // the affected subtree re-resolves CSS.
            pending.request_flags(crate::event::InvalidationFlags::layout());
        }
        if app_key_handled {
            return;
        }

        let bind = crate::event::KeyBind::from_event(&key);
        let mapped_action = self.action_map.lookup(&bind);

        // Priority actions (command palette) before raw key dispatch.
        if let Some(action) = mapped_action.filter(|a| is_priority_action(*a)) {
            // Wave 1: ctrl+p opens the composed CommandPaletteScreen via the
            // adapter, NOT via Action::CommandPalette to the legacy host.
            let mut outcome = if matches!(action, Action::CommandPalette) {
                self.dispatch_command_palette_open(root)
            } else {
                self.dispatch_event_auto(root, Event::Action(action))
            };
            self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
            let mut msg_outcome =
                self.dispatch_message_queue_with_runtime(root, outcome.messages);
            self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
            if outcome.handled || matches!(action, Action::CommandPalette) {
                return;
            }
        }

        // Declarative BINDINGS: active chain (focused→root, or screen-body root
        // when unfocused) plus App::BINDINGS beneath an active screen.
        let mut binding_clashes = Vec::new();
        let binding_match = self.active_widget_tree().and_then(|tree| {
            let root_target = tree.root().unwrap_or_default();
            match_binding_chain(
                tree,
                self.app_root_tree_when_screen_active(),
                &key,
                self.check_action_fn.as_deref(),
                &self.keymap,
                Some(&mut binding_clashes),
            )
            .map(|(node_id, action_str, source)| (node_id, action_str, source, root_target))
        });
        // Deliver keymap clash reports after the tree borrow ends (per
        // clashing keypress, Python cadence).
        self.deliver_binding_clashes(&binding_clashes);
        if let Some((binding_node_id, action_str, binding_source, root_target)) = binding_match
            && let Ok(parsed) = crate::action::parse_action(&action_str)
        {
            // CLUSTER 7: execute the binding on its source node when no registry
            // owner resolves (binding source IS target).
            if binding_source == BindingSource::Active
                && let Some(tree_mut) = self.active_widget_tree_mut()
            {
                let focused = focused_node_id_tree(tree_mut);
                let resolved = {
                    let tree_ref = &*tree_mut;
                    focused.and_then(|fid| {
                        crate::action::resolve_action(&parsed, tree_ref, fid, |nid| {
                            tree_ref
                                .get(nid)
                                .map(|n| (n.widget.action_namespace(), n.widget.action_registry()))
                        })
                    })
                };
                let target = resolved.map(|ra| ra.node).unwrap_or(binding_node_id);
                if let Some(node) = tree_mut.get_mut(target) {
                    let mut ctx = EventCtx::default();
                    let handled = execute_action_with_dispatch_target(
                        &mut *node.widget,
                        &parsed,
                        &mut ctx,
                        target,
                    );
                    if handled || ctx.handled() {
                        let mut binding_outcome = DispatchOutcome {
                            handled: handled || ctx.handled(),
                            repaint_requested: ctx.repaint_requested(),
                            invalidation: ctx.invalidation(),
                            stop_requested: ctx.stop_requested(),
                            messages: ctx.take_messages(),
                            animation_requests: ctx.take_animation_requests(),
                            style_animation_requests: ctx.take_style_animation_requests(),
                            worker_requests: ctx.take_worker_requests(),
                            recompose_nodes: ctx.take_recompose_nodes(),
                            default_prevented: false,
                            class_ops: ctx.take_class_ops(),
                        };
                        self.absorb_outcome(&mut binding_outcome, pending, InvalidationScope::Global);
                        let messages = binding_outcome.messages;
                        if !messages.is_empty() {
                            let mut msg_outcome =
                                self.dispatch_message_queue_with_runtime(root, messages);
                            self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
                        }
                        return;
                    }
                }
            }

            let mut root_ctx = EventCtx::default();
            let handled =
                execute_action_with_dispatch_target(root, &parsed, &mut root_ctx, root_target);
            if handled || root_ctx.handled() {
                let mut root_binding_outcome = DispatchOutcome {
                    handled: handled || root_ctx.handled(),
                    repaint_requested: root_ctx.repaint_requested(),
                    invalidation: root_ctx.invalidation(),
                    stop_requested: root_ctx.stop_requested(),
                    messages: root_ctx.take_messages(),
                    animation_requests: root_ctx.take_animation_requests(),
                    style_animation_requests: root_ctx.take_style_animation_requests(),
                    worker_requests: root_ctx.take_worker_requests(),
                    recompose_nodes: root_ctx.take_recompose_nodes(),
                    default_prevented: false,
                    class_ops: root_ctx.take_class_ops(),
                };
                self.absorb_outcome(&mut root_binding_outcome, pending, InvalidationScope::Global);
                let messages = root_binding_outcome.messages;
                if !messages.is_empty() {
                    let mut msg_outcome =
                        self.dispatch_message_queue_with_runtime(root, messages);
                    self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
                }
                return;
            }

            // App-defined custom action fallback.
            let mut fallback_ctx = EventCtx::default();
            {
                let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut fallback_ctx);
                root.on_app_unhandled_action(self, &action_str, &mut __wctx);
                __wctx.__enqueue_reactive_if_dirty();
            }
            if fallback_ctx.handled() {
                let mut fallback_outcome = DispatchOutcome {
                    handled: true,
                    repaint_requested: fallback_ctx.repaint_requested(),
                    invalidation: fallback_ctx.invalidation(),
                    stop_requested: fallback_ctx.stop_requested(),
                    messages: fallback_ctx.take_messages(),
                    animation_requests: fallback_ctx.take_animation_requests(),
                    style_animation_requests: fallback_ctx.take_style_animation_requests(),
                    worker_requests: fallback_ctx.take_worker_requests(),
                    recompose_nodes: fallback_ctx.take_recompose_nodes(),
                    default_prevented: false,
                    class_ops: fallback_ctx.take_class_ops(),
                };
                self.absorb_outcome(&mut fallback_outcome, pending, InvalidationScope::Global);
                let messages = fallback_outcome.messages;
                if !messages.is_empty() {
                    let mut msg_outcome =
                        self.dispatch_message_queue_with_runtime(root, messages);
                    self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
                }
                return;
            }

            // The binding matched but no layer handled its action: report the
            // silent no-op (debug channel + test-observable buffer) before the
            // key falls through to raw dispatch.
            report_unhandled_binding_action(binding_node_id, &action_str);
        }

        // Raw key dispatch so focused widgets (Input etc.) can consume it.
        let mut key_outcome = self.dispatch_event_auto(root, Event::Key(key.clone()));
        self.absorb_outcome(&mut key_outcome, pending, InvalidationScope::Global);
        let mut msg_outcome =
            self.dispatch_message_queue_with_runtime(root, key_outcome.messages);
        self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
        if key_outcome.handled {
            return;
        }

        // Action-map fallback (non-priority).
        if let Some(action) = mapped_action.filter(|a| !is_priority_action(*a)) {
            if matches!(action, Action::FocusNext | Action::FocusPrev) {
                let mut focus_outcome = self.dispatch_event_auto(root, Event::Action(action));
                self.absorb_outcome(&mut focus_outcome, pending, InvalidationScope::Global);
                let mut focus_msg_outcome =
                    self.dispatch_message_queue_with_runtime(root, focus_outcome.messages);
                self.absorb_outcome(&mut focus_msg_outcome, pending, InvalidationScope::Global);
                if focus_outcome.handled {
                    return;
                }
                if self.move_focus_auto(action) {
                    pending.request_full_content();
                    return;
                }
            }
            let mut outcome = if is_scroll_action(action) {
                self.dispatch_scroll_action_auto(root, action, self.hovered)
            } else {
                self.dispatch_event_auto(root, Event::Action(action))
            };
            self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
            let mut msg_outcome =
                self.dispatch_message_queue_with_runtime(root, outcome.messages);
            self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
        }
    }

    /// Inject a mouse click (down + up at a screen coordinate) through the same
    /// path the live loop uses (which synthesizes a `Click` from the click
    /// tracker), then pump to idle. Mirrors `pilot.click`.
    pub(crate) fn headless_inject_click(
        &mut self,
        root: &mut dyn Widget,
        screen_x: u16,
        screen_y: u16,
    ) -> crate::Result<()> {
        let mut pending = PendingInvalidation::default();
        self.with_headless_style_context(|app| {
            app.headless_process_mouse_down(root, screen_x, screen_y, &mut pending);
            app.headless_process_mouse_up(root, screen_x, screen_y, &mut pending);
        });
        self.headless_pump(root, &mut pending)
    }

    fn headless_process_mouse_down(
        &mut self,
        root: &mut dyn Widget,
        screen_x: u16,
        screen_y: u16,
        pending: &mut PendingInvalidation,
    ) {
        if let Some(target) = self.widget_at_auto(screen_x, screen_y) {
            let (x, y) = self.content_local_coords_auto(target, screen_x, screen_y);
            self.click_tracker
                .on_mouse_down(target, x, y, screen_x, screen_y, 0);
            // Python `Screen._forward_event` (MouseDown): focus the nearest
            // focusable widget under the pointer BEFORE forwarding the event.
            let focus_target = self
                .active_widget_tree()
                .and_then(|tree| crate::runtime::helpers::focusable_node_for_click(tree, target));
            if let Some(focus_target) = focus_target
                && self.set_focus_node(focus_target)
            {
                pending.request_full_content();
            }
            let down_event = Event::MouseDown(MouseDownEvent {
                target,
                screen_x,
                screen_y,
                x,
                y,
            });
            let mut outcome = self.dispatch_event_to_target_auto(root, target, &down_event);
            self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
            let mut msg_outcome =
                self.dispatch_message_queue_with_runtime(root, outcome.messages);
            self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
        }
    }

    fn headless_process_mouse_up(
        &mut self,
        root: &mut dyn Widget,
        screen_x: u16,
        screen_y: u16,
        pending: &mut PendingInvalidation,
    ) {
        let down_target = self.click_tracker.down_target();
        let target = self.widget_at_auto(screen_x, screen_y);
        let (x, y) = target
            .map(|id| self.content_local_coords_auto(id, screen_x, screen_y))
            .unwrap_or((screen_x, screen_y));
        let up_event = Event::MouseUp(MouseUpEvent {
            target,
            screen_x,
            screen_y,
            x,
            y,
        });
        if let Some(capture_target) = down_target.filter(|id| Some(*id) != target) {
            let (cx, cy) = self.content_local_coords_auto(capture_target, screen_x, screen_y);
            let capture_up = Event::MouseUp(MouseUpEvent {
                target: Some(capture_target),
                screen_x,
                screen_y,
                x: cx,
                y: cy,
            });
            let mut capture_outcome =
                self.dispatch_event_to_target_auto(root, capture_target, &capture_up);
            self.absorb_outcome(&mut capture_outcome, pending, InvalidationScope::Global);
            let mut capture_msg_outcome =
                self.dispatch_message_queue_with_runtime(root, capture_outcome.messages);
            self.absorb_outcome(&mut capture_msg_outcome, pending, InvalidationScope::Global);
        }

        let mut outcome = if let Some(target) = target {
            self.dispatch_event_to_target_auto(root, target, &up_event)
        } else {
            self.dispatch_event_auto(root, up_event)
        };
        self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);

        if let Some((click_target, click_event)) =
            self.click_tracker.on_mouse_up(target, x, y, screen_x, screen_y)
        {
            let mut click_outcome =
                self.dispatch_event_to_target_auto(root, click_target, &click_event);
            let click_stopped = click_outcome.stop_requested;
            self.absorb_outcome(&mut click_outcome, pending, InvalidationScope::Global);

            // `@click` action-link routing (mirrors the live loop's
            // `MouseEventKind::Up` arm): consult the style meta baked into the
            // clicked cell. If a `[@click=...]` span stamped an action string
            // there, dispatch it with the clicked widget as the default action
            // namespace — so headless clicks on action-link spans (actions03's
            // `app.set_background('red')`) fire the action, not just MouseUp/Click.
            if !click_stopped
                && let Some(action) = self.click_action_at(screen_x, screen_y)
            {
                let msg = MessageEvent::new(
                    click_target,
                    crate::message::ActionDispatchRequested { action },
                );
                let mut action_outcome =
                    self.dispatch_message_queue_with_runtime(root, vec![msg]);
                self.absorb_outcome(&mut action_outcome, pending, InvalidationScope::Global);
            }

            let mut click_msg_outcome =
                self.dispatch_message_queue_with_runtime(root, click_outcome.messages);
            self.absorb_outcome(&mut click_msg_outcome, pending, InvalidationScope::Global);
        }
        let mut msg_outcome =
            self.dispatch_message_queue_with_runtime(root, outcome.messages);
        self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
    }

    /// Inject a mouse move to absolute screen `(x, y)` through the same cascade
    /// the live loop runs on a `MouseEventKind::Moved`: update hover + dispatch
    /// Enter/Leave, arm/update the system tooltip, then dispatch a `MouseMove`
    /// to the widget under the cursor. Mirrors `pilot.hover` / `pilot.move_to`.
    pub(crate) fn headless_inject_mouse_move(
        &mut self,
        root: &mut dyn Widget,
        screen_x: u16,
        screen_y: u16,
    ) -> crate::Result<()> {
        let mut pending = PendingInvalidation::default();
        self.with_headless_style_context(|app| {
            app.headless_process_mouse_move(root, screen_x, screen_y, &mut pending);
        });
        self.headless_pump(root, &mut pending)
    }

    fn headless_process_mouse_move(
        &mut self,
        root: &mut dyn Widget,
        screen_x: u16,
        screen_y: u16,
        pending: &mut PendingInvalidation,
    ) {
        // Hover transition (drives `:hover` pseudo + Enter/Leave events), exactly
        // as the live `MouseEventKind::Moved` arm does.
        let before = self.hovered;
        if self.update_hover_from_frame(screen_x, screen_y, root) {
            if let Some(id) = before {
                pending.request_widget_rect(&self.hit_test, id);
            }
            if let Some(id) = self.hovered {
                pending.request_widget_rect(&self.hit_test, id);
            } else {
                pending.request_full_content();
            }
            let enter_leave = generate_enter_leave_events(
                before, self.hovered, screen_x, screen_y, screen_x, screen_y,
            );
            for (target, event) in enter_leave {
                let mut outcome = self.dispatch_event_to_target_auto(root, target, &event);
                self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
                let mut msg_outcome =
                    self.dispatch_message_queue_with_runtime(root, outcome.messages);
                self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
            }
        }

        // Arm/refresh the shared system tooltip for the hovered owner.
        if self.update_hover_tooltip(screen_x, screen_y) {
            pending.request_flags(crate::event::InvalidationFlags::layout());
            pending.request_full_content();
        }

        // Dispatch the MouseMove to the widget under the cursor (the demo-facing
        // event: mouse01's RichLog write + ball offset both fire here).
        if let Some(target) = self.widget_at_auto(screen_x, screen_y) {
            let changed = self.call_on_mouse_move_auto(root, target, screen_x, screen_y, false);
            let (x, y) = self.content_local_coords_auto(target, screen_x, screen_y);
            let move_event = Event::MouseMove(crate::event::MouseMoveEvent {
                target,
                screen_x,
                screen_y,
                x,
                y,
            });
            let mut outcome = self.dispatch_event_to_target_auto(root, target, &move_event);
            self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
            let mut msg_outcome =
                self.dispatch_message_queue_with_runtime(root, outcome.messages);
            self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
            if changed {
                pending.request_full_content();
            }
        }
    }

    /// Headless: advance to idle without injecting input. Mirrors `pilot.pause`.
    pub(crate) fn headless_pause(&mut self, root: &mut dyn Widget) -> crate::Result<()> {
        let mut pending = PendingInvalidation::default();
        self.headless_pump(root, &mut pending)
    }

    /// Headless: resize the virtual terminal, dispatch Resize, and pump to idle.
    pub(crate) fn headless_resize(
        &mut self,
        root: &mut dyn Widget,
        width: u16,
        height: u16,
    ) -> crate::Result<()> {
        self.headless_size = (width.max(1), height.max(1));
        self.refresh_size()?;
        root.on_resize(width, height);
        let mut pending = PendingInvalidation::default();
        let mut outcome = self.dispatch_event_auto(root, Event::Resize(width, height));
        self.absorb_outcome(&mut outcome, &mut pending, InvalidationScope::Global);
        let mut msg_outcome = self.dispatch_message_queue_with_runtime(root, outcome.messages);
        self.absorb_outcome(&mut msg_outcome, &mut pending, InvalidationScope::Global);
        pending.request_full_content();
        self.headless_pump(root, &mut pending)
    }

    /// Set the virtual terminal size used in headless (`run_test`) mode.
    /// Has no effect once the app is running in a real terminal.
    pub fn set_headless_size(&mut self, width: u16, height: u16) {
        self.headless_size = (width.max(1), height.max(1));
    }

    /// Screen-space rect `(x0, y0, x1, y1)` of a rendered node, from the
    /// hit-test map. Used by Pilot to target clicks at a selector's centre.
    pub fn node_screen_rect(&self, node: NodeId) -> Option<(u16, u16, u16, u16)> {
        self.hit_test.rect(node).map(|r| (r.x0, r.y0, r.x1, r.y1))
    }

    /// Whether any interaction dispatched under the headless pump has requested
    /// the app to stop (`ctx.request_stop()`), e.g. a "press a button to quit"
    /// demo. Sticky once set. The live loop breaks on stop, but the headless
    /// pump keeps running so the Pilot test body can read state — so this is the
    /// way to assert that an exit-on-interaction demo actually fired its handler
    /// (its rendered frame is otherwise unchanged). Test/Pilot helper.
    pub fn headless_stop_requested(&self) -> bool {
        self.headless_stop_requested
    }

    /// The currently rendered frame as plain text rows (one `String` per
    /// screen row, styling stripped; each row is padded/cropped to the frame
    /// width).
    ///
    /// Reads the same in-memory [`FrameBuffer`] as [`save_frame_svg`] and
    /// [`frame_fingerprint`], so it works in headless (`run_test`/Pilot) mode
    /// where nothing is written to a real terminal. Useful for dev tooling
    /// and test assertions on visible text.
    ///
    /// [`FrameBuffer`]: crate::render::FrameBuffer
    /// [`save_frame_svg`]: Self::save_frame_svg
    /// [`frame_fingerprint`]: Self::frame_fingerprint
    pub fn frame_plain_lines(&self) -> Vec<String> {
        self.frame.as_plain_lines()
    }

    /// The currently rendered frame as one plain-text string:
    /// [`frame_plain_lines`] joined with `\n`.
    ///
    /// [`frame_plain_lines`]: Self::frame_plain_lines
    pub fn frame_plain_text(&self) -> String {
        self.frame_plain_lines().join("\n")
    }

    /// A cheap fingerprint of the currently rendered frame (text + per-cell
    /// foreground/background). Two equal fingerprints mean visually identical
    /// frames; a change after input proves rendered output changed. Test/Pilot
    /// helper.
    pub fn frame_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.frame.width.hash(&mut hasher);
        self.frame.height.hash(&mut hasher);
        for y in 0..self.frame.height {
            for x in 0..self.frame.width {
                let cell = self.frame.get(x, y);
                cell.text.hash(&mut hasher);
                if let Some(style) = cell.style {
                    if let Some(bg) = style.bgcolor {
                        format!("{bg:?}").hash(&mut hasher);
                    }
                    if let Some(fg) = style.color {
                        format!("{fg:?}").hash(&mut hasher);
                    }
                }
            }
        }
        hasher.finish()
    }

    /// Export the currently rendered frame as a "rich terminal" SVG file.
    ///
    /// Reads the same in-memory [`FrameBuffer`] that [`frame_fingerprint`]
    /// hashes, so it works in headless (`run_test`/Pilot) mode where nothing is
    /// written to a real terminal — the Rust analogue of Python Textual's
    /// `App.save_screenshot` / `take_svg_screenshot` doc-screenshot path.
    ///
    /// [`frame_fingerprint`]: Self::frame_fingerprint
    pub fn save_frame_svg(&self, path: &str, title: &str) -> crate::Result<()> {
        struct FrameSegments(rich_rs::Segments);
        impl rich_rs::Renderable for FrameSegments {
            fn render(
                &self,
                _console: &rich_rs::Console,
                _options: &rich_rs::ConsoleOptions,
            ) -> rich_rs::Segments {
                self.0.clone()
            }
        }

        // `FrameBuffer::to_segments` emits one segment per cell; merge
        // consecutive same-style runs (never across row separators) so the
        // exported SVG stays compact (per-cell segments inflate it ~40x).
        let mut runs: Vec<rich_rs::Segment> = Vec::new();
        for seg in self.frame.to_segments() {
            let cell = seg.control.is_none() && seg.text != "\n";
            match runs.last_mut() {
                Some(prev)
                    if cell
                        && prev.control.is_none()
                        && prev.text != "\n"
                        && prev.style == seg.style =>
                {
                    prev.text = format!("{}{}", prev.text, seg.text).into();
                }
                _ => runs.push(seg),
            }
        }
        let mut merged = rich_rs::Segments::new();
        merged.extend(runs);

        let mut console = rich_rs::Console::new_with_record();
        {
            let options = console.options_mut();
            options.size = (self.frame.width, self.frame.height);
            options.max_width = self.frame.width;
            options.max_height = self.frame.height;
            console.sync_from_options();
        }
        console.print(&FrameSegments(merged), None, None, None, false, "")?;
        console.save_svg(path, title, None, true, 0.61, None)?;
        Ok(())
    }

    /// The explicit inline background color of a tree node, if any.
    ///
    /// Mirrors reading `widget.styles.background` in Python Textual — the value
    /// set via `query_mut(sel).set_styles(|s| s.set_bg(..))`. Used by
    /// Pilot-driven tests to assert state the way Python's `test_rgb` does.
    pub fn node_explicit_bg(&self, node: NodeId) -> Option<crate::style::Color> {
        self.active_widget_tree()
            .and_then(|tree| tree.get(node))
            .and_then(|n| n.styles.style.bg)
    }

    /// Headless unmount + finish (returns the app to a clean state).
    pub(crate) fn headless_finish(&mut self, root: &mut dyn Widget) -> crate::Result<()> {
        root.on_unmount();
        // Cancel any still-active headless workers so a leftover background
        // thread can't post into a torn-down app, then unregister the UI thread
        // (dropping pending `call_from_thread` jobs, which unblocks any worker
        // still parked on `call_from_thread`) — but only if we registered it
        // (i.e. a worker was actually spawned). Mirrors `CallFromThreadGuard::drop`
        // in the live loop.
        self.headless_worker_registry = None;
        if self.headless_ui_thread_registered {
            crate::runtime::tasks::unregister_ui_thread();
            self.headless_ui_thread_registered = false;
        }
        self.finish()
    }

    /// Read the resolved background color of the rendered cell at `(x, y)` in the
    /// in-memory frame. Used by Pilot-driven tests to assert visible state
    /// (mirrors reading `app.screen.styles.background`).
    pub fn frame_cell_bg(&self, x: usize, y: usize) -> Option<crate::style::Color> {
        if x >= self.frame.width || y >= self.frame.height {
            return None;
        }
        self.frame
            .get(x, y)
            .style
            .and_then(|s| s.bgcolor)
            .map(crate::style::color_from_simple)
    }

    /// Read the resolved foreground color of the rendered cell at `(x, y)` in
    /// the in-memory frame (companion to [`App::frame_cell_bg`]). Used by
    /// Pilot-driven tests to assert visible text colour.
    pub fn frame_cell_fg(&self, x: usize, y: usize) -> Option<crate::style::Color> {
        if x >= self.frame.width || y >= self.frame.height {
            return None;
        }
        self.frame
            .get(x, y)
            .style
            .and_then(|s| s.color)
            .map(crate::style::color_from_simple)
    }

    pub(super) fn dispatch_binding_hints_changed(
        &mut self,
        root: &mut dyn Widget,
    ) -> DispatchOutcome {
        let (widget_hints, current_sources) = self.active_binding_hints_auto(root);
        let mut current = widget_hints;
        current.extend(self.binding_hints());
        self.apply_check_action(&mut current);
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
        let outcome = if let Some(tree) = self.active_widget_tree_mut() {
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
            style_animation_requests: {
                let mut requests = outcome.style_animation_requests;
                requests.extend(msg_outcome.style_animation_requests);
                requests
            },
            worker_requests: {
                let mut requests = outcome.worker_requests;
                requests.extend(msg_outcome.worker_requests);
                requests
            },
            class_ops: {
                let mut ops = outcome.class_ops;
                ops.extend(msg_outcome.class_ops);
                ops
            },
            recompose_nodes: {
                let mut nodes = outcome.recompose_nodes;
                nodes.extend(msg_outcome.recompose_nodes);
                nodes
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
        // Anchor to the timer clock so animations follow the manual clock under
        // `run_test` (deterministic) and the wall clock otherwise.
        let now = self.clock_now();
        self.animator.enqueue_many(requests, now);
    }

    pub(super) fn enqueue_style_animation_requests(
        &mut self,
        requests: Vec<StyleAnimationRequest>,
    ) {
        if requests.is_empty() {
            return;
        }
        let now = self.clock_now();
        self.animator.enqueue_style_many(requests, now);
    }

    pub(crate) fn absorb_outcome(
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
        // Record (stickily) that a stop was requested so headless/Pilot tests can
        // observe exit-on-interaction demos. The live loop breaks on stop; the
        // headless pump does not, so without this the request would be invisible.
        if outcome.stop_requested {
            self.headless_stop_requested = true;
        }
        let requests = std::mem::take(&mut outcome.animation_requests);
        self.enqueue_animation_requests(requests);
        let style_requests = std::mem::take(&mut outcome.style_animation_requests);
        self.enqueue_style_animation_requests(style_requests);
        let recompose_nodes = std::mem::take(&mut outcome.recompose_nodes);
        if !recompose_nodes.is_empty() {
            self.request_widget_recompose_nodes(&recompose_nodes);
        }
        let class_ops = std::mem::take(&mut outcome.class_ops);
        if !class_ops.is_empty() {
            if let Some(tree) = self.active_widget_tree_mut() {
                for (node, op) in class_ops {
                    match op {
                        ClassOp::Add(c) => tree.add_class(node, &c),
                        ClassOp::Remove(c) => tree.remove_class(node, &c),
                    }
                }
            }
            // A runtime class change can flip descendant `display`/`visibility`
            // and other layout-affecting CSS (e.g. stopwatch04's `.started #start
            // { display: none }`). Mirror Python `DOMNode.add_class`/`remove_class`
            // -> `_update_styles()` -> `refresh(layout=True)`: re-resolve CSS and
            // relayout so the affected subtree shows/hides. A bare repaint skips
            // `run_layout_pass` (which drives `apply_display_visibility_to_tree`),
            // leaving stale display state on screen.
            pending.request_flags(crate::event::InvalidationFlags::layout());
        }
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
        let affected = if let Some(tree) = self.active_widget_tree() {
            collect_stylesheet_affected_widgets_tree(
                tree,
                &reload.changed_rules,
                self.app_active,
                AppRuntimePseudos {
                    dark: self.dark_mode,
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
                    dark: self.dark_mode,
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

    fn absorb_pending_query_refreshes(&mut self, pending: &mut PendingInvalidation) {
        let queued = self.take_pending_query_refresh_nodes();
        if queued.is_empty() {
            return;
        }

        let mut missing_rect = false;
        for id in queued {
            if self.hit_test.rect(id).is_some() {
                pending.request_widget_rect(&self.hit_test, id);
            } else {
                missing_rect = true;
            }
        }

        if missing_rect {
            pending.request_full_content();
        }
    }

    fn absorb_pending_recompositions(&mut self, pending: &mut PendingInvalidation) {
        let queued = self.take_pending_recompose_nodes();
        if queued.is_empty() {
            return;
        }
        if let Some(tree) = self.active_widget_tree_mut() {
            for node_id in queued {
                if tree.contains(node_id) {
                    recompose_node_subtree(tree, node_id);
                }
            }
            pending.request_flags(crate::event::InvalidationFlags::layout());
            pending.request_full_content();
        }
    }

    fn collect_current_resolved_styles(
        &self,
        root: &dyn Widget,
    ) -> HashMap<NodeId, crate::style::Style> {
        let mut out = HashMap::new();
        if let Some(tree) = self.active_widget_tree() {
            if let Some(root_id) = tree.root() {
                for node_id in tree.walk_depth_first(root_id) {
                    if tree.get(node_id).is_none() {
                        continue;
                    }
                    let meta = crate::css::node_selector_meta(tree, node_id);
                    out.insert(
                        node_id,
                        crate::css::resolve_node_style(tree, node_id, &meta),
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
        if let Some(screen_sheet) = self.active_screen_stylesheet() {
            sheet.extend(screen_sheet);
        }
        let _active = set_app_active(self.app_active);
        let _pseudo_state = set_app_runtime_pseudos(AppRuntimePseudos {
            dark: self.dark_mode,
            inline: self.app_inline,
            ansi: self.app_ansi,
            nocolor: self.app_nocolor,
        });
        let _guard = set_style_context(sheet);

        let current_styles = self.collect_current_resolved_styles(root);
        let mut numeric_requests = Vec::new();
        let mut style_requests = Vec::new();
        for (node_id, current_style) in &current_styles {
            if let Some(previous_style) = self.style_snapshot_cache.get(node_id) {
                let (nr, sr) =
                    transition_requests_for_style_change(*node_id, previous_style, current_style);
                numeric_requests.extend(nr);
                style_requests.extend(sr);
            }
        }
        self.style_snapshot_cache = current_styles;
        self.enqueue_animation_requests(numeric_requests);
        self.enqueue_style_animation_requests(style_requests);
    }

    pub(super) fn dispatch_animation_frame(&mut self, root: &mut dyn Widget) -> DispatchOutcome {
        // Step against the timer clock so animation progress is deterministic
        // under `run_test` (driven by `advance_clock`) and wall-clock otherwise.
        let now = self.clock_now();
        let updates = self.animator.step(now, self.animation_level);
        let style_updates = self.animator.step_style(now, self.animation_level);

        if updates.is_empty() && style_updates.is_empty() {
            return DispatchOutcome::default();
        }

        // Apply style-value animation updates directly to node inline styles.
        // This mirrors Python Textual's CSSAnimation.animate() which temporarily sets
        // widget.styles.{property} = intermediate_value each tick.
        for style_update in style_updates {
            if let Some(tree) = self.active_widget_tree_mut() {
                tree.update_styles(style_update.target, |styles| {
                    apply_style_value_to_property(
                        &mut styles.style,
                        &style_update.property,
                        &style_update.value,
                    );
                });
            }
        }

        if updates.is_empty() {
            // Only style updates — request repaint without event dispatch.
            let mut aggregate = DispatchOutcome {
                repaint_requested: true,
                ..DispatchOutcome::default()
            };
            aggregate
                .invalidation
                .merge(crate::event::InvalidationFlags::content());
            return aggregate;
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
            aggregate
                .class_ops
                .extend(std::mem::take(&mut outcome.class_ops));
            aggregate
                .class_ops
                .extend(std::mem::take(&mut msg_outcome.class_ops));
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
    /// Drain the tree's pending `Mount`/`Unmount` lifecycle events and dispatch
    /// each: the `Mount`/`Unmount` event itself, and — for mounts — the
    /// widget-owned [`Widget::on_mount`] hook (which registers `set_interval`
    /// timers and posts any mount-time messages via `ctx.post_message`); for
    /// unmounts, purge the node's timers.
    ///
    /// Shared by the live loop (`run_widget_tree`) and the headless pump
    /// (`headless_pump`) so a widget mounted via **dynamic recompose** receives
    /// `on_mount_ctx` in BOTH paths — closing the last live-vs-headless mount
    /// divergence (initial mount is already dispatched by both loops; unmount is
    /// covered by the `get_mut`-None backstop). Extending this ONE function is
    /// what keeps the mount path from drifting between the two loops.
    ///
    /// Returns whether any event was drained (`progressed`, so the pump counts a
    /// freshly-registered timer as progress and iterates again) and whether a
    /// handler requested app exit (`stop_requested`). `absorb_outcome` also
    /// records the sticky headless stop flag, so the pump observes exit even
    /// though it does not break on the return value.
    fn drain_tree_lifecycle_events(
        &mut self,
        root: &mut dyn Widget,
        pending: &mut PendingInvalidation,
    ) -> LifecycleDrainOutcome {
        let lifecycle_events: Vec<(NodeId, bool)> = self
            .active_widget_tree_mut()
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
        let mut drain = LifecycleDrainOutcome::default();
        for (node_id, is_mount) in lifecycle_events {
            drain.progressed = true;
            let event = if is_mount {
                Event::Mount(MountEvent { node: node_id })
            } else {
                Event::Unmount(UnmountEvent { node: node_id })
            };
            let mut outcome = self.dispatch_event_to_target_auto(root, node_id, &event);
            self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
            let mut msg_outcome =
                self.dispatch_message_queue_with_runtime(root, outcome.messages);
            self.absorb_outcome(&mut msg_outcome, pending, InvalidationScope::Global);
            // Widget-owned mount hook (registers set_interval timers, posts any
            // mount-time messages via `ctx.post_message` — e.g. Select/ListView
            // initial selection) on mount; purge the node's timers on unmount.
            // The posted messages bubble through the shared flush's PostUp path.
            if is_mount {
                self.run_on_node_widget(node_id, |w, ctx| w.on_mount(ctx), pending);
            } else {
                self.purge_node_widget_timers(node_id);
            }
            if outcome.stop_requested || msg_outcome.stop_requested {
                // Match the live loop's early-out: stop processing further
                // lifecycle events once app exit is requested.
                drain.stop_requested = true;
                break;
            }
        }
        drain
    }

    /// Shared post-dispatch flush: drain the runtime reactive queue **and** the
    /// deferred [`WidgetCommand`](crate::runtime::commands) queue to convergence.
    ///
    /// Called from BOTH the live event loop (`run_widget_tree`) and the headless
    /// pump (`headless_pump`) — extending this ONE function is what keeps
    /// commands + reactivity converging identically in both paths (the
    /// loop-convergence keystone). Reactive dispatch or command application can
    /// enqueue further work, so the drain runs in rounds under a global budget
    /// (`MAX_REACTIVE_ITERATIONS`); a self-re-enqueueing handler hits the budget,
    /// gets its residue dropped, and is logged rather than hanging.
    ///
    /// With no commands pending this converges in a single round (nothing in
    /// `src/` enqueues a reactive entry from *within* reactive dispatch), so
    /// existing reactive behavior is unchanged.
    fn run_event_loop_reactive_phase(
        &mut self,
        root: &mut dyn Widget,
        pending: &mut PendingInvalidation,
    ) {
        for _round in 0..crate::reactive::MAX_REACTIVE_ITERATIONS {
            let queued = crate::reactive::take_runtime_reactive_entries();
            let commands = crate::runtime::commands::take_widget_commands();
            if queued.is_empty() && commands.is_empty() {
                break;
            }

            if !queued.is_empty() {
                self.dispatch_runtime_reactive_entries(queued, pending);
            }
            // Commands are applied in FIFO order after this round's reactive
            // entries (an add-class from a handler lands before the next render).
            for cmd in commands {
                self.apply_widget_command(cmd, pending);
            }
        }

        // PostUp: bubble any messages posted from update/timer/mount closures from
        // their originating node (sender is set on each `MessageEvent`). Handlers
        // they trigger may enqueue further reactive/commands, drained by the next
        // flush call (messages already defer in this design).
        if !self.pending_widget_posts.is_empty() {
            let posts = std::mem::take(&mut self.pending_widget_posts);
            let mut outcome = self.dispatch_message_queue_with_runtime(root, posts);
            self.absorb_outcome(&mut outcome, pending, InvalidationScope::Global);
        }

        // Budget exhausted with work still pending: a command/reactive cycle
        // (e.g. a handler that re-enqueues itself). Drain-and-drop the residue so
        // the queues cannot grow unbounded, and log loudly — mirroring
        // `run_reactive_phase_with_dispatch`'s per-node cycle diagnostics.
        if crate::reactive::runtime_reactive_queue_is_nonempty()
            || crate::runtime::commands::command_queue_is_nonempty()
        {
            let dropped_entries = crate::reactive::take_runtime_reactive_entries().len();
            let dropped_commands = crate::runtime::commands::take_widget_commands().len();
            crate::debug::debug_render(&format!(
                "[reactive-phase] command/reactive cycle: {} rounds exceeded; dropped {} \
                 reactive entr{} + {} command{} (likely a self-re-enqueueing handler)",
                crate::reactive::MAX_REACTIVE_ITERATIONS,
                dropped_entries,
                if dropped_entries == 1 { "y" } else { "ies" },
                dropped_commands,
                if dropped_commands == 1 { "" } else { "s" },
            ));
        }

        // Land any focus request deferred because its target was not yet
        // displayed when the `AppFocus` message routed (the class op that
        // reveals it was flushed above, in this same call). See
        // `retry_pending_focus`.
        self.retry_pending_focus(pending);

        // Python parity (`Widget._on_hide` -> `Screen._reset_focus`): if the
        // flush hid the focused widget (e.g. a class op flipped its CSS
        // `display`), hand focus to its first shown focusable sibling — before
        // the event loop's focus-transition detection so Blur/Focus dispatch
        // and the `:focus` styling repaint land in the same frame.
        self.reset_focus_on_hidden(pending);
    }

    /// Transfer focus away from a widget that the just-flushed reactive phase
    /// made non-displayed (or `visibility: hidden`). See
    /// [`crate::runtime::helpers::reset_focus_for_hidden_node`] for the Python
    /// `Screen._reset_focus` semantics.
    ///
    /// Gated on style/layout invalidation from this phase: `display` can only
    /// flip when styles or layout inputs changed, so idle loops pay nothing.
    fn reset_focus_on_hidden(&mut self, pending: &mut PendingInvalidation) {
        if !(pending.flags.style || pending.flags.layout) {
            return;
        }
        {
            let Some(tree) = self.active_widget_tree() else {
                return;
            };
            // Raw focus state, NOT `focused_node_id_tree` — the latter filters
            // out hidden nodes, which is precisely the state being repaired.
            if crate::runtime::helpers::raw_focused_node_id(tree).is_none() {
                return;
            }
        }
        // CSS display resolution needs the app's style context installed. The
        // live loop's `set_style_context` guards are scoped to the input/tick
        // dispatch blocks and are NOT active by the time the reactive phase
        // runs, so install one here (`with_headless_style_context` is the
        // general "app stylesheet + runtime pseudo state" installer, despite
        // the name).
        let changed = self.with_headless_style_context(|app| {
            let Some(tree) = app.active_widget_tree_mut() else {
                return false;
            };
            // Refresh cached `display`/`visibility` from the now-applied
            // classes; the tree values are stale until the next layout pass.
            crate::css::apply_display_visibility_to_tree(tree);
            crate::runtime::helpers::reset_focus_for_hidden_node(tree)
        });
        if changed {
            pending.request_flags(crate::event::InvalidationFlags::layout());
            pending.request_full_content();
        }
    }

    /// Retry a focus request deferred by the `AppFocus` handler because its
    /// target existed but was not yet displayed when the message routed.
    ///
    /// A widget handler can, in one pass, reveal a `display: none` child (by
    /// adding a class that flips its CSS `display`) AND request focus of that
    /// child. The class op is deferred to the post-dispatch command flush (run
    /// by `run_event_loop_reactive_phase`, whose end calls this), but the
    /// generated `AppFocus` message routes DURING dispatch — before the flush —
    /// so `set_focus_node` sees a stale (`false`) cached `display` and rejects
    /// it. Here, after the flush has applied the reveal, we re-resolve CSS
    /// display so the target's cached `display` is fresh, then retry focus ONCE.
    ///
    /// Bounded + self-clearing: the request is `take`n (retried exactly once,
    /// never spins) and dropped whether it lands or not, so a target that never
    /// becomes displayed cannot leak into a later frame and steal focus. A
    /// successful focus flags a repaint so the new `:focus` styling paints.
    fn retry_pending_focus(&mut self, pending: &mut PendingInvalidation) {
        let Some(widget_id) = self.pending_focus.take() else {
            return;
        };
        // Refresh cached `display` from the now-applied classes so a just-
        // revealed target passes `set_focus_node`'s displayed gate. The next
        // render's `run_layout_pass` re-resolves anyway; doing it here only
        // when a focus is pending keeps the cost off the common path.
        if let Some(tree) = self.active_widget_tree_mut() {
            crate::css::apply_display_visibility_to_tree(tree);
        }
        match self.action_focus(&widget_id) {
            Ok(true) => {
                pending.request_flags(crate::event::InvalidationFlags::layout());
                pending.request_full_content();
            }
            _ => {
                // Give up silently: the target still isn't focusable/displayed
                // (e.g. an open raced a dismiss). Not an error.
                debug_input(&format!(
                    "[runtime] deferred app.focus gave up widget_id={widget_id:?} (target never displayed)"
                ));
            }
        }
    }

    /// Process one batch of runtime reactive entries in tree order (parents
    /// before children), then any nodes no longer in the tree in stable id
    /// order. Extracted from the flush so the rounds loop can call it per round.
    fn dispatch_runtime_reactive_entries(
        &mut self,
        queued: Vec<RuntimeReactiveEntry>,
        pending: &mut PendingInvalidation,
    ) {
        let mut by_node: std::collections::HashMap<NodeId, Vec<RuntimeReactiveEntry>> =
            std::collections::HashMap::new();
        for entry in queued {
            by_node.entry(entry.node_id()).or_default().push(entry);
        }

        if let Some(tree) = self.active_widget_tree() {
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
        let mut recompose_requested = false;
        let mut all_class_ops: Vec<(NodeId, ClassOp)> = Vec::new();
        let mut watcher_messages: Vec<crate::message::MessageEvent> = Vec::new();
        for mut entry in entries {
            // Dynamic watchers (Python `DOMNode.watch(obj, field, cb)`): fire for
            // the entry's initial changes BEFORE widget dispatch, while no tree
            // borrow is held (the callbacks receive `&mut App`). Only fields with a
            // registered watcher are processed; the changes are then re-recorded so
            // the widget's own `reactive_dispatch` still runs.
            let has_watcher = entry
                .pending_field_names()
                .iter()
                .any(|field| self.has_dynamic_watcher(node_id, field));
            if has_watcher {
                let changes = entry.take_pending_changes();
                for change in changes {
                    if self.has_dynamic_watcher(node_id, change.field_name) {
                        self.notify_dynamic_watchers(
                            node_id,
                            change.field_name,
                            change.new_value.as_ref(),
                        );
                    }
                    entry.record_change(change);
                }
            }

            // Take the widget out of the tree and dispatch with `&mut App`, so a
            // widget's `watch_with_app` watchers (which `query_one`/mutate sibling
            // nodes) run — matching Python widget watchers and the `data_bind`
            // fan-out path. `reactive_dispatch_with_app` defaults to plain
            // `reactive_dispatch`, so widgets with only `watch` fields are
            // unaffected. Previously this path called the no-app `reactive_dispatch`
            // only, silently dropping widget `watch_with_app` watchers (e.g.
            // set_reactive01's greeting Label, dynamic_watch's Counter Label).
            let dispatched = self.with_node_widget_taken_dyn(node_id, |widget, app| {
                entry.run_with_dispatch(|changes, ctx| {
                    if let Some(reactive_widget) = widget.reactive_widget() {
                        reactive_widget.reactive_dispatch_with_app(app, changes, ctx);
                    }
                })
            });
            let mut result = match dispatched {
                Some(r) => r,
                None => entry.run_without_dispatch(),
            };
            repaint_requested |= result.needs_repaint;
            layout_requested |= result.needs_layout;
            recompose_requested |= result.needs_recompose;
            if !result.class_ops.is_empty() {
                all_class_ops.append(&mut result.class_ops);
            }
            if !result.messages.is_empty() {
                watcher_messages.append(&mut result.messages);
            }
        }

        // Messages posted by watchers (Python `watch_*` → `self.post_message`)
        // bubble from their node in the PostUp step of the shared flush. Each
        // was already filtered/stamped against the prevent scopes re-activated
        // during watcher dispatch, so prevention spans the reactive
        // update→re-dispatch cycle.
        if !watcher_messages.is_empty() {
            self.pending_widget_posts.extend(watcher_messages);
        }

        if !all_class_ops.is_empty() {
            if let Some(tree) = self.active_widget_tree_mut() {
                for (node, op) in all_class_ops {
                    match op {
                        ClassOp::Add(c) => tree.add_class(node, &c),
                        ClassOp::Remove(c) => tree.remove_class(node, &c),
                    }
                }
            }
            // A class change can flip descendant display/visibility + other
            // layout-affecting CSS; re-resolve + relayout (Python
            // `add_class`/`remove_class` -> `refresh(layout=True)`).
            layout_requested = true;
        }

        // A recompose request rebuilds the node's subtree (Python
        // `reactive(recompose=True)`): re-run `compose()` and remount children.
        // This implies layout + repaint, which the recompose machinery handles.
        if recompose_requested {
            self.request_widget_recompose_nodes(&[node_id]);
            pending.request_flags(crate::event::InvalidationFlags::layout());
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
    // Runtime dispatch/layout/input are tree-driven. The root widget remains
    // outside the arena as an app hook bridge (capture/app-action/app-message),
    // but event routing to widgets always goes through the arena tree.
    // ===================================================================

    /// Move focus forward/backward in the tree focus chain.
    ///
    /// Returns `true` when focus changed.
    fn move_focus_auto(&mut self, action: Action) -> bool {
        let Some(tree) = self.active_widget_tree_mut() else {
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
            tree.set_focus_state(current, false);
        }
        if tree.contains(next) {
            tree.set_focus_state(next, true);
            return true;
        }
        false
    }

    fn ensure_runtime_tree(&mut self, root: &mut dyn Widget) {
        if self.active_widget_tree().is_none() {
            self.build_widget_tree(root);
        }
    }

    /// Dispatch an event through the arena tree.
    fn dispatch_event_auto(&mut self, root: &mut dyn Widget, event: Event) -> DispatchOutcome {
        self.ensure_runtime_tree(root);
        // ctrl+p dismisses the Header's command-palette tooltip (a Header feature,
        // independent of how the palette itself opens).
        let dismissed_tooltip = matches!(&event, Event::Action(Action::CommandPalette))
            && self.start_command_palette_tooltip_cooldown();

        // The root widget (e.g. TextualAppAdapter) is not mounted in the arena,
        // so app-level key capture on root must run before tree dispatch.
        let mut root_capture_ctx = EventCtx::default();
        if matches!(&event, Event::Key(..)) {
            {
                let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut root_capture_ctx);
                root.on_event_capture(&event, &mut __wctx);
                __wctx.__enqueue_reactive_if_dirty();
            }
            if root_capture_ctx.handled() {
                return DispatchOutcome {
                    handled: root_capture_ctx.handled(),
                    repaint_requested: root_capture_ctx.repaint_requested(),
                    invalidation: root_capture_ctx.invalidation(),
                    stop_requested: root_capture_ctx.stop_requested(),
                    messages: root_capture_ctx.take_messages(),
                    animation_requests: root_capture_ctx.take_animation_requests(),
                    style_animation_requests: root_capture_ctx.take_style_animation_requests(),
                    worker_requests: root_capture_ctx.take_worker_requests(),
                    recompose_nodes: root_capture_ctx.take_recompose_nodes(),
                    default_prevented: false,
                    class_ops: root_capture_ctx.take_class_ops(),
                };
            }
        }

        let mut outcome = {
            let tree = self.active_widget_tree_mut().expect("tree should exist");
            let focused = focused_node_id_tree(tree);
            dispatch_event_tree(tree, focused, &event)
        };

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

            let mut root_recompose_nodes = root_capture_ctx.take_recompose_nodes();
            if !root_recompose_nodes.is_empty() {
                root_recompose_nodes.extend(outcome.recompose_nodes);
                outcome.recompose_nodes = root_recompose_nodes;
            }
            let mut root_class_ops = root_capture_ctx.take_class_ops();
            if !root_class_ops.is_empty() {
                root_class_ops.extend(outcome.class_ops);
                outcome.class_ops = root_class_ops;
            }
        }

        // Root bridge for app-level behavior not mounted in the arena tree.
        if !outcome.handled
            && matches!(
                &event,
                Event::Action(_)
                    | Event::Key(_)
                    | Event::MouseDown(_)
                    | Event::MouseUp(_)
                    | Event::MouseScroll(_)
                    | Event::AppFocus(_)
            )
        {
            let mut root_event_ctx = EventCtx::default();
            {
                let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut root_event_ctx);
                root.on_event(&event, &mut __wctx);
                __wctx.__enqueue_reactive_if_dirty();
            }
            outcome.handled |= root_event_ctx.handled();
            outcome.repaint_requested |= root_event_ctx.repaint_requested();
            outcome.invalidation.merge(root_event_ctx.invalidation());
            outcome.stop_requested |= root_event_ctx.stop_requested();
            outcome.messages.extend(root_event_ctx.take_messages());
            outcome
                .animation_requests
                .extend(root_event_ctx.take_animation_requests());
            outcome
                .worker_requests
                .extend(root_event_ctx.take_worker_requests());
            outcome
                .recompose_nodes
                .extend(root_event_ctx.take_recompose_nodes());
            outcome.class_ops.extend(root_event_ctx.take_class_ops());
        }

        if !outcome.handled
            && let Event::Action(action) = &event
        {
            let mut app_action_ctx = EventCtx::default();
            {
                let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut app_action_ctx);
                root.on_app_action(self, *action, &mut __wctx);
                __wctx.__enqueue_reactive_if_dirty();
            }
            outcome.handled |= app_action_ctx.handled();
            outcome.repaint_requested |= app_action_ctx.repaint_requested();
            outcome.invalidation.merge(app_action_ctx.invalidation());
            outcome.stop_requested |= app_action_ctx.stop_requested();
            outcome.messages.extend(app_action_ctx.take_messages());
            outcome
                .animation_requests
                .extend(app_action_ctx.take_animation_requests());
            outcome
                .worker_requests
                .extend(app_action_ctx.take_worker_requests());
            outcome
                .recompose_nodes
                .extend(app_action_ctx.take_recompose_nodes());
            outcome.class_ops.extend(app_action_ctx.take_class_ops());
        }
        if dismissed_tooltip {
            outcome.repaint_requested = true;
            outcome
                .invalidation
                .merge(crate::event::InvalidationFlags::layout());
        }
        outcome
    }

    /// Dispatch an event to a specific target via the arena tree.
    fn dispatch_event_to_target_auto(
        &mut self,
        root: &mut dyn Widget,
        _target: NodeId,
        event: &Event,
    ) -> DispatchOutcome {
        self.ensure_runtime_tree(root);
        // ctrl+p dismisses the Header's command-palette tooltip (a Header feature,
        // independent of how the palette itself opens).
        let dismissed_tooltip = matches!(event, Event::Action(Action::CommandPalette))
            && self.start_command_palette_tooltip_cooldown();
        let tree = self.active_widget_tree_mut().expect("tree should exist");
        let mut outcome = dispatch_event_to_target_tree(tree, _target, event);
        if dismissed_tooltip {
            outcome.repaint_requested = true;
            outcome
                .invalidation
                .merge(crate::event::InvalidationFlags::layout());
        }
        outcome
    }

    /// Dispatch a scroll action via the arena tree.
    fn dispatch_scroll_action_auto(
        &mut self,
        root: &mut dyn Widget,
        action: Action,
        hovered: Option<NodeId>,
    ) -> DispatchOutcome {
        self.ensure_runtime_tree(root);
        let tree = self.active_widget_tree_mut().expect("tree should exist");
        dispatch_scroll_action_tree(tree, action, hovered)
    }

    /// Dispatch mouse scroll to a specific target via the arena tree.
    fn dispatch_mouse_scroll_to_target_auto(
        &mut self,
        root: &mut dyn Widget,
        _target: NodeId,
        delta_x: i32,
        delta_y: i32,
    ) -> DispatchOutcome {
        self.ensure_runtime_tree(root);
        let tree = self.active_widget_tree_mut().expect("tree should exist");
        dispatch_mouse_scroll_to_target_tree(tree, _target, delta_x, delta_y)
    }

    /// Dispatch a message queue via the arena tree.
    fn dispatch_message_queue_auto(
        &mut self,
        root: &mut dyn Widget,
        initial: Vec<MessageEvent>,
    ) -> DispatchOutcome {
        self.ensure_runtime_tree(root);
        let mut outcome = {
            let tree = self.active_widget_tree_mut().expect("tree should exist");
            dispatch_message_queue_tree(tree, initial.clone())
        };

        // Tree routing delivers to arena nodes, but the TextualApp adapter root
        // also hosts typed hooks (e.g. on_button_pressed). Forward top-level
        // messages to root so app hooks still run in tree mode.
        for message in initial {
            let mut ctx = EventCtx::default();
            // Re-activate the message's prevent-set snapshot around the root/app
            // hooks, exactly like the tree pump does per envelope (Python
            // `_dispatch_message`: `with self.prevent(*message._prevent):`).
            let prevent_frame = message.prevent_snapshot().to_vec();
            let _prevent_scope = crate::message::enter_prevent_scope(&prevent_frame);
            {
                let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
                root.on_message(&message, &mut __wctx);
                __wctx.__enqueue_reactive_if_dirty();
            }
            if !ctx.handled() {
                {
                    let mut __wctx = WidgetCtx::__from_dispatch(NodeId::default(), &mut ctx);
                    root.on_app_message(self, &message, &mut __wctx);
                    __wctx.__enqueue_reactive_if_dirty();
                }
            }
            outcome.handled |= ctx.handled();
            outcome.repaint_requested |= ctx.repaint_requested();
            outcome.invalidation.merge(ctx.invalidation());
            outcome.stop_requested |= ctx.stop_requested();
            outcome.messages.extend(ctx.take_messages());
            outcome
                .animation_requests
                .extend(ctx.take_animation_requests());
            outcome
                .style_animation_requests
                .extend(ctx.take_style_animation_requests());
            outcome.worker_requests.extend(ctx.take_worker_requests());
            outcome.class_ops.extend(ctx.take_class_ops());
        }
        outcome
    }

    /// Check whether any widget is active in the arena tree.
    fn any_widget_active_auto(&mut self, root: &mut dyn Widget) -> bool {
        self.ensure_runtime_tree(root);
        if let Some(tree) = self.active_widget_tree() {
            any_widget_active_tree(tree)
        } else {
            false
        }
    }

    /// Collect active binding hints from the arena tree.
    fn active_binding_hints_auto(
        &mut self,
        root: &mut dyn Widget,
    ) -> (Vec<crate::event::BindingHint>, Vec<NodeId>) {
        self.ensure_runtime_tree(root);
        let app_root = self.app_root_tree_when_screen_active();
        if let Some(tree) = self.active_widget_tree() {
            active_binding_hints_tree(tree, app_root, &self.keymap)
        } else {
            (Vec::new(), Vec::new())
        }
    }

    /// Get focused help metadata from the arena tree.
    fn focused_help_metadata_auto(&mut self, root: &mut dyn Widget) -> Option<(NodeId, String)> {
        self.ensure_runtime_tree(root);
        if let Some(tree) = self.active_widget_tree() {
            focused_help_metadata_tree(tree)
        } else {
            None
        }
    }

    /// Forward `on_mouse_move` via the arena tree.
    pub(super) fn call_on_mouse_move_auto(
        &mut self,
        root: &mut dyn Widget,
        _target: NodeId,
        x: u16,
        y: u16,
        capture_only: bool,
    ) -> bool {
        self.ensure_runtime_tree(root);
        if let Some(tree) = self.active_widget_tree_mut() {
            if capture_only {
                let (lx, ly) = tree_content_local_coords(tree, _target, x, y);
                if let Some(node) = tree.get_mut(_target) {
                    let _dispatch_guard = set_dispatch_recipient(_target, node.state);
                    node.widget.on_mouse_move(lx, ly)
                } else {
                    false
                }
            } else {
                call_on_mouse_move_tree(tree, _target, x, y)
            }
        } else {
            false
        }
    }

    /// Determine pointer shape for hover via tree data.
    pub(super) fn pointer_shape_for_hover_auto(
        &self,
        _root: &mut dyn Widget,
        hovered: Option<NodeId>,
    ) -> crate::driver::PointerShape {
        if let Some(tree) = self.active_widget_tree() {
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
        let mut sheet = self.default_stylesheet.clone();
        sheet.extend(&self.stylesheet);
        if let Some(screen_sheet) = self.active_screen_stylesheet() {
            sheet.extend(screen_sheet);
        }
        let _active = set_app_active(self.app_active);
        let _pseudo_state = set_app_runtime_pseudos(AppRuntimePseudos {
            dark: self.dark_mode,
            inline: self.app_inline,
            ansi: self.app_ansi,
            nocolor: self.app_nocolor,
        });
        let _guard = set_style_context(sheet);
        // Always drive widget on_layout from solved tree geometry, not from
        // painted hit-test bounds. Hit-test rects can exclude transparent /
        // overpainted areas and collapse scroll viewport state (e.g. AppRoot),
        // which then clips subsequent renders and causes visible flicker.
        if let Some(tree) = self.active_widget_tree_mut() {
            apply_layout_info_tree_from_layout_rects(tree);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClipboardBackend, collect_clipboard_runtime_messages_with_backend,
        collect_stylesheet_affected_widgets_root, focused_help_message, parse_simulated_key,
        set_overlay_modal_display_tree, should_dispatch_binding_hints,
        should_dispatch_focused_help, transition_requests_for_style_change,
    };
    use crate::App;
    use crate::action::{ActionDecl, ParsedAction, parse_action, resolve_action};
    use crate::css::StyleSheet;
    use crate::event::{Action, BindingHint, Event, EventCtx, MountEvent};
    use crate::keys::KeyEventData;
    use crate::message::MessageEvent;
    use crate::node_id::{NodeId, node_id_from_ffi};
    use crate::reactive::{
        ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget, enqueue_runtime_reactive_entry,
        take_runtime_reactive_entries,
    };
    use crate::style::{Offset, OffsetValue, PropertyTransition, Style, TransitionTiming};
    use crate::widgets::{AppRoot, BindingDecl, Widget};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rich_rs::{Console, ConsoleOptions, Segments};
    use std::collections::VecDeque;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn parse_simulated_key_ctrl_chord() {
        let key = parse_simulated_key("^p").expect("parse ^p");
        assert_eq!(key.code, KeyCode::Char('p'));
        assert_eq!(key.modifiers, KeyModifiers::CONTROL);
        assert_eq!(key.key, "ctrl+p");
    }

    #[test]
    fn parse_simulated_key_shift_tab() {
        let key = parse_simulated_key("shift+tab").expect("parse shift+tab");
        assert_eq!(key.code, KeyCode::Tab);
        assert_eq!(key.modifiers, KeyModifiers::SHIFT);
    }

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

    #[derive(Default)]
    struct RootBindingsHost {
        extracted: bool,
    }

    impl Widget for RootBindingsHost {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn bindings(&self) -> Vec<BindingDecl> {
            vec![BindingDecl::new("l", "show_tab('leto')", "Leto")]
        }

        fn compose(&mut self) -> crate::compose::ComposeResult {
            if self.extracted {
                Vec::new()
            } else {
                self.extracted = true;
                vec![crate::compose::ChildDecl::new(Box::new(FocusedProbe))]
            }
        }
    }

    struct FocusedProbe;

    impl Widget for FocusedProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn focusable(&self) -> bool {
            true
        }
    }

    struct SimulatedKeyBindingHost {
        hits_l: Arc<AtomicUsize>,
        hits_j: Arc<AtomicUsize>,
        hits_p: Arc<AtomicUsize>,
    }

    impl Widget for SimulatedKeyBindingHost {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn focusable(&self) -> bool {
            true
        }

        fn bindings(&self) -> Vec<BindingDecl> {
            vec![
                BindingDecl::new("l", "select('l')", "Leto"),
                BindingDecl::new("j", "select('j')", "Jessica"),
                BindingDecl::new("p", "select('p')", "Paul"),
            ]
        }

        fn action_registry(&self) -> &[ActionDecl] {
            const ACTIONS: &[ActionDecl] = &[ActionDecl {
                name: "select",
                namespace: "",
                description: "select key",
                default_binding: None,
            }];
            ACTIONS
        }

        fn execute_action(&mut self, action: &ParsedAction, ctx: &mut crate::event::WidgetCtx) -> bool {
            if action.name != "select" || action.arguments.len() != 1 {
                return false;
            }
            match action.arguments[0].as_str() {
                Some("l") => {
                    self.hits_l.fetch_add(1, Ordering::SeqCst);
                    ctx.set_handled();
                    true
                }
                Some("j") => {
                    self.hits_j.fetch_add(1, Ordering::SeqCst);
                    ctx.set_handled();
                    true
                }
                Some("p") => {
                    self.hits_p.fetch_add(1, Ordering::SeqCst);
                    ctx.set_handled();
                    true
                }
                _ => false,
            }
        }
    }

    struct RootHookProbe {
        key_hits: Arc<AtomicUsize>,
        action_hits: Arc<AtomicUsize>,
        app_action_hits: Arc<AtomicUsize>,
        message_hits: Arc<AtomicUsize>,
        app_message_hits: Arc<AtomicUsize>,
        app_tick_hits: Arc<AtomicUsize>,
        handle_key: bool,
        handle_action: bool,
        handle_message: bool,
    }

    impl Widget for RootHookProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_event_capture(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
            if matches!(event, Event::Key(..)) {
                self.key_hits.fetch_add(1, Ordering::SeqCst);
                if self.handle_key {
                    ctx.set_handled();
                }
            }
        }

        fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
            if matches!(event, Event::Action(..)) {
                self.action_hits.fetch_add(1, Ordering::SeqCst);
                if self.handle_action {
                    ctx.set_handled();
                }
            }
        }

        fn on_app_action(&mut self, _app: &mut App, _action: Action, ctx: &mut crate::event::WidgetCtx) {
            self.app_action_hits.fetch_add(1, Ordering::SeqCst);
            ctx.set_handled();
        }

        fn on_message(&mut self, _message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
            self.message_hits.fetch_add(1, Ordering::SeqCst);
            if self.handle_message {
                ctx.set_handled();
            }
        }

        fn on_app_message(&mut self, _app: &mut App, _message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
            self.app_message_hits.fetch_add(1, Ordering::SeqCst);
            ctx.set_handled();
        }

        fn on_app_tick(&mut self, _app: &mut App, _tick: u64, _ctx: &mut crate::event::WidgetCtx) {
            self.app_tick_hits.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct TreeEventProbe {
        capture_hits: Arc<AtomicUsize>,
    }

    impl Widget for TreeEventProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_event_capture(&mut self, event: &Event, _ctx: &mut crate::event::WidgetCtx) {
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
            capture_hits: Arc::clone(&tree_root_capture_hits),
        }));
        let tree_focused = tree.mount(
            tree_root,
            Box::new(TreeEventProbe {
                capture_hits: Arc::clone(&tree_focused_capture_hits),
            }),
        );
        tree.set_focus_state(tree_focused, true);

        let mut app = test_app_with_tree(tree);
        let mut runtime_root = RootHookProbe {
            key_hits: Arc::clone(&root_key_hits),
            action_hits: Arc::clone(&root_action_hits),
            app_action_hits: Arc::new(AtomicUsize::new(0)),
            message_hits: Arc::new(AtomicUsize::new(0)),
            app_message_hits: Arc::new(AtomicUsize::new(0)),
            app_tick_hits: Arc::new(AtomicUsize::new(0)),
            handle_key: false,
            handle_action: true,
            handle_message: false,
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
            capture_hits: Arc::clone(&tree_root_capture_hits),
        }));
        let tree_focused = tree.mount(
            tree_root,
            Box::new(TreeEventProbe {
                capture_hits: Arc::clone(&tree_focused_capture_hits),
            }),
        );
        tree.set_focus_state(tree_focused, true);

        let mut app = test_app_with_tree(tree);
        let mut runtime_root = RootHookProbe {
            key_hits: Arc::clone(&root_key_hits),
            action_hits: Arc::clone(&root_action_hits),
            app_action_hits: Arc::new(AtomicUsize::new(0)),
            message_hits: Arc::new(AtomicUsize::new(0)),
            app_message_hits: Arc::new(AtomicUsize::new(0)),
            app_tick_hits: Arc::new(AtomicUsize::new(0)),
            handle_key: true,
            handle_action: true,
            handle_message: false,
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
        let app_action_hits = Arc::new(AtomicUsize::new(0));
        let tree_capture_hits = Arc::new(AtomicUsize::new(0));

        let mut tree = crate::widget_tree::WidgetTree::new();
        let probe_root = tree.set_root(Box::new(TreeEventProbe {
            capture_hits: Arc::clone(&tree_capture_hits),
        }));
        tree.set_focus_state(probe_root, true);

        let mut app = test_app_with_tree(tree);
        let mut runtime_root = RootHookProbe {
            key_hits: Arc::clone(&root_key_hits),
            action_hits: Arc::clone(&root_action_hits),
            app_action_hits: Arc::clone(&app_action_hits),
            message_hits: Arc::new(AtomicUsize::new(0)),
            app_message_hits: Arc::new(AtomicUsize::new(0)),
            app_tick_hits: Arc::new(AtomicUsize::new(0)),
            handle_key: false,
            handle_action: true,
            handle_message: false,
        };

        let outcome = app.dispatch_event_auto(&mut runtime_root, Event::Action(Action::HelpQuit));

        assert_eq!(root_action_hits.load(Ordering::SeqCst), 1);
        assert_eq!(app_action_hits.load(Ordering::SeqCst), 0);
        assert!(outcome.handled);
        assert_eq!(root_key_hits.load(Ordering::SeqCst), 0);
        assert_eq!(tree_capture_hits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn dispatch_event_auto_tree_uses_app_action_hook_when_root_fallback_unhandled() {
        let root_action_hits = Arc::new(AtomicUsize::new(0));
        let app_action_hits = Arc::new(AtomicUsize::new(0));

        let mut tree = crate::widget_tree::WidgetTree::new();
        let probe_root = tree.set_root(Box::new(TreeEventProbe {
            capture_hits: Arc::new(AtomicUsize::new(0)),
        }));
        tree.set_focus_state(probe_root, true);

        let mut app = test_app_with_tree(tree);
        let mut runtime_root = RootHookProbe {
            key_hits: Arc::new(AtomicUsize::new(0)),
            action_hits: Arc::clone(&root_action_hits),
            app_action_hits: Arc::clone(&app_action_hits),
            message_hits: Arc::new(AtomicUsize::new(0)),
            app_message_hits: Arc::new(AtomicUsize::new(0)),
            app_tick_hits: Arc::new(AtomicUsize::new(0)),
            handle_key: false,
            handle_action: false,
            handle_message: false,
        };

        let outcome = app.dispatch_event_auto(&mut runtime_root, Event::Action(Action::HelpQuit));

        assert_eq!(root_action_hits.load(Ordering::SeqCst), 1);
        assert_eq!(app_action_hits.load(Ordering::SeqCst), 1);
        assert!(outcome.handled);
    }

    #[test]
    fn dispatch_event_auto_tree_runs_root_key_fallback_when_unhandled() {
        struct RootKeyFallbackProbe {
            on_event_key_hits: Arc<AtomicUsize>,
        }

        impl Widget for RootKeyFallbackProbe {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }

            fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
                if matches!(event, Event::Key(..)) {
                    self.on_event_key_hits.fetch_add(1, Ordering::SeqCst);
                    ctx.set_handled();
                }
            }
        }

        let mut tree = crate::widget_tree::WidgetTree::new();
        let probe_root = tree.set_root(Box::new(TreeEventProbe {
            capture_hits: Arc::new(AtomicUsize::new(0)),
        }));
        tree.set_focus_state(probe_root, true);

        let mut app = test_app_with_tree(tree);
        let key_hits = Arc::new(AtomicUsize::new(0));
        let mut runtime_root = RootKeyFallbackProbe {
            on_event_key_hits: Arc::clone(&key_hits),
        };

        let outcome = app.dispatch_event_auto(
            &mut runtime_root,
            Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::NONE,
            ))),
        );

        assert!(outcome.handled);
        assert_eq!(key_hits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn command_palette_action_dismisses_visible_system_tooltip_immediately() {
        struct TooltipHost;

        impl Widget for TooltipHost {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }

            fn tooltip(&self) -> Option<String> {
                Some("Open command palette".to_string())
            }
        }

        let mut tree = crate::widget_tree::WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let tooltip_owner = tree.mount(root_id, Box::new(TooltipHost));
        App::mount_system_tooltip(&mut tree, root_id);
        if let Some(node) = tree.get_mut(tooltip_owner) {
            node.layout_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 0,
                x1: 8,
                y1: 1,
            };
            node.content_rect = node.layout_rect;
        }

        let mut app = test_app_with_tree(tree);
        app.options.size = (80, 24);
        app.options.max_width = 80;
        app.options.max_height = 24;
        app.hovered = Some(tooltip_owner);
        assert!(app.update_hover_tooltip(1, 0));

        let tooltip_id = app
            .get_widget_by_id(crate::widgets::SYSTEM_TOOLTIP_STYLE_ID)
            .expect("system tooltip should exist");
        let visible_before = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .map(|node| node.runtime_display)
            .unwrap_or(false);
        assert!(visible_before, "precondition: tooltip should be visible");

        let mut runtime_root = AppRoot::new();
        let outcome =
            app.dispatch_event_auto(&mut runtime_root, Event::Action(Action::CommandPalette));
        assert!(
            outcome.repaint_requested,
            "opening command palette should request repaint when dismissing tooltip"
        );

        let visible_after = app
            .active_widget_tree()
            .and_then(|tree| tree.get(tooltip_id))
            .map(|node| node.runtime_display)
            .unwrap_or(true);
        assert!(
            !visible_after,
            "command palette open should dismiss tooltip immediately"
        );
        assert!(
            !app.update_hover_tooltip(1, 0),
            "command palette open should start a cooldown that suppresses immediate tooltip re-show"
        );
    }

    #[test]
    fn dispatch_message_queue_auto_calls_app_message_when_root_message_unhandled() {
        let mut app = super::App::new().expect("app should initialize");
        let mut runtime_root = RootHookProbe {
            key_hits: Arc::new(AtomicUsize::new(0)),
            action_hits: Arc::new(AtomicUsize::new(0)),
            app_action_hits: Arc::new(AtomicUsize::new(0)),
            message_hits: Arc::new(AtomicUsize::new(0)),
            app_message_hits: Arc::new(AtomicUsize::new(0)),
            app_tick_hits: Arc::new(AtomicUsize::new(0)),
            handle_key: false,
            handle_action: true,
            handle_message: false,
        };

        let outcome = app.dispatch_message_queue_auto(
            &mut runtime_root,
            vec![MessageEvent::new(
                node_id_from_ffi(7),
                crate::message::FooterBindingsUpdated { count: 0 },
            )],
        );

        assert_eq!(runtime_root.message_hits.load(Ordering::SeqCst), 1);
        assert_eq!(runtime_root.app_message_hits.load(Ordering::SeqCst), 1);
        assert!(outcome.handled);
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
    fn dispatch_binding_hints_changed_includes_root_bindings_in_tree_mode() {
        let mut app = App::new().expect("app should initialize");
        let mut root = RootBindingsHost::default();
        app.build_widget_tree(&mut root);
        assert!(app.widget_tree.is_some(), "tree mode should be active");

        let _ = app.dispatch_binding_hints_changed(&mut root);
        assert!(
            app.last_binding_hints
                .iter()
                .any(|hint| hint.key == "l" && hint.description == "Leto"),
            "root-declared app bindings should be present in computed binding hints"
        );
    }

    #[test]
    fn app_simulate_key_uses_binding_pipeline_before_action_map_fallback() {
        let hits_l = Arc::new(AtomicUsize::new(0));
        let hits_j = Arc::new(AtomicUsize::new(0));
        let hits_p = Arc::new(AtomicUsize::new(0));

        let mut tree = crate::widget_tree::WidgetTree::new();
        let host_root = tree.set_root(Box::new(SimulatedKeyBindingHost {
            hits_l: Arc::clone(&hits_l),
            hits_j: Arc::clone(&hits_j),
            hits_p: Arc::clone(&hits_p),
        }));
        tree.set_focus_state(host_root, true);

        let mut app = test_app_with_tree(tree);
        let mut runtime_root = StyleNode::new("RuntimeRoot");

        for key in ["j", "p", "l"] {
            let _outcome = app.dispatch_message_queue_with_runtime(
                &mut runtime_root,
                vec![
                    MessageEvent::new(
                        node_id_from_ffi(1),
                        crate::message::AppSimulateKey {
                            key: key.to_string(),
                        },
                    )
                    .with_control(node_id_from_ffi(1)),
                ],
            );
        }

        assert_eq!(hits_j.load(Ordering::SeqCst), 1, "j binding should fire");
        assert_eq!(hits_p.load(Ordering::SeqCst), 1, "p binding should fire");
        assert_eq!(hits_l.load(Ordering::SeqCst), 1, "l binding should fire");
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
        assert!(
            event
                .downcast_ref::<crate::message::HelpPanelFocusedHelpChanged>()
                .is_some_and(|m| m.source == source && m.markup == "## Source help")
        );
    }

    #[test]
    fn focused_help_message_emits_clear_payload() {
        let event = focused_help_message(None);
        assert_eq!(event.sender, node_id_from_ffi(0));
        assert!(event.is::<crate::message::HelpPanelFocusedHelpCleared>());
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
                MessageEvent::new(
                    node_id_from_ffi(1),
                    crate::message::TextEditClipboardCopyRequested {
                        text: "hello".to_string(),
                        cut: false,
                    },
                ),
                MessageEvent::new(
                    node_id_from_ffi(2),
                    crate::message::TextEditClipboardPasteRequested { target },
                ),
            ],
            &mut backend,
        );
        assert_eq!(clipboard.as_deref(), Some("hello"));
        assert_eq!(backend.copied, vec!["hello".to_string()]);
        assert_eq!(generated.len(), 1);
        assert!(
            generated[0]
                .downcast_ref::<crate::message::TextEditClipboardPaste>()
                .is_some_and(|m| m.target == target && m.text == "hello")
        );
    }

    #[test]
    fn clipboard_runtime_ignores_paste_without_buffered_text() {
        let target = node_id_from_ffi(7);
        let mut clipboard = None;
        let mut backend = StubClipboardBackend::default();
        let generated = collect_clipboard_runtime_messages_with_backend(
            &mut clipboard,
            &[MessageEvent::new(
                node_id_from_ffi(2),
                crate::message::TextEditClipboardPasteRequested { target },
            )],
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
            &[MessageEvent::new(
                node_id_from_ffi(2),
                crate::message::TextEditClipboardPasteRequested { target },
            )],
            &mut backend,
        );

        assert_eq!(clipboard.as_deref(), Some("system"));
        assert_eq!(generated.len(), 1);
        assert!(
            generated[0]
                .downcast_ref::<crate::message::TextEditClipboardPaste>()
                .is_some_and(|m| m.target == target && m.text == "system")
        );
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

        fn take_node_seed(&mut self) -> crate::widgets::NodeSeed {
            crate::widgets::NodeSeed {
                css_id: self.style_id.take(),
                classes: std::mem::take(&mut self.classes),
                styles: Default::default(),
            }
        }
    }

    /// Build a `WidgetTree` from a `StyleNode` hierarchy for testing.
    ///
    /// Applies each node's `focused` field as `set_focus_state` on the tree after
    /// mounting (Step 6: focus lives on the node record, not the widget).
    fn build_tree_from_style_node(node: StyleNode) -> (crate::widget_tree::WidgetTree, NodeId) {
        let mut tree = crate::widget_tree::WidgetTree::new();
        fn insert(
            tree: &mut crate::widget_tree::WidgetTree,
            mut node: StyleNode,
            parent: Option<NodeId>,
        ) -> NodeId {
            let focused = node.focused;
            let children = std::mem::take(&mut node.children);
            let id = if let Some(p) = parent {
                tree.mount(p, Box::new(node))
            } else {
                tree.set_root(Box::new(node))
            };
            if focused {
                tree.set_focus_state(id, true);
            }
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

        let (requests, style_requests) = transition_requests_for_style_change(target, &old, &new);
        assert!(style_requests.is_empty(), "opacity is a numeric property");
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

        let (requests, style_requests) = transition_requests_for_style_change(target, &old, &new);
        assert!(style_requests.is_empty(), "offset_y is a numeric property");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].attribute, "offset_y");
        assert_eq!(requests[0].start, 0.0);
        assert_eq!(requests[0].end, 6.0);
        assert_eq!(requests[0].duration, std::time::Duration::from_millis(120));
    }

    // ── CSS transition style-value property tests ─────────────────────

    #[test]
    fn p2g36_transition_emits_style_request_for_bg_change() {
        use crate::event::{AnimationEase, StyleValue};
        use crate::style::{Color, PropertyTransition, TransitionTiming};

        let target = node_id_from_ffi(200);
        let mut old = Style::new();
        old.bg = Some(Color::rgb(0, 0, 0));
        let mut new = Style::new();
        new.bg = Some(Color::rgb(255, 0, 0));
        new.transitions = Some(vec![PropertyTransition {
            property: "bg".to_string(),
            duration: std::time::Duration::from_millis(300),
            timing: TransitionTiming::Linear,
            delay: std::time::Duration::ZERO,
        }]);

        let (numeric, style) = transition_requests_for_style_change(target, &old, &new);

        assert!(
            numeric.is_empty(),
            "bg is a style-value property, not numeric"
        );
        assert_eq!(
            style.len(),
            1,
            "should emit one StyleAnimationRequest for bg"
        );
        assert_eq!(style[0].target, target);
        assert_eq!(style[0].property, "bg");
        assert_eq!(style[0].from, StyleValue::Color(Color::rgb(0, 0, 0)));
        assert_eq!(style[0].to, StyleValue::Color(Color::rgb(255, 0, 0)));
        assert_eq!(style[0].duration, std::time::Duration::from_millis(300));
        assert_eq!(style[0].ease, AnimationEase::Linear);
    }

    #[test]
    fn p2g36_transition_emits_style_request_for_fg_and_margin() {
        use crate::event::StyleValue;
        use crate::style::{Color, PropertyTransition, Spacing, TransitionTiming};

        let target = node_id_from_ffi(201);
        let mut old = Style::new();
        old.fg = Some(Color::rgb(10, 20, 30));
        old.margin = Some(Spacing::all(0));
        let mut new = Style::new();
        new.fg = Some(Color::rgb(100, 200, 255));
        new.margin = Some(Spacing::all(4));
        new.transitions = Some(vec![
            PropertyTransition {
                property: "fg".to_string(),
                duration: std::time::Duration::from_millis(200),
                timing: TransitionTiming::InOutCubic,
                delay: std::time::Duration::ZERO,
            },
            PropertyTransition {
                property: "margin".to_string(),
                duration: std::time::Duration::from_millis(150),
                timing: TransitionTiming::Linear,
                delay: std::time::Duration::ZERO,
            },
        ]);

        let (numeric, style) = transition_requests_for_style_change(target, &old, &new);
        assert!(numeric.is_empty());
        assert_eq!(style.len(), 2);

        let fg_req = style.iter().find(|r| r.property == "fg").unwrap();
        assert_eq!(fg_req.from, StyleValue::Color(Color::rgb(10, 20, 30)));
        assert_eq!(fg_req.to, StyleValue::Color(Color::rgb(100, 200, 255)));

        let margin_req = style.iter().find(|r| r.property == "margin").unwrap();
        assert_eq!(margin_req.from, StyleValue::Spacing(Spacing::all(0)));
        assert_eq!(margin_req.to, StyleValue::Spacing(Spacing::all(4)));
    }

    #[test]
    fn p2g36_transition_no_request_when_property_unchanged() {
        use crate::style::{Color, PropertyTransition, TransitionTiming};

        let target = node_id_from_ffi(202);
        let mut old = Style::new();
        old.bg = Some(Color::rgb(50, 50, 50));
        let mut new = Style::new();
        new.bg = Some(Color::rgb(50, 50, 50)); // same color
        new.transitions = Some(vec![PropertyTransition {
            property: "bg".to_string(),
            duration: std::time::Duration::from_millis(300),
            timing: TransitionTiming::Linear,
            delay: std::time::Duration::ZERO,
        }]);

        let (numeric, style) = transition_requests_for_style_change(target, &old, &new);
        assert!(numeric.is_empty());
        assert!(
            style.is_empty(),
            "identical values should not produce animation requests"
        );
    }

    #[test]
    fn p2g36_apply_style_value_to_property_sets_correct_fields() {
        use super::apply_style_value_to_property;
        use crate::event::StyleValue;
        use crate::style::{Color, Scalar, Spacing, Tint};

        let mut style = Style::new();

        // Color fields
        apply_style_value_to_property(&mut style, "bg", &StyleValue::Color(Color::rgb(1, 2, 3)));
        assert_eq!(style.bg, Some(Color::rgb(1, 2, 3)));

        apply_style_value_to_property(&mut style, "fg", &StyleValue::Color(Color::rgb(4, 5, 6)));
        assert_eq!(style.fg, Some(Color::rgb(4, 5, 6)));

        // Scalar fields
        apply_style_value_to_property(&mut style, "width", &StyleValue::Scalar(Scalar::Cells(42)));
        assert_eq!(style.width, Some(Scalar::Cells(42)));

        apply_style_value_to_property(
            &mut style,
            "height",
            &StyleValue::Scalar(Scalar::Percent(75.0)),
        );
        assert_eq!(style.height, Some(Scalar::Percent(75.0)));

        // Spacing fields
        let sp = Spacing {
            top: 2,
            right: 4,
            bottom: 2,
            left: 4,
        };
        apply_style_value_to_property(&mut style, "margin", &StyleValue::Spacing(sp));
        assert_eq!(style.margin, Some(sp));

        apply_style_value_to_property(&mut style, "padding", &StyleValue::Spacing(sp));
        assert_eq!(style.padding, Some(sp));

        // Tint field
        let tint = Tint::new(Color::rgb(255, 0, 0), 50);
        apply_style_value_to_property(&mut style, "tint", &StyleValue::Tint(tint));
        assert_eq!(style.tint, Some(tint));

        apply_style_value_to_property(&mut style, "background_tint", &StyleValue::Tint(tint));
        assert_eq!(style.background_tint, Some(tint));
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

        fn on_message(&mut self, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
            if let Some(m) = message.downcast_ref::<crate::message::WorkerStateChanged>() {
                match m.state {
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
        assert!(messages.iter().any(|event| {
            event.sender == owner_success
                && event
                    .downcast_ref::<crate::message::WorkerStateChanged>()
                    .is_some_and(|m| m.state == WorkerState::Success)
        }));
        assert!(messages.iter().any(|event| {
            event.sender == owner_error
                && event
                    .downcast_ref::<crate::message::WorkerStateChanged>()
                    .is_some_and(|m| m.state == WorkerState::Error("boom".into()))
        }));

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
        assert!(
            messages[0]
                .downcast_ref::<crate::message::WorkerStateChanged>()
                .is_some_and(|m| m.state == crate::worker::WorkerState::Cancelled)
        );
    }

    #[test]
    fn runtime_app_selector_messages_mutate_tree_and_request_layout_invalidation() {
        let mut tree = crate::widget_tree::WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        tree.mount(root_id, Box::new(crate::widgets::Button::new("go")));
        let mut app = test_app_with_tree(tree);
        let mut runtime_root = StyleNode::new("RuntimeRoot");

        let messages = vec![
            MessageEvent::new(
                node_id_from_ffi(1),
                crate::message::AppAddClass {
                    selector: "Button".to_string(),
                    class_name: "highlight".to_string(),
                },
            )
            .with_control(node_id_from_ffi(1)),
        ];
        let outcome = app.dispatch_message_queue_with_runtime(&mut runtime_root, messages);
        assert!(outcome.repaint_requested);
        assert!(outcome.invalidation.layout);
        let highlighted = app.query(".highlight").expect("selector parses");
        assert_eq!(highlighted.len(), 1);
    }

    struct ChainedAppSelectorEmitter;

    impl Widget for ChainedAppSelectorEmitter {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_message(&mut self, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
            if message.is::<crate::message::FooterBindingsUpdated>() {
                ctx.post_message(crate::message::AppAddClass {
                    selector: "Button".to_string(),
                    class_name: "from-chained-message".to_string(),
                });
                ctx.set_handled();
            }
        }
    }

    #[test]
    fn runtime_dispatch_processes_messages_emitted_during_message_handling() {
        let mut tree = crate::widget_tree::WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let emitter_id = tree.mount(root_id, Box::new(ChainedAppSelectorEmitter));
        tree.mount(root_id, Box::new(crate::widgets::Button::new("go")));
        let mut app = test_app_with_tree(tree);
        let mut runtime_root = StyleNode::new("RuntimeRoot");

        let initial = vec![
            MessageEvent::new(
                emitter_id,
                crate::message::FooterBindingsUpdated { count: 0 },
            )
            .with_control(emitter_id),
        ];
        let outcome = app.dispatch_message_queue_with_runtime(&mut runtime_root, initial);
        assert!(outcome.repaint_requested);
        assert!(outcome.invalidation.layout);
        let highlighted = app
            .query(".from-chained-message")
            .expect("selector should parse");
        assert_eq!(
            highlighted.len(),
            1,
            "messages emitted from on_message handlers must flow back through runtime control routing"
        );
    }

    struct FocusIdProbe {
        id: String,
    }

    impl FocusIdProbe {
        fn new(id: &str) -> Self {
            Self { id: id.to_string() }
        }
    }

    impl Widget for FocusIdProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn focusable(&self) -> bool {
            true
        }

        fn take_node_seed(&mut self) -> crate::widgets::NodeSeed {
            crate::widgets::NodeSeed {
                css_id: Some(self.id.clone()),
                classes: Vec::new(),
                styles: Default::default(),
            }
        }
    }

    #[test]
    fn app_blur_clears_tree_focus_and_remembers_last_focused_node() {
        let mut tree = crate::widget_tree::WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(root_id, Box::new(FocusIdProbe::new("first")));
        let _second = tree.mount(root_id, Box::new(FocusIdProbe::new("second")));
        tree.set_focus_state(first, true);

        let mut app = test_app_with_tree(tree);
        app.apply_app_blur_focus_state();

        assert!(!app.app_active);
        assert_eq!(app.last_focused_on_app_blur, Some(first));
        let first_focused = app
            .active_widget_tree()
            .and_then(|t| t.get(first))
            .map(|n| n.state.focused)
            .expect("first widget should exist");
        assert!(!first_focused);
    }

    #[test]
    fn app_focus_restores_blurred_focus_when_no_new_focus_exists() {
        let mut tree = crate::widget_tree::WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(root_id, Box::new(FocusIdProbe::new("first")));
        let second = tree.mount(root_id, Box::new(FocusIdProbe::new("second")));
        tree.set_focus_state(first, true);

        let mut app = test_app_with_tree(tree);
        app.apply_app_blur_focus_state();
        app.apply_app_focus_restore_state();

        assert!(app.app_active);
        assert_eq!(app.last_focused_on_app_blur, None);
        let first_focused = app
            .active_widget_tree()
            .and_then(|t| t.get(first))
            .map(|n| n.state.focused)
            .expect("first widget should exist");
        let second_focused = app
            .active_widget_tree()
            .and_then(|t| t.get(second))
            .map(|n| n.state.focused)
            .expect("second widget should exist");
        assert!(first_focused);
        assert!(!second_focused);
    }

    struct RuntimeModeScreen;

    impl crate::screen::Screen for RuntimeModeScreen {
        fn compose(&self) -> Box<dyn Widget> {
            Box::new(AppRoot::new())
        }
    }

    #[derive(Default)]
    struct HelpPanelMessageProbe {
        show_messages: usize,
        hide_messages: usize,
    }

    impl Widget for HelpPanelMessageProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_message(&mut self, message: &MessageEvent, _ctx: &mut crate::event::WidgetCtx) {
            if message.is::<crate::message::AppShowHelpPanel>() {
                self.show_messages += 1;
            } else if message.is::<crate::message::AppHideHelpPanel>() {
                self.hide_messages += 1;
            }
        }
    }

    #[test]
    fn runtime_help_panel_control_messages_are_delivered_to_widgets() {
        let mut app = App::new().expect("app runtime should initialize");
        let mut root = HelpPanelMessageProbe::default();
        let sender = node_id_from_ffi(1);
        let messages = vec![
            MessageEvent::new(sender, crate::message::AppShowHelpPanel).with_control(sender),
            MessageEvent::new(sender, crate::message::AppHideHelpPanel).with_control(sender),
        ];

        let _ = app.dispatch_message_queue_with_runtime(&mut root, messages);
        assert_eq!(root.show_messages, 1);
        assert_eq!(root.hide_messages, 1);
    }

    #[test]
    fn app_copy_selected_text_falls_back_to_help_quit_notification() {
        let mut app = App::new().expect("app runtime should initialize");
        let mut root = StyleNode::new("RuntimeRoot");
        let sender = node_id_from_ffi(1);

        let outcome = app.dispatch_message_queue_with_runtime(
            &mut root,
            vec![
                MessageEvent::new(sender, crate::message::AppCopySelectedText).with_control(sender),
            ],
        );

        assert!(outcome.repaint_requested);
        assert_eq!(app.notifications.len(), 1);
        let note = app.notifications.last().expect("help quit notification");
        assert_eq!(note.title, "Do you want to quit?");
        assert!(
            note.message.contains("Press"),
            "help quit notification should include quit guidance"
        );
    }

    #[test]
    fn runtime_app_action_messages_cover_non_selector_paths() {
        let mut tree = crate::widget_tree::WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(root_id, Box::new(FocusIdProbe::new("first")));
        let _second = tree.mount(root_id, Box::new(FocusIdProbe::new("second")));
        tree.set_focus_state(first, true);

        let mut app = test_app_with_tree(tree);
        // Prevent environment-dependent SIGTSTP suspension during this action-matrix
        // coverage test; we only need to verify routing/handling, not real process stop.
        app.set_suspend_process_impl_for_test(|| Ok(()));
        app.add_mode("home", || Box::new(RuntimeModeScreen));
        app.add_mode("main", || Box::new(RuntimeModeScreen));
        let mut runtime_root = StyleNode::new("RuntimeRoot");
        let screenshot_filename = format!(
            "textual-rs-test-{}.svg",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time after epoch")
                .as_nanos()
        );
        let screenshot_dir = std::env::temp_dir();
        let screenshot_path = screenshot_dir.join(&screenshot_filename);
        let screenshot_dir_str = screenshot_dir.to_string_lossy().to_string();

        let messages = vec![
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppBell)
                .with_control(node_id_from_ffi(1)),
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppChangeTheme)
                .with_control(node_id_from_ffi(1)),
            MessageEvent::new(
                node_id_from_ffi(1),
                crate::message::AppFocus {
                    widget_id: "second".to_string(),
                },
            )
            .with_control(node_id_from_ffi(1)),
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppFocusNext)
                .with_control(node_id_from_ffi(1)),
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppFocusPrevious)
                .with_control(node_id_from_ffi(1)),
            MessageEvent::new(
                node_id_from_ffi(1),
                crate::message::AppNotify {
                    message: "hello".to_string(),
                    title: "title".to_string(),
                    severity: "warning".to_string(),
                },
            )
            .with_control(node_id_from_ffi(1)),
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppHelpQuit)
                .with_control(node_id_from_ffi(1)),
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppCopySelectedText)
                .with_control(node_id_from_ffi(1)),
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppCommandPalette)
                .with_control(node_id_from_ffi(1)),
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppShowHelpPanel)
                .with_control(node_id_from_ffi(1)),
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppHideHelpPanel)
                .with_control(node_id_from_ffi(1)),
            MessageEvent::new(
                node_id_from_ffi(1),
                crate::message::AppPushScreen {
                    screen: "home".to_string(),
                },
            )
            .with_control(node_id_from_ffi(1)),
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppPopScreen)
                .with_control(node_id_from_ffi(1)),
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppBack)
                .with_control(node_id_from_ffi(1)),
            MessageEvent::new(
                node_id_from_ffi(1),
                crate::message::AppSwitchMode {
                    mode: "home".to_string(),
                },
            )
            .with_control(node_id_from_ffi(1)),
            MessageEvent::new(
                node_id_from_ffi(1),
                crate::message::AppSwitchScreen {
                    screen: "main".to_string(),
                },
            )
            .with_control(node_id_from_ffi(1)),
            MessageEvent::new(
                node_id_from_ffi(1),
                crate::message::AppScreenshot {
                    filename: Some(screenshot_filename.clone()),
                    path: Some(screenshot_dir_str.clone()),
                },
            )
            .with_control(node_id_from_ffi(1)),
            MessageEvent::new(
                node_id_from_ffi(1),
                crate::message::AppSimulateKey {
                    key: "tab".to_string(),
                },
            )
            .with_control(node_id_from_ffi(1)),
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppSuspendProcess)
                .with_control(node_id_from_ffi(1)),
            MessageEvent::new(node_id_from_ffi(1), crate::message::AppToggleDark)
                .with_control(node_id_from_ffi(1)),
        ];

        let outcome = app.dispatch_message_queue_with_runtime(&mut runtime_root, messages);
        assert!(outcome.repaint_requested);
        assert!(app.notifications.len() >= 3);
        let tree = app.widget_tree.as_ref().expect("tree should still exist");
        assert!(
            tree.walk_depth_first(tree.root().expect("root exists"))
                .into_iter()
                .any(|id| tree.get(id).is_some_and(|node| node.state.focused)),
            "one probe widget should stay focused"
        );
        assert!(app.screen_count() >= 1);
        assert!(screenshot_path.exists());
        let _ = std::fs::remove_file(&screenshot_path);
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

        fn execute_action(&mut self, action: &ParsedAction, ctx: &mut crate::event::WidgetCtx) -> bool {
            if action.name != "add_class" || action.arguments.len() != 2 {
                return false;
            }
            let (Some(selector), Some(class_name)) =
                (action.arguments[0].as_str(), action.arguments[1].as_str())
            else {
                return false;
            };
            ctx.post_message(crate::message::AppAddClass {
                selector: selector.to_string(),
                class_name: class_name.to_string(),
            });
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
        tree.set_focus_state(button, true);

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
            assert!({ let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); node.widget.execute_action(&parsed, &mut __w) });
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

    #[test]
    fn action_dispatch_requested_message_executes_routed_action() {
        let mut tree = crate::widget_tree::WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let app_node = tree.mount(root, Box::new(AppActionHost));
        let button = tree.mount(app_node, Box::new(crate::widgets::Button::new("ok")));
        tree.set_focus_state(button, true);

        let mut app = test_app_with_tree(tree);
        let mut runtime_root = StyleNode::new("RuntimeRoot");
        let outcome = app.dispatch_message_queue_with_runtime(
            &mut runtime_root,
            vec![
                MessageEvent::new(
                    button,
                    crate::message::ActionDispatchRequested {
                        action: "app.add_class('Button', 'from-action')".to_string(),
                    },
                )
                .with_control(button),
            ],
        );

        assert!(outcome.repaint_requested);
        assert!(outcome.invalidation.layout);
        let mutated = app.query(".from-action").expect("selector should parse");
        assert_eq!(mutated.len(), 1);
    }

    // Wave 1: `AppCommandPalette` is no longer consumed by the runtime to
    // dispatch `Action::CommandPalette` into the legacy always-mounted host — it
    // is DELIVERED to the adapter root's `on_app_message`, which builds + pushes
    // the composed `CommandPaletteScreen`. This test guards that delivery seam
    // (the old-host open path is now inert). The end-to-end ctrl+p → screen →
    // search flow is covered by `textual_app::tests::ctrl_p_opens_command_palette_screen_*`.
    #[test]
    fn app_command_palette_message_is_delivered_to_root_app_message_handler() {
        let mut app = super::App::new().expect("app should initialize");
        let mut root = RootHookProbe {
            key_hits: Arc::new(AtomicUsize::new(0)),
            action_hits: Arc::new(AtomicUsize::new(0)),
            app_action_hits: Arc::new(AtomicUsize::new(0)),
            message_hits: Arc::new(AtomicUsize::new(0)),
            app_message_hits: Arc::new(AtomicUsize::new(0)),
            app_tick_hits: Arc::new(AtomicUsize::new(0)),
            handle_key: false,
            handle_action: false,
            handle_message: false,
        };
        let app_message_hits = root.app_message_hits.clone();
        let sender = node_id_from_ffi(1);

        let _ = app.dispatch_message_queue_with_runtime(
            &mut root,
            vec![MessageEvent::new(sender, crate::message::AppCommandPalette).with_control(sender)],
        );

        assert_eq!(
            app_message_hits.load(Ordering::SeqCst),
            1,
            "AppCommandPalette must be delivered to the adapter root's on_app_message, not consumed by the runtime"
        );
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

        fn on_event(&mut self, event: &Event, _ctx: &mut crate::event::WidgetCtx) {
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
    fn apply_layout_info_to_tree_uses_tree_geometry_not_hit_test_bounds() {
        let mut tree = crate::widget_tree::WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let log_id = tree.mount(
            root_id,
            Box::new(crate::widgets::Log::new().auto_scroll(false)),
        );

        if let Some(node) = tree.get_mut(root_id) {
            node.layout_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 0,
                x1: 80,
                y1: 24,
            };
            node.content_rect = node.layout_rect;
        }
        if let Some(node) = tree.get_mut(log_id) {
            node.layout_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 1,
                x1: 69,
                y1: 23,
            };
            node.content_rect = node.layout_rect;
        }

        let mut app = test_app_with_tree(tree);
        // Simulate sparse painted metadata where the root only appears in a
        // narrow right-side strip. This must not collapse AppRoot viewport.
        app.hit_test.bounds.insert(
            root_id,
            crate::runtime::types::Rect {
                x0: 69,
                y0: 1,
                x1: 79,
                y1: 22,
            },
        );
        app.hit_test.bounds.insert(
            log_id,
            crate::runtime::types::Rect {
                x0: 0,
                y0: 1,
                x1: 11,
                y1: 22,
            },
        );

        app.apply_layout_info_to_tree();

        let viewport = app
            .active_widget_tree()
            .and_then(|tree| tree.get(root_id))
            .and_then(|node| node.widget.scroll_viewport_size())
            .expect("AppRoot viewport should be available");
        assert_eq!(
            viewport,
            (80, 24),
            "viewport must follow solved layout rects, not painted hit-test bounds",
        );
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
    fn reactive_phase_fires_dynamic_watcher_with_value() {
        // A dynamic watcher registered via App::watch_reactive on a node's field
        // fires during the reactive phase, receiving the new value — and the
        // widget's own watcher still runs.
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
        // Toggle sets `value` to 1, recording a "value" change.
        let _ =
            super::dispatch_event_to_target_tree(&mut tree, target, &Event::Action(Action::Toggle));

        let mut app = test_app_with_tree(tree);

        // Register a dynamic watcher on the probe's `value` field.
        let observed = Arc::new(AtomicUsize::new(usize::MAX));
        let observed_cb = Arc::clone(&observed);
        app.watch_reactive(target, "value", move |_app, value| {
            if let Some(v) = value.downcast_ref::<i32>() {
                observed_cb.store(*v as usize, Ordering::SeqCst);
            }
        });

        let mut pending = super::PendingInvalidation::default();
        let mut root = StyleNode::new("Root");
        app.run_event_loop_reactive_phase(&mut root, &mut pending);

        assert_eq!(
            observed.load(Ordering::SeqCst),
            1,
            "dynamic watcher fired with the new value (1)"
        );
        assert_eq!(
            watch_calls.load(Ordering::SeqCst),
            1,
            "widget's own reactive_dispatch still ran after dynamic-watcher notification"
        );
    }

    // -------------------------------------------------------------------
    // prevent(...) across the reactive update→re-dispatch cycle
    // -------------------------------------------------------------------

    #[derive(Debug, Clone)]
    struct WatcherPing;
    crate::impl_message!(WatcherPing);

    /// Reactive widget whose watcher posts `WatcherPing` on every `value`
    /// change (the `Switch.watch_value` → `Switch.Changed` shape).
    struct PreventPoster {
        value: bool,
    }

    impl PreventPoster {
        fn set_value(&mut self, value: bool, ctx: &mut ReactiveCtx) {
            if self.value != value {
                let old = self.value;
                self.value = value;
                ctx.record_change(
                    "value",
                    ReactiveFlags::reactive(),
                    Box::new(old),
                    Box::new(value),
                );
            }
        }
    }

    impl ReactiveWidget for PreventPoster {
        fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
            for change in changes {
                if change.field_name == "value" {
                    ctx.post_message(WatcherPing);
                }
            }
        }
    }

    impl Widget for PreventPoster {
        fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
            Some(self)
        }

        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }
    }

    /// Tree root that counts delivered `WatcherPing` messages.
    struct PingCounter {
        hits: Arc<AtomicUsize>,
    }

    impl Widget for PingCounter {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_message(
            &mut self,
            message: &crate::message::MessageEvent,
            _ctx: &mut crate::event::WidgetCtx,
        ) {
            if message.is::<WatcherPing>() {
                self.hits.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    /// REGRESSION (byte03 Input→Switch): `prevent(M)` must span the reactive
    /// update→re-dispatch cycle. A handler running inside a `prevent(M)` scope
    /// mutates a reactive via `Handle::update_in`; the widget's watcher — which
    /// posts `M` — runs LATER, in the runtime reactive phase, after the scope
    /// closed. Python keeps the prevention active there (the `ContextVar`
    /// prevent stack is live across the synchronous `_check_watchers`, and the
    /// snapshot rides on posted messages); Rust must too, or `M` leaks and the
    /// example needs a behavior-equivalent guard bool instead of the real
    /// `prevent` scope.
    #[test]
    fn prevent_scope_spans_reactive_update_redispatch_cycle() {
        let _ = take_runtime_reactive_entries();
        let hits = Arc::new(AtomicUsize::new(0));

        let mut tree = crate::widget_tree::WidgetTree::new();
        let root_id = tree.set_root(Box::new(PingCounter {
            hits: Arc::clone(&hits),
        }));
        let poster_id = tree.mount(root_id, Box::new(PreventPoster { value: false }));
        let handle = crate::handle::Handle::<PreventPoster>::resolve(&tree, poster_id)
            .expect("poster handle resolves");
        let mut app = test_app_with_tree(tree);

        // Handler shape: inside `ctx.prevent::<WatcherPing, _>(...)`,
        // programmatically update the reactive (byte03's `handle.update`).
        let mut ectx = crate::event::EventCtx::default();
        ectx.prevent::<WatcherPing, _>(|_ctx| {
            let tree = app.active_widget_tree_mut().expect("tree installed");
            handle
                .update_in(tree, |w, rctx| w.set_value(true, rctx))
                .expect("update_in succeeds");
        });

        // The prevent scope has exited; the watcher dispatch happens NOW, in
        // the deferred reactive phase — the captured snapshot must still apply.
        let mut pending = super::PendingInvalidation::default();
        let mut root = StyleNode::new("Root");
        app.run_event_loop_reactive_phase(&mut root, &mut pending);

        assert!(
            app.active_widget_tree()
                .and_then(|t| handle.read_in(t, |w| w.value).ok())
                .unwrap_or(false),
            "the reactive mutation itself still lands"
        );
        assert_eq!(
            hits.load(Ordering::SeqCst),
            0,
            "WatcherPing posted by the deferred watcher must be suppressed by the \
             prevent scope that was active when the reactive was mutated"
        );
    }

    /// Control: the same programmatic update WITHOUT a prevent scope delivers
    /// the watcher's message (this also pins the watcher→post→bubble plumbing
    /// the suppression test relies on, so it cannot pass vacuously).
    #[test]
    fn reactive_watcher_post_delivers_without_prevent_scope() {
        let _ = take_runtime_reactive_entries();
        let hits = Arc::new(AtomicUsize::new(0));

        let mut tree = crate::widget_tree::WidgetTree::new();
        let root_id = tree.set_root(Box::new(PingCounter {
            hits: Arc::clone(&hits),
        }));
        let poster_id = tree.mount(root_id, Box::new(PreventPoster { value: false }));
        let handle = crate::handle::Handle::<PreventPoster>::resolve(&tree, poster_id)
            .expect("poster handle resolves");
        let mut app = test_app_with_tree(tree);

        {
            let tree = app.active_widget_tree_mut().expect("tree installed");
            handle
                .update_in(tree, |w, rctx| w.set_value(true, rctx))
                .expect("update_in succeeds");
        }

        let mut pending = super::PendingInvalidation::default();
        let mut root = StyleNode::new("Root");
        app.run_event_loop_reactive_phase(&mut root, &mut pending);

        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "without prevent, the watcher-posted message bubbles to the root"
        );
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

    // ── WidgetCtx build, sub-step 1: deferred command queue + shared flush ──

    /// Enqueues an `AddClass` command on its own node when it handles `Toggle`,
    /// exactly as a real handler would (the command is applied later, by the
    /// shared flush — not in-place, since the tree is borrowed during dispatch).
    struct CommandProbeWidget;

    impl Widget for CommandProbeWidget {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            "CommandProbe"
        }

        fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
            if let Event::Action(Action::Toggle) = event {
                crate::runtime::commands::enqueue_widget_command(
                    crate::runtime::commands::WidgetCommand::AddClass {
                        target: crate::runtime::commands::CommandTarget::Node {
                            node: ctx.node_id(),
                            tree: crate::runtime::dispatch_ctx::dispatch_tree_id(),
                        },
                        class: "active".to_string(),
                    },
                );
            }
        }
    }

    /// A reactive widget whose dispatch re-enqueues a fresh entry for itself,
    /// so the shared flush's rounds loop never converges — used to prove the
    /// global round budget terminates (no hang) and drains the residue.
    struct CyclingReactiveWidget {
        fires: Arc<AtomicUsize>,
    }

    impl Widget for CyclingReactiveWidget {
        fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
            Some(self)
        }

        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            "Cycling"
        }
    }

    impl ReactiveWidget for CyclingReactiveWidget {
        fn reactive_dispatch(&mut self, _changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
            self.fires.fetch_add(1, Ordering::SeqCst);
            // Re-enqueue a fresh entry for the same node → the flush never
            // converges and must hit the round budget.
            let node_id = ctx.node_id();
            let mut rctx = ReactiveCtx::new(node_id);
            rctx.record_change(
                "v",
                ReactiveFlags::reactive(),
                Box::new(0i32),
                Box::new(1i32),
            );
            enqueue_runtime_reactive_entry(crate::reactive::RuntimeReactiveEntry::new(
                node_id, rctx,
            ));
        }
    }

    /// Live-loop path: the loop calls `run_event_loop_reactive_phase`
    /// unconditionally each iteration (event_loop.rs:3897). A command enqueued
    /// by a handler is deferred, then applied by that shared flush.
    #[test]
    fn widget_command_applied_by_flush_live_loop_path() {
        let _ = take_runtime_reactive_entries();
        let _ = crate::runtime::commands::take_widget_commands();

        let mut tree = crate::widget_tree::WidgetTree::new();
        let node = tree.set_root(Box::new(CommandProbeWidget));

        // Handler runs during dispatch and enqueues the command.
        let _ =
            super::dispatch_event_to_target_tree(&mut tree, node, &Event::Action(Action::Toggle));
        // Deferred: NOT applied yet (tree was borrowed during the handler).
        assert!(
            !tree.get(node).unwrap().classes.contains("active"),
            "command must be deferred, not applied in-place during dispatch"
        );

        let mut app = test_app_with_tree(tree);
        let mut pending = super::PendingInvalidation::default();
        let mut root = StyleNode::new("Root");
        app.run_event_loop_reactive_phase(&mut root, &mut pending);

        // Applied after the shared flush; layout invalidation requested.
        assert!(
            app.active_widget_tree()
                .unwrap()
                .get(node)
                .unwrap()
                .classes
                .contains("active"),
            "AddClass command applied by the shared flush"
        );
        assert!(pending.flags.layout, "class change requests relayout");
    }

    /// Headless path: the pump gate (event_loop.rs:4537) now also fires on a
    /// pending command, so a command enqueued by a handler drains through the
    /// same shared flush under `headless_pump` — no reactive entry required.
    #[test]
    fn widget_command_applied_by_flush_headless_pump_path() {
        let _ = take_runtime_reactive_entries();
        let _ = crate::runtime::commands::take_widget_commands();

        let mut tree = crate::widget_tree::WidgetTree::new();
        let node = tree.set_root(Box::new(CommandProbeWidget));
        let _ =
            super::dispatch_event_to_target_tree(&mut tree, node, &Event::Action(Action::Toggle));

        let mut app = test_app_with_tree(tree);
        let mut pending = super::PendingInvalidation::default();
        let mut root = StyleNode::new("Root");
        app.headless_pump(&mut root, &mut pending)
            .expect("headless pump should drain the command queue and settle");

        assert!(
            app.active_widget_tree()
                .unwrap()
                .get(node)
                .unwrap()
                .classes
                .contains("active"),
            "AddClass command drained by the headless pump via the shared flush"
        );
    }

    /// Cycle cap: a self-re-enqueueing actor hits the global round budget; the
    /// flush terminates (this test completing proves no hang), fires exactly the
    /// budget's worth of rounds, and drains the residue so the queue is empty.
    #[test]
    fn shared_flush_round_budget_terminates_on_cycle() {
        let _ = take_runtime_reactive_entries();
        let _ = crate::runtime::commands::take_widget_commands();

        let fires = Arc::new(AtomicUsize::new(0));
        let mut tree = crate::widget_tree::WidgetTree::new();
        let node = tree.set_root(Box::new(CyclingReactiveWidget {
            fires: Arc::clone(&fires),
        }));

        // Seed the first entry (as a handler would have).
        let mut rctx = ReactiveCtx::new(node);
        rctx.record_change(
            "v",
            ReactiveFlags::reactive(),
            Box::new(0i32),
            Box::new(1i32),
        );
        enqueue_runtime_reactive_entry(crate::reactive::RuntimeReactiveEntry::new(node, rctx));

        let mut app = test_app_with_tree(tree);
        let mut pending = super::PendingInvalidation::default();
        let mut root = StyleNode::new("Root");
        app.run_event_loop_reactive_phase(&mut root, &mut pending);

        // Bounded by the round budget, not unbounded (no hang).
        assert_eq!(
            fires.load(Ordering::SeqCst),
            crate::reactive::MAX_REACTIVE_ITERATIONS,
            "cycling reactive dispatch fires once per round, up to the budget"
        );
        // Residue drained so the queue cannot grow across future flushes.
        assert!(
            !crate::reactive::runtime_reactive_queue_is_nonempty(),
            "flush drains the residual entries on cycle detection"
        );
        assert!(!crate::runtime::commands::command_queue_is_nonempty());
    }

    // ── WidgetCtx build, sub-step 2: query_one + Handle::update_via ──

    /// Parent widget A (no reactive state) — the query root.
    struct ParentA;

    impl Widget for ParentA {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            "ParentA"
        }
    }

    /// Child widget B: a reactive widget whose watcher increments `watched`.
    /// `bump` records a change through the `WidgetCtx` (which `DerefMut`s to
    /// `ReactiveCtx`), exactly as a generated `set_*` setter would.
    struct ChildB {
        watched: Arc<AtomicUsize>,
        n: i32,
    }

    impl ChildB {
        fn bump(&mut self, ctx: &mut crate::event::WidgetCtx) {
            let old = self.n;
            self.n += 1;
            ctx.record_change(
                "n",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(self.n),
            );
        }
    }

    impl Widget for ChildB {
        fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
            Some(self)
        }

        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            "ChildB"
        }
    }

    impl ReactiveWidget for ChildB {
        fn reactive_dispatch(&mut self, changes: &[ReactiveChange], _ctx: &mut ReactiveCtx) {
            for c in changes {
                if c.field_name == "n" {
                    self.watched.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
    }

    /// GATE 2: widget A's handler updates child B via `query_one::<B>().update_via`;
    /// B's watcher fires in the SAME flush pass (drain resolves B by type, runs the
    /// closure with a fresh WidgetCtx, then dispatches B's reactive fixpoint).
    #[test]
    fn query_one_update_via_fires_target_watcher_same_pass() {
        let _ = take_runtime_reactive_entries();
        let _ = crate::runtime::commands::take_widget_commands();

        let mut tree = crate::widget_tree::WidgetTree::new();
        let a = tree.set_root(Box::new(ParentA));
        let watched = Arc::new(AtomicUsize::new(0));
        let _b = tree.mount(
            a,
            Box::new(ChildB {
                watched: Arc::clone(&watched),
                n: 0,
            }),
        );

        // Simulate A's handler: build a WidgetCtx over its EventCtx, query the
        // child by type, enqueue a deferred update.
        {
            let mut ectx = EventCtx::default();
            ectx.set_node_id(a);
            let mut wctx = crate::event::WidgetCtx::new(a, &mut ectx);
            let q = wctx.query_one::<ChildB>();
            q.update_via(&mut wctx, |b, bctx| b.bump(bctx));
        }
        // Deferred — nothing applied, watcher not fired yet.
        assert_eq!(watched.load(Ordering::SeqCst), 0);

        let mut app = test_app_with_tree(tree);
        let mut pending = super::PendingInvalidation::default();
        let mut root = StyleNode::new("Root");
        app.run_event_loop_reactive_phase(&mut root, &mut pending);

        assert_eq!(
            watched.load(Ordering::SeqCst),
            1,
            "B's watcher fired in the same flush pass as A's deferred update"
        );
    }

    /// GATE 2: an update targeting a removed node drops with a debug log and does
    /// NOT panic (generational id → `get`/resolve returns `None`).
    #[test]
    fn update_via_removed_target_drops_no_panic() {
        let _ = take_runtime_reactive_entries();
        let _ = crate::runtime::commands::take_widget_commands();

        let mut tree = crate::widget_tree::WidgetTree::new();
        let a = tree.set_root(Box::new(ParentA));
        let watched = Arc::new(AtomicUsize::new(0));
        let b = tree.mount(
            a,
            Box::new(ChildB {
                watched: Arc::clone(&watched),
                n: 0,
            }),
        );
        let handle = crate::handle::Handle::<ChildB>::resolve(&tree, b).unwrap();
        tree.remove(b);

        {
            let mut ectx = EventCtx::default();
            ectx.set_node_id(a);
            let mut wctx = crate::event::WidgetCtx::new(a, &mut ectx);
            handle.update_via(&mut wctx, |b, bctx| b.bump(bctx));
        }

        let mut app = test_app_with_tree(tree);
        let mut pending = super::PendingInvalidation::default();
        let mut root = StyleNode::new("Root");
        // Must not panic.
        app.run_event_loop_reactive_phase(&mut root, &mut pending);

        assert_eq!(
            watched.load(Ordering::SeqCst),
            0,
            "closure never ran for the removed target"
        );
    }

    /// GATE 2: a downcast miss (target resolved to a different concrete type) is
    /// logged loudly and dropped — no panic, no mutation.
    #[test]
    fn update_via_downcast_miss_drops_no_panic() {
        let _ = take_runtime_reactive_entries();
        let _ = crate::runtime::commands::take_widget_commands();

        let mut tree = crate::widget_tree::WidgetTree::new();
        let a = tree.set_root(Box::new(ParentA));
        let ran = Arc::new(AtomicUsize::new(0));
        // A Handle<ChildB> that actually names A's node (a ParentA) — wrong type.
        let bad = crate::handle::Handle::<ChildB>::new(a, tree.tree_id());

        {
            let mut ectx = EventCtx::default();
            ectx.set_node_id(a);
            let mut wctx = crate::event::WidgetCtx::new(a, &mut ectx);
            let ran_cb = Arc::clone(&ran);
            bad.update_via(&mut wctx, move |b, bctx| {
                ran_cb.fetch_add(1, Ordering::SeqCst);
                b.bump(bctx);
            });
        }

        let mut app = test_app_with_tree(tree);
        let mut pending = super::PendingInvalidation::default();
        let mut root = StyleNode::new("Root");
        // Must not panic; closure body must not run (downcast fails first).
        app.run_event_loop_reactive_phase(&mut root, &mut pending);

        assert_eq!(
            ran.load(Ordering::SeqCst),
            0,
            "closure body must not run on a downcast miss"
        );
    }

    /// Unit-1 fixup (Fable): an update closure that requests recompose via the
    /// REACTIVE path only (no field change) must NOT be silently dropped by the
    /// flush. Before the fix, `run_update_widget`'s enqueue condition omitted
    /// `needs_recompose()`/`needs_styles()`, so a recompose-only ctx was discarded.
    #[test]
    fn update_via_recompose_only_reactive_ctx_is_not_dropped() {
        let _ = take_runtime_reactive_entries();
        let _ = crate::runtime::commands::take_widget_commands();

        let mut tree = crate::widget_tree::WidgetTree::new();
        let a = tree.set_root(Box::new(ParentA));
        let watched = Arc::new(AtomicUsize::new(0));
        let b = tree.mount(
            a,
            Box::new(ChildB {
                watched: Arc::clone(&watched),
                n: 0,
            }),
        );
        let handle = crate::handle::Handle::<ChildB>::resolve(&tree, b).unwrap();

        {
            let mut ectx = EventCtx::default();
            ectx.set_node_id(a);
            let mut wctx = crate::event::WidgetCtx::new(a, &mut ectx);
            handle.update_via(&mut wctx, |_b, bctx| {
                // Reactive-path recompose only (no `record_change`) — reach the
                // ReactiveCtx flag directly, bypassing the inherent EventCtx shadow,
                // to exercise the branch the flush previously dropped.
                use std::ops::DerefMut;
                bctx.deref_mut().request_recompose();
            });
        }

        let mut app = test_app_with_tree(tree);
        let mut pending = super::PendingInvalidation::default();
        let mut root = StyleNode::new("Root");
        app.run_event_loop_reactive_phase(&mut root, &mut pending);

        assert!(
            pending.flags.layout,
            "recompose-only reactive ctx must be enqueued + processed (drives layout), not dropped"
        );
    }

    // WidgetCtx build, step 5: PostUp — a message posted from an update/timer
    // closure bubbles from that node to ancestor handlers.

    #[derive(Debug, Clone)]
    struct Ping;
    crate::impl_message!(Ping);

    struct PostSink {
        pings: Arc<AtomicUsize>,
    }

    impl Widget for PostSink {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }
        fn style_type(&self) -> &'static str {
            "PostSink"
        }
        fn on_message(&mut self, message: &MessageEvent, _ctx: &mut crate::event::WidgetCtx) {
            if message.downcast_ref::<Ping>().is_some() {
                self.pings.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    #[test]
    fn post_up_bubbles_closure_posted_message_to_ancestor() {
        let _ = take_runtime_reactive_entries();
        let _ = crate::runtime::commands::take_widget_commands();

        let pings = Arc::new(AtomicUsize::new(0));
        let mut tree = crate::widget_tree::WidgetTree::new();
        let root = tree.set_root(Box::new(PostSink {
            pings: Arc::clone(&pings),
        }));
        let child = tree.mount(
            root,
            Box::new(ChildB {
                watched: Arc::new(AtomicUsize::new(0)),
                n: 0,
            }),
        );

        // An update_via closure on the child posts a Ping (sender = child).
        let handle = crate::handle::Handle::<ChildB>::resolve(&tree, child).unwrap();
        {
            let mut ectx = EventCtx::default();
            ectx.set_node_id(root);
            let mut wctx = crate::event::WidgetCtx::new(root, &mut ectx);
            handle.update_via(&mut wctx, |_b, c| c.post_message(Ping));
        }

        let mut app = test_app_with_tree(tree);
        let mut pending = super::PendingInvalidation::default();
        let mut app_root = StyleNode::new("Root");
        app.run_event_loop_reactive_phase(&mut app_root, &mut pending);

        assert_eq!(
            pings.load(Ordering::SeqCst),
            1,
            "closure-posted Ping bubbled from the child up to the PostSink ancestor"
        );
    }

    // SPEC-P2 Step 6a (option b): test for on_app_unhandled_action fallback.
    // dispatch_simulated_key_like_input is private; test lives here where it is in scope.
    #[test]
    fn on_app_unhandled_action_fires_for_custom_binding() {
        use std::sync::Mutex;

        // A widget tree node with a declarative binding x->frob but no action_registry entry.
        struct FrobBindingNode;

        impl Widget for FrobBindingNode {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }

            fn focusable(&self) -> bool {
                true
            }

            fn bindings(&self) -> Vec<BindingDecl> {
                vec![BindingDecl::new("x", "frob", "Frob thing")]
            }
        }

        // Runtime root that overrides on_app_unhandled_action and records the action.
        struct FallbackRecorder {
            recorded: Arc<Mutex<Option<String>>>,
        }

        impl Widget for FallbackRecorder {
            fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
                Segments::new()
            }

            fn on_app_unhandled_action(
                &mut self,
                _app: &mut App,
                action: &str,
                ctx: &mut crate::event::WidgetCtx,
            ) {
                *self.recorded.lock().unwrap() = Some(action.to_string());
                ctx.set_handled();
            }
        }

        let mut tree = crate::widget_tree::WidgetTree::new();
        let frob_root = tree.set_root(Box::new(FrobBindingNode));
        tree.set_focus_state(frob_root, true);

        let mut app = test_app_with_tree(tree);
        let recorded = Arc::new(Mutex::new(None::<String>));
        let mut runtime_root = FallbackRecorder {
            recorded: Arc::clone(&recorded),
        };

        // Simulate pressing 'x', which maps to binding action "frob".
        let _outcome = app.dispatch_message_queue_with_runtime(
            &mut runtime_root,
            vec![
                crate::message::MessageEvent::new(
                    crate::node_id::node_id_from_ffi(1),
                    crate::message::AppSimulateKey {
                        key: "x".to_string(),
                    },
                )
                .with_control(crate::node_id::node_id_from_ffi(1)),
            ],
        );

        let got = recorded.lock().unwrap().clone();
        assert_eq!(
            got.as_deref(),
            Some("frob"),
            "on_app_unhandled_action must be called with action='frob'"
        );
    }

    // Gap 6 drop site B: the ROOT widget's own `on_mount` (fired before the
    // tree exists, in `headless_startup`/`run_widget_tree`) staged worker
    // requests and messages on a throwaway synth ctx that kept only the
    // reactive-dirty enqueue. Now its outcome is captured and absorbed after
    // the tree is built, so a worker requested from a raw root's `on_mount`
    // actually spawns and its posted message reaches the root's own handler.
    #[derive(Debug, Clone)]
    struct RootMountPing;
    crate::impl_message!(RootMountPing);

    struct RootMountProbe {
        worker_ran: Arc<std::sync::atomic::AtomicBool>,
        ping_seen: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Widget for RootMountProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_mount(&mut self, ctx: &mut crate::event::WidgetCtx) {
            let ran = Arc::clone(&self.worker_ran);
            ctx.request_worker_task(Some("root-mount-scan"), move |_cancel| {
                ran.store(true, Ordering::SeqCst);
                Ok(())
            });
            ctx.post_message(RootMountPing);
        }

        fn on_message(&mut self, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
            if message.is::<RootMountPing>() {
                self.ping_seen.store(true, Ordering::SeqCst);
                ctx.set_handled();
            }
        }
    }

    #[test]
    fn root_on_mount_worker_and_message_survive_drop_site_b() {
        use std::sync::atomic::AtomicBool;

        let worker_ran = Arc::new(AtomicBool::new(false));
        let ping_seen = Arc::new(AtomicBool::new(false));
        let mut root = RootMountProbe {
            worker_ran: Arc::clone(&worker_ran),
            ping_seen: Arc::clone(&ping_seen),
        };

        let mut app = App::new().expect("app inits for runtime tests");
        app.set_headless_size(80, 24);
        app.headless_startup(&mut root)
            .expect("headless startup succeeds");

        assert!(
            worker_ran.load(Ordering::SeqCst),
            "worker requested from the raw ROOT widget's on_mount must spawn \
             (drop site B: was kept only as a reactive enqueue pre-fix)"
        );
        assert!(
            ping_seen.load(Ordering::SeqCst),
            "message posted from the root's on_mount must reach its handler"
        );

        let _ = app.headless_finish(&mut root);
    }
}
