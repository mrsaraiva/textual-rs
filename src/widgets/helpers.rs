use rich_rs::{MetaValue, Renderable, Segment, Segments, StyleMeta};
use std::sync::OnceLock;

use crate::debug::{border_debug_matches, debug_border};
use crate::style::{BorderEdge, Margin, Scalar, Style, parse_color_like};

use super::{LayoutConstraints, Widget};

pub(crate) fn merge_constraints(
    primary: LayoutConstraints,
    fallback: LayoutConstraints,
) -> LayoutConstraints {
    LayoutConstraints {
        min_width: primary.min_width.or(fallback.min_width),
        max_width: primary.max_width.or(fallback.max_width),
        min_height: primary.min_height.or(fallback.min_height),
        max_height: primary.max_height.or(fallback.max_height),
    }
}

pub(crate) fn clamp_with_constraints(
    value: usize,
    min: Option<usize>,
    max: Option<usize>,
    limit: usize,
) -> usize {
    let mut out = value.max(1);
    if let Some(min) = min {
        out = out.max(min);
    }
    if let Some(max) = max {
        out = out.min(max);
    }
    out.min(limit.max(1))
}

pub(crate) fn adjust_line_length_no_bg(line: &[Segment], width: usize) -> Vec<Segment> {
    let width = width.max(1);
    // Crop to width, but do not pad with the end-style background. We'll add an explicit
    // no-style padding segment instead.
    let mut out = Segment::adjust_line_length(line, width, None, false);
    let len = Segment::get_line_length(&out);
    if len < width {
        out.push(no_text_style_space_segment(width - len));
    }
    out
}

fn no_text_style_space_segment(width: usize) -> Segment {
    let mut segment = Segment::new(" ".repeat(width));
    let mut map = std::collections::BTreeMap::new();
    map.insert("textual:no_text_style".to_string(), MetaValue::Bool(true));
    let mut meta = StyleMeta::new();
    meta.meta = Some(std::sync::Arc::new(map));
    segment.meta = Some(meta);
    segment
}

pub(crate) fn pad_lines_to_width(lines: Vec<Vec<Segment>>, width: usize) -> Vec<Vec<Segment>> {
    lines
        .into_iter()
        .map(|line| adjust_line_length_no_bg(&line, width))
        .collect()
}

pub(crate) fn crop_line_horizontal(line: &[Segment], start: usize, width: usize) -> Vec<Segment> {
    if width == 0 {
        return Vec::new();
    }
    if start == 0 {
        return adjust_line_length_no_bg(line, width);
    }

    let mut out: Vec<Segment> = Vec::new();
    let mut skipped = 0usize;
    let mut remaining = width;

    for segment in line {
        if segment.control.is_some() {
            out.push(segment.clone());
            continue;
        }

        let seg_len = segment.cell_len();
        if skipped + seg_len <= start {
            skipped += seg_len;
            continue;
        }

        let offset_in_seg = start.saturating_sub(skipped);
        let visible_len = seg_len.saturating_sub(offset_in_seg);
        if visible_len == 0 {
            skipped += seg_len;
            continue;
        }

        let slice_len = visible_len.min(remaining);
        let mut text = segment.text.to_string();
        if offset_in_seg > 0 {
            // Drop the leading `offset_in_seg` cells; keep everything after them.
            // (Do NOT truncate the tail first — that would lose `offset_in_seg` cells
            // off the end, breaking left-clipped content such as horizontal scrolling.)
            text = text.chars().skip(offset_in_seg).collect();
        }
        let cropped_text = rich_rs::set_cell_size(&text, slice_len);
        let mut out_segment = segment.clone();
        out_segment.text = cropped_text.into();
        out_segment.control = None;
        out.push(out_segment);
        remaining = remaining.saturating_sub(slice_len);
        skipped += seg_len;
        if remaining == 0 {
            break;
        }
    }

    if remaining > 0 {
        out.push(Segment::new(" ".repeat(remaining)));
    }

    out
}

pub struct WidgetRenderable<'a> {
    widget: &'a dyn Widget,
}

impl<'a> WidgetRenderable<'a> {
    pub fn new(widget: &'a dyn Widget) -> Self {
        Self { widget }
    }
}

impl Renderable for WidgetRenderable<'_> {
    fn render(&self, console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Segments {
        self.widget.render_styled(console, options)
    }
}

pub(crate) fn apply_debug_box(
    lines: Vec<Vec<Segment>>,
    width: usize,
    height: usize,
    label: Option<&str>,
    style: rich_rs::Style,
) -> Vec<Vec<Segment>> {
    if width < 3 || height < 3 {
        return lines;
    }

    let b = rich_rs::r#box::SQUARE;
    let mut out: Vec<Vec<Segment>> = Vec::new();

    let mut top = String::new();
    top.push(b.top_left);
    let mut label_text = String::new();
    if let Some(text) = label {
        for ch in text.chars() {
            label_text.push(ch);
            if rich_rs::cell_len(&label_text) > width - 2 {
                label_text.pop();
                break;
            }
        }
    }
    let label_width = rich_rs::cell_len(&label_text);
    let fill_width = (width - 2).saturating_sub(label_width);
    top.push_str(&label_text);
    top.push_str(
        &std::iter::repeat(b.top)
            .take(fill_width)
            .collect::<String>(),
    );
    top.push(b.top_right);
    out.push(vec![Segment::styled(top, style)]);

    let mut content = lines;
    content = Segment::set_shape(&content, width - 2, Some(height - 2), None, false);

    for line in content.into_iter().take(height - 2) {
        let mut row: Vec<Segment> = Vec::new();
        row.push(Segment::styled(b.mid_left.to_string(), style));
        let inner = adjust_line_length_no_bg(&line, width - 2);
        row.extend(inner);
        row.push(Segment::styled(b.mid_right.to_string(), style));
        out.push(row);
    }

    let mut bottom = String::new();
    bottom.push(b.bottom_left);
    bottom.push_str(
        &std::iter::repeat(b.bottom)
            .take(width - 2)
            .collect::<String>(),
    );
    bottom.push(b.bottom_right);
    out.push(vec![Segment::styled(bottom, style)]);

    out
}

pub(crate) fn margin_from_style(style: &crate::style::Style) -> Margin {
    style.margin.unwrap_or_default()
}

/// Extract a `Scalar::Cells` value as `usize`, returning `None` for other variants.
pub(crate) fn scalar_cells_or(scalar: Option<Scalar>) -> Option<usize> {
    match scalar {
        Some(Scalar::Cells(n)) => Some(n as usize),
        _ => None,
    }
}

pub(crate) fn constraints_from_style(style: &Style) -> LayoutConstraints {
    LayoutConstraints {
        min_width: scalar_cells_or(style.min_width),
        max_width: scalar_cells_or(style.max_width),
        min_height: scalar_cells_or(style.min_height),
        max_height: scalar_cells_or(style.max_height),
    }
}

pub(crate) fn border_spacing_from_style(style: &Style) -> (usize, usize, usize, usize) {
    let top = if style.border_top.is_set() { 1 } else { 0 };
    let right = if style.border_right.is_set() { 1 } else { 0 };
    let bottom = if style.border_bottom.is_set() { 1 } else { 0 };
    let left = if style.border_left.is_set() { 1 } else { 0 };
    (top, bottom, left, right)
}

pub(crate) fn border_vertical_padding(style: &Style) -> usize {
    let (top, bottom, _, _) = border_spacing_from_style(style);
    top + bottom
}

/// Resolve a widget's own vertical chrome (border top/bottom + padding
/// top/bottom) by resolving its cascaded style off-tree.
///
/// `layout_height()` reports a widget's OUTER auto height (content + own
/// chrome); the layout side adds only margin (`extract_child_spec`). Leaf
/// widgets that have a border via CSS (`Checkbox`, `Switch`, `Digits`, …) use
/// this so an example that gives them `border: tall`/`double` is allocated the
/// border rows instead of being clipped to content height.
pub(crate) fn resolved_vertical_chrome<T: Widget + ?Sized>(widget: &T) -> usize {
    let meta = crate::css::selector_meta_generic(widget);
    let resolved = crate::css::resolve_style(widget, &meta);
    let padding = resolved.effective_padding();
    let (bt, bb, _, _) = border_spacing_from_style(&resolved);
    usize::from(padding.top.saturating_add(padding.bottom)) + bt + bb
}

// `opacity_percent`: When set, pre-blend border colors by opacity, matching Python's
// `base_background + border_color.multiply_alpha(opacity)` step. This is necessary so
// that after `apply_widget_opacity_to_segments` applies a second blend, the net result
// matches Python's double-application of opacity on border-character foreground colors.
pub(crate) fn apply_border_edges(
    segments: Segments,
    inner_width: usize,
    style: Style,
    parent_style: Option<Style>,
    full_width: usize,
    full_height: usize,
    debug_widget_label: &str,
    border_title: Option<&str>,
    border_subtitle: Option<&str>,
    opacity_percent: Option<u8>,
) -> Segments {
    let border_top = style.border_top;
    let border_right = style.border_right;
    let border_bottom = style.border_bottom;
    let border_left = style.border_left;

    if !border_top.is_set()
        && !border_right.is_set()
        && !border_bottom.is_set()
        && !border_left.is_set()
    {
        return segments;
    }

    // Inner (widget) and outer (parent) backgrounds used for border blending.
    let fallback_bg = parse_color_like("$background").unwrap_or(crate::style::Color::rgb(0, 0, 0));
    let parent_bg = parent_style.and_then(|s| s.bg).unwrap_or(fallback_bg);
    let inner_bg = style
        .bg
        .map(|c| c.flatten_over(parent_bg))
        .unwrap_or(parent_bg);
    let outer_bg = parent_bg;

    // Pre-blend opacity for border colors: when `opacity_percent` is set,
    // Python pre-composites the border color over `base_background` at the
    // given opacity inside `render_line`, BEFORE `_apply_opacity` sees it.
    // We replicate that as a float factor passed down to `border_inner_outer_styles`.
    let pre_blend_opacity: Option<f32> = opacity_percent
        .filter(|&o| o < 100)
        .map(|o| o as f32 / 100.0);
    let border_debug = border_debug_matches(debug_widget_label);
    if border_debug {
        debug_border(&format!(
            "[border] widget={} size={}x{} inner_width={} edges=top:{} right:{} bottom:{} left:{} edge_colors=top:{:?} right:{:?} bottom:{:?} left:{:?} parent_bg={:?} inner_bg={:?} outer_bg={:?}",
            debug_widget_label,
            full_width,
            full_height,
            inner_width,
            border_top.edge_type(),
            border_right.edge_type(),
            border_bottom.edge_type(),
            border_left.edge_type(),
            border_top.color(),
            border_right.color(),
            border_bottom.color(),
            border_left.color(),
            parent_bg,
            inner_bg,
            outer_bg,
        ));
    }

    let mut lines = Segment::split_and_crop_lines(segments, inner_width.max(1), None, false, false);
    // Ensure the widget interior is fully painted with the widget style (at least background).
    // `split_and_crop_lines` may trim trailing whitespace, which would otherwise expose the
    // parent background and make the widget look "hollow".
    let mut fill = rich_rs::Style::new().with_bgcolor(inner_bg.to_simple_opaque());
    if let Some(fg) = style.fg {
        fill = fill.with_color(fg.flatten_over(inner_bg).to_simple_opaque());
    }
    let border_rows = usize::from(border_top.is_set()) + usize::from(border_bottom.is_set());
    let interior_height = full_height.max(1).saturating_sub(border_rows);
    lines = Segment::set_shape(
        &lines,
        inner_width.max(1),
        Some(interior_height),
        Some(fill),
        false,
    );

    let has_left = border_left.is_set();
    let has_right = border_right.is_set();

    // Wrap content lines with left/right borders (if any).
    let mut edged: Vec<Vec<Segment>> = Vec::with_capacity(lines.len());
    for line in lines {
        let mut row: Vec<Segment> = Vec::new();
        if has_left {
            row.push(border_side_segment(
                border_left,
                Some(inner_bg),
                Some(outer_bg),
                Side::Left,
                pre_blend_opacity,
            ));
        }
        row.extend(line);
        if has_right {
            row.push(border_side_segment(
                border_right,
                Some(inner_bg),
                Some(outer_bg),
                Side::Right,
                pre_blend_opacity,
            ));
        }
        let row = adjust_line_length_no_bg(&row, full_width.max(1));
        edged.push(row);
    }

    // Add top/bottom borders (if any).
    if border_top.is_set() {
        let mut top_row = border_horizontal_row(
            border_top,
            Some(inner_bg),
            Some(outer_bg),
            full_width.max(1),
            has_left,
            has_right,
            true,
            pre_blend_opacity,
        );
        if let Some(title) = border_title.filter(|t| !t.is_empty()) {
            overlay_border_text(
                &mut top_row,
                title,
                full_width.max(1),
                has_left,
                has_right,
                style
                    .border_title_align
                    .unwrap_or(crate::style::HorizontalAlign::Left),
                style.border_title_color,
                style.border_title_background,
                style.border_title_style,
                inner_bg,
                border_title_flip(border_top.edge_type()).0,
            );
        }
        if border_debug {
            debug_border(&format!(
                "[border_row] widget={} row=top segments={}",
                debug_widget_label,
                debug_border_row_segments(&top_row)
            ));
        }
        edged.insert(0, top_row);
    }
    if border_bottom.is_set() {
        let mut bottom_row = border_horizontal_row(
            border_bottom,
            Some(inner_bg),
            Some(outer_bg),
            full_width.max(1),
            has_left,
            has_right,
            false,
            pre_blend_opacity,
        );
        if let Some(subtitle) = border_subtitle.filter(|t| !t.is_empty()) {
            overlay_border_text(
                &mut bottom_row,
                subtitle,
                full_width.max(1),
                has_left,
                has_right,
                style
                    .border_subtitle_align
                    .unwrap_or(crate::style::HorizontalAlign::Right),
                style.border_subtitle_color,
                style.border_subtitle_background,
                style.border_subtitle_style,
                inner_bg,
                border_title_flip(border_bottom.edge_type()).1,
            );
        }
        if border_debug {
            debug_border(&format!(
                "[border_row] widget={} row=bottom segments={}",
                debug_widget_label,
                debug_border_row_segments(&bottom_row)
            ));
        }
        edged.push(bottom_row);
    }

    // Clamp/pad to requested height.
    edged = Segment::set_shape(
        &edged,
        full_width.max(1),
        Some(full_height.max(1)),
        None,
        false,
    );

    let line_count = edged.len();
    let mut out = Segments::new();
    for (idx, line) in edged.into_iter().enumerate() {
        out.extend(line);
        if idx + 1 < line_count {
            out.push(Segment::line());
        }
    }
    out
}

fn debug_border_row_segments(row: &[Segment]) -> String {
    row.iter()
        .enumerate()
        .map(|(idx, seg)| {
            let style = seg.style.unwrap_or_default();
            format!(
                "#{idx} text={:?} len={} fg={:?} bg={:?} rev={:?}",
                seg.text,
                seg.cell_len(),
                style.color,
                style.bgcolor,
                style.reverse
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

#[derive(Debug, Clone, Copy)]
enum Side {
    Left,
    Right,
}

pub(crate) fn border_chars(edge_type: &str) -> ([[char; 3]; 3], [[u8; 3]; 3]) {
    let edge_type = effective_border_edge_type(edge_type);
    match edge_type {
        "solid" => (
            [['┌', '─', '┐'], ['│', ' ', '│'], ['└', '─', '┘']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
        "heavy" => (
            [['┏', '━', '┓'], ['┃', ' ', '┃'], ['┗', '━', '┛']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
        "block" => (
            [['▄', '▄', '▄'], ['█', ' ', '█'], ['▀', '▀', '▀']],
            [[1, 1, 1], [0, 0, 0], [1, 1, 1]],
        ),
        "tall" => (
            [['▊', '▔', '▎'], ['▊', ' ', '▎'], ['▊', '▁', '▎']],
            [[2, 0, 1], [2, 0, 1], [2, 0, 1]],
        ),
        "outer" => (
            [['▛', '▀', '▜'], ['▌', ' ', '▐'], ['▙', '▄', '▟']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
        "hkey" => (
            [['▔', '▔', '▔'], [' ', ' ', ' '], ['▁', '▁', '▁']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
        "vkey" => (
            [['▏', ' ', '▕'], ['▏', ' ', '▕'], ['▏', ' ', '▕']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
        "ascii" => (
            [['+', '-', '+'], ['|', ' ', '|'], ['+', '-', '+']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
        "blank" => (
            [[' ', ' ', ' '], [' ', ' ', ' '], [' ', ' ', ' ']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
        "round" => (
            [['╭', '─', '╮'], ['│', ' ', '│'], ['╰', '─', '╯']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
        "double" => (
            [['╔', '═', '╗'], ['║', ' ', '║'], ['╚', '═', '╝']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
        "dashed" => (
            [['┏', '╍', '┓'], ['╏', ' ', '╏'], ['┗', '╍', '┛']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
        "inner" => (
            [['▗', '▄', '▖'], ['▐', ' ', '▌'], ['▝', '▀', '▘']],
            [[1, 1, 1], [1, 1, 1], [1, 1, 1]],
        ),
        "thick" => (
            [['█', '▀', '█'], ['█', ' ', '█'], ['█', '▄', '█']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
        "panel" => (
            [['▊', '█', '▎'], ['▊', ' ', '▎'], ['▊', '▁', '▎']],
            [[2, 0, 1], [2, 0, 1], [2, 0, 1]],
        ),
        "tab" => (
            [['▁', '▁', '▁'], ['▎', ' ', '▊'], ['▔', '▔', '▔']],
            [[1, 1, 1], [0, 1, 3], [1, 1, 1]],
        ),
        "wide" => (
            [['▁', '▁', '▁'], ['▎', ' ', '▊'], ['▔', '▔', '▔']],
            [[1, 1, 1], [0, 1, 3], [1, 1, 1]],
        ),
        _ => (
            [[' ', ' ', ' '], [' ', ' ', ' '], [' ', ' ', ' ']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
    }
}

/// Python `BORDER_TITLE_FLIP` (_border.py:238-241): whether the (title, subtitle)
/// must render with foreground/background swapped for this border type.
pub(crate) fn border_title_flip(edge_type: &str) -> (bool, bool) {
    match edge_type {
        "panel" => (true, false),
        "tab" => (true, true),
        _ => (false, false),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowsSafeBordersMode {
    Off,
    On,
    Auto,
}

fn parse_windows_safe_borders_mode(value: Option<&str>) -> WindowsSafeBordersMode {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("1") | Some("true") | Some("yes") | Some("on") => WindowsSafeBordersMode::On,
        Some("0") | Some("false") | Some("no") | Some("off") => WindowsSafeBordersMode::Off,
        Some("auto") | None => WindowsSafeBordersMode::Auto,
        _ => WindowsSafeBordersMode::Auto,
    }
}

fn windows_safe_border_fallback_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let mode = parse_windows_safe_borders_mode(
            std::env::var("TEXTUAL_WINDOWS_SAFE_BORDERS")
                .ok()
                .as_deref(),
        );
        match mode {
            WindowsSafeBordersMode::On => true,
            WindowsSafeBordersMode::Off => false,
            // Keep auto conservative for now; enable explicitly in known-problematic terminals.
            WindowsSafeBordersMode::Auto => false,
        }
    })
}

fn effective_border_edge_type(edge_type: &str) -> &str {
    if cfg!(target_os = "windows") && windows_safe_border_fallback_enabled() && edge_type == "block"
    {
        return "solid";
    }
    edge_type
}

fn resolve_border_char_style(
    location: u8,
    inner: rich_rs::Style,
    outer: rich_rs::Style,
) -> rich_rs::Style {
    match location {
        0 => inner,
        1 => outer,
        2 => {
            // Cross-combination (Textual): outer background + inner foreground, with reverse.
            let mut s = rich_rs::Style::new();
            s.color = inner.color;
            s.bgcolor = outer.bgcolor;
            s.reverse = Some(true);
            s
        }
        3 => {
            // Cross-combination (Textual): inner background + outer foreground, with reverse.
            let mut s = rich_rs::Style::new();
            s.color = outer.color;
            s.bgcolor = inner.bgcolor;
            s.reverse = Some(true);
            s
        }
        _ => inner,
    }
}

// `pre_blend_opacity`: When the widget carries a CSS `opacity` value, pre-blend
// the border color by that factor.  Python's `_styles_cache.render_line` does:
//   `border_color = base_background + border_color.multiply_alpha(opacity)`
// meaning the border fg is already opacity-reduced BEFORE `_apply_opacity` sees it.
// Pre-blending here ensures that when `apply_widget_opacity_to_segments` applies a
// single blend, the net result matches Python's effective double-blend for border chars.
fn border_inner_outer_styles(
    edge: BorderEdge,
    inner_bg: Option<crate::style::Color>,
    outer_bg: Option<crate::style::Color>,
    pre_blend_opacity: Option<f32>,
) -> (rich_rs::Style, rich_rs::Style) {
    let raw_border_color = edge
        .color()
        .unwrap_or_else(|| parse_color_like("$foreground").unwrap());
    let fallback_bg = parse_color_like("$background").unwrap_or(crate::style::Color::rgb(0, 0, 0));
    let inner_bg = inner_bg.unwrap_or(fallback_bg);
    let outer_bg = outer_bg.unwrap_or(fallback_bg);

    // When `opacity` is active, Python pre-composites the border color over
    // `base_background` (= outer/parent bg) at the given opacity factor:
    //   `base_background + border_color.multiply_alpha(opacity)`
    // = `outer_bg.blend(border_color, border_color.a * opacity)`.
    // For fully-opaque border colors (alpha == 1.0), this simplifies to:
    //   `outer_bg.blend(border_color, opacity)`.
    // We replicate that here so the subsequent `apply_widget_opacity_to_segments`
    // single-pass gives the same final value as Python's double-pass.
    let border_color = if let Some(opacity) = pre_blend_opacity {
        // blend_over_float(under, factor): under + (self - under) * factor
        raw_border_color.blend_over_float(outer_bg, opacity)
    } else {
        raw_border_color
    };

    // Border edge colors may carry alpha (e.g. `$foreground 30%`). Compose over
    // each local surface before converting to terminal color so dim separators
    // (HelpPanel/KeyPanel) render with the expected muted tone.
    let inner_border_style =
        rich_rs::Style::new().with_color(border_color.flatten_over(inner_bg).to_simple_opaque());
    let outer_border_style =
        rich_rs::Style::new().with_color(border_color.flatten_over(outer_bg).to_simple_opaque());
    let inner = rich_rs::Style::new()
        .with_bgcolor(inner_bg.to_simple_opaque())
        .combine(&inner_border_style);
    let outer = rich_rs::Style::new()
        .with_bgcolor(outer_bg.to_simple_opaque())
        .combine(&outer_border_style);
    (inner, outer)
}

fn border_horizontal_row(
    edge: BorderEdge,
    inner_bg: Option<crate::style::Color>,
    outer_bg: Option<crate::style::Color>,
    width: usize,
    has_left: bool,
    has_right: bool,
    top: bool,
    pre_blend_opacity: Option<f32>,
) -> Vec<Segment> {
    let edge_type = edge.edge_type();
    let (chars, locations) = border_chars(edge_type);
    let (inner, outer) = border_inner_outer_styles(edge, inner_bg, outer_bg, pre_blend_opacity);
    let row_idx = if top { 0 } else { 2 };
    let row_chars = chars[row_idx];
    let row_locs = locations[row_idx];

    let left_w = if has_left { 1 } else { 0 };
    let right_w = if has_right { 1 } else { 0 };
    let mid_w = width.saturating_sub(left_w + right_w).max(0);

    let mut out: Vec<Segment> = Vec::new();
    if has_left {
        let s = resolve_border_char_style(row_locs[0], inner, outer);
        out.push(Segment::styled(row_chars[0].to_string(), s));
    }
    {
        let s = resolve_border_char_style(row_locs[1], inner, outer);
        out.push(Segment::styled(row_chars[1].to_string().repeat(mid_w), s));
    }
    if has_right {
        let s = resolve_border_char_style(row_locs[2], inner, outer);
        out.push(Segment::styled(row_chars[2].to_string(), s));
    }
    adjust_line_length_no_bg(&out, width)
}

fn border_side_segment(
    edge: BorderEdge,
    inner_bg: Option<crate::style::Color>,
    outer_bg: Option<crate::style::Color>,
    side: Side,
    pre_blend_opacity: Option<f32>,
) -> Segment {
    let edge_type = edge.edge_type();
    let (chars, locations) = border_chars(edge_type);
    let (inner, outer) = border_inner_outer_styles(edge, inner_bg, outer_bg, pre_blend_opacity);
    let col = match side {
        Side::Left => 0,
        Side::Right => 2,
    };
    let ch = chars[1][col];
    let loc = locations[1][col];
    let s = resolve_border_char_style(loc, inner, outer);
    Segment::styled(ch.to_string(), s)
}

/// A single outline perimeter cell: `(col, row, glyph, style)` in widget-local
/// coordinates (col `0..width`, row `0..height`).
pub(crate) type OutlineCell = (usize, usize, char, rich_rs::Style);

/// Compute the perimeter cells for a widget's CSS `outline`.
///
/// Unlike `border` (which reserves layout space), `outline` is drawn ON TOP of
/// the widget's own edge cells without changing its size, and — critically for
/// containers — ON TOP of any already-composited child content at those edges.
/// This mirrors Python `StylesCache.render_line` (the "Draw any outline" block):
/// the top/bottom rows become full outline rows (with corners when side outlines
/// are present), and each interior row gets a side glyph at col 0 / col w-1.
///
/// The returned cells are painted into the frame buffer AFTER children render,
/// so the outline correctly overdraws the widget's edges regardless of whether
/// the outlined node is a leaf or a container wrapping a child. Glyphs are
/// colored with the outline color flattened over the base/parent background
/// (`outer_bg`); the cell background is the widget surface (`inner_bg`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn outline_edge_cells(
    width: usize,
    height: usize,
    outline_top: BorderEdge,
    outline_right: BorderEdge,
    outline_bottom: BorderEdge,
    outline_left: BorderEdge,
    inner_bg: crate::style::Color,
    outer_bg: crate::style::Color,
) -> Vec<OutlineCell> {
    let has_top = outline_top.is_set();
    let has_right = outline_right.is_set();
    let has_bottom = outline_bottom.is_set();
    let has_left = outline_left.is_set();
    if !has_top && !has_right && !has_bottom && !has_left {
        return Vec::new();
    }
    let width = width.max(1);
    let height = height.max(1);
    let inner = Some(inner_bg);
    let outer = Some(outer_bg);

    let mut cells: Vec<OutlineCell> = Vec::new();

    // Build a horizontal edge row (top or bottom) and emit its per-cell glyphs.
    let mut push_horizontal_row = |edge: BorderEdge, row: usize| {
        let segs = border_horizontal_row(edge, inner, outer, width, has_left, has_right, row == 0, None);
        let mut col = 0usize;
        for seg in segs {
            let style = seg.style.unwrap_or_default();
            for ch in seg.text.chars() {
                if col >= width {
                    break;
                }
                cells.push((col, row, ch, style));
                col += 1;
            }
        }
    };

    if has_top {
        push_horizontal_row(outline_top, 0);
    }
    if has_bottom && height > 1 {
        push_horizontal_row(outline_bottom, height - 1);
    }

    // Side glyphs on interior rows (rows not covered by the top/bottom rows).
    let first_interior = usize::from(has_top);
    let last_interior = height.saturating_sub(usize::from(has_bottom));
    for row in first_interior..last_interior {
        if has_left {
            let seg = border_side_segment(outline_left, inner, outer, Side::Left, None);
            if let Some(ch) = seg.text.chars().next() {
                cells.push((0, row, ch, seg.style.unwrap_or_default()));
            }
        }
        if has_right && width > 1 {
            let seg = border_side_segment(outline_right, inner, outer, Side::Right, None);
            if let Some(ch) = seg.text.chars().next() {
                cells.push((width - 1, row, ch, seg.style.unwrap_or_default()));
            }
        }
    }

    cells
}

fn apply_text_style_flags(style: &mut rich_rs::Style, flags: &crate::style::TextStyleFlags) {
    if flags.bold {
        *style = style.clone().with_bold(true);
    }
    if flags.dim {
        *style = style.clone().with_dim(true);
    }
    if flags.italic {
        *style = style.clone().with_italic(true);
    }
    if flags.underline {
        *style = style.clone().with_underline(true);
    }
    if flags.reverse {
        style.reverse = Some(true);
    }
    if flags.strike {
        style.strike = Some(true);
    }
}

fn overlay_border_text(
    row: &mut Vec<Segment>,
    text: &str,
    width: usize,
    has_left: bool,
    has_right: bool,
    align: crate::style::HorizontalAlign,
    fg: Option<crate::style::Color>,
    bg: Option<crate::style::Color>,
    flags: Option<crate::style::TextStyleFlags>,
    fallback_bg: crate::style::Color,
    flip: bool,
) {
    let left_w = usize::from(has_left);
    let right_w = usize::from(has_right);
    if width <= left_w + right_w {
        return;
    }
    let inner_w = width - left_w - right_w;

    // Python `_border.render_border_label` pads the title with one blank on each
    // side that has a corner; `render_row` then fills the remaining edge with the
    // border character (NOT spaces). The title may use at most
    // `inner_w - (pad_left + pad_right)` cells before padding.
    let pad_left = left_w;
    let pad_right = right_w;
    let max_title_w = inner_w.saturating_sub(pad_left + pad_right);
    if max_title_w == 0 {
        return;
    }
    let truncated = if rich_rs::cell_len(text) > max_title_w {
        rich_rs::set_cell_size(text, max_title_w)
    } else {
        text.to_string()
    };
    let title_w = rich_rs::cell_len(&truncated);
    if title_w == 0 {
        return;
    }
    // Padded label = blank + title + blank (per present corner).
    let padded_title = format!(
        "{}{}{}",
        " ".repeat(pad_left),
        truncated,
        " ".repeat(pad_right)
    );
    let padded_w = title_w + pad_left + pad_right;
    let space_available = inner_w.saturating_sub(padded_w);

    // Fill character + style come from the existing border-line middle segment
    // (the repeated `─`/`━`/etc.), so the dashes keep the border color.
    let middle_idx = usize::from(has_left);
    let middle_seg = row.get(middle_idx).cloned();
    let fill_char: String = middle_seg
        .as_ref()
        .and_then(|s| s.text.chars().next())
        .map(|c| c.to_string())
        .unwrap_or_else(|| "─".to_string());
    let make_fill = |count: usize| -> Option<Segment> {
        if count == 0 {
            return None;
        }
        let text = fill_char.repeat(count);
        Some(match middle_seg.as_ref() {
            Some(seg) => {
                let mut s = seg.clone();
                s.text = text.into();
                s
            }
            None => Segment::new(text),
        })
    };
    // For left/right alignment Python reserves one fill char on the "anchor"
    // side and puts the rest on the other side. Center splits evenly.
    let (before, after) = match align {
        crate::style::HorizontalAlign::Left => {
            (space_available.min(1), space_available.saturating_sub(1))
        }
        crate::style::HorizontalAlign::Right => {
            (space_available.saturating_sub(1), space_available.min(1))
        }
        crate::style::HorizontalAlign::Center => {
            let left = space_available / 2;
            (left, space_available - left)
        }
    };

    let mut middle_style = middle_seg
        .as_ref()
        .and_then(|s| s.style)
        .unwrap_or_default();
    if flip {
        // Python _border.py:397-401: swap fg/bg of the base style for panel/tab titles.
        std::mem::swap(&mut middle_style.color, &mut middle_style.bgcolor);
    }
    if let Some(c) = fg {
        middle_style = middle_style.with_color(c.to_simple_opaque());
    }
    if let Some(c) = bg {
        middle_style = middle_style.with_bgcolor(c.to_simple_opaque());
    } else if middle_style.bgcolor.is_none() {
        middle_style = middle_style.with_bgcolor(fallback_bg.to_simple_opaque());
    }
    if let Some(f) = flags.as_ref() {
        apply_text_style_flags(&mut middle_style, f);
    }

    let mut rebuilt: Vec<Segment> = Vec::with_capacity(5);
    if has_left {
        if let Some(left) = row.first().cloned() {
            rebuilt.push(left);
        }
    }
    if let Some(seg) = make_fill(before) {
        rebuilt.push(seg);
    }
    rebuilt.push(Segment::styled(padded_title, middle_style));
    if let Some(seg) = make_fill(after) {
        rebuilt.push(seg);
    }
    if has_right {
        if let Some(right) = row.last().cloned() {
            rebuilt.push(right);
        }
    }
    *row = adjust_line_length_no_bg(&rebuilt, width);
}

pub(crate) fn apply_line_pad(
    segments: Segments,
    content_width: usize,
    full_width: usize,
    line_pad: usize,
) -> Segments {
    if line_pad == 0 || full_width == content_width {
        return segments;
    }
    let mut lines =
        Segment::split_and_crop_lines(segments, content_width.max(1), None, true, false);
    // Ensure consistent shaping before we pad.
    lines = Segment::set_shape(&lines, content_width.max(1), None, None, false);

    let mut padded: Vec<Vec<Segment>> = Vec::with_capacity(lines.len());
    let left = vec![no_text_style_space_segment(line_pad)];
    let right = vec![no_text_style_space_segment(line_pad)];

    for line in lines {
        let mut row: Vec<Segment> = Vec::new();
        row.extend(left.iter().cloned());
        row.extend(adjust_line_length_no_bg(&line, content_width.max(1)));
        row.extend(right.iter().cloned());
        let row = adjust_line_length_no_bg(&row, full_width.max(1));
        padded.push(row);
    }

    let line_count = padded.len();
    let mut out = Segments::new();
    for (idx, line) in padded.into_iter().enumerate() {
        out.extend(line);
        if idx + 1 < line_count {
            out.push(Segment::line());
        }
    }
    out
}

pub(crate) fn apply_margin(
    lines: Vec<Vec<Segment>>,
    width: usize,
    margin: Margin,
) -> Vec<Vec<Segment>> {
    let mut out: Vec<Vec<Segment>> = Vec::new();
    let pad_line = vec![Segment::new(" ".repeat(width))];
    for _ in 0..margin.top as usize {
        out.push(pad_line.clone());
    }
    for line in lines {
        let mut row: Vec<Segment> = Vec::new();
        if margin.left > 0 {
            row.push(Segment::new(" ".repeat(margin.left as usize)));
        }
        row.extend(line);
        if margin.right > 0 {
            row.push(Segment::new(" ".repeat(margin.right as usize)));
        }
        let adjusted = adjust_line_length_no_bg(&row, width);
        out.push(adjusted);
    }
    for _ in 0..margin.bottom as usize {
        out.push(pad_line.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{WindowsSafeBordersMode, parse_windows_safe_borders_mode};

    #[test]
    fn windows_safe_borders_mode_parses_on_values() {
        for value in ["1", "true", "TRUE", "yes", "on"] {
            assert_eq!(
                parse_windows_safe_borders_mode(Some(value)),
                WindowsSafeBordersMode::On
            );
        }
    }

    #[test]
    fn windows_safe_borders_mode_parses_off_values() {
        for value in ["0", "false", "FALSE", "no", "off"] {
            assert_eq!(
                parse_windows_safe_borders_mode(Some(value)),
                WindowsSafeBordersMode::Off
            );
        }
    }

    #[test]
    fn windows_safe_borders_mode_defaults_to_auto() {
        assert_eq!(
            parse_windows_safe_borders_mode(None),
            WindowsSafeBordersMode::Auto
        );
        assert_eq!(
            parse_windows_safe_borders_mode(Some("auto")),
            WindowsSafeBordersMode::Auto
        );
        assert_eq!(
            parse_windows_safe_borders_mode(Some("unexpected")),
            WindowsSafeBordersMode::Auto
        );
    }
}
