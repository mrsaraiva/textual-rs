use crate::css::{set_app_active, set_style_context};
use crate::debug::{debug_input, debug_render};
use crate::event::{
    Action, AnimationRequest, AnimationValueEvent, Event, MouseDownEvent, MouseScrollEvent,
    MouseUpEvent,
};
use crate::keys::KeyEventData;
use crossterm::event::{self, Event as CrosstermEvent, KeyEventKind, MouseEventKind};
use rich_rs::Renderable;
use std::time::{Duration, Instant};

use super::App;
use super::helpers::{any_widget_active, mouse_scroll_deltas, should_quit_key};
use super::routing::{
    dispatch_event, dispatch_event_to_target, dispatch_message_queue, dispatch_mouse_scroll,
    dispatch_mouse_scroll_to_target, dispatch_scroll_action, is_priority_action, is_scroll_action,
};
use super::types::DispatchOutcome;
use crate::widgets::Widget;

impl App {
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

        let mut tick: u64 = 0;
        let idle_tick_rate = Duration::from_millis(100);
        let active_tick_rate = Duration::from_millis(16);
        let mut dirty = false;
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
                            let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
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
                        let mut msg_outcome = dispatch_message_queue(root, key_outcome.messages);
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
                                let mut msg_outcome =
                                    dispatch_message_queue(root, outcome.messages);
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
                                let mut msg_outcome =
                                    dispatch_message_queue(root, outcome.messages);
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
                            let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
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
                            let mut msg_outcome =
                                dispatch_message_queue(root, diag_outcome.messages);
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
                            let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
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
                        let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
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
                        let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
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
                        let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
                        self.absorb_outcome(&mut msg_outcome, &mut dirty);
                        if outcome.stop_requested || msg_outcome.stop_requested {
                            break 'event_loop;
                        }
                    }
                    _ => {}
                }
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
                let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
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
        let current = self.binding_hints();
        if current == self.last_binding_hints {
            return DispatchOutcome::default();
        }
        self.last_binding_hints = current.clone();
        let outcome = dispatch_event(root, Event::BindingsChanged(current));
        let msg_outcome = dispatch_message_queue(root, outcome.messages);
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
            let mut msg_outcome = dispatch_message_queue(root, outcome.messages);
            self.absorb_outcome(&mut msg_outcome, &mut aggregate.repaint_requested);

            aggregate.handled |= outcome.handled || msg_outcome.handled;
            aggregate.stop_requested |= outcome.stop_requested || msg_outcome.stop_requested;
            aggregate.messages.extend(msg_outcome.messages);
        }
        aggregate.repaint_requested = true;
        aggregate
    }
}
