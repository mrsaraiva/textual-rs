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
                        let cell = self.get_mut(prev_x, y);
                        cell.text.push(ch);
                        cell.style = style;
                        cell.meta = meta.clone();
                        last_non_zero = Some((prev_x, prev_w));
                    }
                    continue;
                }

                if x >= self.width {
                    return;
                }

                if w == 2 && x + 1 >= self.width {
                    *self.get_mut(x, y) = Cell::blank(style);
                    x += 1;
                    last_non_zero = Some((x.saturating_sub(1), 1));
                    continue;
                }

                *self.get_mut(x, y) = Cell {
                    text: ch.to_string(),
                    style,
                    meta: meta.clone(),
                    continuation: false,
                };
                last_non_zero = Some((x, w));

                if w == 2 {
                    *self.get_mut(x + 1, y) = Cell::continuation(style, meta.clone());
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

        let mut cursor_x: usize = 0;
        let mut cursor_y: usize = 0;

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

                if y != cursor_y {
                    if y > cursor_y {
                        out.push(Segment::control(rich_rs::ControlType::CursorDown(
                            (y - cursor_y) as u16,
                        )));
                    } else {
                        out.push(Segment::control(rich_rs::ControlType::CursorUp(
                            (cursor_y - y) as u16,
                        )));
                    }
                    cursor_y = y;
                    cursor_x = 0;
                    out.push(Segment::control(rich_rs::ControlType::CarriageReturn));
                }

                if x != cursor_x {
                    out.push(Segment::control(rich_rs::ControlType::CarriageReturn));
                    cursor_x = 0;
                    if x > 0 {
                        out.push(Segment::control(rich_rs::ControlType::CursorForward(
                            x as u16,
                        )));
                        cursor_x = x;
                    }
                }

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
                    cursor_x += w;
                    run_x += w;
                }

                x = end_x;
            }
        }

        out
    }
}

fn cell_len(text: &str) -> usize {
    rich_rs::cell_len(text)
}

fn char_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}
