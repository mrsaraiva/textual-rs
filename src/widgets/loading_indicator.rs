use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::style::{Color, parse_color_like};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{adjust_line_length_no_bg, fixed_height_from_constraints},
};

/// An animated loading indicator that displays cycling gradient dots.
///
/// When animation is conceptually disabled (no tick events), falls back to
/// a static "Loading..." text.
///
/// This widget is **not focusable** and blocks all input events during the
/// capture phase so that widgets underneath cannot be interacted with.
///
/// # Default CSS
///
/// ```css
/// LoadingIndicator { width: 1fr; height: 1fr; min-height: 1; fg: $primary; }
/// ```
#[derive(Debug, Clone)]
pub struct LoadingIndicator {
    id: WidgetId,
    /// Tick counter driving the animation cycle.
    tick: u64,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl LoadingIndicator {
    /// Create a new `LoadingIndicator`.
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            tick: 0,
            classes: vec!["loading-indicator".to_string()],
            styles: WidgetStyles::default(),
        }
    }
}

impl Default for LoadingIndicator {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for LoadingIndicator {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        false
    }

    /// Always active so the runtime delivers tick events for animation.
    fn is_active(&self) -> bool {
        true
    }

    fn on_tick(&mut self, tick: u64) {
        self.tick = tick;
    }

    /// Block input events during capture phase (like Python's `on_input` stopper).
    /// Non-input events (Tick, Resize, AppFocus, BindingsChanged) are allowed through.
    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::Key(_)
            | Event::Action(_)
            | Event::MouseDown(_)
            | Event::MouseUp(_)
            | Event::MouseScroll(_) => ctx.set_handled(),
            _ => {}
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        let dot = "\u{25cf}"; // ●
        let dot_count = 5;

        // Resolve foreground/background from CSS.
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);

        let fg = resolved
            .fg
            .or_else(|| parse_color_like("$primary"))
            .unwrap_or(Color::rgb(0, 120, 215));

        let bg = resolved
            .bg
            .or_else(|| parse_color_like("$background"))
            .unwrap_or(Color::rgb(0, 0, 0));

        // Animation: each dot cycles through a gradient from dim to bright.
        // speed controls how fast the cycle moves (ticks → phase).
        let speed = 0.08; // ticks to phase multiplier
        let elapsed = self.tick as f64 * speed;

        let mut text = String::new();
        let mut styles: Vec<(usize, rich_rs::Style)> = Vec::new();

        for i in 0..dot_count {
            // Each dot is offset in phase from the previous.
            let phase = (elapsed - i as f64 / 8.0).rem_euclid(1.0);
            // Quadratic easing: brighter at the leading edge.
            let blend_factor = (1.0 - phase).powi(2);

            // Gradient: from bg blended slightly toward fg (dim) → fg → fg lightened (bright).
            let color = gradient_3stop(bg, fg, lighten(fg, 0.1), blend_factor);

            let start = text.len();
            text.push_str(dot);
            if i + 1 < dot_count {
                text.push(' ');
            }
            let style = rich_rs::Style::new().with_color(color.to_simple_opaque());
            styles.push((start, style));
        }

        // Build segments: one per dot+space pair for proper coloring.
        let mut segs = Vec::new();
        for (idx, (start, style)) in styles.iter().enumerate() {
            let end = if idx + 1 < styles.len() {
                styles[idx + 1].0
            } else {
                text.len()
            };
            let chunk = &text[*start..end];
            segs.push(Segment::styled(chunk.to_string(), *style));
        }

        // Center the dots within the available width.
        let dots_width = rich_rs::cell_len(&text);
        let line = if dots_width < width {
            let left_pad = (width - dots_width) / 2;
            let mut centered: Vec<Segment> = Vec::new();
            if left_pad > 0 {
                centered.push(Segment::styled(" ".repeat(left_pad), rich_rs::Style::new()));
            }
            centered.extend(segs);
            adjust_line_length_no_bg(&centered, width)
        } else {
            adjust_line_length_no_bg(&segs, width)
        };

        let mut out = Segments::new();
        // For multi-line height, center vertically.
        let height = options.size.1.max(1);
        let top_pad = if height > 1 { (height - 1) / 2 } else { 0 };
        let blank = " ".repeat(width);
        for _ in 0..top_pad {
            out.push(Segment::styled(blank.clone(), rich_rs::Style::new()));
            out.push(Segment::line());
        }
        out.extend(line);
        let bottom_pad = height.saturating_sub(top_pad + 1);
        for i in 0..bottom_pad {
            out.push(Segment::line());
            out.push(Segment::styled(blank.clone(), rich_rs::Style::new()));
            let _ = i;
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints())
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }

    fn style_type(&self) -> &'static str {
        "LoadingIndicator"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for LoadingIndicator {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

// ── Color helpers (private) ──────────────────────────────────────────

/// Linear RGB blend between two colors. `t` in 0.0..=1.0.
fn blend_rgb(a: Color, b: Color, t: f64) -> Color {
    let t = t.clamp(0.0, 1.0) as f32;
    let mix = |x: u8, y: u8| -> u8 {
        let xf = x as f32;
        let yf = y as f32;
        (xf + (yf - xf) * t).round().clamp(0.0, 255.0) as u8
    };
    Color::rgb(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b))
}

/// Simple lighten: blend toward white by `amount` (0.0..1.0).
fn lighten(c: Color, amount: f64) -> Color {
    blend_rgb(c, Color::rgb(255, 255, 255), amount)
}

/// 3-stop gradient: bg_blend(0.0) → mid(0.7) → bright(1.0).
/// `t` is the position along the gradient (0.0..=1.0).
fn gradient_3stop(dim: Color, mid: Color, bright: Color, t: f64) -> Color {
    let dim_blended = blend_rgb(dim, mid, 0.1); // bg at 10% toward fg
    if t <= 0.7 {
        blend_rgb(dim_blended, mid, t / 0.7)
    } else {
        blend_rgb(mid, bright, (t - 0.7) / 0.3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_indicator_not_focusable() {
        let li = LoadingIndicator::new();
        assert!(!li.focusable());
    }

    #[test]
    fn loading_indicator_is_active() {
        let li = LoadingIndicator::new();
        assert!(li.is_active());
    }

    #[test]
    fn loading_indicator_style_type() {
        let li = LoadingIndicator::new();
        assert_eq!(li.style_type(), "LoadingIndicator");
    }

    #[test]
    fn loading_indicator_blocks_input_events() {
        use crate::event::Action;
        let mut li = LoadingIndicator::new();
        let event = Event::Action(Action::FocusNext);
        let mut ctx = EventCtx::default();
        li.on_event_capture(&event, &mut ctx);
        assert!(ctx.handled());
    }

    #[test]
    fn loading_indicator_allows_non_input_events() {
        let mut li = LoadingIndicator::new();
        let event = Event::Tick(0);
        let mut ctx = EventCtx::default();
        li.on_event_capture(&event, &mut ctx);
        assert!(!ctx.handled());
    }

    #[test]
    fn blend_rgb_extremes() {
        let a = Color::rgb(0, 0, 0);
        let b = Color::rgb(255, 255, 255);
        let mid = blend_rgb(a, b, 0.5);
        assert_eq!(mid.r, 128);
        assert_eq!(mid.g, 128);
        assert_eq!(mid.b, 128);
    }
}
