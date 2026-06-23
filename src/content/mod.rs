//! Textual `Content` subsystem — Phase A (data type + markup parser).
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
//! ## NOT in Phase A
//! - Wrap/format (`_wrap_and_format`, `wrap`, `fold`).
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
}
