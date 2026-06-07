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

pub(crate) fn fixed_height_from_constraints(constraints: LayoutConstraints) -> Option<usize> {
    match (constraints.min_height, constraints.max_height) {
        (Some(min), Some(max)) if min == max => Some(min),
        _ => None,
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

pub(crate) fn empty_classes() -> &'static [String] {
    use std::sync::OnceLock;
    static EMPTY: OnceLock<Vec<String>> = OnceLock::new();
    EMPTY.get_or_init(Vec::new)
}

pub(crate) fn focused_classes() -> &'static [String] {
    use std::sync::OnceLock;
    static FOCUSED: OnceLock<Vec<String>> = OnceLock::new();
    FOCUSED.get_or_init(|| vec!["focused".to_string()])
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
            ));
        }
        row.extend(line);
        if has_right {
            row.push(border_side_segment(
                border_right,
                Some(inner_bg),
                Some(outer_bg),
                Side::Right,
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

fn border_chars(edge_type: &str) -> ([[char; 3]; 3], [[u8; 3]; 3]) {
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
        _ => (
            [[' ', ' ', ' '], [' ', ' ', ' '], [' ', ' ', ' ']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
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

fn border_inner_outer_styles(
    edge: BorderEdge,
    inner_bg: Option<crate::style::Color>,
    outer_bg: Option<crate::style::Color>,
) -> (rich_rs::Style, rich_rs::Style) {
    let border_color = edge
        .color()
        .unwrap_or_else(|| parse_color_like("$foreground").unwrap());
    let fallback_bg = parse_color_like("$background").unwrap_or(crate::style::Color::rgb(0, 0, 0));
    let inner_bg = inner_bg.unwrap_or(fallback_bg);
    let outer_bg = outer_bg.unwrap_or(fallback_bg);

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
) -> Vec<Segment> {
    let edge_type = edge.edge_type();
    let (chars, locations) = border_chars(edge_type);
    let (inner, outer) = border_inner_outer_styles(edge, inner_bg, outer_bg);
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
) -> Segment {
    let edge_type = edge.edge_type();
    let (chars, locations) = border_chars(edge_type);
    let (inner, outer) = border_inner_outer_styles(edge, inner_bg, outer_bg);
    let col = match side {
        Side::Left => 0,
        Side::Right => 2,
    };
    let ch = chars[1][col];
    let loc = locations[1][col];
    let s = resolve_border_char_style(loc, inner, outer);
    Segment::styled(ch.to_string(), s)
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
) {
    let left_w = usize::from(has_left);
    let right_w = usize::from(has_right);
    if width <= left_w + right_w {
        return;
    }
    let inner_w = width - left_w - right_w;
    let clipped = rich_rs::set_cell_size(text, inner_w);
    let text_w = rich_rs::cell_len(&clipped);
    if text_w == 0 {
        return;
    }
    let start = match align {
        crate::style::HorizontalAlign::Left => 0,
        crate::style::HorizontalAlign::Center => inner_w.saturating_sub(text_w) / 2,
        crate::style::HorizontalAlign::Right => inner_w.saturating_sub(text_w),
    };
    let prefix = " ".repeat(start);
    let suffix = " ".repeat(inner_w.saturating_sub(start + text_w));

    let mut middle_style = row
        .get(usize::from(has_left))
        .and_then(|s| s.style)
        .unwrap_or_default();
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

    let mut rebuilt: Vec<Segment> = Vec::with_capacity(3);
    if has_left {
        if let Some(left) = row.first().cloned() {
            rebuilt.push(left);
        }
    }
    rebuilt.push(Segment::styled(
        format!("{prefix}{clipped}{suffix}"),
        middle_style,
    ));
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
