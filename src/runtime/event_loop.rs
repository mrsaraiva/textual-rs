use crate::css::{set_app_active, set_style_context};
use crate::debug::{debug_input, debug_render};
use crate::event::{
    Action, AnimationRequest, AnimationValueEvent, Event, MouseDownEvent, MouseScrollEvent,
    MouseUpEvent,
};
use crate::keys::KeyEventData;
use crate::message::{Message, MessageEvent};
use crossterm::event::{self, Event as CrosstermEvent, KeyEventKind, MouseEventKind};
use rich_rs::Renderable;
use std::collections::HashSet;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::App;
use super::devtools::DevtoolsCommand;
use super::helpers::{any_widget_active, mouse_scroll_deltas, should_quit_key};
use super::routing::{
    active_binding_hints, dispatch_event, dispatch_event_to_target, dispatch_message_queue,
    dispatch_mouse_scroll, dispatch_mouse_scroll_to_target, dispatch_scroll_action,
    focused_help_metadata, is_priority_action, is_scroll_action,
};
use super::types::{DispatchOutcome, PendingInvalidation, StylesheetReload};
use crate::widgets::{Widget, WidgetId};

fn should_dispatch_binding_hints(
    last_hints: &[crate::event::BindingHint],
    last_sources: &[crate::widgets::WidgetId],
    current_hints: &[crate::event::BindingHint],
    current_sources: &[crate::widgets::WidgetId],
) -> bool {
    last_hints != current_hints || last_sources != current_sources
}

fn should_dispatch_focused_help(
    last_source: Option<crate::widgets::WidgetId>,
    last_markup: Option<&str>,
    current_source: Option<crate::widgets::WidgetId>,
    current_markup: Option<&str>,
) -> bool {
    last_source != current_source || last_markup != current_markup
}

fn focused_help_message(current: Option<(crate::widgets::WidgetId, String)>) -> MessageEvent {
    if let Some((source, markup)) = current {
        MessageEvent {
            sender: source,
            message: Message::HelpPanelFocusedHelpChanged { source, markup },
        }
    } else {
        MessageEvent {
            sender: App::runtime_message_sender(),
            message: Message::HelpPanelFocusedHelpCleared,
        }
    }
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
            Message::TextEditClipboardCopyRequested { text, .. } => {
                *clipboard = Some(text.clone());
                if !backend.copy(text) {
                    debug_input("[clipboard] system copy unavailable; runtime fallback updated");
                }
            }
            Message::TextEditClipboardPasteRequested { target } => {
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
}

fn split_runtime_control_messages(app: &mut App, queue: Vec<MessageEvent>) -> RuntimeMessagePass {
    let mut pass = RuntimeMessagePass::default();
    for event in queue {
        match event.message {
            Message::AsyncTaskSpawn {
                task_id,
                target,
                request,
            } => {
                if let Some(cancelled) = app.async_tasks.spawn(task_id, target, request) {
                    pass.generated.push(cancelled);
                }
            }
            Message::AsyncTaskCancel { task_id } => {
                if let Some(cancelled) = app.async_tasks.cancel(task_id) {
                    pass.generated.push(cancelled);
                }
            }
            Message::AsyncTaskCancelTarget { target } => {
                pass.generated
                    .extend(app.async_tasks.cancel_for_target(target));
            }
            Message::TimerSchedule {
                timer_id,
                target,
                delay,
            } => {
                if let Some(cancelled) = app.one_shot_timers.schedule(timer_id, target, delay) {
                    pass.generated.push(cancelled);
                }
            }
            Message::TimerCancel { timer_id } => {
                if let Some(cancelled) = app.one_shot_timers.cancel(timer_id) {
                    pass.generated.push(cancelled);
                }
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
    id: WidgetId,
    type_name: String,
    style_id: Option<String>,
    classes: Vec<String>,
    disabled: bool,
    focused: bool,
    hovered: bool,
    active: bool,
}

fn snapshot_for(widget: &mut dyn Widget, app_active: bool) -> SelectorSnapshot {
    SelectorSnapshot {
        id: widget.id(),
        type_name: widget.style_type().to_string(),
        style_id: widget.style_id().map(str::to_string),
        classes: widget.style_classes().to_vec(),
        disabled: widget.is_disabled(),
        focused: widget.has_focus() && app_active,
        hovered: widget.is_hovered(),
        active: widget.is_active(),
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

fn collect_stylesheet_affected_widgets(
    root: &mut dyn Widget,
    changed_rules: &[crate::css::StyleRule],
    app_active: bool,
) -> Vec<WidgetId> {
    if changed_rules.is_empty() {
        return Vec::new();
    }

    fn visit(
        widget: &mut dyn Widget,
        rules: &[crate::css::StyleRule],
        app_active: bool,
        ancestors: &mut Vec<SelectorSnapshot>,
        affected: &mut HashSet<WidgetId>,
    ) {
        let current = snapshot_for(widget, app_active);
        if rules
            .iter()
            .any(|rule| rule_matches_snapshot_chain(rule, &current, ancestors))
        {
            affected.insert(current.id);
        }
        ancestors.push(current);
        widget
            .visit_children_mut(&mut |child| visit(child, rules, app_active, ancestors, affected));
        ancestors.pop();
    }

    let mut affected = HashSet::new();
    let mut ancestors = Vec::new();
    visit(
        root,
        changed_rules,
        app_active,
        &mut ancestors,
        &mut affected,
    );
    let mut out = affected.into_iter().collect::<Vec<_>>();
    out.sort_by_key(|id| id.as_u64());
    out
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
    Widget(WidgetId),
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
        root: &mut dyn Widget,
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
                    crate::widgets::set_focus_by_id(root, Some(id));
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

        fn collect_widgets(
            widget: &mut dyn Widget,
            depth: usize,
            app_active: bool,
            hovered: Option<WidgetId>,
            hit_test: &crate::runtime::types::HitTestMap,
            out: &mut Vec<String>,
            focused_out: &mut Option<WidgetId>,
        ) {
            let id = widget.id();
            let focused = widget.has_focus() && app_active;
            if focused {
                *focused_out = Some(id);
            }
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
                id.as_u64(),
                sanitize_snapshot_field(widget.style_type()),
                style_id,
                classes,
                bool_flag(focused),
                bool_flag(hovered == Some(id)),
                bool_flag(widget.is_active()),
                bool_flag(widget.is_disabled()),
                rect_field
            );
            out.push(line);
            widget.visit_children_mut(&mut |child| {
                collect_widgets(
                    child,
                    depth + 1,
                    app_active,
                    hovered,
                    hit_test,
                    out,
                    focused_out,
                )
            });
        }

        let mut widget_lines = Vec::new();
        let mut focused = None;
        collect_widgets(
            root,
            0,
            self.app_active,
            self.hovered,
            &self.hit_test,
            &mut widget_lines,
            &mut focused,
        );

        let mut snapshot = String::new();
        snapshot.push_str("version\t1\n");
        snapshot.push_str(&format!("pid\t{}\n", std::process::id()));
        snapshot.push_str(&format!("app_active\t{}\n", bool_flag(self.app_active)));
        snapshot.push_str(&format!(
            "debug_layout\t{}\n",
            bool_flag(self.debug_layout.enabled)
        ));
        snapshot.push_str(&format!("frame\t{}\t{}\n", self.frame.width, self.frame.height));
        snapshot.push_str(&format!(
            "hovered\t{}\n",
            self.hovered
                .map(|id| id.as_u64().to_string())
                .unwrap_or_else(|| "-".to_string())
        ));
        snapshot.push_str(&format!(
            "focused\t{}\n",
            focused
                .map(|id| id.as_u64().to_string())
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
            let mut runtime_messages =
                collect_clipboard_runtime_messages(&mut self.clipboard, &pass.deliver);
            runtime_messages.extend(pass.generated);
            let mut outcome = if pass.deliver.is_empty() {
                DispatchOutcome::default()
            } else {
                dispatch_message_queue(root, pass.deliver)
            };
            aggregate.handled |= outcome.handled;
            aggregate.repaint_requested |= outcome.repaint_requested;
            aggregate.invalidation.merge(outcome.invalidation);
            aggregate.stop_requested |= outcome.stop_requested;
            aggregate.messages.append(&mut outcome.messages);
            aggregate
                .animation_requests
                .append(&mut outcome.animation_requests);

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

        // Auto-focus the first focusable widget.
        let mut ids = Vec::new();
        crate::widgets::collect_focus_ids(root, &mut ids);
        if let Some(first) = ids.first().copied() {
            crate::widgets::set_focus_by_id(root, Some(first));
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
        let mut pending_invalidation = PendingInvalidation::default();
        pending_invalidation.request_flags(initial_help_outcome.invalidation);
        if initial_help_outcome.should_repaint() {
            pending_invalidation.request_full_content();
        }
        let mut prev_any_active = false;
        self.render_widget(root)?;
        self.publish_devtools_snapshot(root);
        pending_invalidation = PendingInvalidation::default();
        let mut last_render = Instant::now();

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
            if event::poll(timeout)? {
                let mut sheet = self.default_stylesheet.clone();
                sheet.extend(&self.stylesheet);
                let _active = set_app_active(self.app_active);
                let _guard = set_style_context(sheet);
                match event::read()? {
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
                        let bind = crate::event::KeyBind::from_event(&key);
                        let mapped_action = self.action_map.lookup(&bind);

                        // Priority actions (e.g. command palette) run before raw key dispatch.
                        if let Some(action) = mapped_action.filter(|a| is_priority_action(*a)) {
                            debug_input(&format!(
                                "[input] priority action-map {:?} -> {:?}",
                                bind, action
                            ));
                            let mut outcome = dispatch_event(root, Event::Action(action));
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

                        // Dispatch the raw key so focused widgets (e.g. Input) can consume it.
                        let mut key_outcome = dispatch_event(root, Event::Key(key.clone()));
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
                                debug_input(&format!(
                                    "[input] action-map {:?} -> {:?}",
                                    bind, action
                                ));
                                let mut outcome = if is_scroll_action(action) {
                                    dispatch_scroll_action(root, action, self.hovered)
                                } else {
                                    dispatch_event(root, Event::Action(action))
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
                    CrosstermEvent::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                            let before = self.hovered;
                            if self.update_hover_from_frame(mouse.column, mouse.row, root) {
                                if let Some(id) = before {
                                    pending_invalidation.request_widget_rect(&self.hit_test, id);
                                }
                                if let Some(id) = self.hovered {
                                    pending_invalidation.request_widget_rect(&self.hit_test, id);
                                } else {
                                    pending_invalidation.request_full_content();
                                }
                            }
                        }
                        MouseEventKind::Down(_) => {
                            debug_input(&format!(
                                "[input] mouse down x={} y={} hovered={:?}",
                                mouse.column,
                                mouse.row,
                                self.hovered.map(|id| id.as_u64())
                            ));
                            if let Some(target) = self.widget_at(mouse.column, mouse.row) {
                                let (x, y) = self.hit_test.content_local_coords(
                                    root,
                                    target,
                                    mouse.column,
                                    mouse.row,
                                );
                                debug_input(&format!(
                                    "[input] mouse target id={}",
                                    target.as_u64()
                                ));
                                let mut outcome = dispatch_event(
                                    root,
                                    Event::MouseDown(MouseDownEvent {
                                        target,
                                        screen_x: mouse.column,
                                        screen_y: mouse.row,
                                        x,
                                        y,
                                    }),
                                );
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
                            }
                        }
                        MouseEventKind::Up(_) => {
                            let target = self.widget_at(mouse.column, mouse.row);
                            let (x, y) = target
                                .map(|id| {
                                    self.hit_test.content_local_coords(
                                        root,
                                        id,
                                        mouse.column,
                                        mouse.row,
                                    )
                                })
                                .unwrap_or((0, 0));
                            let mut outcome = dispatch_event(
                                root,
                                Event::MouseUp(MouseUpEvent {
                                    target,
                                    screen_x: mouse.column,
                                    screen_y: mouse.row,
                                    x,
                                    y,
                                }),
                            );
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
                                    pending_invalidation.request_widget_rect(&self.hit_test, id);
                                }
                                if let Some(id) = self.hovered {
                                    pending_invalidation.request_widget_rect(&self.hit_test, id);
                                } else {
                                    pending_invalidation.request_full_content();
                                }
                            }
                            let (delta_x, delta_y) =
                                mouse_scroll_deltas(mouse.kind, mouse.modifiers);
                            let target = self.widget_at(mouse.column, mouse.row);
                            let (local_x, local_y) = target
                                .map(|id| {
                                    self.hit_test.content_local_coords(
                                        root,
                                        id,
                                        mouse.column,
                                        mouse.row,
                                    )
                                })
                                .unwrap_or((0, 0));
                            debug_input(&format!(
                                "[input] mouse scroll route target={:?} dx={} dy={}",
                                target.map(|id| id.as_u64()),
                                delta_x,
                                delta_y
                            ));
                            let mut diag_outcome = if let Some(target) = target {
                                dispatch_event_to_target(
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
                                dispatch_event(
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
                            let mut msg_outcome = self
                                .dispatch_message_queue_with_runtime(root, diag_outcome.messages);
                            self.absorb_outcome(
                                &mut msg_outcome,
                                &mut pending_invalidation,
                                InvalidationScope::Global,
                            );
                            let mut outcome = if let Some(target) = target {
                                dispatch_mouse_scroll_to_target(root, target, delta_x, delta_y)
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
                            let mut msg_outcome =
                                self.dispatch_message_queue_with_runtime(root, outcome.messages);
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
                    },
                    CrosstermEvent::Resize(_, _) => {
                        let size = self.driver.size();
                        debug_render(&format!("[event] Resize({}x{})", size.width, size.height));
                        self.refresh_size()?;
                        let size = self.driver.size();
                        root.on_resize(size.width, size.height);
                        let mut outcome =
                            dispatch_event(root, Event::Resize(size.width, size.height));
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
                        let mut outcome = dispatch_event(root, Event::AppFocus(false));
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
                        let mut outcome = dispatch_event(root, Event::AppFocus(true));
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

            if pending_invalidation.is_dirty() || self.resized_since_last_render {
                let regions = pending_invalidation
                    .content_regions
                    .as_render_regions(self.frame.width, self.frame.height);
                let layout_invalidation = pending_invalidation.flags.layout
                    || pending_invalidation.flags.style
                    || self.resized_since_last_render;
                self.render_widget_with_regions(root, regions.as_deref(), layout_invalidation)?;
                self.publish_devtools_snapshot(root);
                pending_invalidation = PendingInvalidation::default();
                last_render = Instant::now();
            }

            if last_render.elapsed() >= tick_rate {
                let mut sheet = self.default_stylesheet.clone();
                sheet.extend(&self.stylesheet);
                let _active = set_app_active(self.app_active);
                let _guard = set_style_context(sheet);
                if let Some(reload) = self.poll_stylesheet() {
                    self.absorb_stylesheet_reload(root, reload, &mut pending_invalidation);
                }
                root.on_tick(tick);
                // `on_tick` mutates widget state without an `EventCtx`, so request a repaint
                // for this frame to keep tick-driven widgets (e.g. counters/cursors) in sync.
                pending_invalidation.request_full_content();
                let mut outcome = dispatch_event(root, Event::Tick(tick));
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
                if outcome.stop_requested || msg_outcome.stop_requested {
                    break 'event_loop;
                }

                let any_active = any_widget_active(root);
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
        let (widget_hints, current_sources) = active_binding_hints(root);
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
        let outcome = dispatch_event(root, Event::BindingsChanged(current));
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
        }
    }

    pub(super) fn dispatch_focused_help_changed(
        &mut self,
        root: &mut dyn Widget,
    ) -> DispatchOutcome {
        let current = focused_help_metadata(root);
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
    }

    fn absorb_stylesheet_reload(
        &mut self,
        root: &mut dyn Widget,
        reload: StylesheetReload,
        pending: &mut PendingInvalidation,
    ) {
        if reload.previous == reload.next {
            return;
        }
        let affected =
            collect_stylesheet_affected_widgets(root, &reload.changed_rules, self.app_active);
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

    pub(super) fn dispatch_animation_frame(&mut self, root: &mut dyn Widget) -> DispatchOutcome {
        let updates = self.animator.step(Instant::now(), self.animation_level);
        if updates.is_empty() {
            return DispatchOutcome::default();
        }

        let mut aggregate = DispatchOutcome::default();
        for update in updates {
            let mut outcome = dispatch_event_to_target(
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

            aggregate.stop_requested |= outcome.stop_requested || msg_outcome.stop_requested;
            aggregate.messages.extend(msg_outcome.messages);
        }
        aggregate.repaint_requested = true;
        aggregate
            .invalidation
            .merge(crate::event::InvalidationFlags::content());
        aggregate
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClipboardBackend, collect_clipboard_runtime_messages_with_backend,
        collect_stylesheet_affected_widgets, focused_help_message, should_dispatch_binding_hints,
        should_dispatch_focused_help,
    };
    use crate::css::StyleSheet;
    use crate::event::BindingHint;
    use crate::message::{Message, MessageEvent};
    use crate::widgets::{Widget, WidgetId};
    use rich_rs::{Console, ConsoleOptions, Segments};
    use std::collections::VecDeque;

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

    #[test]
    fn binding_hints_dispatch_when_hint_payload_changes() {
        let last_hints = vec![BindingHint::new("tab", "next")];
        let current_hints = vec![
            BindingHint::new("tab", "next"),
            BindingHint::new("q", "quit"),
        ];
        let last_sources = vec![WidgetId::from_u64(1)];
        let current_sources = vec![WidgetId::from_u64(1)];

        assert!(should_dispatch_binding_hints(
            &last_hints,
            &last_sources,
            &current_hints,
            &current_sources,
        ));
    }

    #[test]
    fn binding_hints_dispatch_when_sources_change_with_same_hints() {
        let hints = vec![BindingHint::new("tab", "next")];
        let last_sources = vec![WidgetId::from_u64(1)];
        let current_sources = vec![WidgetId::from_u64(2)];

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
        let sources = vec![WidgetId::from_u64(1)];

        assert!(!should_dispatch_binding_hints(
            &hints, &sources, &hints, &sources,
        ));
    }

    #[test]
    fn focused_help_dispatches_when_focus_source_changes() {
        assert!(should_dispatch_focused_help(
            Some(WidgetId::from_u64(1)),
            Some("## First"),
            Some(WidgetId::from_u64(2)),
            Some("## Second"),
        ));
    }

    #[test]
    fn focused_help_dispatches_when_help_clears() {
        assert!(should_dispatch_focused_help(
            Some(WidgetId::from_u64(1)),
            Some("## First"),
            None,
            None,
        ));
    }

    #[test]
    fn focused_help_skips_when_source_and_markup_stable() {
        assert!(!should_dispatch_focused_help(
            Some(WidgetId::from_u64(1)),
            Some("## Stable"),
            Some(WidgetId::from_u64(1)),
            Some("## Stable"),
        ));
    }

    #[test]
    fn focused_help_message_emits_set_payload() {
        let source = WidgetId::from_u64(7);
        let event = focused_help_message(Some((source, "## Source help".to_string())));
        assert_eq!(event.sender, source);
        assert!(matches!(
            event.message,
            Message::HelpPanelFocusedHelpChanged {
                source: msg_source,
                markup,
            } if msg_source == source && markup == "## Source help"
        ));
    }

    #[test]
    fn focused_help_message_emits_clear_payload() {
        let event = focused_help_message(None);
        assert_eq!(event.sender, WidgetId::from_u64(0));
        assert!(matches!(
            event.message,
            Message::HelpPanelFocusedHelpCleared
        ));
    }

    #[test]
    fn clipboard_runtime_handles_copy_then_paste_request() {
        let target = WidgetId::from_u64(42);
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
                    sender: WidgetId::from_u64(1),
                    message: Message::TextEditClipboardCopyRequested {
                        text: "hello".to_string(),
                        cut: false,
                    },
                },
                MessageEvent {
                    sender: WidgetId::from_u64(2),
                    message: Message::TextEditClipboardPasteRequested { target },
                },
            ],
            &mut backend,
        );
        assert_eq!(clipboard.as_deref(), Some("hello"));
        assert_eq!(backend.copied, vec!["hello".to_string()]);
        assert_eq!(generated.len(), 1);
        assert!(matches!(
            &generated[0].message,
            Message::TextEditClipboardPaste {
                target: t,
                text
            } if *t == target && text == "hello"
        ));
    }

    #[test]
    fn clipboard_runtime_ignores_paste_without_buffered_text() {
        let target = WidgetId::from_u64(7);
        let mut clipboard = None;
        let mut backend = StubClipboardBackend::default();
        let generated = collect_clipboard_runtime_messages_with_backend(
            &mut clipboard,
            &[MessageEvent {
                sender: WidgetId::from_u64(2),
                message: Message::TextEditClipboardPasteRequested { target },
            }],
            &mut backend,
        );
        assert!(generated.is_empty());
    }

    #[test]
    fn clipboard_runtime_prefers_system_clipboard_on_paste() {
        let target = WidgetId::from_u64(9);
        let mut clipboard = Some("fallback".to_string());
        let mut backend = StubClipboardBackend {
            copy_results: VecDeque::new(),
            paste_results: VecDeque::from(vec![Some("system".to_string())]),
            copied: Vec::new(),
        };

        let generated = collect_clipboard_runtime_messages_with_backend(
            &mut clipboard,
            &[MessageEvent {
                sender: WidgetId::from_u64(2),
                message: Message::TextEditClipboardPasteRequested { target },
            }],
            &mut backend,
        );

        assert_eq!(clipboard.as_deref(), Some("system"));
        assert_eq!(generated.len(), 1);
        assert!(matches!(
            &generated[0].message,
            Message::TextEditClipboardPaste { target: t, text } if *t == target && text == "system"
        ));
    }

    struct StyleNode {
        id: WidgetId,
        type_name: &'static str,
        style_id: Option<String>,
        classes: Vec<String>,
        focused: bool,
        children: Vec<StyleNode>,
    }

    impl StyleNode {
        fn new(type_name: &'static str) -> Self {
            Self {
                id: WidgetId::new(),
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
        fn id(&self) -> WidgetId {
            self.id
        }

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

        fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
            for child in &mut self.children {
                f(child);
            }
        }
    }

    #[test]
    fn stylesheet_invalidation_matches_descendant_selectors_selectively() {
        let button = StyleNode::new("Button").with_class("special");
        let button_id = button.id();
        let mut root = StyleNode::new("Container")
            .with_class("panel")
            .with_child(button)
            .with_child(StyleNode::new("Label"));

        let changed = StyleSheet::parse("Container.panel > Button.special { bg: #334455; }");
        let affected = collect_stylesheet_affected_widgets(&mut root, changed.rules(), true);

        assert_eq!(affected, vec![button_id]);
    }

    #[test]
    fn stylesheet_invalidation_respects_focus_pseudo_state() {
        let button = StyleNode::new("Button").with_focus(true);
        let button_id = button.id();
        let mut root = StyleNode::new("Container").with_child(button);

        let changed = StyleSheet::parse("Button:focus { fg: #ffffff; }");
        let affected_active = collect_stylesheet_affected_widgets(&mut root, changed.rules(), true);
        let affected_inactive =
            collect_stylesheet_affected_widgets(&mut root, changed.rules(), false);

        assert_eq!(affected_active, vec![button_id]);
        assert!(affected_inactive.is_empty());
    }
}
