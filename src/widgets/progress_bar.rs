use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints},
};

/// A progress bar widget that displays determinate or indeterminate progress.
///
/// When `total` is `Some`, renders a filled bar proportional to `progress / total`.
/// When `total` is `None`, renders an animated indeterminate sliding highlight
/// driven by [`on_tick`](Widget::on_tick).
///
/// This widget is **not focusable** (display-only).
///
/// # Component classes
///
/// | Class | Description |
/// | :--- | :--- |
/// | `bar--bar` | The bar in its normal (incomplete) state. |
/// | `bar--complete` | The bar when progress reaches 100%. |
/// | `bar--indeterminate` | The bar when total is unknown. |
#[derive(Debug, Clone)]
pub struct ProgressBar {
    id: WidgetId,
    /// Total number of steps, or `None` for indeterminate.
    total: Option<f64>,
    /// Current progress (number of steps completed).
    progress: f64,
    /// Tick counter for indeterminate animation.
    tick: u64,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl ProgressBar {
    /// Create a new `ProgressBar`.
    ///
    /// Pass `Some(total)` for a determinate bar, or `None` for indeterminate.
    pub fn new(total: Option<f64>) -> Self {
        Self {
            id: WidgetId::new(),
            total: total.map(|t| t.max(0.0)),
            progress: 0.0,
            tick: 0,
            classes: vec!["progress-bar".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    // ── Public API ──────────────────────────────────────────────────

    /// Current progress value.
    pub fn progress(&self) -> f64 {
        self.progress
    }

    /// Current total, or `None` if indeterminate.
    pub fn total(&self) -> Option<f64> {
        self.total
    }

    /// The percentage of completion as a value in `0.0..=1.0`, or `None` if indeterminate.
    pub fn percentage(&self) -> Option<f64> {
        match self.total {
            Some(total) if total > 0.0 => Some((self.progress / total).clamp(0.0, 1.0)),
            Some(_) => Some(1.0), // total == 0 means complete
            None => None,
        }
    }

    /// Advance progress by `amount` steps.
    pub fn advance(&mut self, amount: f64) {
        self.progress += amount;
    }

    /// Set the total number of steps.
    pub fn set_total(&mut self, total: Option<f64>) {
        self.total = total.map(|t| t.max(0.0));
    }

    /// Set the current progress.
    pub fn set_progress(&mut self, progress: f64) {
        self.progress = progress;
    }

    /// Batch update: optionally set total, progress, and/or advance.
    pub fn update(
        &mut self,
        total: Option<Option<f64>>,
        progress: Option<f64>,
        advance: Option<f64>,
    ) {
        if let Some(t) = total {
            self.total = t.map(|v| v.max(0.0));
        }
        if let Some(p) = progress {
            self.progress = p;
        }
        if let Some(a) = advance {
            self.progress += a;
        }
    }

    // ── Rendering helpers ───────────────────────────────────────────

    fn render_determinate(&self, width: usize) -> (String, &str) {
        let pct = self.percentage().unwrap_or(0.0);
        let component = if pct >= 1.0 {
            "bar--complete"
        } else {
            "bar--bar"
        };

        let filled_cells = (pct * width as f64).round() as usize;
        let filled = filled_cells.min(width);
        let empty = width.saturating_sub(filled);
        let text = format!("{}{}", "█".repeat(filled), " ".repeat(empty));
        (text, component)
    }

    fn render_indeterminate(&self, width: usize) -> (String, &str) {
        let component = "bar--indeterminate";

        if width == 0 {
            return (String::new(), component);
        }

        let highlight_width = (width as f64 * 0.25).max(1.0) as usize;
        let total_travel = width + highlight_width;

        // Bounce the highlight back and forth.
        let cycle = 2 * total_travel;
        let pos = (self.tick as usize) % cycle.max(1);
        let raw_start = if pos < total_travel {
            pos as isize - highlight_width as isize
        } else {
            (2 * total_travel - pos) as isize - highlight_width as isize
        };

        let start = raw_start.max(0) as usize;
        let end = ((raw_start + highlight_width as isize) as usize).min(width);

        let mut text = String::with_capacity(width * 3);
        for i in 0..width {
            if i >= start && i < end {
                text.push('█');
            } else {
                text.push(' ');
            }
        }
        (text, component)
    }
}

impl Widget for ProgressBar {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        false
    }

    /// Indeterminate bars report as active so the runtime repaints on every tick.
    fn is_active(&self) -> bool {
        self.total.is_none()
    }

    fn on_tick(&mut self, tick: u64) {
        self.tick = tick;
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        let (text, component) = if self.total.is_some() {
            self.render_determinate(width)
        } else {
            self.render_indeterminate(width)
        };

        let style = crate::css::resolve_component_style(self, &[component])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);

        let line = adjust_line_length_no_bg(&[Segment::styled(text, style)], width);
        let mut out = Segments::new();
        out.extend(line);
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
    }

    fn content_width(&self) -> Option<usize> {
        Some(32) // Default width matching Python Textual
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn style_type(&self) -> &'static str {
        "ProgressBar"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for ProgressBar {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_percentage_none_when_indeterminate() {
        let bar = ProgressBar::new(None);
        assert!(bar.percentage().is_none());
    }

    #[test]
    fn progress_bar_percentage_zero_total() {
        let bar = ProgressBar::new(Some(0.0));
        assert_eq!(bar.percentage(), Some(1.0));
    }

    #[test]
    fn progress_bar_percentage_half() {
        let mut bar = ProgressBar::new(Some(100.0));
        bar.advance(50.0);
        assert_eq!(bar.percentage(), Some(0.5));
    }

    #[test]
    fn progress_bar_percentage_clamped() {
        let mut bar = ProgressBar::new(Some(100.0));
        bar.advance(200.0);
        assert_eq!(bar.percentage(), Some(1.0));
    }

    #[test]
    fn progress_bar_advance() {
        let mut bar = ProgressBar::new(Some(10.0));
        bar.advance(3.0);
        bar.advance(2.0);
        assert_eq!(bar.progress(), 5.0);
    }

    #[test]
    fn progress_bar_update_batch() {
        let mut bar = ProgressBar::new(None);
        bar.update(Some(Some(50.0)), Some(10.0), Some(5.0));
        assert_eq!(bar.total(), Some(50.0));
        assert_eq!(bar.progress(), 15.0);
    }

    #[test]
    fn progress_bar_not_focusable() {
        let bar = ProgressBar::new(Some(100.0));
        assert!(!bar.focusable());
    }

    #[test]
    fn progress_bar_determinate_render_text() {
        let mut bar = ProgressBar::new(Some(100.0));
        bar.advance(50.0);
        let (text, component) = bar.render_determinate(10);
        // 50% of 10 = 5 filled, 5 empty
        assert_eq!(text, "█████     ");
        assert_eq!(component, "bar--bar");
    }

    #[test]
    fn progress_bar_complete_component_class() {
        let mut bar = ProgressBar::new(Some(100.0));
        bar.advance(100.0);
        let (_, component) = bar.render_determinate(10);
        assert_eq!(component, "bar--complete");
    }

    #[test]
    fn progress_bar_indeterminate_render_bounces() {
        let mut bar = ProgressBar::new(None);
        // At tick 0, the highlight should start near the left.
        bar.tick = 0;
        let (text0, component) = bar.render_indeterminate(20);
        assert_eq!(component, "bar--indeterminate");
        assert_eq!(text0.chars().count(), 20);
    }

    #[test]
    fn progress_bar_negative_total_clamped() {
        let bar = ProgressBar::new(Some(-5.0));
        assert_eq!(bar.total(), Some(0.0));
        assert_eq!(bar.percentage(), Some(1.0));
    }
}
