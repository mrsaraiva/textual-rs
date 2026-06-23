//! Textual `Content` subsystem — Phase A + B (data type, markup parser,
//! wrap/format, truncate, pad/align).
//!
//! `Content` is the styled-text model that replaces rich-rs `Text` for all
//! Textual-level rendering.  It mirrors `textual/content.py`'s `Content` +
//! `Span` types in ownership-safe, Rust-idiomatic form.
//!
//! ## Phase A scope
//! - [`Span`] and [`Content`] structs (owned, `Clone`).
//! - Constructors: [`Content::from_text`], [`Content::from_markup`],
//!   [`Content::styled`], [`Content::blank`], [`Content::empty`],
//!   [`Content::assemble`].
//! - Markup parser (Textual `[bold]`/`[link=url]`/`[@click=...]` syntax).
//!   `[link=url]` carries link meta only — **no** visual cyan/underline.
//! - Accessors: [`Content::plain`], [`Content::cell_length`],
//!   [`Content::spans`], [`Content::get_style_at_offset`].
//!
//! ## Phase B scope
//! - [`Content::truncate`] — cell-width truncation with optional ellipsis.
//! - [`Content::pad_left`], [`Content::pad_right`], [`Content::pad`] — padding.
//! - [`Content::center`], [`Content::right`] — alignment helpers.
//! - [`Content::divide`], [`Content::split`] — splitting on offsets / separator.
//! - [`Content::rstrip`], [`Content::rstrip_end`], [`Content::right_crop`] —
//!   trailing-whitespace removal.
//! - [`Content::wrap_and_format`] — word-wrapped lines (reuses
//!   `rich_rs::divide_line` for break positions; does rstrip/truncate/pad here
//!   per Textual semantics, never in rich-rs).
//!
//! ## NOT yet in Phase B
//! - Render (`render_strips`, `render_segments`).
//! - Integration with the render path (`core.rs`, `segments.rs`, `text.rs`).
//!
//! See `docs/devel/CONTENT_LAYER_KEYSTONE.md` for the full phasing plan.

pub mod markup;

use crate::style::Style;
use markup::parse_markup;

// ---------------------------------------------------------------------------
// Span
// ---------------------------------------------------------------------------

/// A styled range of character (byte) offsets within a [`Content`]'s text.
///
/// Mirrors Python `textual.content.Span(start, end, style)`.
///
/// # Byte vs char offsets
/// Offsets are **byte** offsets into the UTF-8 string, consistent with how
/// Rust strings are indexed.  Callers that construct spans from character
/// counts must convert via `str::char_indices` or similar.  The markup
/// parser in [`markup`] produces byte offsets because it processes `&str`
/// directly.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    /// Byte offset of the first character in this span (inclusive).
    pub start: usize,
    /// Byte offset past the last character in this span (exclusive).
    pub end: usize,
    /// Visual style applied over `text[start..end]`.
    pub style: Style,
}

impl Span {
    /// Create a new `Span`.
    pub fn new(start: usize, end: usize, style: Style) -> Self {
        Self { start, end, style }
    }

    /// Return true if this span covers a non-empty range.
    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    /// Shift the span's start and end by `distance` bytes (can be negative via
    /// saturating arithmetic — start clamps to 0).
    pub fn shift(&self, distance: isize) -> Self {
        let start = (self.start as isize + distance).max(0) as usize;
        let end = (self.end as isize + distance).max(0) as usize;
        Span::new(start, end, self.style.clone())
    }

    /// Extend the span's end by `cells` bytes.
    pub fn extend(&self, cells: usize) -> Self {
        Span::new(self.start, self.end + cells, self.style.clone())
    }
}

// ---------------------------------------------------------------------------
// Content
// ---------------------------------------------------------------------------

/// Immutable styled-text container.
///
/// Mirrors Python `textual.content.Content`:
/// - `text` — the plain string.
/// - `spans` — list of [`Span`] values marking styled regions.
///
/// `Content` is `Clone` but intended to be treated as logically immutable:
/// most methods return new `Content` instances rather than mutating in-place.
#[derive(Debug, Clone, PartialEq)]
pub struct Content {
    text: String,
    spans: Vec<Span>,
    /// Cached cell length (computed lazily from `text`).
    cell_length_cache: Option<usize>,
}

impl Content {
    // -----------------------------------------------------------------------
    // Internal constructor
    // -----------------------------------------------------------------------

    fn new_raw(text: String, spans: Vec<Span>) -> Self {
        Self {
            text,
            spans,
            cell_length_cache: None,
        }
    }

    fn new_with_cell_len(text: String, spans: Vec<Span>, cell_length: Option<usize>) -> Self {
        Self {
            text,
            spans,
            cell_length_cache: cell_length,
        }
    }

    // -----------------------------------------------------------------------
    // Constructors
    // -----------------------------------------------------------------------

    /// Return the shared empty `Content` instance.
    ///
    /// Mirrors Python `Content.empty()`.
    pub fn empty() -> Self {
        Self::new_raw(String::new(), Vec::new())
    }

    /// Create `Content` from a plain string (no markup parsing).
    ///
    /// Mirrors Python `Content(text)` (the plain constructor path).
    pub fn from_text(text: impl Into<String>) -> Self {
        let text = strip_control_codes(text.into());
        if text.is_empty() {
            return Self::empty();
        }
        Self::new_raw(text, Vec::new())
    }

    /// Create `Content` by parsing Textual markup.
    ///
    /// Mirrors Python `Content.from_markup(markup)`.
    ///
    /// If the string contains no `[` characters it is treated as plain text
    /// (fast path, no allocation).
    ///
    /// `[link=url]` produces a span with link metadata but **no** visual
    /// fg/underline — visual link styling is applied at Theme level, not here.
    pub fn from_markup(markup: impl AsRef<str>) -> Self {
        let markup = markup.as_ref();
        let markup = strip_control_codes(markup.to_string());

        if markup.is_empty() {
            return Self::empty();
        }

        if !markup.contains('[') {
            return Self::from_text(markup);
        }

        let (text, raw_spans) = parse_markup(&markup);

        let spans: Vec<Span> = raw_spans
            .into_iter()
            .map(|rs| Span::new(rs.start, rs.end, rs.style))
            .collect();

        Self::new_raw(text, spans)
    }

    /// Create `Content` from plain text with a single style covering the whole
    /// string.
    ///
    /// Mirrors Python `Content.styled(text, style)`.
    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        let text = strip_control_codes(text.into());
        if text.is_empty() {
            return Self::empty();
        }
        let len = text.len();
        let spans = vec![Span::new(0, len, style)];
        Self::new_raw(text, spans)
    }

    /// Create `Content` from plain text with a single style, providing a
    /// pre-computed cell length to avoid recomputation.
    pub fn styled_with_cell_len(
        text: impl Into<String>,
        style: Style,
        cell_length: usize,
    ) -> Self {
        let text = strip_control_codes(text.into());
        if text.is_empty() {
            return Self::empty();
        }
        let len = text.len();
        let spans = vec![Span::new(0, len, style)];
        Self::new_with_cell_len(text, spans, Some(cell_length))
    }

    /// Create `Content` consisting of `width` space characters, optionally
    /// styled.
    ///
    /// Mirrors Python `Content.blank(width, style)`.
    pub fn blank(width: usize, style: Option<Style>) -> Self {
        if width == 0 {
            return Self::empty();
        }
        let text = " ".repeat(width);
        match style {
            Some(s) => {
                let spans = vec![Span::new(0, width, s)];
                Self::new_with_cell_len(text, spans, Some(width))
            }
            None => Self::new_with_cell_len(text, Vec::new(), Some(width)),
        }
    }

    /// Assemble `Content` from multiple parts.
    ///
    /// Each part may be:
    /// - A plain `&str` → appended as unstyled text.
    /// - A `(text, style)` tuple → appended with the given style.
    /// - Another `Content` → spans are offset-adjusted and merged.
    ///
    /// Mirrors Python `Content.assemble(*parts, end="")`.
    pub fn assemble(parts: impl IntoIterator<Item = ContentPart>) -> Self {
        let mut text = String::new();
        let mut spans: Vec<Span> = Vec::new();
        let mut position = 0usize;

        for part in parts {
            match part {
                ContentPart::Text(s) => {
                    let s = strip_control_codes(s);
                    position += s.len();
                    text.push_str(&s);
                }
                ContentPart::Styled(s, style) => {
                    let s = strip_control_codes(s);
                    let end = position + s.len();
                    if !s.is_empty() {
                        spans.push(Span::new(position, end, style));
                    }
                    position = end;
                    text.push_str(&s);
                }
                ContentPart::Content(c) => {
                    let offset = position;
                    for span in &c.spans {
                        spans.push(Span::new(
                            span.start + offset,
                            span.end + offset,
                            span.style.clone(),
                        ));
                    }
                    position += c.text.len();
                    text.push_str(&c.text);
                }
            }
        }

        if text.is_empty() {
            return Self::empty();
        }

        Self::new_raw(text, spans)
    }

    // -----------------------------------------------------------------------
    // Accessors
    // -----------------------------------------------------------------------

    /// Return the plain (unstyled) text.
    ///
    /// Mirrors Python `Content.plain`.
    pub fn plain(&self) -> &str {
        &self.text
    }

    /// Return the cell display width of the plain text.
    ///
    /// Uses `rich_rs::cell_len` which accounts for double-wide Unicode
    /// characters (CJK, emoji).
    ///
    /// Mirrors Python `Content.cell_length`.
    pub fn cell_length(&mut self) -> usize {
        if let Some(cached) = self.cell_length_cache {
            return cached;
        }
        let len = rich_rs::cell_len(&self.text);
        self.cell_length_cache = Some(len);
        len
    }

    /// Return the cell display width of the plain text (read-only version,
    /// not cached).
    pub fn cell_length_ref(&self) -> usize {
        if let Some(cached) = self.cell_length_cache {
            return cached;
        }
        rich_rs::cell_len(&self.text)
    }

    /// Return a reference to the list of styled spans.
    ///
    /// Mirrors Python `Content.spans`.
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Return true if there is no text content.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Return the byte length of the underlying text (NOT the cell width).
    ///
    /// Use [`Content::cell_length`] for display width.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Get the cumulative `Style` at a specific byte offset in the text.
    ///
    /// Returns the merged style from all spans that cover the given offset.
    /// Spans are applied in order so later spans overlay earlier ones.
    ///
    /// Mirrors Python `Content.get_style_at_offset(offset)`.
    pub fn get_style_at_offset(&self, offset: usize) -> Style {
        let mut result = Style::new();
        for span in &self.spans {
            if span.start <= offset && offset < span.end {
                result = result.combine(&span.style);
            }
        }
        result
    }

    // -----------------------------------------------------------------------
    // Basic manipulation (needed for unit tests and Phase B prep)
    // -----------------------------------------------------------------------

    /// Append another `Content` to this one, returning a new `Content`.
    pub fn append(&self, other: &Content) -> Content {
        let offset = self.text.len();
        let mut spans = self.spans.clone();
        for span in &other.spans {
            spans.push(Span::new(span.start + offset, span.end + offset, span.style.clone()));
        }
        let mut text = self.text.clone();
        text.push_str(&other.text);
        Content::new_raw(text, spans)
    }

    /// Apply an additional style over a range `[start, end)` (byte offsets),
    /// returning a new `Content` with the extra span inserted.
    ///
    /// Mirrors Python `Content.stylize(style, start, end)`.
    pub fn stylize(&self, style: Style, start: usize, end: usize) -> Content {
        let end = end.min(self.text.len());
        if start >= end {
            return self.clone();
        }
        let mut spans = self.spans.clone();
        spans.push(Span::new(start, end, style));
        spans.sort_by_key(|s| s.start);
        Content::new_raw(self.text.clone(), spans)
    }

    // -----------------------------------------------------------------------
    // Phase B — span trimming helper
    // -----------------------------------------------------------------------

    /// Remove or trim spans that extend past `text`'s byte length.
    ///
    /// Mirrors Python `Content._trim_spans(text, spans)`.
    fn trim_spans(text: &str, spans: &[Span]) -> Vec<Span> {
        let max_offset = text.len();
        spans
            .iter()
            .filter(|s| s.start < max_offset)
            .map(|s| {
                if s.end <= max_offset {
                    s.clone()
                } else {
                    Span::new(s.start, max_offset, s.style.clone())
                }
            })
            .collect()
    }

    // -----------------------------------------------------------------------
    // Phase B — truncate
    // -----------------------------------------------------------------------

    /// Truncate the content to `max_width` display cells.
    ///
    /// When `ellipsis` is true and the content is longer than `max_width`, the
    /// last cell is replaced by `…` (U+2026).  When the content is already at
    /// most `max_width` cells wide, `self` is returned unchanged.
    ///
    /// Mirrors Python `Content.truncate(max_width, ellipsis=False)`.
    pub fn truncate(&self, max_width: usize, ellipsis: bool) -> Content {
        let length = self.cell_length_ref();
        if length <= max_width {
            return self.clone();
        }
        // Content is longer than max_width — need to cut.
        let new_text = if ellipsis && max_width > 0 {
            let cut = rich_rs::set_cell_size(&self.text, max_width.saturating_sub(1));
            format!("{cut}…")
        } else {
            rich_rs::set_cell_size(&self.text, max_width)
        };
        let trimmed_spans = Self::trim_spans(&new_text, &self.spans);
        Content::new_with_cell_len(new_text, trimmed_spans, Some(max_width))
    }

    // -----------------------------------------------------------------------
    // Phase B — right_crop
    // -----------------------------------------------------------------------

    /// Remove `amount` bytes from the end of the text.
    ///
    /// Mirrors Python `Content.right_crop(amount)`.
    pub fn right_crop(&self, amount: usize) -> Content {
        if amount == 0 {
            return self.clone();
        }
        let byte_len = self.text.len();
        if amount >= byte_len {
            return Content::empty();
        }
        let max_offset = byte_len - amount;
        let text = &self.text[..max_offset];
        let spans: Vec<Span> = self
            .spans
            .iter()
            .filter(|s| s.start < max_offset)
            .map(|s| {
                if s.end <= max_offset {
                    s.clone()
                } else {
                    Span::new(s.start, max_offset, s.style.clone())
                }
            })
            .collect();
        Content::new_raw(text.to_string(), spans)
    }

    // -----------------------------------------------------------------------
    // Phase B — rstrip / rstrip_end
    // -----------------------------------------------------------------------

    /// Strip trailing whitespace from the plain text, adjusting spans.
    ///
    /// Mirrors Python `Content.rstrip()`.
    pub fn rstrip(&self) -> Content {
        let stripped = self.text.trim_end();
        if stripped.len() == self.text.len() {
            return self.clone();
        }
        let spans = Self::trim_spans(stripped, &self.spans);
        Content::new_raw(stripped.to_string(), spans)
    }

    /// Remove trailing whitespace that extends beyond `size` bytes.
    ///
    /// If the text is longer than `size` bytes, up to that many trailing
    /// whitespace bytes are removed.  Mirrors Python `Content.rstrip_end(size)`.
    pub fn rstrip_end(&self, size: usize) -> Content {
        let text_length = self.text.len();
        if text_length > size {
            let excess = text_length - size;
            // Count trailing whitespace bytes
            let trailing_ws = self
                .text
                .chars()
                .rev()
                .take_while(|c| c.is_whitespace())
                .map(|c| c.len_utf8())
                .sum::<usize>();
            if trailing_ws > 0 {
                let crop = trailing_ws.min(excess);
                return self.right_crop(crop);
            }
        }
        self.clone()
    }

    // -----------------------------------------------------------------------
    // Phase B — pad_left / pad_right / pad / center / right
    // -----------------------------------------------------------------------

    /// Pad the left side with `count` spaces (no style on padding).
    ///
    /// Mirrors Python `Content.pad_left(count)`.
    pub fn pad_left(&self, count: usize) -> Content {
        if count == 0 {
            return self.clone();
        }
        let padding = " ".repeat(count);
        let text = format!("{}{}", padding, self.text);
        let spans: Vec<Span> = self
            .spans
            .iter()
            .map(|s| Span::new(s.start + count, s.end + count, s.style.clone()))
            .collect();
        let cell_len = self.cell_length_cache.map(|l| l + count);
        Content::new_with_cell_len(text, spans, cell_len)
    }

    /// Pad the right side with `count` spaces (no style on padding).
    ///
    /// Mirrors Python `Content.pad_right(count)`.
    pub fn pad_right(&self, count: usize) -> Content {
        if count == 0 {
            return self.clone();
        }
        let padding = " ".repeat(count);
        let text = format!("{}{}", self.text, padding);
        let cell_len = self.cell_length_cache.map(|l| l + count);
        // Right-padding keeps existing spans unchanged (they don't extend into the pad).
        Content::new_with_cell_len(text, self.spans.clone(), cell_len)
    }

    /// Pad both the left (`left` spaces) and right (`right` spaces).
    ///
    /// Mirrors Python `Content.pad(left, right)`.
    pub fn pad(&self, left: usize, right: usize) -> Content {
        match (left, right) {
            (0, 0) => self.clone(),
            (0, _) => self.pad_right(right),
            (_, 0) => self.pad_left(left),
            _ => self.pad_left(left).pad_right(right),
        }
    }

    /// Center the content within `width` display cells.
    ///
    /// rstrips trailing whitespace then truncates to `width` before centering.
    /// Mirrors Python `Content.center(width, ellipsis=False)`.
    pub fn center(&self, width: usize, ellipsis: bool) -> Content {
        let content = self.rstrip().truncate(width, ellipsis);
        let len = content.cell_length_ref();
        let left = (width.saturating_sub(len)) / 2;
        let right = width.saturating_sub(left).saturating_sub(len);
        content.pad(left, right)
    }

    /// Right-align the content within `width` display cells.
    ///
    /// rstrips trailing whitespace then truncates to `width` before padding.
    /// Mirrors Python `Content.right(width, ellipsis=False)`.
    pub fn right_align(&self, width: usize, ellipsis: bool) -> Content {
        let content = self.rstrip().truncate(width, ellipsis);
        let len = content.cell_length_ref();
        let pad = width.saturating_sub(len);
        content.pad_left(pad)
    }

    // -----------------------------------------------------------------------
    // Phase B — divide / split
    // -----------------------------------------------------------------------

    /// Divide content at a sequence of **byte** offsets, returning `offsets.len()+1` pieces.
    ///
    /// The offsets are into the plain text.  Spans are clipped to each piece's range.
    ///
    /// Mirrors Python `Content.divide(offsets)`.
    pub fn divide(&self, offsets: &[usize]) -> Vec<Content> {
        if offsets.is_empty() {
            return vec![self.clone()];
        }

        let text = &self.text;
        let text_len = text.len();

        // Build sorted cut points with sentinels.  Do NOT dedup — a trailing
        // offset equal to text_len must still produce a final empty piece, which
        // is how Python `divide` behaves (divide_offsets = [0, *offsets, len(text)]).
        let mut inner: Vec<usize> = offsets.to_vec();
        inner.sort_unstable();
        // Clamp offsets to [0, text_len].
        let inner: Vec<usize> = inner
            .into_iter()
            .map(|o| o.min(text_len))
            .collect();

        let cut_points: Vec<usize> = std::iter::once(0)
            .chain(inner.iter().copied())
            .chain(std::iter::once(text_len))
            .collect();

        // Build (start, end) pairs — consecutive pairs, even if start == end.
        let ranges: Vec<(usize, usize)> = cut_points.windows(2).map(|w| (w[0], w[1])).collect();

        // Allocate one Content per range.
        let mut pieces: Vec<Content> = ranges
            .iter()
            .map(|&(start, end)| Content::new_raw(text[start..end].to_string(), Vec::new()))
            .collect();

        if self.spans.is_empty() {
            return pieces;
        }

        // Distribute spans across pieces.
        for span in &self.spans {
            if span.start >= text_len || span.is_empty() {
                continue;
            }
            let span_end = span.end.min(text_len);

            for (piece_idx, &(range_start, range_end)) in ranges.iter().enumerate() {
                // Does this span overlap [range_start, range_end)?
                if span.start >= range_end || span_end <= range_start {
                    continue;
                }
                let new_start = span.start.saturating_sub(range_start);
                let new_end = (span_end - range_start).min(range_end - range_start);
                if new_end > new_start {
                    pieces[piece_idx]
                        .spans
                        .push(Span::new(new_start, new_end, span.style.clone()));
                }
            }
        }

        pieces
    }

    /// Split content on a separator string, dropping the separators from output.
    ///
    /// If `allow_blank` is false and the text ends with the separator, the
    /// trailing empty piece is dropped.
    ///
    /// Mirrors Python `Content.split(separator, allow_blank=False)`.
    pub fn split_on(&self, separator: &str, allow_blank: bool) -> Vec<Content> {
        assert!(!separator.is_empty(), "separator must not be empty");
        if !self.text.contains(separator) {
            return vec![self.clone()];
        }

        let sep_len = separator.len();
        // Collect (match_start, match_end) pairs for each occurrence.
        let mut offsets: Vec<usize> = Vec::new();
        let mut search_from = 0;
        while let Some(pos) = self.text[search_from..].find(separator) {
            let abs_pos = search_from + pos;
            offsets.push(abs_pos);           // start of separator
            offsets.push(abs_pos + sep_len); // end of separator
            search_from = abs_pos + sep_len;
        }

        // divide at both edges of each separator
        let all_pieces = self.divide(&offsets);

        // drop the separator pieces (they appear at even indices when there's a
        // leading piece, but simpler: just filter out pieces whose text == separator)
        let mut lines: Vec<Content> = all_pieces
            .into_iter()
            .filter(|c| c.text != separator)
            .collect();

        if !allow_blank && self.text.ends_with(separator) {
            lines.pop();
        }

        lines
    }

    // -----------------------------------------------------------------------
    // Phase B — wrap_and_format
    // -----------------------------------------------------------------------

    /// Wrap and format the content into display lines of at most `width` display
    /// cells each.
    ///
    /// This is the core of Textual's text-layout engine.  It reuses
    /// `rich_rs::divide_line` for word-boundary break positions but does
    /// **its own** rstrip / truncate / pad — keeping Textual semantics out of
    /// rich-rs (which must stay a faithful Rich port).
    ///
    /// # Arguments
    /// - `width` — target width in display cells (must be > 0).
    /// - `overflow` — how to handle text that exceeds `width` on a single line:
    ///   - `"fold"` (default) — hard-wrap at `width`.
    ///   - `"ellipsis"` — truncate and append `…`.
    ///   - anything else — truncate flush.
    /// - `no_wrap` — if true, skip word-wrap; fold or truncate each logical line.
    /// - `line_pad` — number of extra spaces prepended *and* appended to each
    ///   wrapped line (matches Python `Content.pad(line_pad, line_pad)`).
    ///
    /// Returns one `Content` per output line.
    ///
    /// Mirrors Python `Content._wrap_and_format` (content.py ~620) and
    /// `Content.wrap` (content.py ~992).
    pub fn wrap_and_format(
        &self,
        width: usize,
        overflow: &str,
        no_wrap: bool,
        line_pad: usize,
    ) -> Vec<Content> {
        if width == 0 {
            return Vec::new();
        }

        let ellipsis = overflow == "ellipsis";
        let fold = overflow == "fold";

        // Inner width available for text (after removing line_pad from both sides).
        let inner_width = width.saturating_sub(line_pad * 2);

        let mut output: Vec<Content> = Vec::new();

        // Split the content on newlines first (mirrors Python `self.split(allow_blank=True)`).
        let logical_lines = self.split_on("\n", true);

        for logical_line in logical_lines {
            if no_wrap {
                if fold {
                    // Hard-fold at inner_width.
                    let offsets =
                        rich_rs::divide_line(logical_line.plain(), inner_width.max(1), true);
                    for piece in logical_line.divide(&offsets) {
                        output.push(piece.pad(line_pad, line_pad));
                    }
                } else {
                    // Truncate (with optional ellipsis) — single output line.
                    let line = logical_line.truncate(inner_width, ellipsis);
                    output.push(line.pad(line_pad, line_pad));
                }
            } else {
                // Word-wrap using divide_line.
                let offsets =
                    rich_rs::divide_line(logical_line.plain(), inner_width.max(1), fold);
                let pieces = logical_line.divide(&offsets);
                let num_pieces = pieces.len();

                for (i, piece) in pieces.into_iter().enumerate() {
                    let is_last = i == num_pieces - 1;
                    // Non-last wrapped lines: rstrip then truncate.
                    // Last line of a word-wrapped block: truncate only.
                    let line = if is_last {
                        piece.truncate(inner_width, ellipsis)
                    } else {
                        piece.rstrip().truncate(inner_width, ellipsis)
                    };
                    output.push(line.pad(line_pad, line_pad));
                }
            }
        }

        output
    }
}

// ---------------------------------------------------------------------------
// ContentPart — input type for assemble()
// ---------------------------------------------------------------------------

/// A part that can be assembled into a [`Content`] via [`Content::assemble`].
pub enum ContentPart {
    /// Plain text, no style.
    Text(String),
    /// Text with a style.
    Styled(String, Style),
    /// Another `Content` instance (spans are offset-adjusted).
    Content(Content),
}

impl From<String> for ContentPart {
    fn from(s: String) -> Self {
        ContentPart::Text(s)
    }
}

impl From<&str> for ContentPart {
    fn from(s: &str) -> Self {
        ContentPart::Text(s.to_string())
    }
}

impl From<(String, Style)> for ContentPart {
    fn from((s, style): (String, Style)) -> Self {
        ContentPart::Styled(s, style)
    }
}

impl From<(&str, Style)> for ContentPart {
    fn from((s, style): (&str, Style)) -> Self {
        ContentPart::Styled(s.to_string(), style)
    }
}

impl From<Content> for ContentPart {
    fn from(c: Content) -> Self {
        ContentPart::Content(c)
    }
}

// ---------------------------------------------------------------------------
// Control-code stripping (mirrors Python _strip_control_codes)
// ---------------------------------------------------------------------------

/// Control codes that may break terminal output. Matches Python's
/// `_STRIP_CONTROL_CODES`: Bell (7), Backspace (8), VTab (11), FF (12), CR (13).
fn strip_control_codes(mut s: String) -> String {
    const STRIP: &[char] = &['\x07', '\x08', '\x0B', '\x0C', '\r'];
    if s.chars().any(|c| STRIP.contains(&c)) {
        s.retain(|c| !STRIP.contains(&c));
    }
    s
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

impl std::fmt::Display for Content {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Style;

    // --- from_text ---

    #[test]
    fn test_from_text_plain() {
        let c = Content::from_text("hello");
        assert_eq!(c.plain(), "hello");
        assert!(c.spans().is_empty());
    }

    #[test]
    fn test_from_text_empty() {
        let c = Content::from_text("");
        assert!(c.is_empty());
    }

    #[test]
    fn test_from_text_strips_control_codes() {
        // CR should be stripped
        let c = Content::from_text("hello\rworld");
        assert_eq!(c.plain(), "helloworld");
    }

    // --- empty ---

    #[test]
    fn test_empty() {
        let c = Content::empty();
        assert!(c.is_empty());
        assert_eq!(c.plain(), "");
        assert!(c.spans().is_empty());
    }

    // --- styled ---

    #[test]
    fn test_styled_single_span() {
        let style = Style::new().bold(true);
        let c = Content::styled("hello", style.clone());
        assert_eq!(c.plain(), "hello");
        assert_eq!(c.spans().len(), 1);
        assert_eq!(c.spans()[0].start, 0);
        assert_eq!(c.spans()[0].end, 5);
        assert_eq!(c.spans()[0].style.bold, Some(true));
    }

    #[test]
    fn test_styled_empty_returns_empty() {
        let c = Content::styled("", Style::new().bold(true));
        assert!(c.is_empty());
    }

    // --- blank ---

    #[test]
    fn test_blank_no_style() {
        let mut c = Content::blank(5, None);
        assert_eq!(c.plain(), "     ");
        assert!(c.spans().is_empty());
        assert_eq!(c.cell_length(), 5);
    }

    #[test]
    fn test_blank_with_style() {
        let style = Style::new().bold(true);
        let c = Content::blank(3, Some(style));
        assert_eq!(c.plain(), "   ");
        assert_eq!(c.spans().len(), 1);
        assert_eq!(c.spans()[0].end, 3);
    }

    #[test]
    fn test_blank_zero() {
        let c = Content::blank(0, None);
        assert!(c.is_empty());
    }

    // --- from_markup ---

    #[test]
    fn test_from_markup_plain_string() {
        let c = Content::from_markup("hello world");
        assert_eq!(c.plain(), "hello world");
        assert!(c.spans().is_empty());
    }

    #[test]
    fn test_from_markup_bold() {
        let c = Content::from_markup("[bold]hello[/bold]");
        assert_eq!(c.plain(), "hello");
        assert_eq!(c.spans().len(), 1);
        assert_eq!(c.spans()[0].style.bold, Some(true));
    }

    #[test]
    fn test_from_markup_link_no_visual_style() {
        // The critical invariant: [link=url] must NOT apply visual style
        let c = Content::from_markup("[link=https://example.com]click[/link]");
        assert_eq!(c.plain(), "click");
        assert_eq!(c.spans().len(), 1);
        let style = &c.spans()[0].style;
        assert!(style.fg.is_none(), "link must not set fg");
        assert!(style.underline.is_none(), "link must not set underline");
        assert!(style.bold.is_none(), "link must not set bold");
    }

    #[test]
    fn test_from_markup_mixed() {
        let c = Content::from_markup("Hello, [bold]world[/bold]!");
        assert_eq!(c.plain(), "Hello, world!");
        assert_eq!(c.spans().len(), 1);
        assert_eq!(c.spans()[0].start, 7);
        assert_eq!(c.spans()[0].end, 12);
    }

    #[test]
    fn test_from_markup_empty() {
        let c = Content::from_markup("");
        assert!(c.is_empty());
    }

    // --- assemble ---

    #[test]
    fn test_assemble_plain() {
        let c = Content::assemble(vec![
            ContentPart::from("hello"),
            ContentPart::from(", "),
            ContentPart::from("world"),
        ]);
        assert_eq!(c.plain(), "hello, world");
        assert!(c.spans().is_empty());
    }

    #[test]
    fn test_assemble_styled_parts() {
        let bold = Style::new().bold(true);
        let c = Content::assemble(vec![
            ContentPart::from("pre "),
            ContentPart::from(("bold", bold.clone())),
            ContentPart::from(" post"),
        ]);
        assert_eq!(c.plain(), "pre bold post");
        assert_eq!(c.spans().len(), 1);
        assert_eq!(c.spans()[0].start, 4);
        assert_eq!(c.spans()[0].end, 8);
    }

    #[test]
    fn test_assemble_content_part() {
        let inner = Content::from_markup("[bold]hi[/bold]");
        let c = Content::assemble(vec![
            ContentPart::from("say: "),
            ContentPart::from(inner),
        ]);
        assert_eq!(c.plain(), "say: hi");
        assert_eq!(c.spans().len(), 1);
        // Span should be offset by len("say: ") = 5
        assert_eq!(c.spans()[0].start, 5);
        assert_eq!(c.spans()[0].end, 7);
    }

    // --- cell_length ---

    #[test]
    fn test_cell_length_ascii() {
        let mut c = Content::from_text("hello");
        assert_eq!(c.cell_length(), 5);
    }

    #[test]
    fn test_cell_length_cached() {
        let mut c = Content::from_text("abc");
        let first = c.cell_length();
        let second = c.cell_length();
        assert_eq!(first, second);
        assert_eq!(first, 3);
    }

    // --- get_style_at_offset ---

    #[test]
    fn test_get_style_at_offset_no_spans() {
        let c = Content::from_text("hello");
        let s = c.get_style_at_offset(2);
        assert_eq!(s, Style::new());
    }

    #[test]
    fn test_get_style_at_offset_within_span() {
        let c = Content::from_markup("[bold]hello[/bold]");
        let s = c.get_style_at_offset(2);
        assert_eq!(s.bold, Some(true));
    }

    #[test]
    fn test_get_style_at_offset_outside_span() {
        let c = Content::from_markup("hi [bold]there[/bold]");
        // offset 0..3 is "hi " — no span
        let s = c.get_style_at_offset(1);
        assert_eq!(s.bold, None);
        // offset 3..8 is "there" — bold span
        let s2 = c.get_style_at_offset(3);
        assert_eq!(s2.bold, Some(true));
    }

    // --- append / stylize ---

    #[test]
    fn test_append() {
        let a = Content::from_markup("[bold]hello[/bold]");
        let b = Content::from_text(" world");
        let c = a.append(&b);
        assert_eq!(c.plain(), "hello world");
        assert_eq!(c.spans().len(), 1);
        assert_eq!(c.spans()[0].start, 0);
        assert_eq!(c.spans()[0].end, 5);
    }

    #[test]
    fn test_stylize() {
        let c = Content::from_text("hello world");
        let bold = Style::new().bold(true);
        let c2 = c.stylize(bold, 6, 11);
        assert_eq!(c2.plain(), "hello world");
        assert_eq!(c2.spans().len(), 1);
        assert_eq!(c2.spans()[0].start, 6);
        assert_eq!(c2.spans()[0].end, 11);
        assert_eq!(c2.spans()[0].style.bold, Some(true));
    }

    // --- display ---

    #[test]
    fn test_display() {
        let c = Content::from_markup("[bold]hi[/bold]");
        assert_eq!(format!("{c}"), "hi");
    }

    // --- control code stripping ---

    #[test]
    fn test_strip_bell() {
        let c = Content::from_text("hel\x07lo");
        assert_eq!(c.plain(), "hello");
    }

    // --- Span helpers ---

    #[test]
    fn test_span_shift() {
        let s = Span::new(5, 10, Style::new());
        let shifted = s.shift(3);
        assert_eq!(shifted.start, 8);
        assert_eq!(shifted.end, 13);
    }

    #[test]
    fn test_span_shift_negative_clamp() {
        let s = Span::new(2, 5, Style::new());
        let shifted = s.shift(-10);
        assert_eq!(shifted.start, 0);
        assert_eq!(shifted.end, 0);
    }

    #[test]
    fn test_span_extend() {
        let s = Span::new(0, 5, Style::new());
        let ext = s.extend(3);
        assert_eq!(ext.end, 8);
    }

    // =========================================================================
    // Phase B tests
    // =========================================================================

    // --- truncate ---

    #[test]
    fn test_truncate_shorter_noop() {
        let c = Content::from_text("hello");
        let t = c.truncate(10, false);
        assert_eq!(t.plain(), "hello");
    }

    #[test]
    fn test_truncate_exact_noop() {
        let c = Content::from_text("hello");
        let t = c.truncate(5, false);
        assert_eq!(t.plain(), "hello");
    }

    #[test]
    fn test_truncate_longer_clips() {
        let c = Content::from_text("hello world");
        let t = c.truncate(5, false);
        assert_eq!(t.plain(), "hello");
        assert_eq!(t.cell_length_ref(), 5);
    }

    #[test]
    fn test_truncate_ellipsis() {
        let c = Content::from_text("hello world");
        let t = c.truncate(6, true);
        // "hello" (5 cells) + "…" (1 cell) = 6 cells
        assert_eq!(t.plain(), "hello…");
        assert_eq!(t.cell_length_ref(), 6);
    }

    #[test]
    fn test_truncate_clips_spans() {
        // Span covers "hello world" (0..11); after truncate(5) span clips to 0..5.
        let bold = Style::new().bold(true);
        let c = Content::styled("hello world", bold);
        let t = c.truncate(5, false);
        assert_eq!(t.plain(), "hello");
        assert_eq!(t.spans().len(), 1);
        assert_eq!(t.spans()[0].end, 5);
    }

    #[test]
    fn test_truncate_zero_width() {
        let c = Content::from_text("hello");
        let t = c.truncate(0, false);
        assert_eq!(t.plain(), "");
    }

    // --- right_crop ---

    #[test]
    fn test_right_crop_basic() {
        let c = Content::from_text("hello");
        let cr = c.right_crop(2);
        assert_eq!(cr.plain(), "hel");
    }

    #[test]
    fn test_right_crop_all() {
        let c = Content::from_text("hi");
        let cr = c.right_crop(2);
        assert!(cr.is_empty());
    }

    #[test]
    fn test_right_crop_zero() {
        let c = Content::from_text("hello");
        let cr = c.right_crop(0);
        assert_eq!(cr.plain(), "hello");
    }

    #[test]
    fn test_right_crop_clips_spans() {
        // Span covers bytes 3..8 ("lo wo"); after crop(3) text="hello wo" (8 bytes)
        // span clips to 3..8 which is still within range
        let bold = Style::new().bold(true);
        let c = Content::styled("hello world", bold.clone()); // span 0..11
        let cr = c.right_crop(6); // text becomes "hello" (5 bytes), span clips to 0..5
        assert_eq!(cr.plain(), "hello");
        assert_eq!(cr.spans().len(), 1);
        assert_eq!(cr.spans()[0].end, 5);
    }

    // --- rstrip ---

    #[test]
    fn test_rstrip_no_trailing() {
        let c = Content::from_text("hello");
        let r = c.rstrip();
        assert_eq!(r.plain(), "hello");
    }

    #[test]
    fn test_rstrip_strips_trailing() {
        let c = Content::from_text("hello   ");
        let r = c.rstrip();
        assert_eq!(r.plain(), "hello");
    }

    #[test]
    fn test_rstrip_adjusts_spans() {
        // span covers "hello   " (0..8); after rstrip text="hello" (5 bytes), span clips.
        let bold = Style::new().bold(true);
        let c = Content::styled("hello   ", bold);
        let r = c.rstrip();
        assert_eq!(r.plain(), "hello");
        assert_eq!(r.spans().len(), 1);
        assert_eq!(r.spans()[0].end, 5);
    }

    #[test]
    fn test_rstrip_empty_string() {
        let c = Content::from_text("   ");
        let r = c.rstrip();
        assert_eq!(r.plain(), "");
    }

    // --- rstrip_end ---

    #[test]
    fn test_rstrip_end_no_excess() {
        let c = Content::from_text("hello");
        let r = c.rstrip_end(10);
        assert_eq!(r.plain(), "hello");
    }

    #[test]
    fn test_rstrip_end_removes_trailing_ws() {
        // "hello   " (8 bytes), size=5 → excess=3 trailing ws=3 → crop 3
        let c = Content::from_text("hello   ");
        let r = c.rstrip_end(5);
        assert_eq!(r.plain(), "hello");
    }

    #[test]
    fn test_rstrip_end_only_trailing_ws() {
        // "hello " (6 bytes), size=5 → excess=1, trailing ws=1 → crop 1
        let c = Content::from_text("hello ");
        let r = c.rstrip_end(5);
        assert_eq!(r.plain(), "hello");
    }

    // --- pad_left / pad_right / pad ---

    #[test]
    fn test_pad_left_basic() {
        let c = Content::from_text("hi");
        let p = c.pad_left(3);
        assert_eq!(p.plain(), "   hi");
        // No spans on left padding (padding is unstyled).
        assert!(p.spans().is_empty());
    }

    #[test]
    fn test_pad_left_shifts_spans() {
        let bold = Style::new().bold(true);
        let c = Content::styled("hi", bold);
        let p = c.pad_left(3);
        assert_eq!(p.plain(), "   hi");
        assert_eq!(p.spans().len(), 1);
        assert_eq!(p.spans()[0].start, 3);
        assert_eq!(p.spans()[0].end, 5);
    }

    #[test]
    fn test_pad_right_basic() {
        let c = Content::from_text("hi");
        let p = c.pad_right(3);
        assert_eq!(p.plain(), "hi   ");
        // Spans unchanged (right pad is unstyled).
        assert!(p.spans().is_empty());
    }

    #[test]
    fn test_pad_right_keeps_spans() {
        let bold = Style::new().bold(true);
        let c = Content::styled("hi", bold);
        let p = c.pad_right(3);
        assert_eq!(p.plain(), "hi   ");
        assert_eq!(p.spans().len(), 1);
        assert_eq!(p.spans()[0].start, 0);
        assert_eq!(p.spans()[0].end, 2);
    }

    #[test]
    fn test_pad_both() {
        let c = Content::from_text("hi");
        let p = c.pad(2, 3);
        assert_eq!(p.plain(), "  hi   ");
        assert!(p.spans().is_empty());
    }

    #[test]
    fn test_pad_zero_noop() {
        let c = Content::from_text("hi");
        let p = c.pad(0, 0);
        assert_eq!(p.plain(), "hi");
    }

    // --- center ---

    #[test]
    fn test_center_even() {
        // "hi" (2 cells) in width 6: left=2 right=2
        let c = Content::from_text("hi");
        let ct = c.center(6, false);
        assert_eq!(ct.plain(), "  hi  ");
        assert_eq!(ct.cell_length_ref(), 6);
    }

    #[test]
    fn test_center_odd_extra_right() {
        // "hi" (2 cells) in width 5: left=1, right=2 (right gets the extra space)
        let c = Content::from_text("hi");
        let ct = c.center(5, false);
        // left = (5-2)/2 = 1, right = 5 - 1 - 2 = 2
        assert_eq!(ct.plain(), " hi  ");
        assert_eq!(ct.cell_length_ref(), 5);
    }

    #[test]
    fn test_center_no_spans_on_pad() {
        // With an unstyled content, padding produces no spans.
        let c = Content::from_text("hello");
        let ct = c.center(10, false);
        assert_eq!(ct.cell_length_ref(), 10);
        assert!(ct.spans().is_empty());
    }

    #[test]
    fn test_center_truncates_if_too_long() {
        let c = Content::from_text("hello world");
        let ct = c.center(5, false);
        // truncated to 5, no room for padding
        assert_eq!(ct.cell_length_ref(), 5);
    }

    // --- right_align ---

    #[test]
    fn test_right_align_basic() {
        let c = Content::from_text("hi");
        let r = c.right_align(6, false);
        assert_eq!(r.plain(), "    hi");
        assert_eq!(r.cell_length_ref(), 6);
    }

    // --- divide ---

    #[test]
    fn test_divide_no_offsets() {
        let c = Content::from_text("hello");
        let pieces = c.divide(&[]);
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].plain(), "hello");
    }

    #[test]
    fn test_divide_single_offset() {
        let c = Content::from_text("hello world");
        let pieces = c.divide(&[5]);
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].plain(), "hello");
        assert_eq!(pieces[1].plain(), " world");
    }

    #[test]
    fn test_divide_multiple_offsets() {
        let c = Content::from_text("abcdef");
        let pieces = c.divide(&[2, 4]);
        assert_eq!(pieces.len(), 3);
        assert_eq!(pieces[0].plain(), "ab");
        assert_eq!(pieces[1].plain(), "cd");
        assert_eq!(pieces[2].plain(), "ef");
    }

    #[test]
    fn test_divide_distributes_spans() {
        // Span covers "world" (bytes 6..11) in "hello world"
        let bold = Style::new().bold(true);
        let c = Content::from_text("hello world").stylize(bold, 6, 11);
        let pieces = c.divide(&[6]);
        // piece 0: "hello " (bytes 0..6) — span 6..11 does not overlap
        assert!(pieces[0].spans().is_empty());
        // piece 1: "world" (bytes 6..11) — span becomes 0..5 in local coords
        assert_eq!(pieces[1].spans().len(), 1);
        assert_eq!(pieces[1].spans()[0].start, 0);
        assert_eq!(pieces[1].spans()[0].end, 5);
    }

    // --- split_on ---

    #[test]
    fn test_split_on_newlines() {
        let c = Content::from_text("line1\nline2\nline3");
        let lines = c.split_on("\n", false);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].plain(), "line1");
        assert_eq!(lines[1].plain(), "line2");
        assert_eq!(lines[2].plain(), "line3");
    }

    #[test]
    fn test_split_on_trailing_newline_no_blank() {
        let c = Content::from_text("line1\n");
        let lines = c.split_on("\n", false);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].plain(), "line1");
    }

    #[test]
    fn test_split_on_trailing_newline_allow_blank() {
        let c = Content::from_text("line1\n");
        let lines = c.split_on("\n", true);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].plain(), "line1");
        assert_eq!(lines[1].plain(), "");
    }

    #[test]
    fn test_split_on_no_separator() {
        let c = Content::from_text("hello");
        let lines = c.split_on("\n", false);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].plain(), "hello");
    }

    // --- wrap_and_format ---

    /// Python baseline: `Content("hello world").wrap(5)` produces `["hello", "world"]`.
    /// Non-last wrapped lines get rstripped.
    #[test]
    fn test_wrap_basic() {
        let c = Content::from_text("hello world");
        let lines = c.wrap_and_format(5, "fold", false, 0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].plain(), "hello");
        assert_eq!(lines[1].plain(), "world");
    }

    /// Trailing whitespace must be stripped from wrapped (non-last) lines — this is
    /// the key Textual semantic that must NOT live in rich-rs.
    #[test]
    fn test_wrap_rstrip_non_last_line() {
        // "hello world" wrapped at 6: divide_line returns a break *after* "hello " so
        // piece[0] = "hello " — rstrip gives "hello".
        let c = Content::from_text("hello world");
        let lines = c.wrap_and_format(6, "fold", false, 0);
        // Last line may vary but first line must be rstripped.
        let first = &lines[0];
        assert!(
            !first.plain().ends_with(' '),
            "non-last wrapped line must be rstripped, got: {:?}",
            first.plain()
        );
    }

    /// With line_pad=1, every output line is padded 1 space on each side.
    #[test]
    fn test_wrap_line_pad() {
        let c = Content::from_text("hello world");
        // width=7, line_pad=1 → inner_width=5
        let lines = c.wrap_and_format(7, "fold", false, 1);
        for line in &lines {
            assert!(
                line.plain().starts_with(' '),
                "line_pad left missing: {:?}",
                line.plain()
            );
            assert!(
                line.plain().ends_with(' '),
                "line_pad right missing: {:?}",
                line.plain()
            );
        }
    }

    /// no_wrap=true + overflow=fold should hard-fold.
    #[test]
    fn test_wrap_no_wrap_fold() {
        let c = Content::from_text("abcdefgh");
        let lines = c.wrap_and_format(4, "fold", true, 0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].plain(), "abcd");
        assert_eq!(lines[1].plain(), "efgh");
    }

    /// no_wrap=true + overflow=ellipsis should truncate with ellipsis.
    #[test]
    fn test_wrap_no_wrap_ellipsis() {
        let c = Content::from_text("hello world");
        let lines = c.wrap_and_format(6, "ellipsis", true, 0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].plain(), "hello…");
    }

    /// Multi-line input: each logical line is wrapped independently.
    #[test]
    fn test_wrap_multiline_input() {
        let c = Content::from_text("foo bar\nbaz");
        let lines = c.wrap_and_format(4, "fold", false, 0);
        // "foo bar" → wraps to ["foo", "bar"]; "baz" stays as ["baz"]
        assert!(lines.len() >= 3);
        assert_eq!(lines[0].plain(), "foo");
        assert_eq!(lines[1].plain(), "bar");
        assert_eq!(lines[2].plain(), "baz");
    }

    /// wrap_and_format with width=0 returns empty.
    #[test]
    fn test_wrap_zero_width() {
        let c = Content::from_text("hello");
        let lines = c.wrap_and_format(0, "fold", false, 0);
        assert!(lines.is_empty());
    }

    /// Single word longer than width with overflow=fold should still appear.
    #[test]
    fn test_wrap_long_word_fold() {
        let c = Content::from_text("abcdefghij");
        let lines = c.wrap_and_format(4, "fold", false, 0);
        // divide_line with fold=true should split the long word.
        assert!(!lines.is_empty());
        let combined: String = lines.iter().map(|l| l.plain()).collect::<Vec<_>>().join("");
        assert_eq!(combined, "abcdefghij");
    }

    /// Left-justify pad consists of spaces only — no fg style.
    #[test]
    fn test_pad_left_is_background_only() {
        let bold = Style::new().bold(true);
        let c = Content::styled("hi", bold);
        let p = c.pad_left(3);
        // The text is "   hi"; spans should only cover "hi" (bytes 3..5).
        // Bytes 0..3 (the padding) must have NO span → unstyled (background only).
        for span in p.spans() {
            assert!(
                span.start >= 3,
                "padding bytes 0..3 must not be covered by any span; got span starting at {}",
                span.start
            );
        }
    }
}
