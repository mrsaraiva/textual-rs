use rich_rs::{Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};

use crate::style::{BorderEdge, Margin, Style, parse_color_like};

use super::{LayoutConstraints, Widget, WidgetId};

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
        out.push(Segment::new(" ".repeat(width - len)));
    }
    out
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
            text = rich_rs::set_cell_size(&text, seg_len - offset_in_seg);
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

pub(crate) fn collect_focus_ids(widget: &mut dyn Widget, out: &mut Vec<WidgetId>) {
    if widget.focusable() {
        out.push(widget.id());
    }
    widget.visit_children_mut(&mut |child| collect_focus_ids(child, out));
}

pub(crate) fn set_focus_by_id(widget: &mut dyn Widget, target: Option<WidgetId>) {
    if widget.focusable() {
        widget.set_focus(target == Some(widget.id()));
    }
    widget.visit_children_mut(&mut |child| set_focus_by_id(child, target));
}

pub(crate) fn set_hover_by_id(widget: &mut dyn Widget, target: Option<WidgetId>) {
    widget.set_hovered(target == Some(widget.id()));
    widget.visit_children_mut(&mut |child| set_hover_by_id(child, target));
}

pub(crate) fn dispatch_event_to_focus(
    widget: &mut dyn Widget,
    target: WidgetId,
    event: &Event,
    ctx: &mut EventCtx,
) {
    if widget.id() == target {
        widget.on_event(event, ctx);
        return;
    }
    widget.visit_children_mut(&mut |child| {
        if !ctx.handled() {
            dispatch_event_to_focus(child, target, event, ctx);
        }
    });
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

pub(crate) fn constraints_from_style(style: &Style) -> LayoutConstraints {
    LayoutConstraints {
        min_width: style.min_width,
        max_width: style.max_width,
        min_height: style.min_height,
        max_height: style.max_height,
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
    let fallback_bg = parse_color_like("$background");
    let inner_bg = style.bg.or(fallback_bg);
    // If the parent doesn't specify a background, prefer the widget background as the
    // "outer" background so block / tall borders don't create high-contrast bands.
    // (Textual effectively blends with a base background even when parents don't set one.)
    let outer_bg = parent_style.and_then(|s| s.bg).or(inner_bg).or(fallback_bg);

    let mut lines = Segment::split_and_crop_lines(segments, inner_width.max(1), None, false, false);
    lines = Segment::set_shape(&lines, inner_width.max(1), None, None, false);
    lines = pad_lines_to_width(lines, inner_width.max(1));

    let has_left = border_left.is_set();
    let has_right = border_right.is_set();

    // Wrap content lines with left/right borders (if any).
    let mut edged: Vec<Vec<Segment>> = Vec::with_capacity(lines.len());
    for line in lines {
        let mut row: Vec<Segment> = Vec::new();
        if has_left {
            row.push(border_side_segment(
                border_left,
                inner_bg,
                outer_bg,
                Side::Left,
            ));
        }
        row.extend(line);
        if has_right {
            row.push(border_side_segment(
                border_right,
                inner_bg,
                outer_bg,
                Side::Right,
            ));
        }
        let row = adjust_line_length_no_bg(&row, full_width.max(1));
        edged.push(row);
    }

    // Add top/bottom borders (if any).
    if border_top.is_set() {
        edged.insert(
            0,
            border_horizontal_row(
                border_top,
                inner_bg,
                outer_bg,
                full_width.max(1),
                has_left,
                has_right,
                true,
            ),
        );
    }
    if border_bottom.is_set() {
        edged.push(border_horizontal_row(
            border_bottom,
            inner_bg,
            outer_bg,
            full_width.max(1),
            has_left,
            has_right,
            false,
        ));
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

#[derive(Debug, Clone, Copy)]
enum Side {
    Left,
    Right,
}

fn border_chars(edge_type: &str) -> ([[char; 3]; 3], [[u8; 3]; 3]) {
    match edge_type {
        "solid" => (
            [['┌', '─', '┐'], ['│', ' ', '│'], ['└', '─', '┘']],
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
        _ => (
            [[' ', ' ', ' '], [' ', ' ', ' '], [' ', ' ', ' ']],
            [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
        ),
    }
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
    let border_style = rich_rs::Style::new().with_color(border_color);
    let inner = rich_rs::Style::new()
        .with_bgcolor(inner_bg.unwrap_or_else(|| parse_color_like("$background").unwrap()))
        .combine(&border_style);
    let outer = rich_rs::Style::new()
        .with_bgcolor(outer_bg.unwrap_or_else(|| parse_color_like("$background").unwrap()))
        .combine(&border_style);
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
    let left = vec![Segment::new(" ".repeat(line_pad))];
    let right = vec![Segment::new(" ".repeat(line_pad))];

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
    for _ in 0..margin.top {
        out.push(pad_line.clone());
    }
    for line in lines {
        let mut row: Vec<Segment> = Vec::new();
        if margin.left > 0 {
            row.push(Segment::new(" ".repeat(margin.left)));
        }
        row.extend(line);
        if margin.right > 0 {
            row.push(Segment::new(" ".repeat(margin.right)));
        }
        let adjusted = adjust_line_length_no_bg(&row, width);
        out.push(adjusted);
    }
    for _ in 0..margin.bottom {
        out.push(pad_line.clone());
    }
    out
}
