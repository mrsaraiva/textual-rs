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
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::App;
use super::helpers::{any_widget_active, mouse_scroll_deltas, should_quit_key};
use super::routing::{
    active_binding_hints, dispatch_event, dispatch_event_to_target, dispatch_message_queue,
    dispatch_mouse_scroll, dispatch_mouse_scroll_to_target, dispatch_scroll_action,
    focused_help_metadata, is_priority_action, is_scroll_action,
};
use super::types::DispatchOutcome;
use crate::widgets::Widget;

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
                app.async_tasks.spawn(task_id, target, request);
            }
            Message::AsyncTaskCancel { task_id } => {
                if let Some(cancelled) = app.async_tasks.cancel(task_id) {
                    pass.generated.push(cancelled);
                }
            }
            _ => pass.deliver.push(event),
        }
    }
    pass.generated.extend(app.async_tasks.drain_completed());
    pass
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
        let queue = self.async_tasks.drain_completed();
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
        let initial_help_outcome = self.dispatch_focused_help_changed(root);
        if initial_help_outcome.stop_requested {
            root.on_unmount();
            self.finish()?;
            return Ok(());
        }

        let mut tick: u64 = 0;
        let idle_tick_rate = Duration::from_millis(100);
        let active_tick_rate = Duration::from_millis(16);
        let mut dirty = initial_help_outcome.should_repaint();
        let mut prev_any_active = false;
        self.render_widget(root)?;
        let mut last_render = Instant::now();

        'event_loop: loop {
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
                            self.absorb_outcome(&mut outcome, &mut dirty);
                            let mut msg_outcome =
                                self.dispatch_message_queue_with_runtime(root, outcome.messages);
                            self.absorb_outcome(&mut msg_outcome, &mut dirty);
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
                        self.absorb_outcome(&mut key_outcome, &mut dirty);
                        let mut msg_outcome =
                            self.dispatch_message_queue_with_runtime(root, key_outcome.messages);
                        self.absorb_outcome(&mut msg_outcome, &mut dirty);
                        if key_outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                        if !key_outcome.handled {
                            if let Some(action) = mapped_action.filter(|a| !is_priority_action(*a))
                            {
                                if action == Action::HelpQuit {
                                    self.notify_help_quit();
                                    dirty = true;
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
                                self.absorb_outcome(&mut outcome, &mut dirty);
                                let mut msg_outcome = self
                                    .dispatch_message_queue_with_runtime(root, outcome.messages);
                                self.absorb_outcome(&mut msg_outcome, &mut dirty);
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
                            if self.update_hover_from_frame(mouse.column, mouse.row, root) {
                                dirty = true;
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
                                self.absorb_outcome(&mut outcome, &mut dirty);
                                let mut msg_outcome = self
                                    .dispatch_message_queue_with_runtime(root, outcome.messages);
                                self.absorb_outcome(&mut msg_outcome, &mut dirty);
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
                            self.absorb_outcome(&mut outcome, &mut dirty);
                            let mut msg_outcome =
                                self.dispatch_message_queue_with_runtime(root, outcome.messages);
                            self.absorb_outcome(&mut msg_outcome, &mut dirty);
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
                            if self.update_hover_from_frame(mouse.column, mouse.row, root) {
                                dirty = true;
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
                            self.absorb_outcome(&mut diag_outcome, &mut dirty);
                            let mut msg_outcome = self
                                .dispatch_message_queue_with_runtime(root, diag_outcome.messages);
                            self.absorb_outcome(&mut msg_outcome, &mut dirty);
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
                            self.absorb_outcome(&mut outcome, &mut dirty);
                            let mut msg_outcome =
                                self.dispatch_message_queue_with_runtime(root, outcome.messages);
                            self.absorb_outcome(&mut msg_outcome, &mut dirty);
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
                        self.absorb_outcome(&mut outcome, &mut dirty);
                        let mut msg_outcome =
                            self.dispatch_message_queue_with_runtime(root, outcome.messages);
                        self.absorb_outcome(&mut msg_outcome, &mut dirty);
                        if outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                    }
                    CrosstermEvent::FocusLost => {
                        self.app_active = false;
                        debug_input("[event] FocusLost");
                        let mut outcome = dispatch_event(root, Event::AppFocus(false));
                        self.absorb_outcome(&mut outcome, &mut dirty);
                        let mut msg_outcome =
                            self.dispatch_message_queue_with_runtime(root, outcome.messages);
                        self.absorb_outcome(&mut msg_outcome, &mut dirty);
                        if outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                    }
                    CrosstermEvent::FocusGained => {
                        self.app_active = true;
                        debug_input("[event] FocusGained");
                        let mut outcome = dispatch_event(root, Event::AppFocus(true));
                        self.absorb_outcome(&mut outcome, &mut dirty);
                        let mut msg_outcome =
                            self.dispatch_message_queue_with_runtime(root, outcome.messages);
                        self.absorb_outcome(&mut msg_outcome, &mut dirty);
                        if outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                    }
                    _ => {}
                }
            }

            let mut background_outcome = self.dispatch_background_runtime_messages(root);
            self.absorb_outcome(&mut background_outcome, &mut dirty);
            if background_outcome.stop_requested {
                break 'event_loop;
            }

            let mut focused_help_outcome = self.dispatch_focused_help_changed(root);
            self.absorb_outcome(&mut focused_help_outcome, &mut dirty);
            if focused_help_outcome.stop_requested {
                break 'event_loop;
            }

            let mut binding_outcome = self.dispatch_binding_hints_changed(root);
            self.absorb_outcome(&mut binding_outcome, &mut dirty);
            if binding_outcome.stop_requested {
                break 'event_loop;
            }

            let mut animation_outcome = self.dispatch_animation_frame(root);
            self.absorb_outcome(&mut animation_outcome, &mut dirty);
            if animation_outcome.stop_requested {
                break 'event_loop;
            }

            if dirty || self.resized_since_last_render {
                self.render_widget(root)?;
                dirty = false;
                last_render = Instant::now();
            }

            if last_render.elapsed() >= tick_rate {
                let mut sheet = self.default_stylesheet.clone();
                sheet.extend(&self.stylesheet);
                let _active = set_app_active(self.app_active);
                let _guard = set_style_context(sheet);
                self.poll_stylesheet();
                root.on_tick(tick);
                // `on_tick` mutates widget state without an `EventCtx`, so request a repaint
                // for this frame to keep tick-driven widgets (e.g. counters/cursors) in sync.
                dirty = true;
                let mut outcome = dispatch_event(root, Event::Tick(tick));
                self.absorb_outcome(&mut outcome, &mut dirty);
                let mut msg_outcome =
                    self.dispatch_message_queue_with_runtime(root, outcome.messages);
                self.absorb_outcome(&mut msg_outcome, &mut dirty);
                let notifications_before = self.notifications.len();
                let now = Instant::now();
                self.notifications.retain(|note| note.expires_at > now);
                if self.notifications.len() != notifications_before {
                    dirty = true;
                }
                if outcome.stop_requested || msg_outcome.stop_requested {
                    break 'event_loop;
                }

                let any_active = any_widget_active(root);
                if dirty || self.resized_since_last_render || any_active || prev_any_active {
                    self.render_widget(root)?;
                    dirty = false;
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
        DispatchOutcome {
            handled: outcome.handled || msg_outcome.handled,
            repaint_requested: outcome.repaint_requested || msg_outcome.repaint_requested,
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

    pub(super) fn absorb_outcome(&mut self, outcome: &mut DispatchOutcome, dirty: &mut bool) {
        *dirty |= outcome.should_repaint();
        let requests = std::mem::take(&mut outcome.animation_requests);
        self.enqueue_animation_requests(requests);
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
            self.absorb_outcome(&mut outcome, &mut aggregate.repaint_requested);
            let mut msg_outcome = self.dispatch_message_queue_with_runtime(root, outcome.messages);
            self.absorb_outcome(&mut msg_outcome, &mut aggregate.repaint_requested);

            aggregate.handled |= outcome.handled || msg_outcome.handled;
            aggregate.stop_requested |= outcome.stop_requested || msg_outcome.stop_requested;
            aggregate.messages.extend(msg_outcome.messages);
        }
        aggregate.repaint_requested = true;
        aggregate
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClipboardBackend, collect_clipboard_runtime_messages_with_backend, focused_help_message,
        should_dispatch_binding_hints, should_dispatch_focused_help,
    };
    use crate::event::BindingHint;
    use crate::message::{Message, MessageEvent};
    use crate::widgets::WidgetId;
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
}
