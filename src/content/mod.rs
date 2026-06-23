//! Textual `Content` subsystem — Phase A + B + C (data type, markup parser,
//! wrap/format, truncate, pad/align, render_strips).
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
//! ## Phase C scope
//! - [`Content::render_strips`] — turn `Content` into fully-styled
//!   `Vec<Vec<rich_rs::Segment>>` for a given width/height/align/overflow +
//!   a visual [`Style`] + a theme-token resolver.  Subsumes the styled-render
//!   semantics that are currently scattered across `core.rs`, `segments.rs`,
//!   and `text.rs`; ADDITIVE — not yet wired into the render path.
//!
//! ## NOT yet in Phase C
//! - Integration with the render path (`core.rs`, `segments.rs`, `text.rs`).
//!
//! See `docs/devel/CONTENT_LAYER_KEYSTONE.md` for the full phasing plan.

pub mod markup;

use crate::style::Style;
use markup::parse_markup;
use std::cell::OnceCell;

// ---------------------------------------------------------------------------
// SpanStyle — deferred vs pre-resolved style
// ---------------------------------------------------------------------------

/// The style carried by a [`Span`].
///
/// Mirrors Python `Span.style: Style | str`:
/// - `Raw(String)` stores the tag body exactly as parsed from markup (e.g.
///   `"bold"`, `"red on blue"`, `"foobar"`, `"link=url"`).  Resolution to a
///   concrete [`Style`] is deferred to render time so that theme tokens
///   (`$primary`, `auto 20%`, etc.) are resolved with live app context.
/// - `Parsed(Style)` is a pre-resolved style used when the caller provides a
///   concrete [`Style`] directly (e.g. [`Content::styled`], padding helpers).
///
/// Unknown raw tags resolve to a null / transparent style at render time,
/// exactly as Python's `Style.parse("foobar")` does.
#[derive(Debug, Clone, PartialEq)]
pub enum SpanStyle {
    /// Raw tag body string, deferred for render-time resolution.
    Raw(String),
    /// Pre-resolved concrete style (skips parse at render time).
    Parsed(Style),
}

impl SpanStyle {
    /// Resolve to a concrete [`Style`], using a fallback parse function for
    /// `Raw` variants.
    ///
    /// At render time call `resolve_with(|raw| theme_context.parse(raw))`.
    /// Outside of render context, the default fallback tries `Style::from_str`
    /// which handles simple keywords but silently returns `Style::new()` for
    /// unrecognised tokens — matching Python's behaviour of null-styling unknown
    /// tags.
    pub fn resolve_with<F>(&self, parse_fn: F) -> Style
    where
        F: Fn(&str) -> Style,
    {
        match self {
            SpanStyle::Parsed(s) => s.clone(),
            SpanStyle::Raw(raw) => parse_fn(raw),
        }
    }

    /// Convenience: resolve without theme context.  Unknown tokens → `Style::new()`.
    pub fn resolve_default(&self) -> Style {
        self.resolve_with(|raw| {
            markup::parse_tag_style(raw)
                .map(|t| t.style)
                .unwrap_or_default()
        })
    }

    /// Return the raw tag body if this is a `Raw` variant.
    pub fn raw(&self) -> Option<&str> {
        match self {
            SpanStyle::Raw(s) => Some(s.as_str()),
            SpanStyle::Parsed(_) => None,
        }
    }
}

impl Default for SpanStyle {
    fn default() -> Self {
        SpanStyle::Parsed(Style::new())
    }
}

// ---------------------------------------------------------------------------
// Span
// ---------------------------------------------------------------------------

/// A styled range of character (byte) offsets within a [`Content`]'s text.
///
/// Mirrors Python `textual.content.Span(start, end, style)` where
/// `style: Style | str`.  The Rust type uses [`SpanStyle`] to carry either a
/// deferred raw tag string (markup-parsed) or a pre-resolved [`Style`].
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
    /// Style for this span — either a deferred raw tag string or a concrete Style.
    pub span_style: SpanStyle,
}

impl Span {
    /// Create a new `Span` with a pre-resolved style.
    pub fn new(start: usize, end: usize, style: Style) -> Self {
        Self {
            start,
            end,
            span_style: SpanStyle::Parsed(style),
        }
    }

    /// Create a new `Span` with a raw (deferred) tag body.
    pub fn new_raw(start: usize, end: usize, raw: impl Into<String>) -> Self {
        Self {
            start,
            end,
            span_style: SpanStyle::Raw(raw.into()),
        }
    }

    /// Return the concrete `Style` for this span using the default (no-context)
    /// resolver.  For render paths, prefer `span_style.resolve_with(parse_fn)`.
    pub fn style(&self) -> Style {
        self.span_style.resolve_default()
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
        Span {
            start,
            end,
            span_style: self.span_style.clone(),
        }
    }

    /// Extend the span's end by `cells` bytes.
    pub fn extend(&self, cells: usize) -> Self {
        Span {
            start: self.start,
            end: self.end + cells,
            span_style: self.span_style.clone(),
        }
    }

    fn with_range(&self, start: usize, end: usize) -> Self {
        Span {
            start,
            end,
            span_style: self.span_style.clone(),
        }
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
///
/// `cell_length` is computed lazily via interior mutability (`OnceCell`) so
/// that `&self` accessors work without requiring `&mut self` — mirroring
/// Python's `@cached_property`.
#[derive(Debug, Clone)]
pub struct Content {
    text: String,
    spans: Vec<Span>,
    /// Cached cell length (lazily computed; interior mutability via OnceCell).
    cell_length_cache: OnceCell<usize>,
}

impl PartialEq for Content {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text && self.spans == other.spans
    }
}

impl Content {
    // -----------------------------------------------------------------------
    // Internal constructor
    // -----------------------------------------------------------------------

    fn new_uncached(text: String, spans: Vec<Span>) -> Self {
        Self {
            text,
            spans,
            cell_length_cache: OnceCell::new(),
        }
    }

    fn new_with_cell_len(text: String, spans: Vec<Span>, cell_length: Option<usize>) -> Self {
        let cell_length_cache = OnceCell::new();
        if let Some(len) = cell_length {
            let _ = cell_length_cache.set(len);
        }
        Self {
            text,
            spans,
            cell_length_cache,
        }
    }

    // -----------------------------------------------------------------------
    // Constructors
    // -----------------------------------------------------------------------

    /// Return the shared empty `Content` instance.
    ///
    /// Mirrors Python `Content.empty()`.
    pub fn empty() -> Self {
        Self::new_uncached(String::new(), Vec::new())
    }

    /// Create `Content` from a plain string (no markup parsing).
    ///
    /// Mirrors Python `Content(text)` (the plain constructor path).
    pub fn from_text(text: impl Into<String>) -> Self {
        let text = strip_control_codes(text.into());
        if text.is_empty() {
            return Self::empty();
        }
        Self::new_uncached(text, Vec::new())
    }

    /// Create `Content` by parsing Textual markup.
    ///
    /// Mirrors Python `Content.from_markup(markup)`.
    ///
    /// If the string contains no `[` characters it is treated as plain text
    /// (fast path, no allocation).
    ///
    /// Tag bodies are stored as raw strings in `Span.span_style = SpanStyle::Raw(...)`,
    /// exactly as Python stores `Span.style: str`.  Resolution to a concrete
    /// [`Style`] is deferred to render time so theme tokens (`$primary`,
    /// `auto 20%`) can be resolved with live app context.
    ///
    /// Unknown tag bodies (e.g. `"foobar"`) are **consumed** (not emitted as
    /// literal text) and stored raw; they resolve to a null/transparent style
    /// at render time — matching Python's behaviour.
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
            .map(|rs| Span::new_raw(rs.start, rs.end, rs.raw_tag))
            .collect();

        Self::new_uncached(text, spans)
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
        Self::new_uncached(text, spans)
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
                        spans.push(Span {
                            start: span.start + offset,
                            end: span.end + offset,
                            span_style: span.span_style.clone(),
                        });
                    }
                    position += c.text.len();
                    text.push_str(&c.text);
                }
            }
        }

        if text.is_empty() {
            return Self::empty();
        }

        Self::new_uncached(text, spans)
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
    /// characters (CJK, emoji).  The result is cached via interior mutability
    /// (`OnceCell`) — this method takes `&self` (not `&mut self`), mirroring
    /// Python's `@cached_property` on `Content.cell_length`.
    ///
    /// Mirrors Python `Content.cell_length`.
    pub fn cell_length(&self) -> usize {
        *self
            .cell_length_cache
            .get_or_init(|| rich_rs::cell_len(&self.text))
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
    /// Raw span styles are resolved via the default resolver (no theme context).
    ///
    /// Mirrors Python `Content.get_style_at_offset(offset)`.
    pub fn get_style_at_offset(&self, offset: usize) -> Style {
        let mut result = Style::new();
        for span in &self.spans {
            if span.start <= offset && offset < span.end {
                result = result.combine(&span.style());
            }
        }
        result
    }

    /// Resolve all `Raw` spans to `Parsed` spans using the provided parse function.
    ///
    /// Mirrors the render-time `get_style(span.style)` call in Python's
    /// `Content.render()`.  Call this before rendering to apply theme context
    /// (e.g. `app.parse_style`) so that `$primary`, `auto 20%`, etc. resolve
    /// correctly.
    ///
    /// Returns a new `Content` with all spans resolved to `SpanStyle::Parsed`.
    pub fn resolve_styles<F>(&self, parse_fn: F) -> Self
    where
        F: Fn(&str) -> Style,
    {
        let resolved: Vec<Span> = self
            .spans
            .iter()
            .map(|span| Span {
                start: span.start,
                end: span.end,
                span_style: SpanStyle::Parsed(span.span_style.resolve_with(&parse_fn)),
            })
            .collect();
        let out = Self::new_uncached(self.text.clone(), resolved);
        // Propagate cached cell length if available
        if let Some(&len) = self.cell_length_cache.get() {
            let _ = out.cell_length_cache.set(len);
        }
        out
    }

    // -----------------------------------------------------------------------
    // Basic manipulation (needed for unit tests and Phase B prep)
    // -----------------------------------------------------------------------

    /// Append another `Content` to this one, returning a new `Content`.
    pub fn append(&self, other: &Content) -> Content {
        let offset = self.text.len();
        let mut spans = self.spans.clone();
        for span in &other.spans {
            spans.push(Span {
                start: span.start + offset,
                end: span.end + offset,
                span_style: span.span_style.clone(),
            });
        }
        let mut text = self.text.clone();
        text.push_str(&other.text);
        Content::new_uncached(text, spans)
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
        Content::new_uncached(self.text.clone(), spans)
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
                    s.with_range(s.start, max_offset)
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
        let length = self.cell_length();
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
                    s.with_range(s.start, max_offset)
                }
            })
            .collect();
        Content::new_uncached(text.to_string(), spans)
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
        Content::new_uncached(stripped.to_string(), spans)
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
            .map(|s| s.with_range(s.start + count, s.end + count))
            .collect();
        let cell_len = self.cell_length_cache.get().map(|l| l + count);
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
        let cell_len = self.cell_length_cache.get().map(|l| l + count);
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
        let len = content.cell_length();
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
        let len = content.cell_length();
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
            .map(|&(start, end)| Content::new_uncached(text[start..end].to_string(), Vec::new()))
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
                    pieces[piece_idx].spans.push(span.with_range(new_start, new_end));
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

    // -----------------------------------------------------------------------
    // Phase C — render_strips
    // -----------------------------------------------------------------------

    /// Render the `Content` into fully-styled output lines.
    ///
    /// This is the core of Phase C: turn a `Content` into
    /// `Vec<Vec<rich_rs::Segment>>` — one inner `Vec<Segment>` per output row —
    /// for a given `width`, optional `height`, alignment, overflow mode, and a
    /// **visual style** (the widget's own resolved style, e.g. `color: $primary;
    /// background: $surface`).
    ///
    /// # Arguments
    /// - `width` — target width in display cells.  Returns empty vec when 0.
    /// - `height` — optional maximum number of output rows.  `None` = no cap.
    /// - `visual_style` — the widget's visual style (carries `fg`, `bg`,
    ///   text-attribute bits).  Applied as the *base* style under span styles,
    ///   mirroring Python `visual.py` `to_strips`.
    /// - `align` — horizontal alignment (`TextAlign::Left/Center/Right`).
    ///   `Justify` is treated as `Left`.
    /// - `overflow` — `"fold"` / `"ellipsis"` / other (=truncate).
    /// - `no_wrap` — if true, skip word-wrap.
    /// - `line_pad` — content-pad cells on each side (background-only, matching
    ///   Python `Content.pad(line_pad, line_pad)`).
    /// - `resolve_fn` — converts a raw span tag string (e.g. `"bold"`,
    ///   `"$primary"`) to a concrete [`Style`].  At app render time this is
    ///   `app.parse_style`; in unit tests a closure suffices.
    ///
    /// # Surface semantics (mirrors Python `_FormattedLine.to_strip`)
    ///
    /// Each output row is made up of three logical surfaces:
    /// 1. **Glyph runs**: segments containing non-whitespace characters carry
    ///    full colour (`fg` from `visual_style + span_style`, `bg` from
    ///    `visual_style`).
    /// 2. **Content-pad / alignment-pad segments**: spaces added by `line_pad`
    ///    or alignment carry only the background (`visual_style.background_style`)
    ///    and NO foreground — matching Python `style.background_style`.
    /// 3. **Vertical fill rows**: rows added to reach `height` are blank rows
    ///    carrying only `visual_style.bg` (no fg).
    ///
    /// # ADDITIVE — not yet wired into the render path
    ///
    /// `render_strips` is built and unit-tested here but is NOT yet called from
    /// `core.rs` / `segments.rs` / `text.rs`.  Migration happens in a later
    /// phase (see `CONTENT_LAYER_KEYSTONE.md §7 Phase D`).
    pub fn render_strips<F>(
        &self,
        width: usize,
        height: Option<usize>,
        visual_style: &Style,
        align: crate::style::TextAlign,
        overflow: &str,
        no_wrap: bool,
        line_pad: usize,
        resolve_fn: F,
    ) -> Vec<Vec<rich_rs::Segment>>
    where
        F: Fn(&str) -> Style,
    {
        if width == 0 {
            return Vec::new();
        }

        // Step 1: resolve all Raw span styles using the provided resolver so
        // theme tokens ($primary, auto …) are concrete before rendering.
        let resolved = self.resolve_styles(&resolve_fn);

        // Step 2: wrap into output lines.
        let lines = resolved.wrap_and_format(width, overflow, no_wrap, line_pad);

        // Step 3: clip to height if requested.
        let lines: Vec<Content> = match height {
            Some(h) => lines.into_iter().take(h).collect(),
            None => lines,
        };

        let n_content_lines = lines.len();

        // Step 4: render each content line into segments.
        let mut strips: Vec<Vec<rich_rs::Segment>> = lines
            .into_iter()
            .map(|line| render_content_line_to_segments(&line, width, visual_style, align))
            .collect();

        // Step 5: vertical fill — pad to height with bg-only blank rows.
        if let Some(h) = height {
            let fill_count = h.saturating_sub(n_content_lines);
            if fill_count > 0 {
                let blank = make_bg_segment(" ".repeat(width), visual_style);
                for _ in 0..fill_count {
                    strips.push(vec![blank.clone()]);
                }
            }
        }

        strips
    }
}

// ---------------------------------------------------------------------------
// Phase C internals
// ---------------------------------------------------------------------------

/// Render a single post-wrap `Content` line into `rich_rs::Segment`s.
///
/// Mirrors `_FormattedLine.to_strip` from Python `content.py:1751`.
///
/// The `line` has already been word-wrapped, rstripped, and padded with
/// `line_pad` spaces on each side by `wrap_and_format`.  We still need to
/// apply horizontal alignment padding (bg-only) and then walk the span map
/// to produce glyph+bg segments.
fn render_content_line_to_segments(
    line: &Content,
    width: usize,
    visual_style: &Style,
    align: crate::style::TextAlign,
) -> Vec<rich_rs::Segment> {
    use crate::style::TextAlign;
    // Compute pad cells for alignment.
    let cell_len = line.cell_length();
    let (pad_left, pad_right) = match align {
        TextAlign::Center => {
            let excess = width.saturating_sub(cell_len);
            let left = excess / 2;
            let right = excess - left;
            (left, right)
        }
        TextAlign::Right => (width.saturating_sub(cell_len), 0),
        // Left and Justify both start from the left edge.
        TextAlign::Left | TextAlign::Justify => (0, 0),
    };

    let mut segs: Vec<rich_rs::Segment> = Vec::new();

    // Left alignment pad — bg only.
    if pad_left > 0 {
        segs.push(make_bg_segment(" ".repeat(pad_left), visual_style));
    }

    // Walk the span map and emit per-text-run segments.
    // This is a direct Rust adaptation of Python `Content.render(base_style, end="")`.
    emit_rendered_segments(line, visual_style, &mut segs);

    // Right alignment pad — bg only.
    if pad_right > 0 {
        segs.push(make_bg_segment(" ".repeat(pad_right), visual_style));
    }

    segs
}

/// Walk the span coverage map of `content` and emit `rich_rs::Segment`s into
/// `out`, applying `visual_style` as the base and span styles layered on top.
///
/// Key surface rule (mirrors Python `_FormattedLine.to_strip` + `apply_style_to_segments`
/// `has_glyph` guard in `segments.rs`):
/// - **Glyph cells** (any non-whitespace character in the run): emit with full
///   `rich_rs::Style` built from `visual_style.fg + span_style.fg` + `visual_style.bg`.
/// - **Whitespace-only runs**: emit with bg only (no fg) — mirrors Python
///   `style.background_style.rich_style` on pad segments.
///
/// This keeps the App/Screen default `color: $foreground` reaching text glyphs
/// but not fill spaces.
fn emit_rendered_segments(
    content: &Content,
    visual_style: &Style,
    out: &mut Vec<rich_rs::Segment>,
) {
    let text = content.plain();
    if text.is_empty() {
        return;
    }

    // Build an event list: (byte_offset, is_closing, span_index)
    // span index 0 = base (visual_style); 1..N = span styles.
    let spans = content.spans();
    let n_spans = spans.len();

    // Collect all (offset, is_end, idx) events.  idx==0 is the sentinel for
    // the entire range [0, text.len()).
    let mut events: Vec<(usize, bool, usize)> = Vec::with_capacity(2 + n_spans * 2);
    events.push((0, false, 0));
    for (i, span) in spans.iter().enumerate() {
        let idx = i + 1;
        events.push((span.start, false, idx));
        events.push((span.end, true, idx));
    }
    events.push((text.len(), true, 0));

    // Sort: primary by offset, secondary so opens (false) come before closes (true).
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    // Walk events, maintaining a stack of active span indices.
    let mut active: Vec<usize> = Vec::new();
    let mut pos = 0usize;

    let mut i = 0;
    while i < events.len() {
        let (offset, _is_end, _idx) = events[i];

        // Process all events at this offset.
        let mut j = i;
        while j < events.len() && events[j].0 == offset {
            let (_, is_end, idx) = events[j];
            if is_end {
                active.retain(|&x| x != idx);
            } else {
                active.push(idx);
            }
            j += 1;
        }

        // The text run is [offset, next_offset).
        let next_offset = events.get(j).map(|e| e.0).unwrap_or(text.len());
        if next_offset > pos {
            let run = &text[pos..next_offset];
            if !run.is_empty() {
                // Compute the effective style for this run.
                // Base = visual_style.  Overlay span styles in order (lowest idx first
                // for determinism; Python uses a stack so last-opened wins).
                let mut effective = visual_style.clone();
                // Add span styles in order of their span index.
                let mut active_sorted = active.clone();
                active_sorted.sort_unstable();
                for &idx in &active_sorted {
                    if idx == 0 {
                        continue; // base already applied
                    }
                    let span = &spans[idx - 1];
                    let span_style = match &span.span_style {
                        SpanStyle::Parsed(s) => s.clone(),
                        SpanStyle::Raw(raw) => {
                            // Should already be resolved by resolve_styles — this is a
                            // fallback for spans added after resolve_styles was called.
                            crate::content::markup::parse_tag_style(raw)
                                .map(|t| t.style)
                                .unwrap_or_default()
                        }
                    };
                    effective = effective.combine(&span_style);
                }

                let has_glyph = run.chars().any(|c| !c.is_whitespace());
                let seg = make_segment(run, &effective, visual_style, has_glyph);
                out.push(seg);
            }
        }

        pos = next_offset;
        i = j;
    }
}

/// Build a `rich_rs::Segment` for a text run.
///
/// - `effective_style` — the merged style (visual_style + span styles) for this run.
/// - `visual_style` — the base visual style (for bg fallback).
/// - `has_glyph` — if true, apply fg; if false (whitespace-only) apply bg only.
fn make_segment(
    text: &str,
    effective_style: &Style,
    visual_style: &Style,
    has_glyph: bool,
) -> rich_rs::Segment {
    // Determine the resolved background color for this cell.
    // bg comes from the effective style (or visual_style as fallback).
    let bg = effective_style.bg.or(visual_style.bg);
    let default_bg = bg.unwrap_or(crate::style::Color::rgb(0, 0, 0));

    let mut rich = rich_rs::Style::new();

    // Apply background if present.
    if let Some(bg_color) = bg {
        if bg_color.a >= 1.0 {
            rich = rich.with_bgcolor(bg_color.to_simple_opaque());
        } else if bg_color.a > 0.0 {
            rich = rich.with_bgcolor(bg_color.flatten_over(default_bg).to_simple_opaque());
        }
    }

    // Foreground: only on glyph cells, mirroring `has_glyph` guard in segments.rs.
    if has_glyph {
        if let Some(fg_color) = effective_style.fg {
            let flat = if fg_color.a >= 1.0 {
                fg_color
            } else {
                fg_color.flatten_over(default_bg)
            };
            if flat.a > 0.0 {
                rich = rich.with_color(flat.to_simple_opaque());
            }
        }
        // Text attributes (bold, italic, etc.).
        if let Some(bold) = effective_style.bold {
            rich = rich.with_bold(bold);
        }
        if let Some(dim) = effective_style.dim {
            rich = rich.with_dim(dim);
        }
        if let Some(italic) = effective_style.italic {
            rich = rich.with_italic(italic);
        }
        if let Some(underline) = effective_style.underline {
            rich = rich.with_underline(underline);
        }
        if let Some(reverse) = effective_style.reverse {
            rich.reverse = Some(reverse);
        }
        if let Some(strike) = effective_style.strike {
            rich = rich.with_strike(strike);
        }
    }

    rich_rs::Segment::styled(text.to_string(), rich)
}

/// Build a bg-only `rich_rs::Segment` (for padding / fill).
///
/// Mirrors Python `style.background_style.rich_style` — only background color,
/// no foreground, no text attributes.
fn make_bg_segment(text: impl Into<String>, visual_style: &Style) -> rich_rs::Segment {
    let mut rich = rich_rs::Style::new();
    if let Some(bg) = visual_style.bg {
        let default_bg = crate::style::Color::rgb(0, 0, 0);
        if bg.a >= 1.0 {
            rich = rich.with_bgcolor(bg.to_simple_opaque());
        } else if bg.a > 0.0 {
            rich = rich.with_bgcolor(bg.flatten_over(default_bg).to_simple_opaque());
        }
    }
    rich_rs::Segment::styled(text.into(), rich)
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
        assert_eq!(c.spans()[0].style().bold, Some(true));
    }

    #[test]
    fn test_styled_empty_returns_empty() {
        let c = Content::styled("", Style::new().bold(true));
        assert!(c.is_empty());
    }

    // --- blank ---

    #[test]
    fn test_blank_no_style() {
        let c = Content::blank(5, None);
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
        // Span carries raw "bold" — resolved style has bold=true
        assert_eq!(c.spans()[0].span_style, SpanStyle::Raw("bold".to_string()));
        assert_eq!(c.spans()[0].style().bold, Some(true));
    }

    #[test]
    fn test_from_markup_link_no_visual_style() {
        // The critical invariant: [link=url] must NOT apply visual style
        let c = Content::from_markup("[link=https://example.com]click[/link]");
        assert_eq!(c.plain(), "click");
        assert_eq!(c.spans().len(), 1);
        // Raw tag preserved
        assert_eq!(
            c.spans()[0].span_style,
            SpanStyle::Raw("link=https://example.com".to_string())
        );
        // Default resolution of "link=url" produces no visual style
        let style = c.spans()[0].style();
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

    // --- unknown tag: Python-faithful consume-and-null-style behavior ---

    /// Python: `from_markup('[foobar]test[/foobar]')` → plain='test', span with
    /// raw='foobar', resolves to null style.  The `[foobar]` text must NOT appear
    /// in the plain output.
    #[test]
    fn test_unknown_tag_consumed_not_literal() {
        let c = Content::from_markup("[foobar]test[/foobar]");
        // Tag is consumed — plain text is just "test"
        assert_eq!(c.plain(), "test", "unknown tag must be consumed, not emitted as literal");
        // One span is produced with the raw tag body
        assert_eq!(c.spans().len(), 1);
        assert_eq!(c.spans()[0].start, 0);
        assert_eq!(c.spans()[0].end, 4);
        assert_eq!(
            c.spans()[0].span_style,
            SpanStyle::Raw("foobar".to_string()),
            "unknown tag body must be stored as raw string"
        );
        // Resolve to null style
        let style = c.spans()[0].style();
        assert!(style.bold.is_none());
        assert!(style.fg.is_none());
        assert!(style.bg.is_none());
    }

    /// Python: `from_markup('[bad on red]y[/]')` → plain='y', span with raw='bad on red'.
    /// Multi-token unrecognised tags are also consumed.
    #[test]
    fn test_unknown_multi_token_tag_consumed() {
        let c = Content::from_markup("[bad on red]y[/]");
        assert_eq!(c.plain(), "y", "multi-token unknown tag must be consumed");
        assert_eq!(c.spans().len(), 1);
        assert_eq!(
            c.spans()[0].span_style,
            SpanStyle::Raw("bad on red".to_string()),
        );
        // "bad" is unrecognised but "on red" would be a valid bg — raw stored, not partially resolved
        // At render time parse_style("bad on red") would fail → null style
    }

    /// Deferred resolution: a theme token like "$primary" stored raw resolves
    /// differently at render time vs. parse time.  At parse time (no context) it
    /// returns `Style::new()`.  At render time the `resolve_styles` hook can
    /// substitute the real theme color.
    #[test]
    fn test_deferred_theme_token_stored_raw() {
        let c = Content::from_markup("[$primary]hello[/]");
        // The tag body "$primary" (or "$ primary"?) is kept raw — test the raw storage
        assert_eq!(c.plain(), "hello");
        assert_eq!(c.spans().len(), 1);
        // Raw variant, not pre-resolved
        assert!(
            matches!(c.spans()[0].span_style, SpanStyle::Raw(_)),
            "theme token must be stored as raw, not eagerly resolved"
        );
    }

    /// `resolve_styles` with a mock parse function applies the theme context.
    #[test]
    fn test_resolve_styles_applies_context() {
        let c = Content::from_markup("[mytoken]hello[/]");
        // Simulate render-time resolution that maps "mytoken" → bold style
        let bold_style = Style::new().bold(true);
        let resolved = c.resolve_styles(|raw| {
            if raw == "mytoken" {
                bold_style.clone()
            } else {
                Style::new()
            }
        });
        assert_eq!(resolved.plain(), "hello");
        assert_eq!(resolved.spans().len(), 1);
        // After resolution, span_style must be Parsed
        match &resolved.spans()[0].span_style {
            SpanStyle::Parsed(s) => assert_eq!(s.bold, Some(true)),
            SpanStyle::Raw(r) => panic!("expected Parsed after resolve_styles, got Raw({r:?})"),
        }
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

    // --- cell_length (now &self via OnceCell) ---

    #[test]
    fn test_cell_length_ascii() {
        let c = Content::from_text("hello");
        assert_eq!(c.cell_length(), 5);
    }

    #[test]
    fn test_cell_length_cached() {
        let c = Content::from_text("abc");
        let first = c.cell_length();
        let second = c.cell_length();
        assert_eq!(first, second);
        assert_eq!(first, 3);
    }

    #[test]
    fn test_cell_length_immutable_ref() {
        // cell_length must work on &Content (not &mut Content)
        let c = Content::from_text("hello");
        let r: &Content = &c;
        assert_eq!(r.cell_length(), 5);
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
        assert_eq!(c2.spans()[0].style().bold, Some(true));
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
        assert_eq!(t.cell_length(), 5);
    }

    #[test]
    fn test_truncate_ellipsis() {
        let c = Content::from_text("hello world");
        let t = c.truncate(6, true);
        // "hello" (5 cells) + "…" (1 cell) = 6 cells
        assert_eq!(t.plain(), "hello…");
        assert_eq!(t.cell_length(), 6);
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
        assert_eq!(ct.cell_length(), 6);
    }

    #[test]
    fn test_center_odd_extra_right() {
        // "hi" (2 cells) in width 5: left=1, right=2 (right gets the extra space)
        let c = Content::from_text("hi");
        let ct = c.center(5, false);
        // left = (5-2)/2 = 1, right = 5 - 1 - 2 = 2
        assert_eq!(ct.plain(), " hi  ");
        assert_eq!(ct.cell_length(), 5);
    }

    #[test]
    fn test_center_no_spans_on_pad() {
        // With an unstyled content, padding produces no spans.
        let c = Content::from_text("hello");
        let ct = c.center(10, false);
        assert_eq!(ct.cell_length(), 10);
        assert!(ct.spans().is_empty());
    }

    #[test]
    fn test_center_truncates_if_too_long() {
        let c = Content::from_text("hello world");
        let ct = c.center(5, false);
        // truncated to 5, no room for padding
        assert_eq!(ct.cell_length(), 5);
    }

    // --- right_align ---

    #[test]
    fn test_right_align_basic() {
        let c = Content::from_text("hi");
        let r = c.right_align(6, false);
        assert_eq!(r.plain(), "    hi");
        assert_eq!(r.cell_length(), 6);
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

    // =========================================================================
    // Phase C tests — render_strips
    // =========================================================================

    // Helper: null resolver (no theme tokens supported).
    fn null_resolver(raw: &str) -> Style {
        crate::content::markup::parse_tag_style(raw)
            .map(|t| t.style)
            .unwrap_or_default()
    }

    // Extract text from all segments in a strip.
    fn strip_text(strip: &[rich_rs::Segment]) -> String {
        strip.iter().map(|s| s.text.as_ref()).collect()
    }

    // Get the fg SimpleColor from the first segment that has one.
    fn first_fg(strip: &[rich_rs::Segment]) -> Option<rich_rs::SimpleColor> {
        strip
            .iter()
            .find_map(|s| s.style.as_ref().and_then(|st| st.color))
    }

    // Get the bg SimpleColor from the first segment that has one.
    fn first_bg(strip: &[rich_rs::Segment]) -> Option<rich_rs::SimpleColor> {
        strip
            .iter()
            .find_map(|s| s.style.as_ref().and_then(|st| st.bgcolor))
    }

    /// Python baseline: plain content, left-align, width=10, height=1.
    /// Segments should contain the text and no explicit fg (visual_style has no fg).
    #[test]
    fn test_render_strips_plain_no_visual_style() {
        let c = Content::from_text("hello");
        let strips = c.render_strips(
            10,
            Some(1),
            &Style::new(),
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 1, "should produce 1 row");
        let text = strip_text(&strips[0]);
        // wrap_and_format with width=10 produces "hello" (5 chars, no padding from
        // wrap_and_format itself — line_pad=0), and render_strips left-align adds no pad.
        assert_eq!(text, "hello", "text should be 'hello'");
        // No fg since visual_style has no fg.
        assert!(
            first_fg(&strips[0]).is_none(),
            "no explicit fg when visual_style has no fg"
        );
    }

    /// Visual style with explicit bg: glyph segment should carry the bg.
    #[test]
    fn test_render_strips_visual_bg_applied() {
        let c = Content::from_text("hi");
        let red = crate::style::Color::rgb(200, 0, 0);
        let visual = Style::new().bg(red);
        let strips = c.render_strips(
            5,
            Some(1),
            &visual,
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 1);
        // The bg should be present on some segment.
        let bg = first_bg(&strips[0]);
        assert!(bg.is_some(), "bg should be set when visual_style has bg");
        assert_eq!(
            bg.unwrap(),
            rich_rs::SimpleColor::Rgb { r: 200, g: 0, b: 0 },
            "bg should match the visual_style bg"
        );
    }

    /// Visual style with fg: glyph cells should carry the fg.
    #[test]
    fn test_render_strips_visual_fg_on_glyph() {
        let c = Content::from_text("hi");
        let blue = crate::style::Color::rgb(0, 0, 200);
        let visual = Style::new().fg(blue);
        let strips = c.render_strips(
            5,
            Some(1),
            &visual,
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 1);
        // Glyph cells ("hi") should have fg = blue.
        // fg.flatten_over(default_bg=black) = blue (alpha=1.0, opaque).
        let fg = first_fg(&strips[0]);
        assert!(fg.is_some(), "fg should be set on glyph cells");
        assert_eq!(
            fg.unwrap(),
            rich_rs::SimpleColor::Rgb { r: 0, g: 0, b: 200 },
            "fg should match the visual_style fg"
        );
    }

    /// Whitespace-only pad segment (from line_pad or alignment) must NOT carry fg.
    ///
    /// Python: pad segments are emitted with `style.background_style.rich_style`,
    /// which has bg but NO fg.  This is the `has_glyph` invariant.
    #[test]
    fn test_render_strips_pad_segment_no_fg() {
        // "hi" in width=6, right-align → 4 spaces pad-left + "hi"
        let blue = crate::style::Color::rgb(0, 0, 200);
        let red = crate::style::Color::rgb(200, 0, 0);
        let visual = Style::new().fg(blue).bg(red);
        let c = Content::from_text("hi");
        let strips = c.render_strips(
            6,
            Some(1),
            &visual,
            crate::style::TextAlign::Right,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 1);
        // First segment should be "    " (4 spaces) — no fg.
        let pad_seg = &strips[0][0];
        assert!(
            pad_seg.text.chars().all(|c| c == ' '),
            "first segment should be spaces, got {:?}",
            pad_seg.text
        );
        let pad_fg = pad_seg.style.as_ref().and_then(|s| s.color);
        assert!(
            pad_fg.is_none(),
            "pad segment must not carry fg, got {:?}",
            pad_fg
        );
        // But the pad segment should have the bg.
        let pad_bg = pad_seg.style.as_ref().and_then(|s| s.bgcolor);
        assert!(pad_bg.is_some(), "pad segment should carry bg");
    }

    /// Span style (fg from markup) overrides visual_style fg on glyph cells.
    /// Python: `style + text_style` (text_style from span wins over visual_style fg).
    #[test]
    fn test_render_strips_span_fg_overrides_visual() {
        // Content "[red]hi[/red]" with visual_style having blue fg.
        // The span sets fg=red, which should win on the "hi" glyph.
        let c = Content::from_markup("[red]hi[/red]");
        let blue = crate::style::Color::rgb(0, 0, 200);
        let visual = Style::new().fg(blue);
        let strips = c.render_strips(
            5,
            Some(1),
            &visual,
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 1);
        let fg = first_fg(&strips[0]);
        // The red color from "[red]" is #800000 in the terminal palette.
        // parse_color_like("red") → Color::rgb(128, 0, 0) (ANSI-ish) — or some red.
        // Just check it is NOT blue.
        assert!(fg.is_some(), "fg must be set");
        assert_ne!(
            fg.unwrap(),
            rich_rs::SimpleColor::Rgb { r: 0, g: 0, b: 200 },
            "span fg should override visual_style fg"
        );
    }

    /// Center alignment: left pad and right pad should both appear; neither should carry fg.
    #[test]
    fn test_render_strips_center_align_pad_no_fg() {
        // "hi" (2 cells) in width=8, center → 3 left pad + "hi" + 3 right pad.
        let blue = crate::style::Color::rgb(0, 0, 255);
        let green = crate::style::Color::rgb(0, 200, 0);
        let visual = Style::new().fg(blue).bg(green);
        let c = Content::from_text("hi");
        let strips = c.render_strips(
            8,
            Some(1),
            &visual,
            crate::style::TextAlign::Center,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 1);
        let full_text = strip_text(&strips[0]);
        // "   hi   " or "   hi   " — 3 left + 2 glyph + 3 right = 8
        assert_eq!(full_text.len(), 8, "total width should be 8");
        assert!(full_text.starts_with("   "), "3 left pad spaces expected");
        assert!(full_text.ends_with("   "), "3 right pad spaces expected");

        // Neither pad segment should have fg.
        for seg in &strips[0] {
            if seg.text.chars().all(|c| c == ' ') {
                let fg = seg.style.as_ref().and_then(|s| s.color);
                assert!(
                    fg.is_none(),
                    "pad segment must not carry fg; got {:?} for {:?}",
                    fg,
                    seg.text
                );
            }
        }
    }

    /// Vertical fill: height > content rows → extra blank bg-only rows.
    #[test]
    fn test_render_strips_vertical_fill_bg_only() {
        let red = crate::style::Color::rgb(200, 0, 0);
        let blue = crate::style::Color::rgb(0, 0, 200);
        let visual = Style::new().fg(blue).bg(red);
        let c = Content::from_text("hi");
        let strips = c.render_strips(
            5,
            Some(3),
            &visual,
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 3, "should produce 3 rows (1 content + 2 fill)");
        // Fill rows (index 1 and 2) must have bg, no fg.
        for fill_row in &strips[1..] {
            for seg in fill_row {
                let fg = seg.style.as_ref().and_then(|s| s.color);
                assert!(
                    fg.is_none(),
                    "vertical fill row must not carry fg; got {:?}",
                    fg
                );
                let bg = seg.style.as_ref().and_then(|s| s.bgcolor);
                assert!(bg.is_some(), "vertical fill row must carry bg");
            }
        }
    }

    /// wrap_and_format integration: long text wraps into multiple rows.
    #[test]
    fn test_render_strips_wraps_text() {
        let c = Content::from_text("hello world");
        let strips = c.render_strips(
            5,
            None,
            &Style::new(),
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 2, "should produce 2 wrapped rows");
        assert_eq!(strip_text(&strips[0]), "hello");
        assert_eq!(strip_text(&strips[1]), "world");
    }

    /// Height=0: return empty.
    #[test]
    fn test_render_strips_height_zero() {
        let c = Content::from_text("hello");
        let strips = c.render_strips(
            10,
            Some(0),
            &Style::new(),
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert!(strips.is_empty(), "height=0 should produce no rows");
    }

    /// Width=0: return empty.
    #[test]
    fn test_render_strips_width_zero() {
        let c = Content::from_text("hello");
        let strips = c.render_strips(
            0,
            Some(1),
            &Style::new(),
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert!(strips.is_empty(), "width=0 should produce no rows");
    }

    /// Bold from markup: glyph segment should carry bold=true.
    #[test]
    fn test_render_strips_bold_span() {
        let c = Content::from_markup("[bold]hi[/bold]");
        let strips = c.render_strips(
            5,
            Some(1),
            &Style::new(),
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 1);
        // Find the segment containing "hi".
        let hi_seg = strips[0]
            .iter()
            .find(|s| s.text.contains('h'))
            .expect("no segment containing 'hi'");
        let bold = hi_seg.style.as_ref().and_then(|s| s.bold);
        assert_eq!(bold, Some(true), "bold span must produce bold=true in segment");
    }

    /// Theme token resolution: a custom resolver maps "$mytoken" to red fg.
    #[test]
    fn test_render_strips_custom_resolver() {
        let c = Content::from_markup("[$mytoken]hi[/]");
        let red = crate::style::Color::rgb(255, 0, 0);
        let my_resolver = |raw: &str| -> Style {
            if raw == "$mytoken" {
                Style::new().fg(red)
            } else {
                null_resolver(raw)
            }
        };
        let strips = c.render_strips(
            5,
            Some(1),
            &Style::new(),
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            my_resolver,
        );
        assert_eq!(strips.len(), 1);
        let fg = first_fg(&strips[0]);
        assert_eq!(
            fg,
            Some(rich_rs::SimpleColor::Rgb { r: 255, g: 0, b: 0 }),
            "custom resolver should apply $mytoken as red fg"
        );
    }

    /// line_pad: content-pad spaces (from wrap_and_format) appear in the output
    /// and must not carry fg.
    #[test]
    fn test_render_strips_line_pad_no_fg() {
        let blue = crate::style::Color::rgb(0, 0, 200);
        let visual = Style::new().fg(blue);
        let c = Content::from_text("hi");
        // width=6, line_pad=1 → wrap_and_format produces " hi " (1+2+1 = 4 cells).
        // With left-align, render_strips emits those 4 cells as segments.
        let strips = c.render_strips(
            6,
            Some(1),
            &visual,
            crate::style::TextAlign::Left,
            "fold",
            false,
            1,
            null_resolver,
        );
        assert_eq!(strips.len(), 1);
        let full_text = strip_text(&strips[0]);
        // The wrapped line is " hi " (4 cells, padded), left-aligned in width=6.
        assert!(full_text.starts_with(' '), "line_pad left space should be present");
        assert!(full_text.ends_with(' '), "line_pad right space should be present");
        // The leading/trailing space segments must NOT carry fg.
        for seg in &strips[0] {
            if seg.text.chars().all(|c| c == ' ') {
                let fg = seg.style.as_ref().and_then(|s| s.color);
                assert!(
                    fg.is_none(),
                    "line_pad space must not carry fg; got {:?}",
                    fg
                );
            }
        }
    }

    /// Ellipsis overflow: long text gets truncated with '…'.
    #[test]
    fn test_render_strips_ellipsis_overflow() {
        let c = Content::from_text("hello world");
        let strips = c.render_strips(
            6,
            Some(1),
            &Style::new(),
            crate::style::TextAlign::Left,
            "ellipsis",
            true, // no_wrap
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 1);
        let text = strip_text(&strips[0]);
        assert!(
            text.contains('…'),
            "ellipsis overflow should produce '…'; got {:?}",
            text
        );
    }
}
