use std::time::Instant;

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::AnimationLevel;
use crate::renderables::{Bar, LinearGradient};
#[cfg(test)]
use crate::style::Color;

use super::{NodeSeed, Widget, helpers::adjust_line_length_no_bg};
use crate::compose::ComposeResult;
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

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
        if self.add_count % 100 == 0 {
            self.prune();
        }
    }

    fn prune(&mut self) {
        if self.samples.len() <= 10 {
            return;
        }
        let prune_time = self.samples.last().map(|s| s.0).unwrap_or(0.0) - self.estimation_period;
        // Binary search for the first sample at or after prune_time.
        let index = self.samples.partition_point(|&(t, _)| t < prune_time);
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
/// # Gradient
///
/// Use [`with_gradient`](ProgressBar::with_gradient) to set a linear color gradient
/// across the filled portion of the bar. Each cell is colored by linearly
/// interpolating between the start and end colors based on its position.
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
    /// Total number of steps, or `None` for indeterminate.
    total: Option<f64>,
    /// Current progress (number of steps completed).
    progress: f64,
    /// Whether to display the bar portion.
    show_bar: bool,
    /// Whether to display a percentage label.
    show_percentage: bool,
    /// Whether to display an ETA countdown.
    show_eta: bool,
    /// Animation level — controls whether indeterminate bar animates.
    animation_level: AnimationLevel,
    /// Optional multi-stop gradient painted across the filled portion of the bar.
    ///
    /// Mirrors Python `Gradient` (arbitrary stops via `Gradient.from_colors`).
    gradient: Option<LinearGradient>,
    /// ETA estimator.
    eta: Eta,
    /// Monotonic reference point for ETA time tracking.
    start_instant: Instant,
    seed: NodeSeed,
}

impl ProgressBar {
    crate::seed_ident_methods!();

    /// Create a new `ProgressBar`.
    ///
    /// Pass `Some(total)` for a determinate bar, or `None` for indeterminate.
    pub fn new(total: Option<f64>) -> Self {
        let mut seed = NodeSeed::default();
        seed.classes.push("progress-bar".to_string());
        Self {
            total: total.map(|t| t.max(0.0)),
            progress: 0.0,
            show_bar: true,
            show_percentage: true,
            show_eta: true,
            animation_level: AnimationLevel::Full,
            gradient: None,
            eta: Eta::new(),
            start_instant: Instant::now(),
            seed,
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

    // ── Reactive setters ─────────────────────────────────────────────

    /// Reactive setter for `total`. Records the change in the provided
    /// [`ReactiveCtx`] and triggers repaint.
    pub fn set_total(&mut self, total: Option<f64>, ctx: &mut ReactiveCtx) {
        let new_total = total.map(|t| t.max(0.0));
        if self.total != new_total {
            let old = self.total;
            self.total = new_total;
            ctx.record_change(
                "total",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(self.total),
            );
        }
    }

    /// Reactive setter for `progress`. Records the change in the provided
    /// [`ReactiveCtx`] and triggers repaint.
    pub fn set_progress(&mut self, progress: f64, ctx: &mut ReactiveCtx) {
        if self.progress != progress {
            let old = self.progress;
            self.progress = progress;
            ctx.record_change(
                "progress",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(self.progress),
            );
        }
    }

    /// Reactive setter for `show_bar`. Records the change in the provided
    /// [`ReactiveCtx`] and triggers layout invalidation.
    pub fn set_show_bar(&mut self, show: bool, ctx: &mut ReactiveCtx) {
        if self.show_bar != show {
            let old = self.show_bar;
            self.show_bar = show;
            ctx.record_change(
                "show_bar",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(show),
            );
        }
    }

    /// Reactive setter for `show_percentage`. Records the change in the provided
    /// [`ReactiveCtx`] and triggers layout invalidation.
    pub fn set_show_percentage(&mut self, show: bool, ctx: &mut ReactiveCtx) {
        if self.show_percentage != show {
            let old = self.show_percentage;
            self.show_percentage = show;
            ctx.record_change(
                "show_percentage",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(show),
            );
        }
    }

    /// Reactive setter for `show_eta`. Records the change in the provided
    /// [`ReactiveCtx`] and triggers layout invalidation.
    pub fn set_show_eta(&mut self, show: bool, ctx: &mut ReactiveCtx) {
        if self.show_eta != show {
            let old = self.show_eta;
            self.show_eta = show;
            ctx.record_change(
                "show_eta",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(show),
            );
        }
    }

    // ── Reactive getters ─────────────────────────────────────────────

    /// Whether the bar portion is shown.
    pub fn show_bar(&self) -> bool {
        self.show_bar
    }

    /// Whether the percentage label is shown.
    pub fn show_percentage(&self) -> bool {
        self.show_percentage
    }

    /// Whether the ETA countdown is shown.
    pub fn show_eta(&self) -> bool {
        self.show_eta
    }

    // ── Watchers ─────────────────────────────────────────────────────

    fn watch_total(&mut self, _old: &Option<f64>, _new: &Option<f64>, _ctx: &mut ReactiveCtx) {
        // Reset ETA when total changes (matching Python behavior).
        self.eta.reset();
    }

    fn watch_progress(&mut self, _old: &f64, _new: &f64, _ctx: &mut ReactiveCtx) {
        self.record_eta_sample();
    }

    /// Batch update: optionally set total, progress, and/or advance.
    ///
    /// This is a convenience method that uses direct field assignment
    /// (bypassing the reactive system) for batch mutations.
    pub fn update(
        &mut self,
        total: Option<Option<f64>>,
        progress: Option<f64>,
        advance: Option<f64>,
    ) {
        if let Some(t) = total {
            let new_total = t.map(|v| v.max(0.0));
            if new_total != self.total {
                self.eta.reset();
            }
            self.total = new_total;
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

    /// Current animation level.
    pub fn animation_level(&self) -> AnimationLevel {
        self.animation_level
    }

    /// Set the animation level (controls indeterminate animation).
    pub fn set_animation_level(&mut self, level: AnimationLevel) {
        self.animation_level = level;
    }

    /// Current gradient, if set.
    pub fn gradient(&self) -> Option<&LinearGradient> {
        self.gradient.as_ref()
    }

    /// Set a multi-stop gradient painted across the filled portion of the bar.
    ///
    /// Pass `None` to remove the gradient and fall back to CSS component styling.
    pub fn set_gradient(&mut self, gradient: Option<LinearGradient>) {
        self.gradient = gradient;
    }

    /// Builder: attach a multi-stop gradient to this bar.
    ///
    /// Mirrors Python `ProgressBar(gradient=Gradient.from_colors(...))`.
    pub fn with_gradient(mut self, gradient: LinearGradient) -> Self {
        self.gradient = Some(gradient);
        self
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
        let style = crate::css::resolve_component_style(self, &[component])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        // Python passes the FRACTIONAL highlight extent (`size.width * percentage`)
        // to the Bar renderable, which rounds to the nearest half-cell (`╸`/`╺`).
        // Pre-rounding to an integer here would drop that half-cell precision.
        let highlight_end = (pct * width as f64).min(width as f64) as f32;
        let text: String = Bar::new((0.0, highlight_end), style, style)
            .width(width)
            .render_for_width(width)
            .iter()
            .map(|seg| seg.text.as_ref())
            .collect();
        (text, component)
    }

    /// Render a determinate bar with per-cell multi-stop gradient coloring.
    ///
    /// Uses `Bar` to produce the correct glyph structure (filled bar + half-cell
    /// boundary), then re-colors each filled-bar cell using `gradient.get_color`
    /// sampled at the cell's fractional position across the full bar width.
    ///
    /// Mirrors Python `Bar(gradient=gradient)` which calls `Gradient.get_color`
    /// per highlighted cell.
    fn render_determinate_gradient(
        &self,
        width: usize,
        gradient: &LinearGradient,
    ) -> (Vec<Segment>, &str) {
        let pct = self.percentage().unwrap_or(0.0);
        let component = if pct >= 1.0 {
            "bar--complete"
        } else {
            "bar--bar"
        };

        let style = crate::css::resolve_component_style(self, &[component])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let highlight_end = (pct * width as f64).min(width as f64) as f32;

        // Render via Bar to get correct glyph structure (half-cell boundary glyphs).
        let base_segs: Vec<Segment> = Bar::new((0.0, highlight_end), style, style)
            .width(width)
            .render_for_width(width)
            .into_iter()
            .collect();

        // Re-color per-cell: mirrors Python `_apply_gradient`.
        //
        // Python applies the gradient REVERSED, keyed off the highlighted
        // (text) length — NOT the absolute cell position:
        //
        //   text_length = len(highlight_bar)   # highlighted cells only
        //   for offset in range(text_length):
        //       bar_offset = text_length - offset    # counts DOWN: high left
        //       t = bar_offset / (width - 1)
        //
        // So the leftmost highlighted cell gets the HIGHEST t value and the
        // rightmost gets the LOWEST — the gradient runs in reverse direction
        // relative to reading order.  `get_color` clamps t to [0, 1], so
        // t > 1 (possible when the bar is partially filled) is handled correctly.

        // First pass: count total highlighted cells.
        let highlighted_count: usize = base_segs
            .iter()
            .filter(|seg| seg.control.is_none() && seg.style.unwrap_or(style) == style)
            .map(|seg| seg.text.chars().count())
            .sum();

        let max_width = width.saturating_sub(1);

        let mut out: Vec<Segment> = Vec::with_capacity(base_segs.len());
        let mut highlight_offset = 0usize; // offset within highlighted cells
        for seg in base_segs {
            if seg.control.is_some() {
                out.push(seg);
                continue;
            }
            let seg_style = seg.style.unwrap_or(style);
            // Only color cells that are part of the highlight (filled bar/boundary).
            let is_highlighted = seg_style == style;
            for ch in seg.text.chars() {
                let cell_style = if is_highlighted {
                    // Python: bar_offset = text_length - offset (counts DOWN from text_length)
                    let bar_offset = highlighted_count.saturating_sub(highlight_offset);
                    let t = if max_width == 0 {
                        0.0
                    } else {
                        bar_offset as f32 / max_width as f32
                    };
                    let color = gradient.get_color(t);
                    highlight_offset += 1;
                    seg_style.with_color(color.to_simple_opaque())
                } else {
                    seg_style
                };
                out.push(Segment::styled(ch.to_string(), cell_style));
            }
        }
        (out, component)
    }

    fn render_indeterminate(&self, width: usize) -> (String, &str) {
        let component = "bar--indeterminate";

        if width == 0 {
            return (String::new(), component);
        }

        let mut start;
        let end;
        let highlighted_bar_width = (0.25 * width as f32).max(1.0);
        let total_imaginary_width = width as f32 + highlighted_bar_width;
        if self.animation_level == AnimationLevel::None {
            start = 0.0;
            end = width as f32;
        } else {
            // Match Python Textual: time-based movement at 30 cells/sec.
            let speed = 30.0_f32;
            start = if total_imaginary_width > 0.0 {
                (speed * self.elapsed_secs() as f32) % (2.0 * total_imaginary_width)
            } else {
                0.0
            };
            if start > total_imaginary_width {
                start = 2.0 * total_imaginary_width - start;
            }
            start -= highlighted_bar_width;
            end = start + highlighted_bar_width;
        }

        let style = crate::css::resolve_component_style(self, &[component])
            .to_rich()
            .unwrap_or_default();
        let range = (start.max(0.0), end.min(width as f32));
        let text: String = Bar::new(range, style, style)
            .width(width)
            .render_for_width(width)
            .iter()
            .map(|seg| seg.text.as_ref())
            .collect();
        (text, component)
    }
}

impl Widget for ProgressBar {
    /// Declare children for tree-based mounting.
    ///
    /// ProgressBar sub-components are logical rendering helpers, not mountable
    /// children, so compose returns an empty list.
    fn compose(&mut self) -> ComposeResult {
        // Monolithic widget: renders inline, declares no arena children.
        Vec::new()
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
        let _ = tick;
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
        let suffix_width = if suffix.is_empty() {
            0
        } else {
            suffix.len() + 1
        };
        let bar_width = if self.show_bar {
            total_width.saturating_sub(suffix_width)
        } else {
            0
        };

        let mut out = Segments::new();

        if self.show_bar && bar_width > 0 {
            if let Some(ref gradient) = self.gradient {
                if self.total.is_some() {
                    // Gradient rendering for determinate bars.
                    let (segments, _component) =
                        self.render_determinate_gradient(bar_width, gradient);
                    out.extend(segments);
                } else {
                    // Gradient not applicable to indeterminate bars — fall back.
                    let (text, component) = self.render_indeterminate(bar_width);
                    let style = crate::css::resolve_component_style(self, &[component])
                        .to_rich()
                        .unwrap_or_else(rich_rs::Style::new);
                    let line = adjust_line_length_no_bg(&[Segment::styled(text, style)], bar_width);
                    out.extend(line);
                }
            } else {
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
        Some(1)
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

    fn style_type(&self) -> &'static str {
        "ProgressBar"
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for ProgressBar {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl ReactiveWidget for ProgressBar {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            match change.field_name {
                "total" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<Option<f64>>(),
                        change.new_value.downcast_ref::<Option<f64>>(),
                    ) {
                        self.watch_total(old, new, ctx);
                    }
                }
                "progress" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<f64>(),
                        change.new_value.downcast_ref::<f64>(),
                    ) {
                        self.watch_progress(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_id::NodeId;
    use crate::reactive::ReactiveCtx;
    use slotmap::SlotMap;

    fn make_node_id() -> NodeId {
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

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
        // 50% of 10: 5 highlighted `━`, a `╺` background half at the boundary,
        // then the `━` background track (Python Bar glyphs `━`/`╺`/`╸`).
        assert_eq!(text, "━━━━━╺━━━━");
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
        let bar = ProgressBar::new(None);
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
        let mut ctx = ReactiveCtx::new(make_node_id());
        bar.set_show_bar(false, &mut ctx);
        bar.set_show_percentage(false, &mut ctx);
        bar.set_show_eta(false, &mut ctx);
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
        assert_eq!(text, "━━━━━━━━━━");
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
        let mut ctx = ReactiveCtx::new(make_node_id());
        // All on: bar(32) + percentage(5) + eta(9) = 46
        assert_eq!(bar.content_width(), Some(46));

        bar.set_show_percentage(false, &mut ctx);
        // bar(32) + eta(9) = 41
        assert_eq!(bar.content_width(), Some(41));

        bar.set_show_eta(false, &mut ctx);
        // bar only = 32
        assert_eq!(bar.content_width(), Some(32));

        bar.set_show_bar(false, &mut ctx);
        // Nothing visible => min 1
        assert_eq!(bar.content_width(), Some(1));
    }

    #[test]
    fn set_total_resets_eta() {
        let mut bar = ProgressBar::new(Some(100.0));
        let mut ctx = ReactiveCtx::new(make_node_id());
        bar.advance(50.0);
        // Change total — ETA should reset.
        bar.set_total(Some(200.0), &mut ctx);
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
        let mut ctx = ReactiveCtx::new(make_node_id());
        bar.set_show_bar(true, &mut ctx);
        bar.set_show_percentage(true, &mut ctx);
        bar.set_show_eta(false, &mut ctx);

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

    // ── Gradient tests ──────────────────────────────────────────────

    fn make_two_stop_gradient(start: Color, end: Color) -> LinearGradient {
        LinearGradient::new(
            0.0,
            vec![(0.0, start), (1.0, end)],
        )
    }

    #[test]
    fn gradient_default_none() {
        let bar = ProgressBar::new(Some(100.0));
        assert!(bar.gradient().is_none());
    }

    #[test]
    fn gradient_builder() {
        let start = Color::rgb(255, 0, 0);
        let end = Color::rgb(0, 0, 255);
        let gradient = make_two_stop_gradient(start, end);
        let bar = ProgressBar::new(Some(100.0)).with_gradient(gradient);
        assert!(bar.gradient().is_some());
    }

    #[test]
    fn gradient_setter() {
        let mut bar = ProgressBar::new(Some(100.0));
        let start = Color::rgb(255, 0, 0);
        let end = Color::rgb(0, 255, 0);
        bar.set_gradient(Some(make_two_stop_gradient(start, end)));
        assert!(bar.gradient().is_some());
        bar.set_gradient(None);
        assert!(bar.gradient().is_none());
    }

    #[test]
    fn gradient_interpolation_via_linear_gradient() {
        // LinearGradient::get_color at t=0 should return start, t=1 should return end.
        let start = Color::rgb(0, 0, 0);
        let end = Color::rgb(100, 200, 50);
        let gradient = make_two_stop_gradient(start, end);

        let c0 = gradient.get_color(0.0);
        assert_eq!(c0.r, 0);
        assert_eq!(c0.g, 0);
        assert_eq!(c0.b, 0);

        let c1 = gradient.get_color(1.0);
        assert_eq!(c1.r, 100);
        assert_eq!(c1.g, 200);
        assert_eq!(c1.b, 50);
    }

    #[test]
    fn gradient_determinate_produces_correct_segments() {
        let start = Color::rgb(255, 0, 0);
        let end = Color::rgb(0, 0, 255);
        let gradient = make_two_stop_gradient(start, end);
        let mut bar = ProgressBar::new(Some(100.0)).with_gradient(gradient.clone());
        bar.advance(100.0); // 100% filled

        let (segments, component) = bar.render_determinate_gradient(5, &gradient);
        assert_eq!(component, "bar--complete");
        // Each non-control segment is a single cell; all should be bar glyphs.
        let text: String = segments
            .iter()
            .filter(|s| s.control.is_none())
            .map(|s| s.text.as_ref())
            .collect();
        assert!(text.chars().all(|c| c == Bar::BAR || c == Bar::HALF_BAR_LEFT || c == Bar::HALF_BAR_RIGHT),
            "unexpected glyph in gradient output: {text:?}");
    }

    #[test]
    fn gradient_determinate_partial_fill() {
        let start = Color::rgb(255, 0, 0);
        let end = Color::rgb(0, 0, 255);
        let gradient = make_two_stop_gradient(start, end);
        let mut bar = ProgressBar::new(Some(100.0)).with_gradient(gradient.clone());
        bar.advance(50.0); // 50% filled

        let (segments, component) = bar.render_determinate_gradient(10, &gradient);
        assert_eq!(component, "bar--bar");
        let text: String = segments
            .iter()
            .filter(|s| s.control.is_none())
            .map(|seg| seg.text.as_ref())
            .collect();
        // 50% of 10 → 5 filled + boundary glyph + 4 background
        assert_eq!(text, "━━━━━╺━━━━");
    }

    #[test]
    fn gradient_multi_stop_rainbow_renders_without_panic() {
        // 12-stop rainbow as in the Python progress_bar_gradient example.
        let stops: Vec<(f32, Color)> = vec![
            (0.0 / 11.0, Color::rgb(0x88, 0x11, 0x77)),
            (1.0 / 11.0, Color::rgb(0xaa, 0x33, 0x55)),
            (2.0 / 11.0, Color::rgb(0xcc, 0x66, 0x66)),
            (3.0 / 11.0, Color::rgb(0xee, 0x99, 0x44)),
            (4.0 / 11.0, Color::rgb(0xee, 0xdd, 0x00)),
            (5.0 / 11.0, Color::rgb(0x99, 0xdd, 0x55)),
            (6.0 / 11.0, Color::rgb(0x44, 0xdd, 0x88)),
            (7.0 / 11.0, Color::rgb(0x22, 0xcc, 0xbb)),
            (8.0 / 11.0, Color::rgb(0x00, 0xbb, 0xcc)),
            (9.0 / 11.0, Color::rgb(0x00, 0x99, 0xcc)),
            (10.0 / 11.0, Color::rgb(0x33, 0x66, 0xbb)),
            (1.0,         Color::rgb(0x66, 0x33, 0x99)),
        ];
        let gradient = LinearGradient::new(0.0, stops);
        let mut bar = ProgressBar::new(Some(100.0)).with_gradient(gradient);
        bar.advance(70.0);

        let console = Console::new();
        let mut opts = console.options().clone();
        opts.size = (40, 1);
        let segs = Widget::render(&bar, &console, &opts);
        let text: String = segs
            .iter()
            .filter(|s| s.control.is_none())
            .map(|s| s.text.as_ref())
            .collect();
        assert!(!text.is_empty(), "gradient bar should render something");
    }

    // ── compose() / take_composed_children() tests ────────────────

    #[test]
    fn compose_returns_empty() {
        let mut bar = ProgressBar::new(Some(100.0));
        let result = bar.compose();
        assert!(result.is_empty());
    }

    #[test]
    fn compose_returns_empty_indeterminate() {
        let mut bar = ProgressBar::new(None);
        assert!(bar.compose().is_empty());
    }

    #[test]
    fn no_gradient_still_works_normally() {
        let mut bar = ProgressBar::new(Some(100.0));
        bar.advance(50.0);
        // No gradient set — should use the standard render path
        assert!(bar.gradient().is_none());
        let (text, component) = bar.render_determinate(10);
        assert_eq!(text, "━━━━━╺━━━━");
        assert_eq!(component, "bar--bar");
    }

    /// Verify that the gradient direction exactly mirrors Python `_apply_gradient`.
    ///
    /// Python applies the gradient REVERSED, keyed off highlighted length:
    ///
    ///   text_length = len(highlight_bar)
    ///   for offset in range(text_length):
    ///       bar_offset = text_length - offset   # DOWN: high left, low right
    ///       t = bar_offset / (width - 1)
    ///
    /// For a fully-filled bar of width=5 (max_width=4):
    ///   - text_length = 5 (all 5 cells highlighted)
    ///   - cell 0 (leftmost):  t = 5/4 = 1.25 → clamped to 1.0 → end color
    ///   - cell 1:             t = 4/4 = 1.0  → end color
    ///   - cell 2:             t = 3/4 = 0.75 → 75% blend
    ///   - cell 3:             t = 2/4 = 0.5  → midpoint
    ///   - cell 4 (rightmost): t = 1/4 = 0.25 → low end → closer to start color
    ///
    /// With start=black (rgb 0,0,0) and end=white (rgb 255,255,255):
    ///   - leftmost cell fg must have r > rightmost cell fg (gradient runs right-to-left)
    #[test]
    fn gradient_direction_reversed_matches_python() {
        // Black → White gradient: start = black (t=0), end = white (t=1).
        let start = Color::rgb(0, 0, 0);
        let end = Color::rgb(255, 255, 255);
        let gradient = make_two_stop_gradient(start, end);

        // 100% filled bar, width=5 → all 5 cells highlighted.
        let mut bar = ProgressBar::new(Some(100.0)).with_gradient(gradient.clone());
        bar.advance(100.0);

        let (segments, _) = bar.render_determinate_gradient(5, &gradient);

        // Collect foreground red-channel values of highlighted (non-control) segments.
        let fg_reds: Vec<u8> = segments
            .iter()
            .filter(|s| s.control.is_none())
            .filter_map(|s| {
                let color = s.style.as_ref()?.color?;
                if let rich_rs::SimpleColor::Rgb { r, .. } = color { Some(r) } else { None }
            })
            .collect();

        assert_eq!(fg_reds.len(), 5, "expected 5 highlighted cells, got: {fg_reds:?}");

        // Python direction: LEFT cell has HIGHER t → closer to white (r=255).
        // RIGHT cell has LOWER t → closer to black (r=0).
        // So fg_reds should be DECREASING left-to-right.
        let leftmost_r = fg_reds[0];
        let rightmost_r = fg_reds[4];
        assert!(
            leftmost_r > rightmost_r,
            "gradient should run right-to-left (Python direction): \
             leftmost r={leftmost_r} should be > rightmost r={rightmost_r}. \
             Full fg_reds: {fg_reds:?}"
        );

        // Specifically, the leftmost cell gets t = 5/4 = 1.25 → clamped 1.0 → white (r=255).
        // The rightmost cell gets t = 1/4 = 0.25 → closer to black.
        assert_eq!(leftmost_r, 255,
            "leftmost cell (t=1.25 clamped to 1.0) should be white (r=255), got r={leftmost_r}");
        assert!(
            rightmost_r < 100,
            "rightmost cell (t=0.25) should be close to black (r<100), got r={rightmost_r}"
        );
    }

    /// Verify gradient direction with a partially-filled bar.
    ///
    /// For a 50%-filled bar of width=10 (max_width=9):
    ///   - highlighted portion: 5 full cells + 1 boundary = 6 highlighted cells
    ///   - text_length = 6
    ///   - cell 0 (leftmost highlighted): t = 6/9 = 0.667
    ///   - cell 5 (rightmost highlighted): t = 1/9 = 0.111
    ///   - leftmost cell fg r should be > rightmost highlighted cell fg r
    #[test]
    fn gradient_direction_partial_fill_reversed() {
        let start = Color::rgb(0, 0, 0);
        let end = Color::rgb(255, 255, 255);
        let gradient = make_two_stop_gradient(start, end);

        let mut bar = ProgressBar::new(Some(100.0)).with_gradient(gradient.clone());
        bar.advance(50.0); // 50% filled → width=10, ~5 highlighted + boundary

        let (segments, _) = bar.render_determinate_gradient(10, &gradient);

        // Collect fg red-channel values from highlighted cells (those with a color set).
        let highlighted_fg_reds: Vec<u8> = segments
            .iter()
            .filter(|s| s.control.is_none())
            .filter_map(|s| {
                let color = s.style.as_ref()?.color?;
                if let rich_rs::SimpleColor::Rgb { r, .. } = color { Some(r) } else { None }
            })
            .collect();

        assert!(
            !highlighted_fg_reds.is_empty(),
            "should have highlighted cells with gradient colors"
        );

        let leftmost_r = *highlighted_fg_reds.first().unwrap();
        let rightmost_r = *highlighted_fg_reds.last().unwrap();

        assert!(
            leftmost_r > rightmost_r,
            "gradient direction wrong for partial fill: \
             leftmost r={leftmost_r} should be > rightmost r={rightmost_r}. \
             fg_reds: {highlighted_fg_reds:?}"
        );
    }
}
