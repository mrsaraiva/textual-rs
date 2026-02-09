use std::time::Instant;

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::AnimationLevel;

use super::{
    Widget, WidgetId, WidgetStyles,
    helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints},
};

// ── ETA estimation ──────────────────────────────────────────────────

/// Estimates time to completion based on progress samples.
///
/// Port of Python Textual's `ETA` class — tracks (time, progress) samples
/// and computes speed over a sliding window, then extrapolates remaining time.
#[derive(Debug, Clone)]
struct Eta {
    /// Period (seconds) over which speed is estimated.
    estimation_period: f64,
    /// Maximum seconds of extrapolation after the last sample.
    max_extrapolate: f64,
    /// (time_secs, progress_ratio) samples, sorted by time.
    samples: Vec<(f64, f64)>,
    /// Counter for periodic pruning.
    add_count: u64,
}

impl Eta {
    fn new() -> Self {
        Self {
            estimation_period: 60.0,
            max_extrapolate: 30.0,
            samples: vec![(0.0, 0.0)],
            add_count: 0,
        }
    }

    fn reset(&mut self) {
        self.samples.clear();
    }

    /// Record a progress sample. `progress` is a ratio in `0.0..=1.0`.
    fn add_sample(&mut self, time: f64, progress: f64) {
        // If progress went backwards, reset.
        if let Some(&(_, last_p)) = self.samples.last()
            && last_p > progress
        {
            self.reset();
        }
        self.samples.push((time, progress));
        self.add_count += 1;
        if self.add_count.is_multiple_of(100) {
            self.prune();
        }
    }

    fn prune(&mut self) {
        if self.samples.len() <= 10 {
            return;
        }
        let prune_time = self.samples.last().map(|s| s.0).unwrap_or(0.0) - self.estimation_period;
        // Binary search for the first sample at or after prune_time.
        let index = self
            .samples
            .partition_point(|&(t, _)| t < prune_time);
        if index > 0 {
            self.samples.drain(..index);
        }
    }

    /// Linearly interpolate progress at `time`.
    fn progress_at(&self, time: f64) -> (f64, f64) {
        if self.samples.is_empty() {
            return (0.0, 0.0);
        }
        let index = self.samples.partition_point(|&(t, _)| t < time);
        if index >= self.samples.len() {
            return *self.samples.last().unwrap();
        }
        if index == 0 {
            return self.samples[0];
        }
        let (t1, p1) = self.samples[index - 1];
        let (t2, p2) = self.samples[index];
        let factor = (time - t1) / (t2 - t1);
        (time, p1 + (p2 - p1) * factor)
    }

    /// Current speed (progress-ratio per second), or `None` if insufficient data.
    fn speed(&self) -> Option<f64> {
        if self.samples.len() < 2 {
            return None;
        }
        let &(recent_time, progress2) = self.samples.last().unwrap();
        let (start_time, progress1) = self.progress_at(recent_time - self.estimation_period);
        let time_delta = recent_time - start_time;
        if time_delta < 1.0 {
            return None;
        }
        let distance = progress2 - progress1;
        let speed = if time_delta > 0.0 {
            distance / time_delta
        } else {
            0.0
        };
        if speed <= 0.0 { None } else { Some(speed) }
    }

    /// Estimated seconds until completion, or `None` if unknown.
    fn get_eta(&self, time: f64) -> Option<u64> {
        let speed = self.speed()?;
        let &(recent_time, recent_progress) = self.samples.last()?;
        let remaining = 1.0 - recent_progress;
        if remaining <= 0.0 {
            return Some(0);
        }
        let time_since_sample = (time - recent_time).min(self.max_extrapolate);
        let extrapolated = speed * time_since_sample;
        let eta = ((remaining - extrapolated) / speed).max(1.0);
        Some(eta.ceil() as u64)
    }
}

// ── Format helpers ──────────────────────────────────────────────────

/// Format a percentage for display (e.g. " 50%" or "--%" when unknown).
fn format_percentage(pct: Option<f64>) -> String {
    match pct {
        Some(p) => format!("{:>3}%", (p * 100.0).round() as u64),
        None => "--%".to_string(),
    }
}

/// Format an ETA duration for display (e.g. "01:23:45" or "--:--:--").
fn format_eta(eta_secs: Option<u64>) -> String {
    match eta_secs {
        None => "--:--:--".to_string(),
        Some(0) => "00:00:00".to_string(),
        Some(secs) => {
            let s = secs % 60;
            let m = (secs / 60) % 60;
            let h = secs / 3600;
            if h > 999999 {
                "+999999h".to_string()
            } else if h > 99 {
                format!("{}h", h)
            } else {
                format!("{:02}:{:02}:{:02}", h, m, s)
            }
        }
    }
}

// ── ProgressBar widget ──────────────────────────────────────────────

/// A progress bar widget that displays determinate or indeterminate progress.
///
/// When `total` is `Some`, renders a filled bar proportional to `progress / total`.
/// When `total` is `None`, renders an animated indeterminate sliding highlight
/// driven by [`on_tick`](Widget::on_tick).
///
/// This widget is **not focusable** (display-only).
///
/// # Display toggles
///
/// - `show_bar` — whether to render the bar portion (default: `true`)
/// - `show_percentage` — whether to show percentage text (default: `true`)
/// - `show_eta` — whether to show estimated time remaining (default: `true`)
///
/// # Animation level
///
/// When `animation_level` is set to [`AnimationLevel::None`], the indeterminate
/// bar renders as a static full-width bar instead of animating.
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
    /// Whether to display the bar portion.
    show_bar: bool,
    /// Whether to display a percentage label.
    show_percentage: bool,
    /// Whether to display an ETA countdown.
    show_eta: bool,
    /// Animation level — controls whether indeterminate bar animates.
    animation_level: AnimationLevel,
    /// ETA estimator.
    eta: Eta,
    /// Monotonic reference point for ETA time tracking.
    start_instant: Instant,
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
            show_bar: true,
            show_percentage: true,
            show_eta: true,
            animation_level: AnimationLevel::Full,
            eta: Eta::new(),
            start_instant: Instant::now(),
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
        self.record_eta_sample();
    }

    /// Set the total number of steps.
    pub fn set_total(&mut self, total: Option<f64>) {
        let new_total = total.map(|t| t.max(0.0));
        // Reset ETA when total changes (matching Python behavior).
        if new_total != self.total {
            self.eta.reset();
        }
        self.total = new_total;
    }

    /// Set the current progress.
    pub fn set_progress(&mut self, progress: f64) {
        self.progress = progress;
        self.record_eta_sample();
    }

    /// Batch update: optionally set total, progress, and/or advance.
    pub fn update(
        &mut self,
        total: Option<Option<f64>>,
        progress: Option<f64>,
        advance: Option<f64>,
    ) {
        if let Some(t) = total {
            self.set_total(t);
        }
        if let Some(p) = progress {
            self.progress = p;
            self.record_eta_sample();
        }
        if let Some(a) = advance {
            self.progress += a;
            self.record_eta_sample();
        }
    }

    /// Whether the bar portion is shown.
    pub fn show_bar(&self) -> bool {
        self.show_bar
    }

    /// Set whether to display the bar portion.
    pub fn set_show_bar(&mut self, show: bool) {
        self.show_bar = show;
    }

    /// Whether the percentage label is shown.
    pub fn show_percentage(&self) -> bool {
        self.show_percentage
    }

    /// Set whether to display the percentage label.
    pub fn set_show_percentage(&mut self, show: bool) {
        self.show_percentage = show;
    }

    /// Whether the ETA countdown is shown.
    pub fn show_eta(&self) -> bool {
        self.show_eta
    }

    /// Set whether to display the ETA countdown.
    pub fn set_show_eta(&mut self, show: bool) {
        self.show_eta = show;
    }

    /// Current animation level.
    pub fn animation_level(&self) -> AnimationLevel {
        self.animation_level
    }

    /// Set the animation level (controls indeterminate animation).
    pub fn set_animation_level(&mut self, level: AnimationLevel) {
        self.animation_level = level;
    }

    /// Estimated seconds until completion, or `None` if unknown.
    pub fn eta_seconds(&self) -> Option<u64> {
        self.total?;
        let now = self.elapsed_secs();
        self.eta.get_eta(now)
    }

    // ── Internal helpers ────────────────────────────────────────────

    fn elapsed_secs(&self) -> f64 {
        self.start_instant.elapsed().as_secs_f64()
    }

    fn record_eta_sample(&mut self) {
        if let Some(total) = self.total
            && total > 0.0
        {
            let time = self.elapsed_secs();
            let ratio = (self.progress / total).clamp(0.0, 1.0);
            self.eta.add_sample(time, ratio);
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

        // When animations are disabled, show a static full-width bar (matching Python).
        if self.animation_level == AnimationLevel::None {
            let text = "█".repeat(width);
            return (text, component);
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

    /// Indeterminate bars report as active so the runtime repaints on every tick,
    /// unless animation is disabled.
    fn is_active(&self) -> bool {
        self.total.is_none() && self.animation_level != AnimationLevel::None
    }

    fn on_tick(&mut self, tick: u64) {
        self.tick = tick;
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let total_width = options.size.0.max(1);

        // Build suffix parts (percentage and/or ETA).
        let mut suffix = String::new();
        if self.show_percentage {
            suffix.push_str(&format_percentage(self.percentage()));
        }
        if self.show_eta {
            if !suffix.is_empty() {
                suffix.push(' ');
            }
            let eta_secs = if self.total.is_some() {
                let now = self.elapsed_secs();
                self.eta.get_eta(now)
            } else {
                None
            };
            suffix.push_str(&format_eta(eta_secs));
        }

        // Compute bar width (leave room for suffix with a space separator).
        let suffix_width = if suffix.is_empty() { 0 } else { suffix.len() + 1 };
        let bar_width = if self.show_bar {
            total_width.saturating_sub(suffix_width)
        } else {
            0
        };

        let mut out = Segments::new();

        if self.show_bar && bar_width > 0 {
            let (text, component) = if self.total.is_some() {
                self.render_determinate(bar_width)
            } else {
                self.render_indeterminate(bar_width)
            };

            let style = crate::css::resolve_component_style(self, &[component])
                .to_rich()
                .unwrap_or_else(rich_rs::Style::new);

            let line = adjust_line_length_no_bg(&[Segment::styled(text, style)], bar_width);
            out.extend(line);
        }

        if !suffix.is_empty() {
            let has_bar = self.show_bar && bar_width > 0;
            if has_bar {
                out.push(Segment::new(" "));
            }
            // Pad or truncate suffix to fill remaining width.
            let sep = usize::from(has_bar);
            let remaining = total_width.saturating_sub(bar_width + sep);
            let padded = if suffix.len() < remaining {
                format!("{:>width$}", suffix, width = remaining)
            } else {
                suffix[..remaining.min(suffix.len())].to_string()
            };
            out.push(Segment::new(padded));
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
    }

    fn content_width(&self) -> Option<usize> {
        // Base bar width (Python default: 32) + suffix widths.
        let mut width = if self.show_bar { 32 } else { 0 };
        if self.show_percentage {
            // " NNN%" = 4 chars + 1 separator
            width += if width > 0 { 5 } else { 4 };
        }
        if self.show_eta {
            // " HH:MM:SS" = 8 chars + 1 separator
            width += if width > 0 { 9 } else { 8 };
        }
        Some(width.max(1))
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

    // ── New tests for uplift features ───────────────────────────────

    #[test]
    fn show_toggles_default_true() {
        let bar = ProgressBar::new(Some(100.0));
        assert!(bar.show_bar());
        assert!(bar.show_percentage());
        assert!(bar.show_eta());
    }

    #[test]
    fn show_toggles_setters() {
        let mut bar = ProgressBar::new(Some(100.0));
        bar.set_show_bar(false);
        bar.set_show_percentage(false);
        bar.set_show_eta(false);
        assert!(!bar.show_bar());
        assert!(!bar.show_percentage());
        assert!(!bar.show_eta());
    }

    #[test]
    fn animation_level_default_full() {
        let bar = ProgressBar::new(None);
        assert_eq!(bar.animation_level(), AnimationLevel::Full);
    }

    #[test]
    fn animation_level_none_static_indeterminate() {
        let mut bar = ProgressBar::new(None);
        bar.set_animation_level(AnimationLevel::None);
        let (text, component) = bar.render_indeterminate(10);
        // Static full-width bar when animations disabled.
        assert_eq!(text, "██████████");
        assert_eq!(component, "bar--indeterminate");
    }

    #[test]
    fn animation_level_none_not_active() {
        let mut bar = ProgressBar::new(None);
        bar.set_animation_level(AnimationLevel::None);
        // Should NOT report as active when animation is disabled.
        assert!(!bar.is_active());
    }

    #[test]
    fn animation_level_full_indeterminate_is_active() {
        let bar = ProgressBar::new(None);
        assert!(bar.is_active());
    }

    #[test]
    fn format_percentage_display() {
        assert_eq!(format_percentage(Some(0.0)), "  0%");
        assert_eq!(format_percentage(Some(0.5)), " 50%");
        assert_eq!(format_percentage(Some(1.0)), "100%");
        assert_eq!(format_percentage(None), "--%");
    }

    #[test]
    fn format_eta_display() {
        assert_eq!(format_eta(None), "--:--:--");
        assert_eq!(format_eta(Some(0)), "00:00:00");
        assert_eq!(format_eta(Some(61)), "00:01:01");
        assert_eq!(format_eta(Some(3661)), "01:01:01");
        assert_eq!(format_eta(Some(360000)), "100h");
        assert_eq!(format_eta(Some(u64::MAX)), "+999999h");
    }

    #[test]
    fn eta_no_estimate_without_samples() {
        let bar = ProgressBar::new(Some(100.0));
        // No advance calls = no speed data = no ETA.
        assert!(bar.eta_seconds().is_none());
    }

    #[test]
    fn eta_basic_estimation() {
        let mut eta = Eta::new();
        // Simulate samples over time: at t=0 we have 0%, at t=10 we have 50%.
        eta.add_sample(0.0, 0.0);
        eta.add_sample(10.0, 0.5);
        // Speed = 0.5/10 = 0.05 per sec. Remaining = 0.5. ETA = 0.5/0.05 = 10s.
        let result = eta.get_eta(10.0);
        assert_eq!(result, Some(10));
    }

    #[test]
    fn eta_complete_returns_zero() {
        let mut eta = Eta::new();
        eta.add_sample(0.0, 0.0);
        eta.add_sample(10.0, 1.0);
        let result = eta.get_eta(10.0);
        assert_eq!(result, Some(0));
    }

    #[test]
    fn eta_reset_on_backwards_progress() {
        let mut eta = Eta::new();
        eta.add_sample(0.0, 0.0);
        eta.add_sample(5.0, 0.5);
        // Progress goes backwards -> should reset.
        eta.add_sample(6.0, 0.1);
        // After reset + one sample, speed cannot be calculated (need > 1s span).
        assert!(eta.speed().is_none());
    }

    #[test]
    fn eta_prunes_old_samples() {
        let mut eta = Eta::new();
        // Add many samples spanning a long period.
        for i in 0..250 {
            let t = i as f64;
            let p = (i as f64 / 250.0).min(1.0);
            eta.add_sample(t, p);
        }
        // After pruning, samples older than (last_time - estimation_period) are removed.
        // With estimation_period=60, samples before t=189 should be pruned.
        assert!(eta.samples.len() < 250);
    }

    #[test]
    fn content_width_varies_with_toggles() {
        let mut bar = ProgressBar::new(Some(100.0));
        // All on: bar(32) + percentage(5) + eta(9) = 46
        assert_eq!(bar.content_width(), Some(46));

        bar.set_show_percentage(false);
        // bar(32) + eta(9) = 41
        assert_eq!(bar.content_width(), Some(41));

        bar.set_show_eta(false);
        // bar only = 32
        assert_eq!(bar.content_width(), Some(32));

        bar.set_show_bar(false);
        // Nothing visible => min 1
        assert_eq!(bar.content_width(), Some(1));
    }

    #[test]
    fn set_total_resets_eta() {
        let mut bar = ProgressBar::new(Some(100.0));
        bar.advance(50.0);
        // Change total — ETA should reset.
        bar.set_total(Some(200.0));
        assert_eq!(bar.total(), Some(200.0));
        // After reset, no speed data => no ETA.
        assert!(bar.eta_seconds().is_none());
    }

    #[test]
    fn update_resets_eta_on_total_change() {
        let mut bar = ProgressBar::new(Some(100.0));
        bar.advance(50.0);
        bar.update(Some(Some(200.0)), None, None);
        assert_eq!(bar.total(), Some(200.0));
    }

    #[test]
    fn tiny_width_suffix_only_no_underfill() {
        // When bar_width=0 due to narrow total_width, suffix should still fill correctly.
        let mut bar = ProgressBar::new(Some(100.0));
        bar.set_show_bar(true);
        bar.set_show_percentage(true);
        bar.set_show_eta(false);

        let console = Console::new();
        let mut opts = console.options().clone();
        // Very narrow: suffix " 50%" is 4 chars, separator would be 1, leaving 0 for bar.
        opts.size.0 = 4;
        bar.advance(50.0);

        let segs = Widget::render(&bar, &console, &opts);
        let total_chars: usize = segs.iter().map(|s| s.text.chars().count()).sum();
        // Should exactly fill the allocated width (4), not underfill.
        assert_eq!(total_chars, 4);
    }
}
