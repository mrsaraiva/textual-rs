use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Action, Event, EventCtx};
use crate::message::*;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

use super::{
    Widget, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

/// The visual width of the switch slider track (in cells).
const SWITCH_WIDTH: usize = 8;

/// Duration of the slide animation in ticks (~60Hz assumed, so 18 ticks ~ 0.3s).
const ANIMATION_TICKS: u64 = 18;

/// A boolean toggle switch widget.
///
/// Renders as a slider track with a knob that smoothly animates left/right.
/// Toggled via click, Enter, or Space.
#[derive(Debug, Clone)]
pub struct Switch {
    value: bool,
    focused: bool,
    hovered: bool,
    pressed: bool,
    disabled: bool,
    /// Animated slider position: 0.0 = off (left), 1.0 = on (right).
    slider_pos: f32,
    /// Animation target (0.0 or 1.0).
    slider_target: f32,
    /// Tick when animation started, None if not animating.
    anim_start_tick: Option<u64>,
    /// Position at start of animation.
    anim_start_pos: f32,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl Switch {
    pub fn new(value: bool) -> Self {
        let pos = if value { 1.0 } else { 0.0 };
        Self {
            value,
            focused: false,
            hovered: false,
            pressed: false,
            disabled: false,
            slider_pos: pos,
            slider_target: pos,
            anim_start_tick: None,
            anim_start_pos: pos,
            classes: Vec::new(),
            focused_classes: Vec::new(),
            styles: WidgetStyles::default(),
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

    fn watch_value(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        // Snap slider immediately (programmatic change, no animation).
        self.slider_target = if self.value { 1.0 } else { 0.0 };
        self.slider_pos = self.slider_target;
        self.anim_start_tick = None;
        self.rebuild_classes_in_place();
    }

    // ── Builder methods ──────────────────────────────────────────────────

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.rebuild_classes()
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    fn emit_changed(&self, ctx: &mut EventCtx) {
        ctx.post_message(Message::SwitchChanged(SwitchChanged { value: self.value }));
    }

    /// Called after an interactive toggle (from event handler).
    /// Starts the slider animation and rebuilds CSS classes.
    fn on_toggled(&mut self) {
        self.slider_target = if self.value { 1.0 } else { 0.0 };
        // Mark animation start; actual tick will be recorded in on_tick.
        self.anim_start_pos = self.slider_pos;
        // Use a sentinel to indicate "start next tick".
        self.anim_start_tick = Some(u64::MAX);
        self.rebuild_classes_in_place();
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
        let mut focused_classes = classes.clone();
        focused_classes.push("focused".to_string());
        self.classes = classes;
        self.focused_classes = focused_classes;
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
            match change.field_name {
                "value" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_value(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
    }
}

impl Widget for Switch {
    fn focusable(&self) -> bool {
        !self.disabled
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn set_disabled_state(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn is_active(&self) -> bool {
        self.pressed && self.hovered
    }

    fn content_width(&self) -> Option<usize> {
        Some(SWITCH_WIDTH)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                self.pressed = true;
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(mouse) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                    if mouse.target.is_some_and(|t| t == self.node_id()) {
                        self.value = !self.value;
                        self.on_toggled();
                        self.emit_changed(ctx);
                        ctx.set_handled();
                    }
                }
            }
            Event::AppFocus(false) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                }
            }
            Event::Action(Action::Toggle) if self.focused => {
                self.value = !self.value;
                self.on_toggled();
                self.emit_changed(ctx);
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.value = !self.value;
                    self.on_toggled();
                    self.emit_changed(ctx);
                    ctx.request_repaint();
                    ctx.set_handled();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if let Some(start) = self.anim_start_tick {
            if start == u64::MAX {
                // First tick of animation — record actual start.
                self.anim_start_tick = Some(tick);
                self.anim_start_pos = self.slider_pos;
            } else {
                let elapsed = tick.saturating_sub(start);
                let t = (elapsed as f32 / ANIMATION_TICKS as f32).min(1.0);
                // Ease-out cubic: 1 - (1 - t)^3
                let eased = 1.0 - (1.0 - t).powi(3);
                self.slider_pos =
                    self.anim_start_pos + (self.slider_target - self.anim_start_pos) * eased;
                if t >= 1.0 {
                    self.slider_pos = self.slider_target;
                    self.anim_start_tick = None;
                }
            }
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        let slider_style = crate::css::resolve_component_style(self, &["switch--slider"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        let track_inner = width.saturating_sub(2); // minus left/right border chars
        if track_inner == 0 {
            let mut out = Segments::new();
            out.push(Segment::styled(
                rich_rs::set_cell_size("▐▌", width),
                slider_style,
            ));
            return out;
        }

        // The knob occupies ~half the track, positioned by slider_pos.
        let knob_size = (track_inner / 2).max(1);
        let slide_range = track_inner.saturating_sub(knob_size) as f32;
        let knob_offset_f = self.slider_pos * slide_range;
        let knob_start = knob_offset_f as usize;

        // Fractional part for sub-cell rendering
        let frac = knob_offset_f - knob_start as f32;

        let mut track = String::with_capacity(track_inner + 2);
        track.push_str("▐");

        for i in 0..track_inner {
            if i == knob_start && knob_start < track_inner {
                // Leading edge with fractional rendering
                if frac < 0.25 {
                    track.push('█');
                } else if frac < 0.75 {
                    track.push('▐');
                } else {
                    track.push(' ');
                }
            } else if i > knob_start && i < knob_start + knob_size {
                track.push('█');
            } else if i == knob_start + knob_size
                && knob_size > 0
                && knob_start + knob_size <= track_inner
            {
                // Trailing edge with fractional rendering
                if frac < 0.25 {
                    track.push(' ');
                } else if frac < 0.75 {
                    track.push('▌');
                } else {
                    track.push('█');
                }
            } else {
                track.push(' ');
            }
        }

        track.push_str("▌");

        let line = rich_rs::set_cell_size(&track, width);
        let mut out = Segments::new();
        out.push(Segment::styled(line, slider_style));
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
    }

    fn style_classes(&self) -> &[String] {
        if self.focused {
            &self.focused_classes
        } else if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn style_type(&self) -> &'static str {
        "Switch"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Switch {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseDownEvent;
    use crate::keys::KeyEventData;
    use crate::node_id::NodeId;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use slotmap::SlotMap;

    fn make_node_id() -> NodeId {
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    #[test]
    fn switch_space_toggles_and_emits_message() {
        let mut widget = Switch::new(false);
        widget.set_focus(true);
        let mut ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        widget.on_event(&Event::Key(key), &mut ctx);
        assert!(widget.value());
        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| matches!(
            m.message,
            Message::SwitchChanged(SwitchChanged { value: true })
        )));
    }

    #[test]
    fn switch_disabled_ignores_input() {
        let mut widget = Switch::new(false).disabled(true);
        widget.set_focus(true);
        let mut ctx = EventCtx::default();
        widget.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
        assert!(!widget.value());
        assert!(!ctx.handled());
    }

    #[test]
    fn switch_animation_progresses_on_tick() {
        let mut widget = Switch::new(false);
        widget.set_focus(true);
        let mut ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        widget.on_event(&Event::Key(key), &mut ctx);
        assert!(widget.value());
        // Animation should be pending.
        assert!(widget.is_animating());

        // Simulate ticks.
        widget.on_tick(1);
        widget.on_tick(2);
        assert!(widget.is_animating());

        // After enough ticks, animation completes.
        for tick in 3..=30 {
            widget.on_tick(tick);
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
