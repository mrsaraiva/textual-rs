use std::time::{Duration, Instant};

use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual_macros::widget;

use crate::event::{AnimationLevel, WidgetCtx};
use crate::renderables::{Bar as BarRenderable, LinearGradient};
#[cfg(test)]
use crate::style::Color;

use super::NodeSeed;
use crate::compose::{ChildDecl, ComposeResult};
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

/// Format a percentage for display (e.g. "50%" or "--%" when unknown).
///
/// Mirrors Python `PercentageStatus.render` (`f"{percentage}%"`, unpadded —
/// the right alignment comes from the widget's `content-align-horizontal`).
fn format_percentage(pct: Option<f64>) -> String {
    match pct {
        Some(p) => format!("{}%", (p * 100.0).round() as u64),
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

// ── Bar sub-widget ──────────────────────────────────────────────────

/// The bar portion of a [`ProgressBar`] (Python `_progress_bar.Bar`).
///
/// A real arena child (id `bar`) composed by [`ProgressBar`]. Declares and
/// resolves the `bar--*` component classes against itself, so the scoped
/// defaults (`ProgressBar Bar > .bar--bar { ... }`) and user CSS written the
/// Python way (`Bar > .bar--indeterminate { ... }`) style it directly.
///
/// # Component classes
///
/// | Class | Description |
/// | :--- | :--- |
/// | `bar--bar` | Style of the bar (may be used to change the color). |
/// | `bar--complete` | Style of the bar when it's complete. |
/// | `bar--indeterminate` | Style of the bar when it's in an indeterminate state. |
#[derive(Debug, Clone)]
#[widget(Focus, Layout, Components)]
pub struct Bar {
    /// The completed ratio in `0.0..=1.0`, or `None` for indeterminate.
    percentage: Option<f64>,
    /// Optional multi-stop gradient painted across the filled portion.
    gradient: Option<LinearGradient>,
    /// Animation level — controls whether the indeterminate bar animates.
    animation_level: AnimationLevel,
    /// Shared animation clock origin (the owning `ProgressBar`'s), so the
    /// indeterminate animation phase survives parent recomposes.
    clock_origin: Instant,
    seed: NodeSeed,
}

impl Bar {
    /// Build a `Bar` for a [`ProgressBar`] (values pushed down at compose).
    pub(crate) fn new(
        percentage: Option<f64>,
        gradient: Option<LinearGradient>,
        animation_level: AnimationLevel,
        clock_origin: Instant,
    ) -> Self {
        Self {
            percentage,
            gradient,
            animation_level,
            clock_origin,
            seed: NodeSeed::default(),
        }
    }

    /// The completed ratio in `0.0..=1.0`, or `None` for indeterminate.
    pub fn percentage(&self) -> Option<f64> {
        self.percentage
    }

    fn elapsed_secs(&self) -> f64 {
        self.clock_origin.elapsed().as_secs_f64()
    }

    /// Resolve a `bar--*` component class into the (highlight, background)
    /// rich styles for the bar renderable.
    ///
    /// Python parity (`Bar.render`): the highlight style carries ONLY the
    /// component's foreground color (`Style.from_color(bar_style.color)`) and
    /// the background style carries ONLY the component's background color as
    /// its FOREGROUND (`Style.from_color(bar_style.bgcolor)`) — the track
    /// glyphs are drawn in the bg color.
    fn component_bar_styles(&self, component: &str) -> (rich_rs::Style, rich_rs::Style) {
        let rich =
            crate::widgets::Widget::get_component_rich_style(self, component).unwrap_or_default();
        let mut highlight = rich_rs::Style::new();
        if let Some(color) = rich.color {
            highlight = highlight.with_color(color);
        }
        let mut background = rich_rs::Style::new();
        if let Some(bg) = rich.bgcolor {
            background = background.with_color(bg);
        }
        (highlight, background)
    }

    /// Render the determinate bar (optionally gradient-recolored).
    fn render_determinate(&self, width: usize) -> (Vec<Segment>, &'static str) {
        let pct = self.percentage.unwrap_or(0.0);
        let component = if pct >= 1.0 {
            "bar--complete"
        } else {
            "bar--bar"
        };
        let (highlight_style, background_style) = self.component_bar_styles(component);
        // Python passes the FRACTIONAL highlight extent (`size.width * percentage`)
        // to the Bar renderable, which rounds to the nearest half-cell (`╸`/`╺`).
        // Pre-rounding to an integer here would drop that half-cell precision.
        let highlight_end = (pct * width as f64).min(width as f64) as f32;
        let segments: Vec<Segment> =
            BarRenderable::new((0.0, highlight_end), highlight_style, background_style)
                .width(width)
                .render_for_width(width)
                .into_iter()
                .collect();
        let segments = match &self.gradient {
            Some(gradient) => apply_gradient(segments, width, highlight_end, gradient),
            None => segments,
        };
        (segments, component)
    }

    /// Render a frame of the indeterminate progress bar animation.
    fn render_indeterminate(&self, width: usize) -> (Vec<Segment>, &'static str) {
        let component = "bar--indeterminate";

        if width == 0 {
            return (Vec::new(), component);
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

        let (highlight_style, background_style) = self.component_bar_styles(component);
        let range = (start.max(0.0), end.min(width as f32));
        let segments: Vec<Segment> = BarRenderable::new(range, highlight_style, background_style)
            .width(width)
            .render_for_width(width)
            .into_iter()
            .collect();
        (segments, component)
    }
}

/// Re-color the highlighted portion of a rendered bar with a gradient.
///
/// Mirrors Python `renderables/bar.py:_apply_gradient`, which stylizes ONLY
/// the highlighted text (`highlight_bar`, including a trailing half glyph)
/// REVERSED, keyed off the highlighted length — NOT the absolute position:
///
/// ```text
/// text_length = len(highlight_bar)     # highlighted cells only
/// for offset in range(text_length):
///     bar_offset = text_length - offset    # counts DOWN: high left
///     t = bar_offset / (width - 1)
/// ```
///
/// So the leftmost highlighted cell gets the HIGHEST t value and the
/// rightmost gets the LOWEST. `get_color` clamps t to [0, 1], so t > 1
/// (possible on a partially filled bar) is handled correctly. Highlighted
/// cells are counted structurally (the renderable emits the highlight run
/// FIRST because the range starts at 0), so the recolor is independent of
/// whether the highlight/background styles happen to compare equal.
fn apply_gradient(
    segments: Vec<Segment>,
    width: usize,
    highlight_end: f32,
    gradient: &LinearGradient,
) -> Vec<Segment> {
    // Mirror the renderable's half-cell quantization to count highlighted
    // cells: full cells plus one half-cell boundary glyph when present.
    let end = (highlight_end.clamp(0.0, width as f32) * 2.0).round() / 2.0;
    let full_cells = end.trunc() as usize;
    let has_half = (end - end.trunc()).abs() > f32::EPSILON;
    let highlighted_count = full_cells + usize::from(has_half);
    let max_width = width.saturating_sub(1);

    let mut out: Vec<Segment> = Vec::with_capacity(segments.len());
    let mut cell_index = 0usize;
    for seg in segments {
        if seg.control.is_some() {
            out.push(seg);
            continue;
        }
        let seg_style = seg.style.unwrap_or_default();
        for ch in seg.text.chars() {
            if cell_index < highlighted_count {
                let bar_offset = highlighted_count - cell_index;
                let t = if max_width == 0 {
                    0.0
                } else {
                    bar_offset as f32 / max_width as f32
                };
                let color = gradient.get_color(t);
                out.push(Segment::styled(
                    ch.to_string(),
                    seg_style.with_color(color.to_simple_opaque()),
                ));
            } else {
                out.push(Segment::styled(ch.to_string(), seg_style));
            }
            cell_index += 1;
        }
    }
    out
}

impl crate::widgets::Focus for Bar {
    fn focusable(&self) -> bool {
        false
    }

    /// The indeterminate bar reports as active so the runtime repaints it on
    /// every frame tick (the animation is time-based and self-contained —
    /// no per-frame recompose of the parent is needed). Determinate bars are
    /// inert between value changes.
    fn is_active(&self) -> bool {
        self.percentage.is_none() && self.animation_level != AnimationLevel::None
    }
}

impl crate::widgets::Layout for Bar {
    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

impl crate::widgets::Components for Bar {
    fn component_classes(&self) -> &[&'static str] {
        &["bar--bar", "bar--complete", "bar--indeterminate"]
    }
}

impl crate::widgets::Render for Bar {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let (segments, _component) = if self.percentage.is_some() {
            self.render_determinate(width)
        } else {
            self.render_indeterminate(width)
        };
        let mut out = Segments::new();
        out.extend(segments);
        out
    }

    fn style_type(&self) -> &'static str {
        "Bar"
    }
}

// ── PercentageStatus sub-widget ─────────────────────────────────────

/// A label displaying the percentage status of a [`ProgressBar`]
/// (Python `_progress_bar.PercentageStatus`). Real arena child, id
/// `percentage`; right-aligned within its CSS width via
/// `content-align-horizontal: right`.
#[derive(Debug, Clone)]
#[widget(Layout)]
pub struct PercentageStatus {
    /// The completed ratio in `0.0..=1.0`, or `None` when unknown.
    percentage: Option<f64>,
    seed: NodeSeed,
}

impl PercentageStatus {
    pub(crate) fn new(percentage: Option<f64>) -> Self {
        Self {
            percentage,
            seed: NodeSeed::default(),
        }
    }

    fn text(&self) -> String {
        format_percentage(self.percentage)
    }
}

impl crate::widgets::Layout for PercentageStatus {
    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn content_width(&self) -> Option<usize> {
        Some(rich_rs::cell_len(&self.text()))
    }
}

impl crate::widgets::Render for PercentageStatus {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        let mut out = Segments::new();
        out.push(Segment::new(self.text()));
        out
    }

    fn style_type(&self) -> &'static str {
        "PercentageStatus"
    }
}

// ── ETAStatus sub-widget ────────────────────────────────────────────

/// A label displaying the estimated time until completion of a
/// [`ProgressBar`] (Python `_progress_bar.ETAStatus`). Real arena child,
/// id `eta`; right-aligned within its CSS width via
/// `content-align-horizontal: right`.
#[derive(Debug, Clone)]
#[widget(Layout)]
pub struct ETAStatus {
    /// Estimated seconds until completion, or `None` if unknown.
    eta_secs: Option<u64>,
    seed: NodeSeed,
}

impl ETAStatus {
    pub(crate) fn new(eta_secs: Option<u64>) -> Self {
        Self {
            eta_secs,
            seed: NodeSeed::default(),
        }
    }

    fn text(&self) -> String {
        format_eta(self.eta_secs)
    }
}

impl crate::widgets::Layout for ETAStatus {
    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn content_width(&self) -> Option<usize> {
        Some(rich_rs::cell_len(&self.text()))
    }
}

impl crate::widgets::Render for ETAStatus {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        let mut out = Segments::new();
        out.push(Segment::new(self.text()));
        out
    }

    fn style_type(&self) -> &'static str {
        "ETAStatus"
    }
}

// ── ProgressBar widget ──────────────────────────────────────────────

/// A progress bar widget that displays determinate or indeterminate progress.
///
/// Composes real sub-widgets mirroring Python `_progress_bar.py`:
/// a [`Bar`] (id `bar`, when `show_bar`), a [`PercentageStatus`] (id
/// `percentage`, when `show_percentage`) and an [`ETAStatus`] (id `eta`,
/// when `show_eta`). The children are laid out by the default CSS
/// (`ProgressBar { layout: horizontal; width: auto }`, `ProgressBar Bar
/// { width: 32 }`, percentage width 5, eta width 9) and are addressable
/// via `#bar` / `#percentage` / `#eta`.
///
/// When `total` is `Some`, the bar renders filled proportionally to
/// `progress / total`. When `total` is `None`, the bar renders an animated
/// indeterminate sliding highlight (self-animating; see [`Bar`]).
///
/// This widget is **not focusable** (display-only).
///
/// # Updating values
///
/// `progress` / `total` mutations go through the reactive setters
/// ([`advance`](ProgressBar::advance), [`update`](ProgressBar::update),
/// [`set_progress`](ProgressBar::set_progress),
/// [`set_total`](ProgressBar::set_total)), which take a [`ReactiveCtx`]:
/// the watchers request a recompose so the composed children pick up the
/// new values (the Rust analogue of Python's `data_bind`). Post-mount,
/// call them through `Handle::update` / `query_one_typed`.
///
/// # Gradient
///
/// Use [`with_gradient`](ProgressBar::with_gradient) to set a color gradient
/// across the filled portion of the bar (passed down into the [`Bar`] child).
///
/// # Animation level
///
/// When `animation_level` is set to [`AnimationLevel::None`], the indeterminate
/// bar renders as a static full-width bar instead of animating.
///
/// # Component classes (declared by the [`Bar`] child)
///
/// | Class | Description |
/// | :--- | :--- |
/// | `bar--bar` | The bar in its normal (incomplete) state. |
/// | `bar--complete` | The bar when progress reaches 100%. |
/// | `bar--indeterminate` | The bar when total is unknown. |
#[derive(Debug, Clone)]
#[widget(Interactive, reactive)]
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
    /// Monotonic reference point for ETA time tracking and the indeterminate
    /// animation clock (shared with the composed `Bar`).
    start_instant: Instant,
    /// The ETA value most recently pushed into the composed `ETAStatus`
    /// (Python `_display_eta`), used by the 1-second refresh interval to
    /// recompose only when the displayed value would change.
    display_eta: Option<u64>,
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
            display_eta: None,
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

    /// Advance progress by `amount` steps (Python `ProgressBar.advance`).
    ///
    /// Routes through the reactive setter so the composed children recompose
    /// with the new value. Post-mount, call through `Handle::update` /
    /// `query_one_typed` so the recorded change reaches the runtime.
    pub fn advance(&mut self, amount: f64, ctx: &mut ReactiveCtx) {
        self.set_progress(self.progress + amount, ctx);
    }

    /// Builder: set the initial progress (pre-mount configuration).
    pub fn with_progress(mut self, progress: f64) -> Self {
        self.progress = progress;
        self
    }

    // ── Reactive setters ─────────────────────────────────────────────

    /// Reactive setter for `total`. Records the change in the provided
    /// [`ReactiveCtx`]; the watcher resets the ETA estimator and recomposes
    /// the children.
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
    /// [`ReactiveCtx`]; the watcher samples the ETA estimator and recomposes
    /// the children.
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

    /// Reactive setter for `show_bar`. Recomposes the children (the `Bar`
    /// child is mounted conditionally) and triggers layout invalidation.
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
            ctx.request_recompose();
        }
    }

    /// Reactive setter for `show_percentage`. Recomposes the children (the
    /// `PercentageStatus` child is mounted conditionally) and triggers layout
    /// invalidation.
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
            ctx.request_recompose();
        }
    }

    /// Reactive setter for `show_eta`. Recomposes the children (the
    /// `ETAStatus` child is mounted conditionally) and triggers layout
    /// invalidation.
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
            ctx.request_recompose();
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

    fn watch_total(&mut self, _old: &Option<f64>, _new: &Option<f64>, ctx: &mut ReactiveCtx) {
        // Reset ETA when total changes (matching Python behavior), then
        // rebuild the composed children with the new state (the Rust
        // analogue of Python's `data_bind` value propagation).
        self.eta.reset();
        ctx.request_recompose();
    }

    fn watch_progress(&mut self, _old: &f64, _new: &f64, ctx: &mut ReactiveCtx) {
        self.record_eta_sample();
        ctx.request_recompose();
    }

    /// Batch update: optionally set total, progress, and/or advance
    /// (Python `ProgressBar.update`). Routes through the reactive setters so
    /// the composed children recompose with the new values.
    pub fn update(
        &mut self,
        total: Option<Option<f64>>,
        progress: Option<f64>,
        advance: Option<f64>,
        ctx: &mut ReactiveCtx,
    ) {
        if let Some(t) = total {
            self.set_total(t, ctx);
        }
        if let Some(p) = progress {
            self.set_progress(p, ctx);
        }
        if let Some(a) = advance {
            self.set_progress(self.progress + a, ctx);
        }
    }

    /// Current animation level.
    pub fn animation_level(&self) -> AnimationLevel {
        self.animation_level
    }

    /// Set the animation level (controls indeterminate animation).
    ///
    /// Pre-mount configuration; a post-mount change takes effect on the next
    /// recompose (progress/total/show flag change).
    pub fn set_animation_level(&mut self, level: AnimationLevel) {
        self.animation_level = level;
    }

    /// Current gradient, if set.
    pub fn gradient(&self) -> Option<&LinearGradient> {
        self.gradient.as_ref()
    }

    /// Set a multi-stop gradient painted across the filled portion of the bar.
    ///
    /// Pass `None` to remove the gradient and fall back to CSS component
    /// styling. Pre-mount configuration; a post-mount change takes effect on
    /// the next recompose (progress/total/show flag change).
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

    /// The ETA value to display right now (Python `_display_eta` recompute).
    fn compute_display_eta(&self) -> Option<u64> {
        if self.total.is_some() {
            self.eta.get_eta(self.elapsed_secs())
        } else {
            None
        }
    }

    /// Periodic ETA display refresh (Python `set_interval(1, self.update)`):
    /// recompose only when the displayed ETA would actually change.
    fn refresh_display_eta(&mut self, ctx: &mut ReactiveCtx) {
        let current = self.compute_display_eta();
        if current != self.display_eta {
            self.display_eta = current;
            if self.show_eta {
                ctx.request_recompose();
            }
        }
    }
}

impl crate::widgets::Interactive for ProgressBar {
    /// Python `ProgressBar.on_mount`: refresh the displayed ETA once and
    /// start a 1-second interval that keeps it counting down between
    /// progress updates.
    fn on_mount(&mut self, ctx: &mut WidgetCtx) {
        self.display_eta = self.compute_display_eta();
        ctx.set_interval::<ProgressBar, _>(Duration::from_secs(1), false, |bar, wctx, _tick| {
            bar.refresh_display_eta(wctx);
        });
    }
}

impl crate::widgets::Render for ProgressBar {
    /// Compose the sub-widgets as real arena children, mirroring Python
    /// `ProgressBar.compose` including the conditionality: `Bar` (id `bar`)
    /// only when `show_bar`, `PercentageStatus` (id `percentage`) when
    /// `show_percentage`, `ETAStatus` (id `eta`) when `show_eta`.
    ///
    /// State-pure and idempotent: every call regenerates the children from
    /// the authoritative fields, so a recompose (the value-propagation path
    /// for progress/total changes) rebuilds an equivalent child set with the
    /// fresh values.
    fn compose(&mut self) -> ComposeResult {
        let mut children = Vec::new();
        if self.show_bar {
            children.push(
                ChildDecl::new(Box::new(Bar::new(
                    self.percentage(),
                    self.gradient.clone(),
                    self.animation_level,
                    self.start_instant,
                )))
                .with_id("bar"),
            );
        }
        if self.show_percentage {
            children.push(
                ChildDecl::new(Box::new(PercentageStatus::new(self.percentage())))
                    .with_id("percentage"),
            );
        }
        if self.show_eta {
            self.display_eta = self.compute_display_eta();
            children
                .push(ChildDecl::new(Box::new(ETAStatus::new(self.display_eta))).with_id("eta"));
        }
        children
    }

    /// Chrome-only: the framework paints this node's resolved surface
    /// (background/border, if any) via the styled render pipeline; the
    /// composed children render themselves and composite over it.
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "ProgressBar"
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
    use crate::widgets::Widget;
    use slotmap::SlotMap;

    fn make_node_id() -> NodeId {
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    fn make_ctx() -> ReactiveCtx {
        ReactiveCtx::new(make_node_id())
    }

    fn segments_text(segments: &[Segment]) -> String {
        segments
            .iter()
            .filter(|s| s.control.is_none())
            .map(|s| s.text.as_ref())
            .collect()
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
        bar.advance(50.0, &mut make_ctx());
        assert_eq!(bar.percentage(), Some(0.5));
    }

    #[test]
    fn progress_bar_percentage_clamped() {
        let mut bar = ProgressBar::new(Some(100.0));
        bar.advance(200.0, &mut make_ctx());
        assert_eq!(bar.percentage(), Some(1.0));
    }

    #[test]
    fn progress_bar_advance() {
        let mut bar = ProgressBar::new(Some(10.0));
        let mut ctx = make_ctx();
        bar.advance(3.0, &mut ctx);
        bar.advance(2.0, &mut ctx);
        assert_eq!(bar.progress(), 5.0);
    }

    #[test]
    fn progress_bar_with_progress_builder() {
        let bar = ProgressBar::new(Some(100.0)).with_progress(65.0);
        assert_eq!(bar.progress(), 65.0);
        assert_eq!(bar.percentage(), Some(0.65));
    }

    #[test]
    fn progress_bar_update_batch() {
        let mut bar = ProgressBar::new(None);
        bar.update(Some(Some(50.0)), Some(10.0), Some(5.0), &mut make_ctx());
        assert_eq!(bar.total(), Some(50.0));
        assert_eq!(bar.progress(), 15.0);
    }

    #[test]
    fn progress_bar_not_focusable() {
        let bar = ProgressBar::new(Some(100.0));
        assert!(!Widget::focusable(&bar));
    }

    #[test]
    fn bar_determinate_render_text() {
        let bar = Bar::new(Some(0.5), None, AnimationLevel::Full, Instant::now());
        let (segments, component) = bar.render_determinate(10);
        // 50% of 10: 5 highlighted `━`, a `╺` background half at the boundary,
        // then the `━` background track (Python Bar glyphs `━`/`╺`/`╸`).
        assert_eq!(segments_text(&segments), "━━━━━╺━━━━");
        assert_eq!(component, "bar--bar");
    }

    #[test]
    fn bar_complete_component_class() {
        let bar = Bar::new(Some(1.0), None, AnimationLevel::Full, Instant::now());
        let (_, component) = bar.render_determinate(10);
        assert_eq!(component, "bar--complete");
    }

    #[test]
    fn bar_indeterminate_render_full_width() {
        let bar = Bar::new(None, None, AnimationLevel::Full, Instant::now());
        let (segments, component) = bar.render_indeterminate(20);
        assert_eq!(component, "bar--indeterminate");
        assert_eq!(segments_text(&segments).chars().count(), 20);
    }

    #[test]
    fn progress_bar_negative_total_clamped() {
        let bar = ProgressBar::new(Some(-5.0));
        assert_eq!(bar.total(), Some(0.0));
        assert_eq!(bar.percentage(), Some(1.0));
    }

    // ── Display toggle tests ────────────────────────────────────────

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
        let mut ctx = make_ctx();
        bar.set_show_bar(false, &mut ctx);
        bar.set_show_percentage(false, &mut ctx);
        bar.set_show_eta(false, &mut ctx);
        assert!(!bar.show_bar());
        assert!(!bar.show_percentage());
        assert!(!bar.show_eta());
    }

    #[test]
    fn show_toggle_setters_request_recompose() {
        let mut bar = ProgressBar::new(Some(100.0));
        let mut ctx = make_ctx();
        bar.set_show_eta(false, &mut ctx);
        assert!(
            ctx.needs_recompose(),
            "a show_* flip must recompose the conditional children"
        );
    }

    #[test]
    fn animation_level_default_full() {
        let bar = ProgressBar::new(None);
        assert_eq!(bar.animation_level(), AnimationLevel::Full);
    }

    #[test]
    fn animation_level_none_static_indeterminate() {
        let bar = Bar::new(None, None, AnimationLevel::None, Instant::now());
        let (segments, component) = bar.render_indeterminate(10);
        // Static full-width bar when animations disabled.
        assert_eq!(segments_text(&segments), "━━━━━━━━━━");
        assert_eq!(component, "bar--indeterminate");
    }

    #[test]
    fn animation_level_none_bar_not_active() {
        let bar = Bar::new(None, None, AnimationLevel::None, Instant::now());
        assert!(!Widget::is_active(&bar));
    }

    #[test]
    fn indeterminate_bar_is_active_determinate_is_not() {
        let indeterminate = Bar::new(None, None, AnimationLevel::Full, Instant::now());
        assert!(
            Widget::is_active(&indeterminate),
            "indeterminate Bar self-animates via the frame tick"
        );
        let determinate = Bar::new(Some(0.5), None, AnimationLevel::Full, Instant::now());
        assert!(
            !Widget::is_active(&determinate),
            "determinate Bar must not force per-frame repaints"
        );
    }

    #[test]
    fn progress_bar_itself_is_not_active() {
        // The parent no longer reports active: the indeterminate animation
        // lives on the composed Bar child (no per-frame parent recompose).
        let bar = ProgressBar::new(None);
        assert!(!Widget::is_active(&bar));
    }

    // ── Format helper tests ─────────────────────────────────────────

    #[test]
    fn format_percentage_display() {
        assert_eq!(format_percentage(Some(0.0)), "0%");
        assert_eq!(format_percentage(Some(0.5)), "50%");
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

    // ── ETA tests ───────────────────────────────────────────────────

    #[test]
    fn eta_no_estimate_without_samples() {
        let bar = ProgressBar::new(Some(100.0));
        // No progress samples = no speed data = no ETA.
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
    fn set_total_resets_eta_via_watcher() {
        let mut bar = ProgressBar::new(Some(100.0));
        let mut ctx = make_ctx();
        bar.advance(50.0, &mut ctx);
        // Simulate the runtime reactive phase: dispatch recorded changes so
        // the watchers (eta sampling / reset) fire.
        crate::reactive::run_reactive_phase(&mut bar, &mut ctx);
        // Change total — the watcher resets the ETA estimator.
        let mut ctx2 = make_ctx();
        bar.set_total(Some(200.0), &mut ctx2);
        crate::reactive::run_reactive_phase(&mut bar, &mut ctx2);
        assert_eq!(bar.total(), Some(200.0));
        // After reset, no speed data => no ETA.
        assert!(bar.eta_seconds().is_none());
    }

    #[test]
    fn progress_watcher_requests_recompose() {
        let mut bar = ProgressBar::new(Some(100.0));
        let mut ctx = make_ctx();
        bar.set_progress(25.0, &mut ctx);
        let result = crate::reactive::run_reactive_phase(&mut bar, &mut ctx);
        assert!(
            result.needs_recompose,
            "a progress change must recompose the composed children"
        );
    }

    #[test]
    fn total_watcher_requests_recompose() {
        let mut bar = ProgressBar::new(None);
        let mut ctx = make_ctx();
        bar.set_total(Some(100.0), &mut ctx);
        let result = crate::reactive::run_reactive_phase(&mut bar, &mut ctx);
        assert!(
            result.needs_recompose,
            "a total change must recompose the composed children"
        );
    }

    // ── compose() tests ─────────────────────────────────────────────

    #[test]
    fn compose_emits_bar_percentage_eta_children() {
        let mut bar = ProgressBar::new(Some(100.0));
        let decls = bar.compose();
        assert_eq!(decls.len(), 3);
        assert_eq!(decls[0].id(), Some("bar"));
        assert_eq!(decls[1].id(), Some("percentage"));
        assert_eq!(decls[2].id(), Some("eta"));
    }

    #[test]
    fn compose_is_conditional_on_show_flags() {
        let mut bar = ProgressBar::new(Some(100.0));
        let mut ctx = make_ctx();
        bar.set_show_bar(false, &mut ctx);
        let decls = bar.compose();
        assert_eq!(decls.len(), 2);
        assert_eq!(decls[0].id(), Some("percentage"));
        assert_eq!(decls[1].id(), Some("eta"));

        bar.set_show_percentage(false, &mut ctx);
        bar.set_show_eta(false, &mut ctx);
        assert!(bar.compose().is_empty());
    }

    #[test]
    fn compose_is_state_pure() {
        // Two consecutive composes rebuild an equivalent child set (RA2.1
        // recompose invariant: generative, state-pure compose).
        let mut bar = ProgressBar::new(Some(100.0)).with_progress(40.0);
        let first: Vec<Option<String>> = bar
            .compose()
            .iter()
            .map(|d| d.id().map(str::to_string))
            .collect();
        let second: Vec<Option<String>> = bar
            .compose()
            .iter()
            .map(|d| d.id().map(str::to_string))
            .collect();
        assert_eq!(first, second);
    }

    #[test]
    fn compose_pushes_current_percentage_into_children() {
        let mut bar = ProgressBar::new(Some(100.0)).with_progress(50.0);
        let decls = bar.compose();
        let bar_child = (decls[0].widget() as &dyn std::any::Any)
            .downcast_ref::<Bar>()
            .expect("first child is the Bar");
        assert_eq!(bar_child.percentage(), Some(0.5));
        let pct_child = (decls[1].widget() as &dyn std::any::Any)
            .downcast_ref::<PercentageStatus>()
            .expect("second child is the PercentageStatus");
        assert_eq!(pct_child.text(), "50%");
        let eta_child = (decls[2].widget() as &dyn std::any::Any)
            .downcast_ref::<ETAStatus>()
            .expect("third child is the ETAStatus");
        assert_eq!(eta_child.text(), "--:--:--");
    }

    // ── Status label tests ──────────────────────────────────────────

    #[test]
    fn percentage_status_text() {
        assert_eq!(PercentageStatus::new(None).text(), "--%");
        assert_eq!(PercentageStatus::new(Some(0.0)).text(), "0%");
        assert_eq!(PercentageStatus::new(Some(0.5)).text(), "50%");
        assert_eq!(PercentageStatus::new(Some(1.0)).text(), "100%");
    }

    #[test]
    fn eta_status_text() {
        assert_eq!(ETAStatus::new(None).text(), "--:--:--");
        assert_eq!(ETAStatus::new(Some(61)).text(), "00:01:01");
    }

    // ── Gradient tests ──────────────────────────────────────────────

    fn make_two_stop_gradient(start: Color, end: Color) -> LinearGradient {
        LinearGradient::new(0.0, vec![(0.0, start), (1.0, end)])
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
    fn gradient_flows_into_composed_bar_child() {
        let start = Color::rgb(255, 0, 0);
        let end = Color::rgb(0, 0, 255);
        let mut bar =
            ProgressBar::new(Some(100.0)).with_gradient(make_two_stop_gradient(start, end));
        let decls = bar.compose();
        let bar_child = (decls[0].widget() as &dyn std::any::Any)
            .downcast_ref::<Bar>()
            .expect("first child is the Bar");
        assert!(bar_child.gradient.is_some());
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
        let bar = Bar::new(
            Some(1.0),
            Some(gradient),
            AnimationLevel::Full,
            Instant::now(),
        );

        let (segments, component) = bar.render_determinate(5);
        assert_eq!(component, "bar--complete");
        // Each non-control segment is a single cell; all should be bar glyphs.
        let text = segments_text(&segments);
        assert!(
            text.chars().all(|c| c == BarRenderable::BAR
                || c == BarRenderable::HALF_BAR_LEFT
                || c == BarRenderable::HALF_BAR_RIGHT),
            "unexpected glyph in gradient output: {text:?}"
        );
    }

    #[test]
    fn gradient_determinate_partial_fill() {
        let start = Color::rgb(255, 0, 0);
        let end = Color::rgb(0, 0, 255);
        let gradient = make_two_stop_gradient(start, end);
        let bar = Bar::new(
            Some(0.5),
            Some(gradient),
            AnimationLevel::Full,
            Instant::now(),
        );

        let (segments, component) = bar.render_determinate(10);
        assert_eq!(component, "bar--bar");
        // 50% of 10 → 5 filled + boundary glyph + 4 background
        assert_eq!(segments_text(&segments), "━━━━━╺━━━━");
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
            (1.0, Color::rgb(0x66, 0x33, 0x99)),
        ];
        let gradient = LinearGradient::new(0.0, stops);
        let bar = Bar::new(
            Some(0.7),
            Some(gradient),
            AnimationLevel::Full,
            Instant::now(),
        );

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

    #[test]
    fn no_gradient_still_works_normally() {
        let bar = Bar::new(Some(0.5), None, AnimationLevel::Full, Instant::now());
        let (segments, component) = bar.render_determinate(10);
        assert_eq!(segments_text(&segments), "━━━━━╺━━━━");
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
        let bar = Bar::new(
            Some(1.0),
            Some(gradient),
            AnimationLevel::Full,
            Instant::now(),
        );
        let (segments, _) = bar.render_determinate(5);

        // Collect foreground red-channel values of highlighted (non-control) segments.
        let fg_reds: Vec<u8> = segments
            .iter()
            .filter(|s| s.control.is_none())
            .filter_map(|s| {
                let color = s.style.as_ref()?.color?;
                if let rich_rs::SimpleColor::Rgb { r, .. } = color {
                    Some(r)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            fg_reds.len(),
            5,
            "expected 5 highlighted cells, got: {fg_reds:?}"
        );

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
        assert_eq!(
            leftmost_r, 255,
            "leftmost cell (t=1.25 clamped to 1.0) should be white (r=255), got r={leftmost_r}"
        );
        assert!(
            rightmost_r < 100,
            "rightmost cell (t=0.25) should be close to black (r<100), got r={rightmost_r}"
        );
    }

    /// Verify gradient direction with a partially-filled bar, and that ONLY
    /// the highlighted cells are recolored (Python stylizes `highlight_bar`
    /// only — the background track keeps its component style).
    #[test]
    fn gradient_direction_partial_fill_reversed_and_track_untouched() {
        let start = Color::rgb(0, 0, 0);
        let end = Color::rgb(255, 255, 255);
        let gradient = make_two_stop_gradient(start, end);

        // 50% filled → width=10 → 5 highlighted cells (no half glyph).
        let bar = Bar::new(
            Some(0.5),
            Some(gradient),
            AnimationLevel::Full,
            Instant::now(),
        );
        let (segments, _) = bar.render_determinate(10);

        // Collect fg red-channel values from cells that carry a color. In this
        // off-tree test the component styles resolve empty, so ONLY the
        // gradient-recolored (highlighted) cells have a color.
        let highlighted_fg_reds: Vec<u8> = segments
            .iter()
            .filter(|s| s.control.is_none())
            .filter_map(|s| {
                let color = s.style.as_ref()?.color?;
                if let rich_rs::SimpleColor::Rgb { r, .. } = color {
                    Some(r)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            highlighted_fg_reds.len(),
            5,
            "exactly the 5 highlighted cells are gradient-recolored \
             (the track keeps the background style): {highlighted_fg_reds:?}"
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
