//! Textual markup parser: converts `[bold]text[/bold]`, `[link=url]`, `[@click=...]`
//! into a list of [`super::Span`] values on top of plain text.
//!
//! This mirrors `textual/markup.py`'s `_to_content` function semantics, but is a
//! clean Rust implementation — **not** a wrapper around rich-rs markup.
//!
//! ## Design rules (from CONTENT_LAYER_KEYSTONE.md)
//! - `[link=url]` carries link META only — **no** visual cyan/underline.
//!   (That was the bug we just removed from rich-rs; Textual applies link visuals
//!   via Theme, not the parser.)
//! - Visual style tokens (`bold`, `italic`, `b`, `i`, colors, `on <bg>`, …) are
//!   parsed into a `crate::style::Style` and stored in the span's style field.
//! - `[@click=...]` / `[key=value]` key-value pairs are stored as meta on
//!   [`SpanMeta`] so they are not lost, but do not contribute visual style.
//! - Unrecognised / unparsable tags are emitted as literal text (same as Python).

use crate::style::{Color, Style, parse_color_like};

// ---------------------------------------------------------------------------
// Style-relevant token kinds that can appear inside a markup tag
// ---------------------------------------------------------------------------

/// True style attributes that contribute to visual rendering.
const STYLE_KEYWORDS: &[(&str, &str)] = &[
    // Full names
    ("bold", "bold"),
    ("dim", "dim"),
    ("italic", "italic"),
    ("underline", "underline"),
    ("underline2", "underline2"),
    ("reverse", "reverse"),
    ("strike", "strike"),
    ("blink", "blink"),
    // Abbreviations
    ("b", "bold"),
    ("d", "dim"),
    ("i", "italic"),
    ("u", "underline"),
    ("uu", "underline2"),
    ("r", "reverse"),
    ("s", "strike"),
];

fn resolve_style_keyword(token: &str) -> Option<&'static str> {
    STYLE_KEYWORDS
        .iter()
        .find(|(k, _)| *k == token)
        .map(|(_, v)| *v)
}

// ---------------------------------------------------------------------------
// Tag-body → Style
// ---------------------------------------------------------------------------

/// Parse the body of a markup tag (the part between `[` and `]`) into a
/// `Style`.  Key-value attributes like `link=url` or `@click=action` are
/// returned separately via `meta` so callers can attach them without mixing
/// with visual style.
///
/// Returns `None` if the tag body cannot be meaningfully parsed (which means
/// the surrounding `[ ]` should be treated as literal text).
pub(crate) fn parse_tag_style(tag_body: &str) -> Option<ParsedTag> {
    let tag = tag_body.trim();
    if tag.is_empty() {
        return None;
    }

    let mut style = Style::new();
    let mut meta: Vec<(String, String)> = Vec::new();
    let mut is_background = false;
    let mut pending_color: Option<Color> = None;
    let mut had_style = false; // did we consume at least one token?

    // Tokenise: split on whitespace, but keep `key=value` pairs intact
    let tokens: Vec<&str> = tag.split_whitespace().collect();
    let mut i = 0;

    while i < tokens.len() {
        let token = tokens[i];
        i += 1;

        // --- key=value attribute (link=url, @click=action, etc.) ---
        if let Some(eq) = token.find('=') {
            let key = &token[..eq];
            let value = token[eq + 1..].trim_matches('"').trim_matches('\'');
            meta.push((key.to_string(), value.to_string()));
            had_style = true;
            continue;
        }

        // --- "not" modifier → next token is negated ---
        if token == "not" {
            // Python toggles style_state to false; we apply it to next token.
            // Simplified: we skip the next token's effect by consuming it without
            // applying to style. In practice Textual markup rarely uses "not".
            if i < tokens.len() {
                i += 1; // skip next token (disabled style)
            }
            had_style = true;
            continue;
        }

        // --- "on" → next color is background ---
        if token == "on" {
            is_background = true;
            had_style = true;
            continue;
        }

        // --- "auto" color ---
        if token == "auto" {
            // auto fg/bg — we don't set a concrete Color; leave the field as None.
            // This is consistent with how Python Textual handles Color.automatic()
            // at the span level (it's resolved at render time).
            had_style = true;
            is_background = false;
            continue;
        }

        // --- "link" bare keyword → link meta with empty url ---
        if token == "link" {
            meta.push(("link".to_string(), String::new()));
            had_style = true;
            continue;
        }

        // --- visual style keyword ---
        if let Some(_canonical) = resolve_style_keyword(token) {
            apply_style_keyword(&mut style, _canonical);
            had_style = true;
            continue;
        }

        // --- color: try parse_color_like (handles hex, rgb(), named, $token) ---
        // But first check for a percentage suffix on the *previous* pending color:
        // `red 10%` means red with alpha 10%.
        if let Some(stripped) = token.strip_suffix('%') {
            if let Ok(percent) = stripped.parse::<f32>() {
                let factor = (percent / 100.0).clamp(0.0, 1.0);
                if let Some(c) = pending_color.take() {
                    let c = c.with_alpha(factor);
                    if is_background {
                        style = style.bg(c);
                        is_background = false;
                    } else {
                        style = style.fg(c);
                    }
                    had_style = true;
                    continue;
                }
            }
        }

        if let Some(color) = parse_color_like(token) {
            // Commit any pending color first (no trailing %)
            if let Some(c) = pending_color.take() {
                if is_background {
                    style = style.bg(c);
                    is_background = false;
                } else {
                    style = style.fg(c);
                }
            }
            pending_color = Some(color);
            had_style = true;
            continue;
        }

        // --- unrecognised token → the whole tag is literal text ---
        return None;
    }

    // Flush any pending color
    if let Some(c) = pending_color.take() {
        if is_background {
            style = style.bg(c);
        } else {
            style = style.fg(c);
        }
    }

    if !had_style {
        return None;
    }

    Some(ParsedTag { style, meta })
}

/// Apply a canonical style keyword string to the style builder.
fn apply_style_keyword(style: &mut Style, keyword: &str) {
    match keyword {
        "bold" => *style = std::mem::take(style).bold(true),
        "dim" => *style = std::mem::take(style).dim(true),
        "italic" => *style = std::mem::take(style).italic(true),
        "underline" | "underline2" => *style = std::mem::take(style).underline(true),
        "reverse" => *style = std::mem::take(style).reverse(true),
        "strike" => *style = std::mem::take(style).strike(true),
        // blink is not modelled in our Style struct — ignore silently
        _ => {}
    }
}

/// Result of parsing a markup tag body.
#[derive(Debug, Clone)]
pub(crate) struct ParsedTag {
    /// The visual style portion.
    pub style: Style,
    /// Non-visual key=value metadata (link=url, @click=action, etc.)
    pub meta: Vec<(String, String)>,
}

// ---------------------------------------------------------------------------
// Normalise a tag body for closing-tag matching (Python _normalize_markup_tag)
// ---------------------------------------------------------------------------

/// Normalise a tag body so that `[b]…[/b]` and `[bold]…[/bold]` both close.
/// The Python side does `Style._normalize_markup_tag(opening_tag.strip())`.
/// We use the same approach: lowercase + expand abbreviations, strip metadata.
pub(crate) fn normalize_tag(tag: &str) -> String {
    let tag = tag.trim();

    // Key=value attribute → use just the key as the canonical id (matches Python)
    if let Some(eq) = tag.find('=') {
        return tag[..eq].to_lowercase();
    }

    // Multi-token tag: build a canonical string by normalising each token
    let parts: Vec<String> = tag
        .split_whitespace()
        .map(|tok| {
            if let Some((_k, v)) = STYLE_KEYWORDS.iter().find(|(k, _)| *k == tok) {
                v.to_string()
            } else {
                tok.to_lowercase()
            }
        })
        .collect();

    parts.join(" ")
}

// ---------------------------------------------------------------------------
// Main markup → (text, spans) parser
// ---------------------------------------------------------------------------

/// Span with style AND optional meta key-value pairs.
#[derive(Debug, Clone)]
pub(crate) struct RawSpan {
    pub start: usize,
    pub end: usize,
    pub style: Style,
    /// Non-visual metadata (link=url, @click=action, ...).
    /// Populated by the parser; consumed in Phase B/C when link-click handling is wired.
    #[allow(dead_code)]
    pub meta: Vec<(String, String)>,
}

/// Parse Textual markup into `(plain_text, spans)`.
///
/// Behaviour mirrors `_to_content` in `textual/markup.py`:
/// - `[bold]text[/bold]` → Span covering "text" with bold style.
/// - `[link=url]text[/link]` → Span with link in meta, no visual style change.
/// - `[@click=action]text[/]` → Span with @click in meta.
/// - Unrecognisable or unparsable tags are emitted as literal text.
/// - `\[` → literal `[` (escape).
/// - Auto-closing unclosed opening tags at end of input.
pub(crate) fn parse_markup(markup: &str) -> (String, Vec<RawSpan>) {
    if !markup.contains('[') {
        // Fast path: no markup at all
        return (markup.to_string(), Vec::new());
    }

    let chars: &[u8] = markup.as_bytes();
    let len = chars.len();

    let mut text = String::with_capacity(markup.len());
    let mut spans: Vec<RawSpan> = Vec::new();

    // Stack of (byte_position_in_text, original_tag_body, normalized_tag_body, parsed_tag)
    let mut style_stack: Vec<(usize, String, String, ParsedTag)> = Vec::new();

    let mut i = 0;
    while i < len {
        // Escaped `\[` → emit literal `[`
        if chars[i] == b'\\' && i + 1 < len && chars[i + 1] == b'[' {
            text.push('[');
            i += 2;
            continue;
        }

        // Opening `[`
        if chars[i] == b'[' {
            // Check for closing tag `[/`
            let is_closing = i + 1 < len && chars[i + 1] == b'/';
            let tag_start = if is_closing { i + 2 } else { i + 1 };

            // Find matching `]`
            if let Some(close_pos) = find_close_bracket(markup, tag_start) {
                let tag_body = &markup[tag_start..close_pos];
                i = close_pos + 1; // advance past `]`

                if is_closing {
                    // Closing tag `[/tag]` or `[/]`
                    let closing = tag_body.trim();
                    if closing.is_empty() {
                        // `[/]` → auto-close the most recent open tag
                        if let Some((tag_pos, _orig_body, _norm, parsed)) = style_stack.pop() {
                            let current_pos = text.len();
                            if tag_pos != current_pos {
                                spans.push(RawSpan {
                                    start: tag_pos,
                                    end: current_pos,
                                    style: parsed.style,
                                    meta: parsed.meta,
                                });
                            }
                        }
                        // (If nothing to close, silently ignore — matches Python)
                    } else {
                        let norm_closing = normalize_tag(closing);
                        // Find matching open tag (most recent first)
                        let stack_len = style_stack.len();
                        let mut found = false;
                        for rev_idx in 0..stack_len {
                            let stack_idx = stack_len - 1 - rev_idx;
                            if style_stack[stack_idx].2 == norm_closing {
                                let (tag_pos, _orig, _norm, parsed) =
                                    style_stack.remove(stack_idx);
                                let current_pos = text.len();
                                if tag_pos != current_pos {
                                    spans.push(RawSpan {
                                        start: tag_pos,
                                        end: current_pos,
                                        style: parsed.style,
                                        meta: parsed.meta,
                                    });
                                }
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            // Unmatched closing tag → emit as literal text
                            let literal = format!("[/{closing}]");
                            text.push_str(&literal);
                        }
                    }
                } else {
                    // Opening tag
                    let tag_trimmed = tag_body.trim();
                    if tag_trimmed.is_empty() {
                        // Empty tag `[ ]` or `[]` → literal
                        let literal = format!("[{tag_body}]");
                        text.push_str(&literal);
                    } else {
                        match parse_tag_style(tag_trimmed) {
                            Some(parsed) => {
                                let norm = normalize_tag(tag_trimmed);
                                let pos = text.len();
                                style_stack.push((pos, tag_trimmed.to_string(), norm, parsed));
                            }
                            None => {
                                // Unrecognised tag → literal text
                                let literal = format!("[{tag_body}]");
                                text.push_str(&literal);
                            }
                        }
                    }
                }
            } else {
                // No closing `]` found → emit `[` literally and continue
                text.push('[');
                i += 1;
            }
            continue;
        }

        // Regular character
        // SAFETY: we've checked ASCII `[` and `\`, but push char by char via str indexing
        let ch = &markup[i..i + char_len_at(markup, i)];
        text.push_str(ch);
        i += ch.len();
    }

    // Auto-close any unclosed opening tags (Python does this at end-of-input)
    let text_len = text.len();
    if text_len > 0 {
        // Process in reverse (innermost first), but Python reverses then appends
        // and then sort-by-start. We replicate: collect unclosed spans, then sort.
        for (tag_pos, _orig, _norm, parsed) in style_stack.into_iter().rev() {
            if tag_pos != text_len {
                spans.push(RawSpan {
                    start: tag_pos,
                    end: text_len,
                    style: parsed.style,
                    meta: parsed.meta,
                });
            }
        }
    }

    // Sort spans by start position (Python: `spans.sort(key=itemgetter(0))`)
    spans.sort_by_key(|s| s.start);

    (text, spans)
}

/// Find the matching `]` for a tag starting at `start` (after the `[` or `[/`).
/// Returns `None` if no closing bracket is found.
fn find_close_bracket(s: &str, start: usize) -> Option<usize> {
    // Simple scan: find the first unescaped `]`
    let bytes = s.as_bytes();
    let mut j = start;
    while j < bytes.len() {
        if bytes[j] == b'\\' && j + 1 < bytes.len() && bytes[j + 1] == b']' {
            j += 2;
            continue;
        }
        if bytes[j] == b']' {
            return Some(j);
        }
        j += 1;
    }
    None
}

/// Return the byte length of the UTF-8 character starting at `pos` in `s`.
fn char_len_at(s: &str, pos: usize) -> usize {
    s[pos..].chars().next().map_or(1, |c| c.len_utf8())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_markup() {
        let (text, spans) = parse_markup("hello world");
        assert_eq!(text, "hello world");
        assert!(spans.is_empty());
    }

    #[test]
    fn test_bold_tag() {
        let (text, spans) = parse_markup("[bold]hello[/bold]");
        assert_eq!(text, "hello");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 5);
        assert_eq!(spans[0].style.bold, Some(true));
    }

    #[test]
    fn test_abbreviation_b() {
        let (text, spans) = parse_markup("[b]hello[/b]");
        assert_eq!(text, "hello");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style.bold, Some(true));
    }

    #[test]
    fn test_italic_tag() {
        let (text, spans) = parse_markup("[italic]world[/italic]");
        assert_eq!(text, "world");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style.italic, Some(true));
    }

    #[test]
    fn test_link_no_visual_style() {
        // [link=url] must carry meta only — no visual cyan/underline
        let (text, spans) = parse_markup("[link=https://example.com]click me[/link]");
        assert_eq!(text, "click me");
        assert_eq!(spans.len(), 1);
        // Visual style should be default (no fg/bold/underline from the link tag)
        let s = &spans[0].style;
        assert!(s.fg.is_none(), "link tag must not set fg color");
        assert!(s.underline.is_none(), "link tag must not set underline");
        // Meta should contain the link url
        let link_meta = spans[0].meta.iter().find(|(k, _)| k == "link");
        assert!(link_meta.is_some(), "link meta must be present");
        assert_eq!(link_meta.unwrap().1, "https://example.com");
    }

    #[test]
    fn test_at_click_meta() {
        let (text, spans) = parse_markup("[@click=my_action]click[/]");
        assert_eq!(text, "click");
        assert_eq!(spans.len(), 1);
        let click_meta = spans[0].meta.iter().find(|(k, _)| k == "@click");
        assert!(click_meta.is_some());
        assert_eq!(click_meta.unwrap().1, "my_action");
        // No visual style from @click
        assert!(spans[0].style.fg.is_none());
        assert!(spans[0].style.bold.is_none());
    }

    #[test]
    fn test_auto_close_at_end() {
        // Unclosed tag should be auto-closed at end of text
        let (text, spans) = parse_markup("[bold]unclosed");
        assert_eq!(text, "unclosed");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 8);
        assert_eq!(spans[0].style.bold, Some(true));
    }

    #[test]
    fn test_escaped_bracket() {
        let (text, spans) = parse_markup(r"hello \[ world");
        assert_eq!(text, "hello [ world");
        assert!(spans.is_empty());
    }

    #[test]
    fn test_nested_spans() {
        let (text, spans) = parse_markup("[bold]hel[italic]lo[/italic][/bold]");
        assert_eq!(text, "hello");
        // bold span covers 0..5, italic span covers 3..5
        let bold_span = spans.iter().find(|s| s.style.bold == Some(true));
        let italic_span = spans.iter().find(|s| s.style.italic == Some(true));
        assert!(bold_span.is_some(), "bold span missing");
        assert!(italic_span.is_some(), "italic span missing");
        assert_eq!(bold_span.unwrap().start, 0);
        assert_eq!(bold_span.unwrap().end, 5);
        assert_eq!(italic_span.unwrap().start, 3);
        assert_eq!(italic_span.unwrap().end, 5);
    }

    #[test]
    fn test_color_tag() {
        let (text, spans) = parse_markup("[red]error[/red]");
        assert_eq!(text, "error");
        assert_eq!(spans.len(), 1);
        assert!(spans[0].style.fg.is_some(), "red tag should set fg color");
        // Red = #800000 or similar from named colors
    }

    #[test]
    fn test_unrecognised_tag_is_literal() {
        // An unrecognised tag like [foobar] should appear as literal text
        // (Python emits the bracket content as text when it can't parse the tag)
        // Note: parse_markup only returns None for unrecognisable single-token names,
        // so this depends on parse_tag_style returning None for "foobar"
        // In our implementation, unknown non-color tokens return None from parse_tag_style
        // which means the tag is emitted as literal text.
        let (text, spans) = parse_markup("[foobar]test");
        // "foobar" is not a color or style keyword, so it becomes literal
        assert_eq!(text, "[foobar]test");
        assert!(spans.is_empty());
    }

    #[test]
    fn test_mixed_text_and_markup() {
        let (text, spans) = parse_markup("Hello, [bold]world[/bold]!");
        assert_eq!(text, "Hello, world!");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 7);
        assert_eq!(spans[0].end, 12);
    }

    #[test]
    fn test_auto_close_slash() {
        // [/] should auto-close the most recent open tag
        let (text, spans) = parse_markup("[bold]hello[/]");
        assert_eq!(text, "hello");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style.bold, Some(true));
    }

    #[test]
    fn test_on_background_color() {
        let (text, spans) = parse_markup("[on red]text[/]");
        assert_eq!(text, "text");
        assert_eq!(spans.len(), 1);
        // "on red" should set bg, not fg
        assert!(spans[0].style.bg.is_some(), "on <color> should set bg");
        assert!(spans[0].style.fg.is_none(), "on <color> must not set fg");
    }

    #[test]
    fn test_normalize_tag() {
        assert_eq!(normalize_tag("b"), "bold");
        assert_eq!(normalize_tag("bold"), "bold");
        assert_eq!(normalize_tag("link=url"), "link");
        assert_eq!(normalize_tag("  italic  "), "italic");
    }
}
