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
//!   semantics previously scattered across `core.rs`, `segments.rs`, and `text.rs`.
//!
//! ## Phase D — wired for Label/Static
//! - [`Content::render_strips`] is now called from `Label::render()` in `text.rs`.
//! - Remaining widgets (Button, DataTable, Input, Tree, etc.) still use the
//!   rich-rs `Text` / `render_str` path; migration is a future phase.
//!
//! See `docs/devel/CONTENT_LAYER_KEYSTONE.md` for the full phasing plan.

pub mod markup;

use crate::style::Style;
use markup::parse_markup;
use std::sync::OnceLock;

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
    /// Non-visual key/value metadata carried by this span (e.g. `@click=action`,
    /// `link=url`).  Mirrors how Python stores `@click` inside `Style._meta`;
    /// here it rides alongside the span so it survives style resolution and
    /// reaches segment emission, where it is stamped into the segment's
    /// `StyleMeta` for runtime hit-testing / action dispatch.
    pub meta: Vec<(String, String)>,
}

impl Span {
    /// Create a new `Span` with a pre-resolved style.
    pub fn new(start: usize, end: usize, style: Style) -> Self {
        Self {
            start,
            end,
            span_style: SpanStyle::Parsed(style),
            meta: Vec::new(),
        }
    }

    /// Create a new `Span` with a raw (deferred) tag body.
    pub fn new_raw(start: usize, end: usize, raw: impl Into<String>) -> Self {
        Self {
            start,
            end,
            span_style: SpanStyle::Raw(raw.into()),
            meta: Vec::new(),
        }
    }

    /// Create a new `Span` with a raw tag body and attached metadata.
    pub fn new_raw_with_meta(
        start: usize,
        end: usize,
        raw: impl Into<String>,
        meta: Vec<(String, String)>,
    ) -> Self {
        Self {
            start,
            end,
            span_style: SpanStyle::Raw(raw.into()),
            meta,
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
            meta: self.meta.clone(),
        }
    }

    /// Extend the span's end by `cells` bytes.
    pub fn extend(&self, cells: usize) -> Self {
        Span {
            start: self.start,
            end: self.end + cells,
            span_style: self.span_style.clone(),
            meta: self.meta.clone(),
        }
    }

    fn with_range(&self, start: usize, end: usize) -> Self {
        Span {
            start,
            end,
            span_style: self.span_style.clone(),
            meta: self.meta.clone(),
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
/// `cell_length` is computed lazily via interior mutability (`OnceLock`) so
/// that `&self` accessors work without requiring `&mut self` — mirroring
/// Python's `@cached_property`.
#[derive(Debug, Clone)]
pub struct Content {
    text: String,
    spans: Vec<Span>,
    /// Cached cell length (lazily computed; interior mutability via OnceLock).
    cell_length_cache: OnceLock<usize>,
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
            cell_length_cache: OnceLock::new(),
        }
    }

    fn new_with_cell_len(text: String, spans: Vec<Span>, cell_length: Option<usize>) -> Self {
        let cell_length_cache = OnceLock::new();
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
            .map(|rs| Span::new_raw_with_meta(rs.start, rs.end, rs.raw_tag, rs.meta))
            .collect();

        Self::new_uncached(text, spans)
    }

    /// Create content from markup, applying `string.Template`-style variable
    /// substitution over the text content **before** tag parsing.
    ///
    /// Mirrors Python `Content.from_markup(markup, **variables)`: each `$name`
    /// or `${name}` in the **text** (not in tag bodies) is replaced via
    /// `string.Template(...).safe_substitute(variables)` — unknown keys are left
    /// untouched, and `$$` is an escaped literal `$`.
    ///
    /// Example (Python parity):
    /// ```ignore
    /// let mut vars = std::collections::HashMap::new();
    /// vars.insert("name".to_string(), "Will".to_string());
    /// let c = Content::from_markup_with_vars("Hello, [b]$name[/b]!", &vars);
    /// assert_eq!(c.plain(), "Hello, Will!");
    /// ```
    ///
    /// Substitution faithfully follows Python:
    /// - Only text tokens are substituted; tag bodies like `[$primary]` are left
    ///   intact (so theme tokens still resolve at render time).
    /// - When `variables` is empty and the markup contains no `[`, this is the
    ///   plain-text fast path (matching Python's `from_markup` early return).
    pub fn from_markup_with_vars(
        markup: impl AsRef<str>,
        variables: &std::collections::HashMap<String, String>,
    ) -> Self {
        let markup = markup.as_ref();
        let markup = strip_control_codes(markup.to_string());

        if markup.is_empty() {
            return Self::empty();
        }

        // Python: `if "[" not in markup and not variables: return Content(markup)`
        if !markup.contains('[') && variables.is_empty() {
            return Self::from_text(markup);
        }

        let vars = if variables.is_empty() {
            None
        } else {
            Some(variables)
        };
        let (text, raw_spans) = markup::parse_markup_with_vars(&markup, vars);

        let spans: Vec<Span> = raw_spans
            .into_iter()
            .map(|rs| Span::new_raw_with_meta(rs.start, rs.end, rs.raw_tag, rs.meta))
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
    pub fn styled_with_cell_len(text: impl Into<String>, style: Style, cell_length: usize) -> Self {
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
                            meta: span.meta.clone(),
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
    /// (`OnceLock`) — this method takes `&self` (not `&mut self`), mirroring
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
                meta: span.meta.clone(),
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
                meta: span.meta.clone(),
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
        let inner: Vec<usize> = inner.into_iter().map(|o| o.min(text_len)).collect();

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
                    pieces[piece_idx]
                        .spans
                        .push(span.with_range(new_start, new_end));
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
            offsets.push(abs_pos); // start of separator
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
        self.wrap_and_format_marked(width, overflow, no_wrap, line_pad)
            .into_iter()
            .map(|(line, _)| line)
            .collect()
    }

    /// Like [`wrap_and_format`] but also returns, per output line, whether it is
    /// the **last** wrapped line of its logical (newline-delimited) paragraph.
    ///
    /// Mirrors Python `_wrap_and_format` setting `new_lines[-1].line_end = True`.
    /// The flag matters for `text-align: justify`, where the final line of a
    /// paragraph is left-aligned (not space-stretched) just like Python's
    /// `to_strip` guard `align == "justify" and self.line_end`.
    pub fn wrap_and_format_marked(
        &self,
        width: usize,
        overflow: &str,
        no_wrap: bool,
        line_pad: usize,
    ) -> Vec<(Content, bool)> {
        if width == 0 {
            return Vec::new();
        }

        let ellipsis = overflow == "ellipsis";
        let fold = overflow == "fold";

        // Inner width available for text (after removing line_pad from both sides).
        let inner_width = width.saturating_sub(line_pad * 2);

        let mut output: Vec<(Content, bool)> = Vec::new();

        // Split the content on newlines first (mirrors Python `self.split(allow_blank=True)`).
        let logical_lines = self.split_on("\n", true);

        for logical_line in logical_lines {
            if no_wrap {
                if fold {
                    // Hard-fold at inner_width.
                    let offsets =
                        rich_rs::divide_line(logical_line.plain(), inner_width.max(1), true);
                    let pieces = logical_line.divide(&offsets);
                    let num_pieces = pieces.len();
                    for (i, piece) in pieces.into_iter().enumerate() {
                        output.push((piece.pad(line_pad, line_pad), i + 1 == num_pieces));
                    }
                } else {
                    // Truncate (with optional ellipsis) — single output line.
                    let line = logical_line.truncate(inner_width, ellipsis);
                    output.push((line.pad(line_pad, line_pad), true));
                }
            } else {
                // Word-wrap using divide_line.
                let offsets = rich_rs::divide_line(logical_line.plain(), inner_width.max(1), fold);
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
                    output.push((line.pad(line_pad, line_pad), is_last));
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
    /// # Surface semantics (mirrors Python `_FormattedLine.to_strip` / `Visual.to_strips`)
    ///
    /// Each output row is made up of three logical surfaces:
    ///
    /// 1. **Content runs** (all characters produced by `Content.render()`): carry
    ///    the **full** style (`visual_style + span_style`) — fg, bg, bold,
    ///    italic, underline, reverse, strike.  This applies even to
    ///    whitespace-only content runs (e.g. a span covering `" "` or padding
    ///    added inside the text itself).  Python's `_FormattedLine.to_strip`
    ///    passes every run through `(style + text_style).rich_style` regardless
    ///    of whether it contains glyphs.
    ///
    ///    **C1 seam 1 fix**: the previous `has_glyph` guard that dropped fg on
    ///    whitespace content runs was incorrect; it is removed.
    ///
    /// 2. **Alignment-pad segments** (`pad_left` / `pad_right` from centering or
    ///    right-alignment): carry only the background
    ///    (`visual_style.background_style`, no fg) — matching Python's
    ///    `style.background_style.rich_style` on those padding spaces.
    ///
    /// 3. **Vertical fill rows**: rows added to reach `height` carry the full
    ///    style with `reverse` forced to `false` — matching Python
    ///    `(style + Style(reverse=False)).rich_style` in `Visual.to_strips`.
    ///
    ///    **C1 seam 2 fix**: the previous bg-only fill surface is replaced with
    ///    a full-style (minus reverse) surface, matching Python's fill contract.
    ///    Chosen approach: compute the fill style inside `render_strips` (full
    ///    style, reverse=false) and pass it to a dedicated `make_full_segment`
    ///    helper — no structural change to widget wiring required.
    ///
    /// # Phase D — wired into Label/Static render path
    ///
    /// `render_strips` is called from `Label::render()` in `text.rs` (Phase D).
    /// Migration of remaining widgets (Button, DataTable, Input, Tree, etc.)
    /// to this path is a future phase.
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

        // Step 2: wrap into output lines (with per-line "is last line of
        // paragraph" markers, needed so `text-align: justify` leaves the final
        // line left-aligned, matching Python `_FormattedLine.line_end`).
        let lines = resolved.wrap_and_format_marked(width, overflow, no_wrap, line_pad);

        // Step 3: clip to height if requested.
        let lines: Vec<(Content, bool)> = match height {
            Some(h) => lines.into_iter().take(h).collect(),
            None => lines,
        };

        let n_content_lines = lines.len();

        // Step 4: render each content line into segments.
        let mut strips: Vec<Vec<rich_rs::Segment>> = lines
            .into_iter()
            .map(|(line, line_end)| {
                render_content_line_to_segments(&line, width, visual_style, align, line_end)
            })
            .collect();

        // Step 5: vertical fill — pad to height.
        //
        // Python `Visual.to_strips` fills missing rows via `strip.extend_cell_length`
        // and `Strip.align` using `rich_style = (style + Style(reverse=False)).rich_style`
        // — i.e. the full style but with `reverse` forced to `false`.
        //
        // C1 seam 2: use full style (fg, bg, bold, etc.) with reverse=false for fill
        // rows rather than bg-only.
        if let Some(h) = height {
            let fill_count = h.saturating_sub(n_content_lines);
            if fill_count > 0 {
                // Build a fill style: full visual_style, reverse forced to false.
                let mut fill_style = visual_style.clone();
                fill_style.reverse = Some(false);
                let blank = make_full_segment(" ".repeat(width), &fill_style);
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
    line_end: bool,
) -> Vec<rich_rs::Segment> {
    use crate::style::TextAlign;

    // `text-align: justify` stretches inter-word spacing so the line fills the
    // full width — EXCEPT for the last line of a paragraph (`line_end`), which
    // is left-aligned. Mirrors Python `_FormattedLine.to_strip`:
    //   `if align in ("start","left") or (align == "justify" and self.line_end)`.
    if align == TextAlign::Justify && !line_end {
        return render_justified_line(line, width, visual_style);
    }

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
        // Left, and the last line of a Justify paragraph, start from the left edge.
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

/// Render a single line under `text-align: justify` (non-final line): the words
/// are separated by stretched runs of spaces so the line fills `width`.
///
/// Direct adaptation of Python `_FormattedLine.to_strip`'s `align == "justify"`
/// branch (content.py ~1772): split on single spaces, compute the base word
/// width, then distribute the slack spaces from the right, round-robin. The
/// inter-word pad spaces carry the full (fg-bearing) style, NOT bg-only —
/// Python emits them with `(style + text_style).rich_style`.
fn render_justified_line(
    line: &Content,
    width: usize,
    visual_style: &Style,
) -> Vec<rich_rs::Segment> {
    // words = content.split(" ", include_separator=False)
    let words = line.split_on(" ", false);
    let num_words = words.len();

    // words_size = sum(cell_len(word.rstrip(" ")))
    let words_size: usize = words.iter().map(|w| w.rstrip().cell_length()).sum();

    let mut num_spaces = num_words.saturating_sub(1);
    let mut spaces = vec![1usize; num_spaces];
    let mut index = 0usize;
    if !spaces.is_empty() {
        // Grow inter-word gaps from the right until the line fills `width`.
        while words_size + num_spaces < width {
            let n = spaces.len();
            spaces[n - index - 1] += 1;
            num_spaces += 1;
            index = (index + 1) % n;
        }
    }

    // Build the pad-space style. Python emits inter-word spaces with
    // `(style + text_style).rich_style`, i.e. fg-bearing. The body text in these
    // lines has no per-word spans, so the widget's foreground (or its `color:
    // auto` contrast) is the effective fg. Resolve `color: auto` here so the
    // blank pad runs carry the same concrete fg the glyphs receive (otherwise
    // `apply_style_to_segments`'s has_glyph guard would leave them bg-only and
    // split the line into many fg/def runs).
    let mut pad_style = visual_style.clone();
    if pad_style.fg.is_none() {
        if let Some(auto) = visual_style.fg_auto {
            if let Some(bg) = visual_style.bg {
                let contrast = crate::style::contrast_text(bg).blend_over_float(bg, auto.alpha());
                pad_style.fg = Some(contrast);
            }
        }
    }

    let mut segs: Vec<rich_rs::Segment> = Vec::new();
    for (i, word) in words.iter().enumerate() {
        emit_rendered_segments(word, visual_style, &mut segs);
        if let Some(&pad) = spaces.get(i) {
            if pad > 0 {
                segs.push(make_full_segment(" ".repeat(pad), &pad_style));
            }
        }
    }

    segs
}

/// Walk the span coverage map of `content` and emit `rich_rs::Segment`s into
/// `out`, applying `visual_style` as the base and span styles layered on top.
///
/// Surface rule (mirrors Python `_FormattedLine.to_strip` / `Content.render()`):
/// - **ALL content runs** (glyph and whitespace alike) receive the **full** merged
///   style: `(visual_style + span_style)` — fg, bg, text attributes.
/// - Python's `_FormattedLine.to_strip` passes every run produced by
///   `Content.render()` through `(style + text_style).rich_style` without any
///   has-glyph discrimination.  A whitespace span styled with `reverse=True` or
///   `underline=True` must show those attributes on the spaces.
///
/// The `has_glyph` guard is **not** applied here (C1 seam 1 fix).
/// Bg-only treatment is restricted to alignment pad segments built by
/// `make_bg_segment` (pad_left / pad_right in `render_content_line_to_segments`).
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
                // Collect non-visual span metadata (e.g. `@click=action`,
                // `link=url`) active over this run so it can be stamped onto the
                // produced segment's `StyleMeta`.  Last-opened span wins for a
                // given key (matches Python `Style.__add__` meta merge order).
                let mut run_meta: Vec<(String, String)> = Vec::new();
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
                    for (k, v) in &span.meta {
                        run_meta.retain(|(ek, _)| ek != k);
                        run_meta.push((k.clone(), v.clone()));
                    }
                }

                // Apply full style to ALL content runs — including whitespace.
                // Python's Content.render() + _FormattedLine.to_strip uses
                // (style + text_style).rich_style for every run without any
                // has_glyph discrimination.  C1 seam 1 fix.
                let mut seg = make_segment(run, &effective, visual_style);
                if !run_meta.is_empty() {
                    attach_span_meta(&mut seg, &run_meta);
                }
                out.push(seg);
            }
        }

        pos = next_offset;
        i = j;
    }
}

/// Attach non-visual span metadata (e.g. `@click=action`, `link=url`) to a
/// segment's [`rich_rs::StyleMeta`].
///
/// Mirrors how Python carries `@click` inside `Style._meta`: the metadata rides
/// on the rendered segment so it survives blitting into the `FrameBuffer` cell,
/// where the runtime can hit-test a click and dispatch the named action
/// (`app._broker_event` / `widget._on_click` in Python).  Each pair becomes a
/// string-valued `MetaValue` keyed by the markup attribute name.
fn attach_span_meta(seg: &mut rich_rs::Segment, meta: &[(String, String)]) {
    use rich_rs::{MetaValue, StyleMeta};
    let mut map = seg
        .meta
        .as_ref()
        .and_then(|m| m.meta.as_ref())
        .map(|m| (**m).clone())
        .unwrap_or_default();
    for (k, v) in meta {
        map.insert(k.clone(), MetaValue::str(v.as_str()));
    }
    let mut style_meta = seg.meta.take().unwrap_or_else(StyleMeta::new);
    style_meta.meta = Some(std::sync::Arc::new(map));
    seg.meta = Some(style_meta);
}

/// Build a `rich_rs::Segment` for a **content** text run applying the **full**
/// effective style (fg + bg + all text attributes).
///
/// C1 seam 1: the previous `has_glyph` parameter that dropped fg/attributes on
/// whitespace-only runs is **removed**.  Every content run — including spaces
/// covered by a span with `reverse`, `underline`, etc. — must carry the full
/// style, matching Python's `(style + text_style).rich_style`.
///
/// - `effective_style` — the merged style (visual_style + span styles) for this run.
/// - `visual_style`    — the base visual style (bg fallback when effective has none).
fn make_segment(text: &str, effective_style: &Style, visual_style: &Style) -> rich_rs::Segment {
    make_full_segment_with_bg_fallback(text, effective_style, visual_style)
}

/// Build a `rich_rs::Segment` applying the **full** style (fg, bg, text attrs).
///
/// Used by the vertical-fill path (seam 2) where the style is already
/// pre-computed (full style, reverse forced off).
fn make_full_segment(text: impl Into<String>, style: &Style) -> rich_rs::Segment {
    make_full_segment_with_bg_fallback(&text.into(), style, style)
}

/// Core helper: build a segment with all style attributes from `effective_style`,
/// using `visual_style.bg` as the fallback background if `effective_style.bg`
/// is absent.
fn make_full_segment_with_bg_fallback(
    text: &str,
    effective_style: &Style,
    visual_style: &Style,
) -> rich_rs::Segment {
    // Determine the resolved background color for this cell.
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

    // Foreground — applied to ALL runs (seam 1: no has_glyph guard).
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
    // Text attributes — applied to ALL runs.
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

    // --- from_markup_with_vars (string.Template safe_substitute parity) ---

    fn vars(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// Python: `Content.from_markup("Hello, [b]$name[/b]!", name="Will")`.
    #[test]
    fn test_from_markup_with_vars_basic() {
        let v = vars(&[("name", "Will")]);
        let c = Content::from_markup_with_vars("Hello, [b]$name[/b]!", &v);
        assert_eq!(c.plain(), "Hello, Will!");
        assert_eq!(c.spans().len(), 1);
        // [b] covers "Will"
        assert_eq!(c.spans()[0].start, 7);
        assert_eq!(c.spans()[0].end, 11);
        assert_eq!(c.spans()[0].span_style, SpanStyle::Raw("b".to_string()));
    }

    /// `${name}` braced form is substituted too.
    #[test]
    fn test_from_markup_with_vars_braced() {
        let v = vars(&[("name", "Will")]);
        let c = Content::from_markup_with_vars("Hi ${name} and $name", &v);
        assert_eq!(c.plain(), "Hi Will and Will");
    }

    /// `$$` is an escaped literal `$` (Python safe_substitute). Substitution only
    /// runs when variables are present (Python checks `variables or None`), so we
    /// pass a non-empty map here; the empty-map plain fast path is covered below.
    #[test]
    fn test_from_markup_with_vars_dollar_escape() {
        let v = vars(&[("unused", "x")]);
        let c = Content::from_markup_with_vars("cost is $$5", &v);
        assert_eq!(c.plain(), "cost is $5");
    }

    /// Empty variables map → no substitution at all (Python parity: `$$` stays).
    #[test]
    fn test_from_markup_with_vars_empty_map_no_escape() {
        let v = vars(&[]);
        let c = Content::from_markup_with_vars("cost is $$5", &v);
        assert_eq!(c.plain(), "cost is $$5");
    }

    /// Unknown keys are left unmodified (safe substitution — no error).
    #[test]
    fn test_from_markup_with_vars_missing_key() {
        let v = vars(&[("present", "x")]);
        let c = Content::from_markup_with_vars("$missing here", &v);
        assert_eq!(c.plain(), "$missing here");
        // `$1bad` — invalid identifier (starts with digit) → left as-is.
        let c2 = Content::from_markup_with_vars("$1bad", &vars(&[("1bad", "x")]));
        assert_eq!(c2.plain(), "$1bad");
        // braced invalid identifier `${a.b}` → left verbatim.
        let c3 = Content::from_markup_with_vars("${a.b}", &vars(&[("a.b", "x")]));
        assert_eq!(c3.plain(), "${a.b}");
    }

    /// Dict lookup is exact-case (`$name` does not match key `Name`).
    #[test]
    fn test_from_markup_with_vars_case_sensitive_lookup() {
        let c = Content::from_markup_with_vars("$name", &vars(&[("Name", "X")]));
        assert_eq!(c.plain(), "$name");
        // But the identifier pattern is case-insensitive: `$Name` matches the
        // pattern and looks up key "Name".
        let c2 = Content::from_markup_with_vars("$Name", &vars(&[("Name", "X")]));
        assert_eq!(c2.plain(), "X");
    }

    /// Critical Python parity: a variable VALUE containing markup-like brackets
    /// is inserted as LITERAL text, never re-parsed as a tag.
    #[test]
    fn test_from_markup_with_vars_value_with_brackets_is_literal() {
        let v = vars(&[("x", "[red]BIG[/red]")]);
        let c = Content::from_markup_with_vars("Hello $x world", &v);
        assert_eq!(c.plain(), "Hello [red]BIG[/red] world");
        assert!(
            c.spans().is_empty(),
            "value brackets must not be re-parsed into spans"
        );
    }

    /// Parity: markup tags + a variable value containing brackets. The tag span
    /// is preserved; the inserted value is literal text (Python parity).
    #[test]
    fn test_from_markup_with_vars_tags_plus_bracket_value() {
        let v = vars(&[("x", "[red]Z[/red]")]);
        let c = Content::from_markup_with_vars("[b]hi[/b] $x", &v);
        assert_eq!(c.plain(), "hi [red]Z[/red]");
        assert_eq!(c.spans().len(), 1);
        assert_eq!(c.spans()[0].start, 0);
        assert_eq!(c.spans()[0].end, 2);
        assert_eq!(c.spans()[0].span_style, SpanStyle::Raw("b".to_string()));
    }

    /// Tag bodies are NOT substituted; only text tokens. `[$primary]` stays raw.
    #[test]
    fn test_from_markup_with_vars_tag_bodies_not_substituted() {
        let v = vars(&[("primary", "red")]);
        let c = Content::from_markup_with_vars("[$primary]$primary[/]", &v);
        // The text token `$primary` is substituted; the tag body `$primary` is not.
        assert_eq!(c.plain(), "red");
        assert_eq!(c.spans().len(), 1);
        assert_eq!(
            c.spans()[0].span_style,
            SpanStyle::Raw("$primary".to_string())
        );
    }

    /// No-`[` markup with variables still substitutes (Python falls into
    /// `to_content` whenever variables are present).
    #[test]
    fn test_from_markup_with_vars_no_tags() {
        let v = vars(&[("who", "world")]);
        let c = Content::from_markup_with_vars("hello $who", &v);
        assert_eq!(c.plain(), "hello world");
        assert!(c.spans().is_empty());
    }

    /// Empty variables map + no `[` → plain-text fast path (no substitution).
    #[test]
    fn test_from_markup_with_vars_empty_map_plain() {
        let v = vars(&[]);
        let c = Content::from_markup_with_vars("hello $who", &v);
        // No variables → `$who` left untouched.
        assert_eq!(c.plain(), "hello $who");
    }

    /// Substitution shifts span offsets correctly (positions tracked post-subst).
    #[test]
    fn test_from_markup_with_vars_span_offsets_after_substitution() {
        let v = vars(&[("name", "Alexander")]);
        let c = Content::from_markup_with_vars("$name [b]X[/b]", &v);
        assert_eq!(c.plain(), "Alexander X");
        assert_eq!(c.spans().len(), 1);
        // "X" sits after "Alexander " (10 bytes).
        assert_eq!(c.spans()[0].start, 10);
        assert_eq!(c.spans()[0].end, 11);
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
        assert_eq!(
            c.plain(),
            "test",
            "unknown tag must be consumed, not emitted as literal"
        );
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
        let c = Content::assemble(vec![ContentPart::from("say: "), ContentPart::from(inner)]);
        assert_eq!(c.plain(), "say: hi");
        assert_eq!(c.spans().len(), 1);
        // Span should be offset by len("say: ") = 5
        assert_eq!(c.spans()[0].start, 5);
        assert_eq!(c.spans()[0].end, 7);
    }

    // --- cell_length (now &self via OnceLock) ---

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

    /// C1 seam 2: vertical fill rows carry the full style (fg + bg, reverse=false).
    ///
    /// Python `Visual.to_strips` uses `(style + Style(reverse=False)).rich_style`
    /// for fill rows — NOT bg-only.  This test ensures the Rust implementation
    /// matches: fill rows carry fg and bg from visual_style, with reverse=false.
    #[test]
    fn test_render_strips_vertical_fill_full_style() {
        let red = crate::style::Color::rgb(200, 0, 0);
        let blue = crate::style::Color::rgb(0, 0, 200);
        // Build visual_style with both fg and bg, plus reverse=true (which fill rows
        // must override to false per Python semantics).
        let visual = Style::new().fg(blue).bg(red).reverse(true);
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
        assert_eq!(
            strips.len(),
            3,
            "should produce 3 rows (1 content + 2 fill)"
        );
        // Fill rows (index 1 and 2) must carry fg AND bg (full style, not bg-only).
        for fill_row in &strips[1..] {
            for seg in fill_row {
                let fg = seg.style.as_ref().and_then(|s| s.color);
                assert!(
                    fg.is_some(),
                    "vertical fill row must carry fg (C1 seam 2); got None"
                );
                assert_eq!(
                    fg.unwrap(),
                    rich_rs::SimpleColor::Rgb { r: 0, g: 0, b: 200 },
                    "fill row fg must match visual_style fg"
                );
                let bg = seg.style.as_ref().and_then(|s| s.bgcolor);
                assert!(bg.is_some(), "vertical fill row must carry bg");
                // reverse must be forced to false (Python: Style(reverse=False) wins).
                let rev = seg.style.as_ref().and_then(|s| s.reverse);
                assert_eq!(
                    rev,
                    Some(false),
                    "fill row reverse must be false (not inherited true); got {:?}",
                    rev
                );
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
        assert_eq!(
            bold,
            Some(true),
            "bold span must produce bold=true in segment"
        );
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

    /// C1 seam 1: `line_pad` spaces are content runs (part of the Content text
    /// produced by `wrap_and_format`), not alignment-pad segments.  Python's
    /// `Content.render()` yields them with `base_style` applied, and
    /// `_FormattedLine.to_strip` wraps them in `(style + text_style).rich_style` —
    /// so they DO carry fg from the visual_style.
    ///
    /// This test verifies via a span-boundary approach: a span covering only the
    /// word ("hi") forces the leading/trailing spaces to be separate segments.
    /// Those space-only segments must carry fg.
    ///
    /// This is distinct from alignment-pad segments (`pad_left` / `pad_right`
    /// produced by `make_bg_segment`) which are bg-only.
    #[test]
    fn test_render_strips_line_pad_carries_fg() {
        let blue = crate::style::Color::rgb(0, 0, 200);
        // Use a span on "hi" — this forces the pad spaces to be emitted as
        // separate segments (before/after the span boundary).
        let bold = Style::new().bold(true);
        let c = Content::styled("hi", bold);
        // width=6, line_pad=1 → wrap_and_format produces " hi " (1+2+1 = 4 cells),
        // with a span covering "hi" at byte 1..3 (after pad_left(1)).
        let visual = Style::new().fg(blue);
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
        assert!(
            full_text.starts_with(' '),
            "line_pad left space should be present"
        );
        assert!(
            full_text.ends_with(' '),
            "line_pad right space should be present"
        );
        // Content-run spaces (line_pad) carry fg from visual_style (C1 seam 1 fix).
        // The span boundary forces separate segments; look for space-only segments
        // with fg set.
        let has_fg_space = strips[0].iter().any(|seg| {
            seg.text.chars().all(|c| c == ' ')
                && !seg.text.is_empty()
                && seg.style.as_ref().and_then(|s| s.color).is_some()
        });
        assert!(
            has_fg_space,
            "line_pad content spaces must carry fg (C1 seam 1); no fg-bearing space found. \
             Segments: {:?}",
            strips[0]
                .iter()
                .map(|s| (&s.text, s.style.as_ref().and_then(|st| st.color)))
                .collect::<Vec<_>>()
        );
    }

    // =========================================================================
    // C1 seam regression tests (SEAM 1 + SEAM 2)
    // =========================================================================

    /// SEAM 1 regression: `reverse` on a whitespace-only content span is PRESERVED.
    ///
    /// Python `_FormattedLine.to_strip` applies `(style + text_style).rich_style`
    /// to ALL content runs — including whitespace.  A span covering only spaces
    /// but styled with `reverse=true` must have that attribute in the output segment.
    ///
    /// Prior (incorrect) behavior: the `has_glyph` guard dropped fg/attributes on
    /// whitespace-only runs.  This test pins the fix.
    #[test]
    fn test_seam1_whitespace_span_reverse_preserved() {
        // Construct content where a span covers only a space and sets reverse=true.
        // "a b" with span covering byte 1 (the " ") with reverse=true.
        let reverse_style = Style::new().reverse(true);
        let c = Content::assemble(vec![
            ContentPart::from("a"),
            ContentPart::from((" ", reverse_style.clone())),
            ContentPart::from("b"),
        ]);
        assert_eq!(c.plain(), "a b");

        let strips = c.render_strips(
            3,
            Some(1),
            &Style::new(),
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 1);

        // Find the segment for the space character.
        let space_seg = strips[0].iter().find(|seg| seg.text == " ");
        assert!(
            space_seg.is_some(),
            "expected a separate segment for the space"
        );
        let space_seg = space_seg.unwrap();
        let rev = space_seg.style.as_ref().and_then(|s| s.reverse);
        assert_eq!(
            rev,
            Some(true),
            "reverse on whitespace-only span must be preserved (C1 seam 1 fix); got {:?}",
            rev
        );
    }

    /// SEAM 1 regression: `underline` on a whitespace-only content span is PRESERVED.
    ///
    /// Same invariant as the reverse test above — text attributes on whitespace
    /// content runs must be present in the output.
    #[test]
    fn test_seam1_whitespace_span_underline_preserved() {
        let underline_style = Style::new().underline(true);
        let c = Content::assemble(vec![
            ContentPart::from("x"),
            ContentPart::from(("   ", underline_style.clone())),
            ContentPart::from("y"),
        ]);
        assert_eq!(c.plain(), "x   y");

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

        // Find a segment that is entirely spaces (from the span).
        let space_seg = strips[0]
            .iter()
            .find(|seg| seg.text.chars().all(|c| c == ' ') && !seg.text.is_empty());
        assert!(
            space_seg.is_some(),
            "expected a segment for the underlined spaces"
        );
        let space_seg = space_seg.unwrap();
        let underline = space_seg.style.as_ref().and_then(|s| s.underline);
        assert_eq!(
            underline,
            Some(true),
            "underline on whitespace-only span must be preserved (C1 seam 1 fix); got {:?}",
            underline
        );
    }

    /// SEAM 2 regression: vertical fill surface matches Python — full style with
    /// reverse forced to false.
    ///
    /// Python `Visual.to_strips` computes:
    ///   `rich_style = (style + Style(reverse=False)).rich_style`
    /// for fill rows.  This means fg + bg ARE present; reverse is forced off.
    ///
    /// Prior (incorrect) behavior: fill rows were bg-only.
    #[test]
    fn test_seam2_vertical_fill_full_style_reverse_false() {
        let red = crate::style::Color::rgb(200, 0, 0);
        let blue = crate::style::Color::rgb(0, 0, 200);
        // visual_style has fg=blue, bg=red, reverse=true.
        let visual = Style::new().fg(blue).bg(red).reverse(true);
        let c = Content::from_text("hi");
        let strips = c.render_strips(
            5,
            Some(3), // 1 content row + 2 fill rows
            &visual,
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 3);

        // Fill rows at index 1 and 2.
        for (row_i, fill_row) in strips[1..].iter().enumerate() {
            assert!(
                !fill_row.is_empty(),
                "fill row {} must not be empty",
                row_i + 1
            );
            for seg in fill_row {
                // fg must be present (full style, not bg-only).
                let fg = seg.style.as_ref().and_then(|s| s.color);
                assert!(
                    fg.is_some(),
                    "fill row {} must carry fg (C1 seam 2); got None",
                    row_i + 1
                );
                assert_eq!(
                    fg.unwrap(),
                    rich_rs::SimpleColor::Rgb { r: 0, g: 0, b: 200 },
                    "fill row fg must equal visual_style fg"
                );
                // bg must be present.
                let bg = seg.style.as_ref().and_then(|s| s.bgcolor);
                assert!(bg.is_some(), "fill row must carry bg");
                // reverse must be forced to false (not inherited from visual_style).
                let rev = seg.style.as_ref().and_then(|s| s.reverse);
                assert_eq!(
                    rev,
                    Some(false),
                    "fill row reverse must be false even when visual_style has reverse=true \
                     (C1 seam 2); got {:?}",
                    rev
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

    // --- @click meta survives parse → resolve → render_strips -----------------

    /// Read the string value of a segment-meta key, if present.
    fn seg_meta_str(seg: &rich_rs::Segment, key: &str) -> Option<String> {
        seg.meta
            .as_ref()
            .and_then(|m| m.meta.as_ref())
            .and_then(|map| map.get(key))
            .and_then(|v| match v {
                rich_rs::MetaValue::Str(s) => Some(s.to_string()),
                _ => None,
            })
    }

    #[test]
    fn render_strips_stamps_click_action_meta() {
        // A `[@click=action]` span must bake the action string into the
        // rendered segment's meta so the runtime can hit-test and dispatch it.
        let c = Content::from_markup("[@click=app.bell]Ring[/]");
        // The span carries the @click meta even before resolution.
        let click_span = c
            .spans()
            .iter()
            .find(|s| s.meta.iter().any(|(k, _)| k == "@click"));
        assert!(click_span.is_some(), "span should carry @click meta");

        let strips = c.render_strips(
            20,
            None,
            &Style::new(),
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        assert_eq!(strips.len(), 1);
        // Some segment over the "Ring" glyphs must carry @click=app.bell.
        let found = strips[0]
            .iter()
            .find_map(|seg| seg_meta_str(seg, "@click"));
        assert_eq!(found.as_deref(), Some("app.bell"));
    }

    #[test]
    fn render_strips_click_meta_only_covers_clickable_span() {
        // Text outside the @click span must NOT carry the meta.
        let c = Content::from_markup("plain [@click=do_it]link[/] tail");
        let strips = c.render_strips(
            40,
            None,
            &Style::new(),
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        let strip = &strips[0];
        // Reconstruct (text, has_click) per segment and assert the clickable
        // run is exactly "link".
        let mut clickable = String::new();
        for seg in strip {
            if seg_meta_str(seg, "@click").as_deref() == Some("do_it") {
                clickable.push_str(seg.text.as_ref());
            }
        }
        assert_eq!(clickable, "link");
    }

    #[test]
    fn click_action_meta_survives_arguments_with_spaces() {
        // Action args with quoted, comma+space-separated values must be kept
        // intact in the @click meta (paren-aware tokenizing).
        let c = Content::from_markup("[@click=set_background('cyan')]Cyan[/]");
        let strips = c.render_strips(
            20,
            None,
            &Style::new(),
            crate::style::TextAlign::Left,
            "fold",
            false,
            0,
            null_resolver,
        );
        let found = strips[0]
            .iter()
            .find_map(|seg| seg_meta_str(seg, "@click"));
        assert_eq!(found.as_deref(), Some("set_background('cyan')"));
    }
}

