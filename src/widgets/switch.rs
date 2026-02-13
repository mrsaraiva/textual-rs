use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::*;

use crate::node_id::NodeId;

use super::{
    Widget, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
    option_list::toggle_option::BinaryToggleState,
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
    state: BinaryToggleState,
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
            state: BinaryToggleState::new(value),
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

    pub fn value(&self) -> bool {
        self.state.value()
    }

    pub fn set_value(&mut self, value: bool) {
        if self.state.value() != value {
            self.state.set_value(value);
            self.slider_target = if value { 1.0 } else { 0.0 };
            // Snap immediately when set programmatically.
            self.slider_pos = self.slider_target;
            self.anim_start_tick = None;
            self.rebuild_classes_in_place();
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.set_disabled(disabled);
        self.rebuild_classes()
    }

    fn emit_changed(&self, ctx: &mut EventCtx) {
        ctx.post_message(
            Message::SwitchChanged(SwitchChanged {
                value: self.state.value(),
            }),
        );
    }

    fn on_toggled(&mut self) {
        self.slider_target = if self.state.value() { 1.0 } else { 0.0 };
        // Mark animation start; actual tick will be recorded in on_tick.
        self.anim_start_tick = None;
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
        if self.state.value() {
            classes.push("-on".to_string());
        } else {
            classes.push("-off".to_string());
        }
        if self.state.disabled() {
            classes.push("disabled".to_string());
        }
        let mut focused_classes = classes.clone();
        focused_classes.push("focused".to_string());
        self.classes = classes;
        self.focused_classes = focused_classes;
    }

    fn is_animating(&self) -> bool {
        (self.slider_pos - self.slider_target).abs() > f32::EPSILON
    }
}

impl Widget for Switch {
    fn focusable(&self) -> bool {
        self.state.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.state.set_focused(focused);
    }

    fn has_focus(&self) -> bool {
        self.state.focused()
    }

    fn is_disabled(&self) -> bool {
        self.state.disabled()
    }

    fn is_hovered(&self) -> bool {
        self.state.hovered()
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.state.set_hovered(hovered);
    }

    fn is_active(&self) -> bool {
        self.state.is_active()
    }

    fn content_width(&self) -> Option<usize> {
        Some(SWITCH_WIDTH)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        let outcome = self.state.handle_event(event, NodeId::default());
        if outcome.toggled {
            self.on_toggled();
            self.emit_changed(ctx);
        }
        if outcome.repaint {
            ctx.request_repaint();
        }
        if outcome.handled {
            ctx.set_handled();
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
            } else if i == knob_start + knob_size && knob_size > 0 && knob_start + knob_size <= track_inner {
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
        if self.state.focused() {
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
        assert!(
            messages
                .iter()
                .any(|m| matches!(m.message, Message::SwitchChanged(SwitchChanged { value: true })))
        );
    }

    #[test]
    fn switch_disabled_ignores_input() {
        let mut widget = Switch::new(false).disabled(true);
        widget.set_focus(true);
        let mut ctx = EventCtx::default();
        widget.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(), // TODO(P1-14 integration): use WidgetTree-assigned NodeId
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
    fn switch_set_value_snaps_position() {
        let mut widget = Switch::new(false);
        widget.set_value(true);
        assert!((widget.slider_pos - 1.0).abs() < f32::EPSILON);
        assert!(!widget.is_animating());
    }
}
