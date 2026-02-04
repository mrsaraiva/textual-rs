use rich_rs::{Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};

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

pub(crate) fn pad_lines_to_width(lines: Vec<Vec<Segment>>, width: usize) -> Vec<Vec<Segment>> {
    lines
        .into_iter()
        .map(|line| Segment::adjust_line_length(&line, width, None, true))
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
        return Segment::adjust_line_length(line, width, None, true);
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
        let padding = " ".repeat(remaining);
        out.push(Segment::new(padding));
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
    top.push_str(&std::iter::repeat(b.top).take(fill_width).collect::<String>());
    top.push(b.top_right);
    out.push(vec![Segment::styled(top, style)]);

    let mut content = lines;
    content = Segment::set_shape(&content, width - 2, Some(height - 2), None, false);

    for line in content.into_iter().take(height - 2) {
        let mut row: Vec<Segment> = Vec::new();
        row.push(Segment::styled(b.mid_left.to_string(), style));
        let inner = Segment::adjust_line_length(&line, width - 2, None, true);
        row.extend(inner);
        row.push(Segment::styled(b.mid_right.to_string(), style));
        out.push(row);
    }

    let mut bottom = String::new();
    bottom.push(b.bottom_left);
    bottom.push_str(&std::iter::repeat(b.bottom).take(width - 2).collect::<String>());
    bottom.push(b.bottom_right);
    out.push(vec![Segment::styled(bottom, style)]);

    out
}
