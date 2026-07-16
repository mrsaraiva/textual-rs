use std::time::Duration;

use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Segments};
use textual_macros::widget;

use crate::event::{Action, AnimationLevel, AnimationRequest, Event};
use crate::message::*;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

use super::scrollbar::ScrollBarRender;
use super::{Focus, Interactive, Layout, NodeSeed, Render};

/// The content width of the switch slider track (in cells).
///
/// Matches Python `Switch.get_content_width` which returns `4`. Padding/border
/// chrome is added by the layout engine, not here.
const SWITCH_WIDTH: usize = 4;

/// `ScrollBarRender` parameters used by the Python `Switch.render`:
/// the slider is a horizontal scrollbar thumb occupying half the track.
const SWITCH_VIRTUAL_SIZE: usize = 100;
const SWITCH_WINDOW_SIZE: usize = 50;

/// Duration of the slide animation (Python `watch_value`:
/// `self.animate("_slider_position", ..., duration=0.3, level="basic")`).
const ANIMATION_DURATION: Duration = Duration::from_millis(300);

/// A boolean toggle switch widget.
///
/// Renders as a slider track with a knob that smoothly animates left/right.
/// Toggled via click, Enter, or Space.
#[derive(Debug, Clone)]
#[widget(Focus, Interactive, Layout, Components, reactive, style_type = "Switch")]
pub struct Switch {
    value: bool,
    pressed: bool,
    disabled: bool,
    /// Animated slider position: 0.0 = off (left), 1.0 = on (right).
    slider_pos: f32,
    /// Animation target (0.0 or 1.0).
    slider_target: f32,
    seed: NodeSeed,
}

impl Switch {
    crate::seed_ident_methods!();

    pub fn new(value: bool) -> Self {
        let pos = if value { 1.0 } else { 0.0 };
        Self {
            value,
            pressed: false,
            disabled: false,
            slider_pos: pos,
            slider_target: pos,
            seed: NodeSeed::default(),
        }
        .rebuild_classes()
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    pub fn value(&self) -> bool {
        self.value
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `value`. Records the change in the provided
    /// [`ReactiveCtx`]. The watcher handles slider snap and class rebuild.
    pub fn set_value(&mut self, value: bool, ctx: &mut ReactiveCtx) {
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

    /// Reactive setter for `slider_pos` (var — no repaint, no layout).
    pub fn set_slider_pos(&mut self, value: f32, ctx: &mut ReactiveCtx) {
        #[allow(clippy::float_cmp)]
        if self.slider_pos != value {
            let old = self.slider_pos;
            self.slider_pos = value;
            ctx.record_change(
                "slider_pos",
                ReactiveFlags::var(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_value(&mut self, _old: &bool, _new: &bool, ctx: &mut ReactiveCtx) {
        // Snap slider immediately (programmatic change, no animation).
        self.slider_target = if self.value { 1.0 } else { 0.0 };
        self.slider_pos = self.slider_target;
        self.rebuild_classes_in_place();
        // Python `watch__slider_position`: `set_class(slider_position == 1,
        // "-on")` — the class must land on the ARENA node (the seed was
        // consumed at mount), or `Switch.-on .switch--slider { color: $success }`
        // never matches after a programmatic value change.
        ctx.set_class(self.value, "-on");
        // Python `watch_value` posts `Switch.Changed` on EVERY value change,
        // including programmatic ones (`_switch.py`). The interactive toggle
        // path posts through its event handler and never reaches this watcher
        // (it writes the field directly), so this does not double-post.
        // Honours active `prevent(SwitchChanged)` scopes via `ReactiveCtx`.
        ctx.post_message(SwitchChanged { value: self.value });
    }

    // ── Builder methods ──────────────────────────────────────────────────

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.rebuild_classes()
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    fn emit_changed(&self, ctx: &mut crate::event::WidgetCtx) {
        ctx.post_message(SwitchChanged { value: self.value });
    }

    /// Called after an interactive toggle (from event handler).
    ///
    /// Python `watch_value` animates `_slider_position` toward the new value
    /// (0.3s, level=basic, default in-out-cubic easing) via the app animator;
    /// the knob position and the `-on` class then follow the animated position
    /// in the `Event::AnimationValue` handler (Python `watch__slider_position`).
    fn on_toggled(&mut self, ctx: &mut crate::event::WidgetCtx) {
        self.slider_target = if self.value { 1.0 } else { 0.0 };
        self.rebuild_classes_in_place();
        let node_id = crate::widgets::Widget::node_id(self);
        ctx.request_animation(
            AnimationRequest::new(
                node_id,
                "slider_pos",
                self.slider_pos,
                self.slider_target,
                ANIMATION_DURATION,
            )
            .with_level(AnimationLevel::Basic),
        );
    }

    fn rebuild_classes(mut self) -> Self {
        self.rebuild_classes_in_place();
        self
    }

    fn rebuild_classes_in_place(&mut self) {
        let mut classes = vec!["switch".to_string()];
        if self.value {
            classes.push("-on".to_string());
        } else {
            classes.push("-off".to_string());
        }
        if self.disabled {
            classes.push("disabled".to_string());
        }
        self.seed.classes = classes;
    }

    /// Test helper for verifying animation correctness.
    #[allow(dead_code)]
    fn is_animating(&self) -> bool {
        (self.slider_pos - self.slider_target).abs() > f32::EPSILON
    }
}

impl ReactiveWidget for Switch {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            if change.field_name == "value" {
                if let (Some(old), Some(new)) = (
                    change.old_value.downcast_ref::<bool>(),
                    change.new_value.downcast_ref::<bool>(),
                ) {
                    self.watch_value(old, new, ctx);
                }
            }
        }
    }
}

impl Focus for Switch {
    fn focusable(&self) -> bool {
        !self.disabled
    }

    fn is_active(&self) -> bool {
        self.pressed && crate::widgets::Widget::node_state(self).hovered
    }
}

impl Layout for Switch {
    fn content_width(&self) -> Option<usize> {
        // Python `Switch.get_content_width` returns the bare content width (4);
        // the layout engine adds padding/border chrome around it.
        Some(SWITCH_WIDTH)
    }

    fn layout_height(&self) -> Option<usize> {
        // PURE content height (1 row). The flow layout adds the CSS-resolved
        // vertical chrome (the default `border: tall` adds 2 rows) with ancestor
        // context, symmetric with the width axis.
        Some(1)
    }
}

impl Interactive for Switch {
    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        if self.disabled {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == crate::widgets::Widget::node_id(self) => {
                self.pressed = true;
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(mouse)
                if self.pressed => {
                    self.pressed = false;
                    ctx.request_repaint();
                    if mouse.target.is_some_and(|t| t == crate::widgets::Widget::node_id(self)) {
                        self.value = !self.value;
                        self.on_toggled(ctx);
                        self.emit_changed(ctx);
                        ctx.set_handled();
                    }
                }
            Event::AppFocus(false)
                if self.pressed => {
                    self.pressed = false;
                    ctx.request_repaint();
                }
            Event::Action(Action::Toggle) if crate::widgets::Widget::node_state(self).focused => {
                self.value = !self.value;
                self.on_toggled(ctx);
                self.emit_changed(ctx);
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::Key(key) if crate::widgets::Widget::node_state(self).focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.value = !self.value;
                    self.on_toggled(ctx);
                    self.emit_changed(ctx);
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                _ => {}
            },
            // Animator update for the knob slide (requested by `on_toggled`).
            // Python `watch__slider_position`: follow the animated position and
            // toggle the `-on` class exactly when the slider reaches 1.
            Event::AnimationValue(anim)
                if anim.target == crate::widgets::Widget::node_id(self)
                    && anim.attribute == "slider_pos" =>
            {
                self.slider_pos = anim.value;
                #[allow(clippy::float_cmp)]
                let on = self.slider_pos == 1.0;
                // Deferred WidgetCommand (`WidgetCtx::set_class` command-queue
                // path) — lands on `node.classes` at the shared flush.
                ctx.set_class(on, "-on");
                ctx.request_repaint();
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

impl Render for Switch {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        // The slider IS a horizontal scrollbar thumb (Python `Switch.render`):
        // `ScrollBarRender(virtual_size=100, window_size=50,
        //  position=_slider_position*50, vertical=False)`. The thumb occupies
        // half the track and slides from the left half (off) to the right
        // half (on). Colors come from the `switch--slider` component style;
        // in plain text the thumb and track are both spaces (the distinction
        // is purely color/reverse), matching Python.
        // Resolve the `switch--slider` component style through the canonical
        // API: during a tree render this node's live meta (real arena classes
        // like `-on`, its css id, and interaction states) is already the top of
        // the selector stack, and `resolve_component_style` resolves the
        // typeless phantom directly against it, so an id rule like
        // `#custom-design > .switch--slider { background: darkslateblue }` and
        // the post-toggle `-on` colour both match.
        let slider = crate::css::resolve_component_style(self, &["switch--slider"]);
        let base_bg = crate::css::current_self_style()
            .and_then(|s| s.bg)
            .or_else(|| crate::style::parse_color_like("$surface"))
            .unwrap_or_else(|| crate::style::Color::rgb(0, 0, 0));
        let back = slider
            .bg
            .map(|c| c.flatten_over(base_bg))
            .unwrap_or(base_bg);
        let thumb = slider
            .fg
            .map(|c| c.flatten_over(back))
            .unwrap_or(back);

        let renderer = ScrollBarRender {
            virtual_size: SWITCH_VIRTUAL_SIZE,
            window_size: SWITCH_WINDOW_SIZE,
            position: self.slider_pos * SWITCH_WINDOW_SIZE as f32,
            thickness: 1,
            vertical: false,
        };
        let lines = renderer.render_bar(width, back, thumb, None);

        let mut out = Segments::new();
        if let Some(row) = lines.into_iter().next() {
            out.extend(row);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::event::MouseDownEvent;
    use crate::keys::KeyEventData;
    use crate::node_id::NodeId;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
    use crate::widgets::NodeState;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use slotmap::SlotMap;

    fn make_node_id() -> NodeId {
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    fn focused_state() -> NodeState {
        NodeState {
            focused: true,
            ..Default::default()
        }
    }

    #[test]
    fn switch_space_toggles_and_emits_message() {
        let mut widget = Switch::new(false);
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        let mut ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            widget.on_event(&Event::Key(key), &mut __w);
        }
        assert!(widget.value());
        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert!(
            messages
                .iter()
                .any(|m| m.downcast_ref::<SwitchChanged>().is_some_and(|s| s.value))
        );
    }

    #[test]
    fn switch_disabled_ignores_input() {
        let mut widget = Switch::new(false).disabled(true);
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            widget.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut __w);
        }
        assert!(!widget.value());
        assert!(!ctx.handled());
    }

    /// Toggling requests a `slider_pos` animation on the widget's node (Python
    /// `watch_value` -> `self.animate("_slider_position", ...)`), and the
    /// `Event::AnimationValue` updates drive the knob position + the `-on`
    /// class op exactly when the slider reaches 1 (Python
    /// `watch__slider_position`).
    #[test]
    fn switch_toggle_requests_animation_and_value_events_drive_knob() {
        let mut widget = Switch::new(false);
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        let mut ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(id, &mut ctx);
            widget.on_event(&Event::Key(key), &mut __w);
        }
        assert!(widget.value());
        // The toggle must enqueue the slider animation request.
        let requests = ctx.take_animation_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].attribute, "slider_pos");
        assert_eq!(requests[0].target, id);
        assert!((requests[0].end - 1.0).abs() < f32::EPSILON);
        assert!(widget.is_animating());

        // Mid-animation value: knob follows, `-on` NOT yet set.
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(id, &mut ctx);
            widget.on_event(
                &Event::AnimationValue(crate::event::AnimationValueEvent {
                    target: id,
                    attribute: "slider_pos".to_string(),
                    value: 0.5,
                    done: false,
                }),
                &mut __w,
            );
        }
        assert!((widget.slider_pos - 0.5).abs() < f32::EPSILON);
        assert!(widget.is_animating());
        // Mid-animation the class op is a Remove (`-on` only at position 1).
        let ops = crate::runtime::commands::drain_class_commands_for_test();
        assert!(
            ops.iter().any(|(node, op)| *node == id
                && matches!(op, crate::event::ClassOp::Remove(c) if c == "-on")),
            "mid-animation the `-on` class must be (still) off"
        );

        // Final value: knob lands, `-on` class op queued for the arena node
        // (via the ReactiveCtx surface WidgetCtx derefs to; the runtime's
        // reactive flush applies it to `node.classes`).
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(id, &mut ctx);
            widget.on_event(
                &Event::AnimationValue(crate::event::AnimationValueEvent {
                    target: id,
                    attribute: "slider_pos".to_string(),
                    value: 1.0,
                    done: true,
                }),
                &mut __w,
            );
            let ops = crate::runtime::commands::drain_class_commands_for_test();
            assert!(
                ops.iter().any(|(node, op)| *node == id
                    && matches!(op, crate::event::ClassOp::Add(c) if c == "-on")),
                "reaching slider position 1 must queue the `-on` class op on the node"
            );
        }
        assert!(!widget.is_animating());
        assert!((widget.slider_pos - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn switch_reactive_set_value_snaps_position() {
        let mut widget = Switch::new(false);
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        widget.set_value(true, &mut ctx);
        assert!(ctx.has_changes());
        assert_eq!(ctx.changes()[0].field_name, "value");
        // Dispatch triggers watch_value → snap slider
        let changes = ctx.take_changes();
        widget.reactive_dispatch(&changes, &mut ctx);
        assert!((widget.slider_pos - 1.0).abs() < f32::EPSILON);
        assert!(!widget.is_animating());
    }

    /// Regression: the runtime reactive phase reaches the Switch watcher ONLY
    /// through `Widget::reactive_widget()`. If that hook returns `None` (the
    /// trait default), a programmatic `set_value` (via `Handle::update`) records
    /// the change but the watcher never runs — the slider stays un-snapped and
    /// the `-on` class un-rebuilt, so an ON switch renders in the OFF position
    /// (the byte03 Input→Switch parity bug). Drive the dispatch exactly the way
    /// the runtime does: fetch the dispatcher via `reactive_widget()`.
    #[test]
    fn switch_reactive_widget_hook_runs_watcher() {
        let mut widget = Switch::new(false);
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        widget.set_value(true, &mut ctx);
        let changes = ctx.take_changes();

        // Mirror the runtime: only `reactive_widget()` can reach the dispatch.
        // `reactive_widget` is a Widget-trait method (generated by `#[widget(..,
        // reactive)]`); Widget is not imported here, so call it via full path.
        let rw = crate::widgets::Widget::reactive_widget(&mut widget)
            .expect("Switch must expose its ReactiveWidget so the runtime runs watch_value");
        rw.reactive_dispatch(&changes, &mut ctx);

        assert!((widget.slider_pos - 1.0).abs() < f32::EPSILON, "slider must snap to on");
        assert!(
            widget.seed.classes.iter().any(|c| c == "-on"),
            "the `-on` class must be rebuilt after a programmatic set_value"
        );
    }

    #[test]
    fn switch_reactive_set_value_noop_when_same() {
        let mut widget = Switch::new(true);
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        widget.set_value(true, &mut ctx);
        assert!(!ctx.has_changes());
    }

    #[test]
    fn switch_reactive_set_slider_pos_is_var() {
        let mut widget = Switch::new(false);
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        widget.set_slider_pos(0.5, &mut ctx);
        assert!(ctx.has_changes());
        // var fields should not request repaint
        assert!(!ctx.needs_repaint());
    }
}

impl crate::widgets::Components for Switch {
    fn component_classes(&self) -> &[&'static str] {
        &[
            "switch--slider",
        ]
    }
}
