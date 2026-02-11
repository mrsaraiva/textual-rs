//! Rendering boundary for textual-rs.
//!
//! Contract:
//! - Renderables must produce rich-rs `Segment`s (with `Style` + `StyleMeta`).
//! - We preserve `StyleMeta` through shaping, clipping, and diffing.
//! - Terminal output is emitted by applying `diff_to_segments` to produce cursor-safe
//!   control codes + styled segments; no direct ANSI writes in widgets.

use std::cmp;

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Style, StyleMeta};
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    pub style: Option<Style>,
    pub meta: Option<StyleMeta>,
    pub continuation: bool,
}

impl Cell {
    pub fn blank(style: Option<Style>) -> Self {
        Self {
            text: " ".to_string(),
            style,
            meta: None,
            continuation: false,
        }
    }

    pub fn continuation(style: Option<Style>, meta: Option<StyleMeta>) -> Self {
        Self {
            text: String::new(),
            style,
            meta,
            continuation: true,
        }
    }

    pub fn width(&self) -> usize {
        if self.continuation {
            0
        } else {
            cell_len(&self.text)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameBuffer {
    pub width: usize,
    pub height: usize,
    default_style: Option<Style>,
    cells: Vec<Cell>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRegion {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

impl FrameBuffer {
    pub fn new(width: usize, height: usize, style: Option<Style>) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            default_style: style,
            cells: vec![Cell::blank(style); width * height],
        }
    }

    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    pub fn get(&self, x: usize, y: usize) -> &Cell {
        &self.cells[self.idx(x, y)]
    }

    pub fn get_mut(&mut self, x: usize, y: usize) -> &mut Cell {
        let idx = self.idx(x, y);
        &mut self.cells[idx]
    }

    pub fn as_plain_lines(&self) -> Vec<String> {
        let mut lines = Vec::with_capacity(self.height);
        for y in 0..self.height {
            let mut line = String::new();
            for x in 0..self.width {
                let cell = self.get(x, y);
                if cell.continuation {
                    continue;
                }
                if cell.text.is_empty() {
                    line.push(' ');
                } else {
                    line.push_str(&cell.text);
                }
            }
            lines.push(rich_rs::set_cell_size(&line, self.width));
        }
        lines
    }

    pub fn debug_dump(&self) -> String {
        let mut out = String::new();
        out.push_str("lines:\n");
        for (y, line) in self.as_plain_lines().iter().enumerate() {
            out.push_str(&format!("{y}: \"{line}\"\n"));
        }
        out.push_str("meta:\n");
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = self.get(x, y);
                if let Some(meta) = &cell.meta {
                    // Keep snapshots stable: widget ownership metadata is useful at runtime for
                    // hover hit-testing, but very noisy in debug dumps.
                    if let Some(map) = meta.meta.as_ref() {
                        if map.len() == 1 && map.contains_key("textual:widget_id") {
                            continue;
                        }
                    }
                    out.push_str(&format!("({x},{y}): {:?}\n", meta));
                }
            }
        }
        out
    }

    pub fn to_segments(&self) -> Segments {
        let mut out = Segments::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = self.get(x, y);
                if cell.continuation {
                    continue;
                }
                let text = if cell.text.is_empty() {
                    " ".to_string()
                } else {
                    cell.text.clone()
                };
                let mut seg = Segment::new(text);
                seg.style = cell.style;
                seg.meta = cell.meta.clone();
                out.push(seg);
            }
            if y + 1 < self.height {
                out.push(Segment::line());
            }
        }
        out
    }

    /// Render a renderable to a FrameBuffer.
    pub fn from_renderable(
        console: &Console,
        options: &ConsoleOptions,
        renderable: &dyn Renderable,
        style: Option<Style>,
    ) -> Self {
        let (width, height) = options.size;
        let lines = console.render_lines(renderable, Some(options), style, true, false);
        let lines = Segment::set_shape(&lines, width, Some(height), style, false);
        Self::from_lines(&lines, width, height, style)
    }

    /// Build a FrameBuffer from pre-rendered lines.
    pub fn from_lines(
        lines: &[Vec<Segment>],
        width: usize,
        height: usize,
        default_style: Option<Style>,
    ) -> Self {
        let mut buffer = FrameBuffer::new(width, height, default_style);

        for (y, line) in lines.iter().take(height).enumerate() {
            buffer.write_line(y, line);
        }

        buffer
    }

    fn clear_line(&mut self, y: usize) {
        for x in 0..self.width {
            *self.get_mut(x, y) = Cell::blank(self.default_style);
        }
    }

    fn write_line(&mut self, y: usize, line: &[Segment]) {
        if y >= self.height {
            return;
        }
        self.clear_line(y);

        let mut x: usize = 0;
        let mut last_non_zero: Option<(usize, usize)> = None; // (x, width)

        for seg in line {
            if seg.control.is_some() {
                continue;
            }
            let style = seg.style;
            let meta = seg.meta.clone();
            for ch in seg.text.chars() {
                let w = char_width(ch);

                if w == 0 {
                    if let Some((prev_x, prev_w)) = last_non_zero {
                        let existing_style = self.get(prev_x, y).style;
                        let existing_meta = self.get(prev_x, y).meta.clone();
                        let cell = self.get_mut(prev_x, y);
                        cell.text.push(ch);
                        cell.style = style.or(existing_style);
                        cell.meta = meta.clone().or(existing_meta);
                        last_non_zero = Some((prev_x, prev_w));
                    }
                    continue;
                }

                if x >= self.width {
                    return;
                }

                if w == 2 && x + 1 >= self.width {
                    let existing_style = self.get(x, y).style;
                    *self.get_mut(x, y) = Cell::blank(style.or(existing_style));
                    x += 1;
                    last_non_zero = Some((x.saturating_sub(1), 1));
                    continue;
                }

                let existing_style = self.get(x, y).style;
                let existing_meta = self.get(x, y).meta.clone();
                *self.get_mut(x, y) = Cell {
                    text: ch.to_string(),
                    style: style.or(existing_style),
                    meta: meta.clone().or(existing_meta),
                    continuation: false,
                };
                last_non_zero = Some((x, w));

                if w == 2 {
                    let existing_style = self.get(x + 1, y).style;
                    let existing_meta = self.get(x + 1, y).meta.clone();
                    *self.get_mut(x + 1, y) = Cell::continuation(
                        style.or(existing_style),
                        meta.clone().or(existing_meta),
                    );
                    x += 2;
                } else {
                    x += 1;
                }
            }
        }
    }

    fn cell_span_width(&self, x: usize, y: usize) -> usize {
        let cell = self.get(x, y);
        if cell.continuation {
            0
        } else {
            let w = cell.width();
            if w == 0 { 1 } else { w }
        }
    }

    /// Compute an update sequence that transforms `previous` into `self`.
    ///
    /// The returned segments:
    /// - Start with `Home` (cursor to 0,0)
    /// - Use cursor controls (no `\n`) for positioning
    /// - Emit styled text + metadata for changed spans
    pub fn diff_to_segments(&self, previous: &FrameBuffer) -> Segments {
        assert_eq!(self.width, previous.width, "buffer widths differ");
        assert_eq!(self.height, previous.height, "buffer heights differ");

        let mut out = Segments::new();
        out.push(Segment::control(rich_rs::ControlType::Home));

        for y in 0..self.height {
            let mut x: usize = 0;

            while x < self.width {
                let curr = self.get(x, y);
                let prev = previous.get(x, y);

                if curr.continuation || prev.continuation {
                    x += 1;
                    continue;
                }

                if curr == prev {
                    x += 1;
                    continue;
                }

                let mut span = self
                    .cell_span_width(x, y)
                    .max(previous.cell_span_width(x, y))
                    .max(1);
                span = span.min(self.width.saturating_sub(x));

                let mut end_x = x + span;
                while end_x < self.width {
                    let c = self.get(end_x, y);
                    let p = previous.get(end_x, y);
                    if c.continuation || p.continuation {
                        end_x += 1;
                        continue;
                    }
                    if c == p {
                        break;
                    }
                    let extra = self
                        .cell_span_width(end_x, y)
                        .max(previous.cell_span_width(end_x, y))
                        .max(1);
                    end_x = cmp::min(end_x + extra, self.width);
                }

                out.push(Segment::control(rich_rs::ControlType::MoveTo {
                    x: x as u16,
                    y: y as u16,
                }));

                let mut run_x = x;
                while run_x < end_x {
                    let cell = self.get(run_x, y);
                    if cell.continuation {
                        run_x += 1;
                        continue;
                    }
                    let w = self.cell_span_width(run_x, y).max(1);
                    let text = if cell.text.is_empty() {
                        " ".to_string()
                    } else {
                        cell.text.clone()
                    };
                    let mut seg = Segment::new(text);
                    seg.style = cell.style;
                    seg.meta = cell.meta.clone();
                    out.push(seg);
                    run_x += w;
                }

                x = end_x;
            }
        }

        out
    }

    /// Compute an update sequence limited to the given dirty regions.
    ///
    /// Cells outside `dirty_regions` are treated as unchanged.
    pub fn diff_to_segments_in_regions(
        &self,
        previous: &FrameBuffer,
        dirty_regions: &[DirtyRegion],
    ) -> Segments {
        assert_eq!(self.width, previous.width, "buffer widths differ");
        assert_eq!(self.height, previous.height, "buffer heights differ");
        if dirty_regions.is_empty() {
            return Segments::new();
        }

        let mut dirty_mask = vec![false; self.width * self.height];
        for region in dirty_regions {
            if self.width == 0 || self.height == 0 {
                continue;
            }
            let x0 = region.x0.min(self.width.saturating_sub(1));
            let y0 = region.y0.min(self.height.saturating_sub(1));
            let x1 = region.x1.min(self.width.saturating_sub(1));
            let y1 = region.y1.min(self.height.saturating_sub(1));
            if x0 > x1 || y0 > y1 {
                continue;
            }
            for y in y0..=y1 {
                for x in x0..=x1 {
                    dirty_mask[self.idx(x, y)] = true;
                }
            }
        }

        let mut masked_previous = previous.clone();
        for (idx, dirty) in dirty_mask.iter().enumerate() {
            if !*dirty {
                masked_previous.cells[idx] = self.cells[idx].clone();
            }
        }
        self.diff_to_segments(&masked_previous)
    }
}

fn cell_len(text: &str) -> usize {
    rich_rs::cell_len(text)
}

fn char_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_uses_absolute_move_to_for_changed_spans() {
        let previous = FrameBuffer::from_lines(&[vec![Segment::new("aaaa")]], 4, 1, None);
        let next = FrameBuffer::from_lines(&[vec![Segment::new("abba")]], 4, 1, None);

        let diff = next.diff_to_segments(&previous);
        let mut saw_move_to = false;

        for segment in diff.iter() {
            if let Some(control) = segment.control.as_ref() {
                match control {
                    rich_rs::ControlType::MoveTo { .. } => saw_move_to = true,
                    rich_rs::ControlType::CarriageReturn
                    | rich_rs::ControlType::CursorDown(_)
                    | rich_rs::ControlType::CursorUp(_)
                    | rich_rs::ControlType::CursorForward(_)
                    | rich_rs::ControlType::CursorBackward(_) => {
                        panic!("diff emitted relative cursor control: {control:?}");
                    }
                    _ => {}
                }
            }
        }

        assert!(saw_move_to, "expected at least one MoveTo in diff stream");
    }

    #[test]
    fn region_diff_ignores_changes_outside_dirty_region() {
        let previous = FrameBuffer::from_lines(
            &[vec![Segment::new("abcd")], vec![Segment::new("wxyz")]],
            4,
            2,
            None,
        );
        let next = FrameBuffer::from_lines(
            &[vec![Segment::new("abXd")], vec![Segment::new("wXyz")]],
            4,
            2,
            None,
        );

        let diff = next.diff_to_segments_in_regions(
            &previous,
            &[DirtyRegion {
                x0: 2,
                y0: 0,
                x1: 2,
                y1: 0,
            }],
        );

        let mut move_tos = Vec::new();
        let mut text = String::new();
        for segment in diff.iter() {
            if let Some(rich_rs::ControlType::MoveTo { x, y }) = segment.control.as_ref() {
                move_tos.push((*x, *y));
            }
            if segment.control.is_none() {
                text.push_str(segment.text.as_ref());
            }
        }

        assert_eq!(move_tos, vec![(2, 0)]);
        assert_eq!(text, "X");
    }
}
