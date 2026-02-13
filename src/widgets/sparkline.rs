use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::style::Color;

use super::{
    Widget, WidgetStyles,
    helpers::{adjust_line_length_no_bg, fixed_height_from_constraints},
};
use crate::reactive::{ReactiveCtx, ReactiveFlags, ReactiveWidget};

/// Unicode bar characters for sparkline rendering (8 levels, bottom to top).
const BARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Summary function type: reduces a slice of data points to a single value.
pub type SummaryFunction = fn(&[f64]) -> f64;

/// Returns the maximum value in the slice (default summary function).
/// Returns 0.0 for empty input or if all values are non-finite.
pub fn summary_max(data: &[f64]) -> f64 {
    data.iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.max(v)))
        })
        .unwrap_or(0.0)
}

/// Returns the minimum value in the slice.
/// Returns 0.0 for empty input or if all values are non-finite.
pub fn summary_min(data: &[f64]) -> f64 {
    data.iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.min(v)))
        })
        .unwrap_or(0.0)
}

/// Returns the mean of the slice.
/// Returns 0.0 for empty input; non-finite values are excluded.
pub fn summary_mean(data: &[f64]) -> f64 {
    let finite: Vec<f64> = data.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return 0.0;
    }
    finite.iter().sum::<f64>() / finite.len() as f64
}

/// A sparkline widget that renders numerical data as a bar chart using Unicode
/// block characters.
///
/// Data is partitioned into buckets matching the widget width. Each bucket is
/// summarised with a configurable function (default: `max`). Bars are colored
/// with a linear gradient between `min_color` and `max_color` based on value.
///
/// # Component classes
///
/// | Class | Description |
/// | :--- | :--- |
/// | `sparkline--max-color` | The color used for the largest values. |
/// | `sparkline--min-color` | The color used for the smallest values. |
///
/// # Default CSS
///
/// ```css
/// Sparkline { height: 1; }
/// Sparkline > .sparkline--max-color { fg: $primary; }
/// Sparkline > .sparkline--min-color { fg: $primary 30%; }
/// ```
#[derive(Debug, Clone)]
pub struct Sparkline {
    data: Vec<f64>,
    summary_function: SummaryFunction,
    /// Explicit min color override (bypasses CSS component class).
    min_color: Option<Color>,
    /// Explicit max color override (bypasses CSS component class).
    max_color: Option<Color>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Sparkline {
    /// Create a new `Sparkline` with the given data.
    pub fn new(data: Vec<f64>) -> Self {
        Self {
            data,
            summary_function: summary_max,
            min_color: None,
            max_color: None,
            classes: vec!["sparkline".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    /// Set the summary function used to reduce each bucket to a single value.
    pub fn summary_function(mut self, f: SummaryFunction) -> Self {
        self.summary_function = f;
        self
    }

    /// Override the minimum color (normally resolved from CSS `sparkline--min-color`).
    pub fn min_color(mut self, color: Color) -> Self {
        self.min_color = Some(color);
        self
    }

    /// Override the maximum color (normally resolved from CSS `sparkline--max-color`).
    pub fn max_color(mut self, color: Color) -> Self {
        self.max_color = Some(color);
        self
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    /// Reactive getter for `data`.
    pub fn get_data(&self) -> &[f64] {
        &self.data
    }

    /// Reactive getter for `summary_function`.
    pub fn get_summary_function(&self) -> SummaryFunction {
        self.summary_function
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `data`. Records the change in the provided
    /// [`ReactiveCtx`].
    pub fn set_data(&mut self, data: Vec<f64>, ctx: &mut ReactiveCtx) {
        if self.data != data {
            let old = std::mem::replace(&mut self.data, data);
            let new = self.data.clone();
            ctx.record_change(
                "data",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(new),
            );
        }
    }

    /// Reactive setter for `summary_function`. Records the change in the provided
    /// [`ReactiveCtx`].
    pub fn set_summary_function(&mut self, f: SummaryFunction, ctx: &mut ReactiveCtx) {
        if (self.summary_function as usize) != (f as usize) {
            let old = self.summary_function;
            self.summary_function = f;
            ctx.record_change(
                "summary_function",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(f),
            );
        }
    }

    // ── Rendering helpers ───────────────────────────────────────────

    /// Partition `data` into `num_buckets` contiguous slices using fractional stepping.
    fn buckets(data: &[f64], num_buckets: usize) -> Vec<Vec<f64>> {
        if data.is_empty() || num_buckets == 0 {
            return vec![vec![]; num_buckets];
        }
        let len = data.len();
        let step_num = len;
        let step_den = num_buckets;
        (0..num_buckets)
            .map(|i| {
                let start = (step_num * i) / step_den;
                let end = (step_num * (i + 1)) / step_den;
                if start < end {
                    data[start..end].to_vec()
                } else {
                    vec![]
                }
            })
            .collect()
    }

    /// Resolve min/max colors from overrides or CSS component classes.
    fn resolve_colors(&self) -> (Color, Color) {
        let min_color = self.min_color.unwrap_or_else(|| {
            crate::css::resolve_component_style(self, &["sparkline--min-color"])
                .fg
                .unwrap_or(Color::rgb(0, 120, 215))
        });
        let max_color = self.max_color.unwrap_or_else(|| {
            crate::css::resolve_component_style(self, &["sparkline--max-color"])
                .fg
                .unwrap_or(Color::rgb(0, 120, 215))
        });
        (min_color, max_color)
    }
}

impl Widget for Sparkline {
    fn focusable(&self) -> bool {
        false
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let (min_color, max_color) = self.resolve_colors();

        let min_style = rich_rs::Style::new().with_color(min_color.to_simple_opaque());
        let max_style = rich_rs::Style::new().with_color(max_color.to_simple_opaque());

        // Empty data: show flat baseline.
        if self.data.is_empty() {
            let mut out = Segments::new();
            let blank = " ".repeat(width);
            for _ in 0..height.saturating_sub(1) {
                out.push(Segment::styled(blank.clone(), rich_rs::Style::new()));
                out.push(Segment::line());
            }
            let baseline = BARS[0].to_string().repeat(width);
            out.push(Segment::styled(baseline, min_style));
            return out;
        }

        // Single data point: full-height bars in max color.
        if self.data.len() == 1 {
            let mut out = Segments::new();
            let full_bar = BARS[BARS.len() - 1].to_string().repeat(width);
            for i in 0..height {
                out.push(Segment::styled(full_bar.clone(), max_style));
                if i + 1 < height {
                    out.push(Segment::line());
                }
            }
            return out;
        }

        let bar_line_segments = BARS.len();
        let bar_segments = bar_line_segments * height - 1;

        // Filter out non-finite values for min/max computation.
        let finite_iter = || self.data.iter().copied().filter(|v| v.is_finite());
        let minimum = finite_iter().fold(f64::INFINITY, f64::min);
        let maximum = finite_iter().fold(f64::NEG_INFINITY, f64::max);
        let (minimum, maximum) = if !minimum.is_finite() || !maximum.is_finite() {
            (0.0, 1.0) // All data is non-finite; fall back to safe range.
        } else {
            (minimum, maximum)
        };
        let extent = if (maximum - minimum).abs() < f64::EPSILON {
            1.0
        } else {
            maximum - minimum
        };

        let buckets = Self::buckets(&self.data, width);
        let summary_fn = self.summary_function;

        // Pre-compute summary values and height ratios for each bucket.
        let summaries: Vec<f64> = buckets
            .iter()
            .map(|bucket| {
                if bucket.is_empty() {
                    minimum
                } else {
                    summary_fn(bucket)
                }
            })
            .collect();

        let height_ratios: Vec<f64> = summaries
            .iter()
            .map(|&val| ((val - minimum) / extent).clamp(0.0, 1.0))
            .collect();

        let mut out = Segments::new();

        // Render from top row to bottom row (reversed).
        for row in (0..height).rev() {
            let current_bar_part_low = row * bar_line_segments;
            let current_bar_part_high = (row + 1) * bar_line_segments;

            let mut bucket_index = 0.0_f64;
            let step = buckets.len() as f64 / width as f64;
            let mut line_segs: Vec<Segment> = Vec::with_capacity(width);

            for _ in 0..width {
                let bi = (bucket_index as usize).min(buckets.len().saturating_sub(1));
                let height_ratio = height_ratios[bi];
                let bar_index = (height_ratio * bar_segments as f64) as usize;

                let (bar_char, with_color) = if bar_index < current_bar_part_low {
                    (' ', false)
                } else if bar_index >= current_bar_part_high {
                    (BARS[BARS.len() - 1], true)
                } else {
                    (BARS[bar_index % bar_line_segments], true)
                };

                let style = if with_color {
                    let blended = blend_rgb(min_color, max_color, height_ratio);
                    rich_rs::Style::new().with_color(blended.to_simple_opaque())
                } else {
                    rich_rs::Style::new()
                };

                line_segs.push(Segment::styled(bar_char.to_string(), style));
                bucket_index += step;
            }

            let line = adjust_line_length_no_bg(&line_segs, width);
            out.extend(line);

            if row > 0 {
                out.push(Segment::line());
            }
        }

        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }

    fn style_type(&self) -> &'static str {
        "Sparkline"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Sparkline {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl ReactiveWidget for Sparkline {}

// ── Color helper (private) ──────────────────────────────────────────

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
    fn sparkline_not_focusable() {
        let s = Sparkline::new(vec![1.0, 2.0, 3.0]);
        assert!(!s.focusable());
    }

    #[test]
    fn sparkline_style_type() {
        let s = Sparkline::new(vec![]);
        assert_eq!(s.style_type(), "Sparkline");
    }

    #[test]
    fn sparkline_default_height() {
        let s = Sparkline::new(vec![1.0]);
        assert_eq!(s.layout_height(), Some(1));
    }

    #[test]
    fn buckets_partition_evenly() {
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let b = Sparkline::buckets(&data, 2);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0], vec![1.0, 2.0]);
        assert_eq!(b[1], vec![3.0, 4.0]);
    }

    #[test]
    fn buckets_uneven() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = Sparkline::buckets(&data, 3);
        assert_eq!(b.len(), 3);
        // 5 items / 3 buckets: bucket sizes should be 1,2,2 or 2,1,2 etc.
        let total: usize = b.iter().map(|x| x.len()).sum();
        assert_eq!(total, 5);
    }

    #[test]
    fn buckets_empty_data() {
        let b = Sparkline::buckets(&[], 4);
        assert_eq!(b.len(), 4);
        for bucket in &b {
            assert!(bucket.is_empty());
        }
    }

    #[test]
    fn summary_max_basic() {
        assert_eq!(summary_max(&[1.0, 5.0, 3.0]), 5.0);
    }

    #[test]
    fn summary_min_basic() {
        assert_eq!(summary_min(&[1.0, 5.0, 3.0]), 1.0);
    }

    #[test]
    fn summary_mean_basic() {
        let m = summary_mean(&[2.0, 4.0, 6.0]);
        assert!((m - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn blend_rgb_midpoint() {
        let a = Color::rgb(0, 0, 0);
        let b = Color::rgb(255, 255, 255);
        let mid = blend_rgb(a, b, 0.5);
        assert_eq!(mid.r, 128);
        assert_eq!(mid.g, 128);
        assert_eq!(mid.b, 128);
    }

    #[test]
    fn blend_rgb_at_zero() {
        let a = Color::rgb(10, 20, 30);
        let b = Color::rgb(200, 100, 50);
        let result = blend_rgb(a, b, 0.0);
        assert_eq!(result.r, 10);
        assert_eq!(result.g, 20);
        assert_eq!(result.b, 30);
    }

    #[test]
    fn blend_rgb_at_one() {
        let a = Color::rgb(10, 20, 30);
        let b = Color::rgb(200, 100, 50);
        let result = blend_rgb(a, b, 1.0);
        assert_eq!(result.r, 200);
        assert_eq!(result.g, 100);
        assert_eq!(result.b, 50);
    }

    #[test]
    fn set_data_replaces_data() {
        let mut s = Sparkline::new(vec![1.0, 2.0]);
        let mut ctx = ReactiveCtx::new(make_node_id());
        s.set_data(vec![3.0, 4.0, 5.0], &mut ctx);
        assert_eq!(s.data, vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn set_summary_function_changes_fn() {
        let mut s = Sparkline::new(vec![1.0, 5.0, 3.0]);
        let mut ctx = ReactiveCtx::new(make_node_id());
        assert_eq!((s.summary_function)(&[1.0, 5.0, 3.0]), 5.0); // default: max
        s.set_summary_function(summary_min, &mut ctx);
        assert_eq!((s.summary_function)(&[1.0, 5.0, 3.0]), 1.0);
    }

    #[test]
    fn builder_summary_function() {
        let s = Sparkline::new(vec![1.0, 5.0]).summary_function(summary_mean);
        assert!((((s.summary_function)(&[2.0, 4.0])) - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn builder_min_max_color() {
        let s = Sparkline::new(vec![1.0])
            .min_color(Color::rgb(0, 0, 0))
            .max_color(Color::rgb(255, 255, 255));
        assert_eq!(s.min_color, Some(Color::rgb(0, 0, 0)));
        assert_eq!(s.max_color, Some(Color::rgb(255, 255, 255)));
    }

    #[test]
    fn summary_max_empty() {
        assert_eq!(summary_max(&[]), 0.0);
    }

    #[test]
    fn summary_min_empty() {
        assert_eq!(summary_min(&[]), 0.0);
    }

    #[test]
    fn summary_mean_empty() {
        assert_eq!(summary_mean(&[]), 0.0);
    }

    #[test]
    fn summary_max_with_nan() {
        assert_eq!(summary_max(&[f64::NAN, 3.0, f64::NAN]), 3.0);
    }

    #[test]
    fn summary_min_with_nan() {
        assert_eq!(summary_min(&[f64::NAN, 3.0, f64::NAN]), 3.0);
    }

    #[test]
    fn summary_mean_with_nan() {
        let m = summary_mean(&[f64::NAN, 2.0, 4.0]);
        assert!((m - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn summary_max_all_nan() {
        assert_eq!(summary_max(&[f64::NAN, f64::NAN]), 0.0);
    }

    #[test]
    fn buckets_single_data_point() {
        let b = Sparkline::buckets(&[42.0], 3);
        assert_eq!(b.len(), 3);
        // Only one data point, some buckets may be empty
        let total: usize = b.iter().map(|x| x.len()).sum();
        assert_eq!(total, 1);
    }

    #[test]
    fn buckets_more_buckets_than_data() {
        let b = Sparkline::buckets(&[1.0, 2.0], 5);
        assert_eq!(b.len(), 5);
        let total: usize = b.iter().map(|x| x.len()).sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn buckets_zero_buckets() {
        let b = Sparkline::buckets(&[1.0, 2.0], 0);
        assert!(b.is_empty());
    }

    #[test]
    fn blend_rgb_clamp_beyond_one() {
        let a = Color::rgb(0, 0, 0);
        let b = Color::rgb(100, 100, 100);
        let result = blend_rgb(a, b, 1.5); // clamped to 1.0
        assert_eq!(result.r, 100);
    }

    #[test]
    fn blend_rgb_negative_clamp() {
        let a = Color::rgb(50, 50, 50);
        let b = Color::rgb(100, 100, 100);
        let result = blend_rgb(a, b, -0.5); // clamped to 0.0
        assert_eq!(result.r, 50);
    }
}
