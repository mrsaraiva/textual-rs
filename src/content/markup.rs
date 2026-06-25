//! Textual markup parser: converts `[bold]text[/bold]`, `[link=url]`, `[@click=...]`
//! into a list of raw spans (with deferred style resolution) on top of plain text.
//!
//! This mirrors `textual/markup.py`'s `_to_content` function semantics, but is a
//! clean Rust implementation — **not** a wrapper around rich-rs markup.
//!
//! ## Design rules (from CONTENT_LAYER_KEYSTONE.md and Python fidelity review)
//!
//! ### Deferred resolution (Python-faithful)
//! Python's `Span.style` is `Style | str` — the raw tag body (e.g. `"bold"`,
//! `"red on blue"`, `"foobar"`, `"link=url"`) is stored as a string and resolved
//! at **render time** with app/theme context.  `RawSpan.raw_tag` mirrors this.
//!
//! ### Unknown/partial-invalid tags are CONSUMED, not emitted as literal text
//! Python's `_to_content` pushes **every** non-empty, bracket-enclosed tag to the
//! style stack regardless of whether the tag body is a recognised style keyword.
//! The raw body is stored; at render time `Style.parse("foobar")` fails silently
//! and returns a null/transparent style.  Only genuinely unparsable bracket content
//! (e.g. `[foo bar baz]` where the tokenizer sees *text* tokens inside, meaning the
//! content cannot even be tokenised as a style expression) is emitted as literal
//! text.  In our simplified tokeniser (no full CSS tokeniser) we classify any
//! bracket-enclosed run of non-newline characters as a valid tag candidate, so ALL
//! `[tag_body]` forms with a matching `]` are consumed.
//!
//! ### `not` modifier
//! `[not bold]` sets `style_state = false` then applies it to `bold`, producing
//! `bold: false` in the resolved style.  The Rust parser mirrors this.
//!
//! ### `blink` keyword
//! `blink` is a valid Python style keyword stored raw like any other.  Our
//! `Style` struct does not have a `blink` field; if/when it does, `blink` will
//! apply automatically through the resolved parse path.
//!
//! - `[link=url]` carries link META only — **no** visual cyan/underline.
//!   (That was the bug we just removed from rich-rs; Textual applies link visuals
//!   via Theme, not the parser.)
//! - Visual style tokens (`bold`, `italic`, `b`, `i`, colors, `on <bg>`, …) are
//!   parsed into a `crate::style::Style` and stored in the span's style field at
//!   resolve time.
//! - `[@click=...]` / `[key=value]` key-value pairs are stored as meta on
//!   [`RawSpan`] so they are not lost during span manipulation.

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
// Tag-body → Style (used at resolve time, not at parse time)
// ---------------------------------------------------------------------------

/// Parse the body of a markup tag (the part between `[` and `]`) into a
/// `Style` + optional meta.
///
/// This function is called at **render / resolve time** (not at parse time).
/// It handles the same token grammar as Python's `parse_style`:
/// - `bold`, `italic`, etc. → set the respective style flag.
/// - `not bold` → set bold=false (style_state toggle).
/// - `on <color>` → background color.
/// - `link=url`, `@click=action` → key-value meta (no visual style).
/// - `auto` → leave color as None (resolved at Theme level).
/// - Unrecognised tokens → whole tag is null-style (no style set), consistent
///   with Python's `Style.parse("foobar")` returning a null style.
///
/// Returns `None` for genuinely empty tag bodies (which should have been
/// emitted as literal text at parse time).  Returns `Some(ParsedTag)` even
/// for fully unrecognised tags — the caller should use the null `style` in
/// that case.
pub(crate) fn parse_tag_style(tag_body: &str) -> Option<ParsedTag> {
    let tag = tag_body.trim();
    if tag.is_empty() {
        return None;
    }

    let mut style = Style::new();
    let mut is_background = false;
    let mut pending_color: Option<Color> = None;
    let mut had_style = false; // did we consume at least one token?
    let mut style_state = true; // false after "not" modifier; reset after applying

    // Tokenise: split on whitespace, but keep `key=value` pairs intact
    let tokens: Vec<&str> = tag.split_whitespace().collect();
    let mut i = 0;

    while i < tokens.len() {
        let token = tokens[i];
        i += 1;

        // --- key=value attribute (link=url, @click=action, etc.) ---
        // Key=value attributes affect only `RawSpan.meta` (populated directly
        // by `parse_markup`/`extract_meta_only`), not the visual style.
        if token.contains('=') {
            had_style = true;
            style_state = true; // reset after consuming the token
            continue;
        }

        // --- "not" modifier → next token is negated ---
        if token == "not" {
            // Python: style_state = False; next recognised token sets style[x]=False
            style_state = false;
            had_style = true;
            continue;
        }

        // --- "on" → next color is background ---
        if token == "on" {
            // Commit any pending color FIRST, in the slot that was active when it
            // was parsed (foreground unless a previous `on` already switched us).
            // `on` only redirects the *next* color to the background; it must not
            // retroactively move an already-parsed color (so `white on black`
            // assigns fg=white, bg=black, not the reverse).
            if let Some(c) = pending_color.take() {
                if is_background {
                    style = style.bg(c);
                } else {
                    style = style.fg(c);
                }
            }
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
            style_state = true;
            continue;
        }

        // --- "link" bare keyword → treated as had_style but no visual change ---
        // (link metadata with empty url lives in RawSpan.meta, not ParsedTag)
        if token == "link" {
            had_style = true;
            style_state = true;
            continue;
        }

        // --- visual style keyword ---
        if let Some(canonical) = resolve_style_keyword(token) {
            apply_style_keyword(&mut style, canonical, style_state);
            had_style = true;
            style_state = true; // reset after applying
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
                    style_state = true;
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
            style_state = true;
            continue;
        }

        // --- unrecognised token ---
        // Unlike the old behaviour (returning None here to emit literal text),
        // we now just note that we had an unrecognised token.  The tag is still
        // consumed; the resulting style will be null/transparent for this span.
        // This matches Python: Style.parse("foobar") returns a null Style.
        had_style = true;
        style_state = true;
        // NOTE: we do NOT break/return None here.  We continue parsing the rest
        // of the token list.  All tokens are consumed; unknown ones are no-ops.
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

    Some(ParsedTag { style })
}

/// Apply a canonical style keyword string to the style builder, respecting `state`.
/// When `state` is false (from a `not` modifier), the attribute is set to false.
fn apply_style_keyword(style: &mut Style, keyword: &str, state: bool) {
    match keyword {
        "bold" => *style = std::mem::take(style).bold(state),
        "dim" => *style = std::mem::take(style).dim(state),
        "italic" => *style = std::mem::take(style).italic(state),
        "underline" | "underline2" => *style = std::mem::take(style).underline(state),
        "reverse" => *style = std::mem::take(style).reverse(state),
        "strike" => *style = std::mem::take(style).strike(state),
        // blink is not modelled in our Style struct — ignore silently for now
        _ => {}
    }
}

/// Result of parsing a markup tag body.
#[derive(Debug, Clone)]
pub(crate) struct ParsedTag {
    /// The visual style portion.
    pub style: Style,
    // Note: non-visual key=value metadata (link=url, @click=action, etc.) is
    // NOT stored here; it is extracted directly into `RawSpan.meta` by
    // `parse_markup`/`extract_meta_only` at parse time.
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

/// Span with raw tag body AND optional meta key-value pairs.
///
/// The `raw_tag` field stores the exact tag body string (e.g. `"bold"`,
/// `"red on blue"`, `"foobar"`, `"link=url"`) — mirroring Python's
/// `Span.style: str`.  Resolution to a concrete `Style` happens at render
/// time via `parse_tag_style` (or the app's `parse_style` for theme context).
#[derive(Debug, Clone)]
pub(crate) struct RawSpan {
    pub start: usize,
    pub end: usize,
    /// Raw tag body, deferred for render-time resolution.
    pub raw_tag: String,
    /// Non-visual metadata (link=url, @click=action, ...).
    /// Populated by the parser and carried onto `content::Span::meta`, where it
    /// is stamped into rendered segment `StyleMeta` for `@click` hit-testing.
    pub meta: Vec<(String, String)>,
}

/// Parse Textual markup into `(plain_text, spans)`.
///
/// Behaviour mirrors `_to_content` in `textual/markup.py`:
/// - `[bold]text[/bold]` → RawSpan covering "text" with raw_tag="bold".
/// - `[foobar]text[/foobar]` → RawSpan with raw_tag="foobar" (null style at render).
/// - `[link=url]text[/link]` → RawSpan with link in meta, raw_tag="link=url".
/// - `[@click=action]text[/]` → RawSpan with @click in meta.
/// - Tags with genuine *text* content inside the brackets (unparsable by the
///   style tokeniser because they contain non-token characters) are emitted as
///   literal text, matching Python's "contains_text" branch.
/// - `\[` → literal `[` (escape).
/// - Auto-closing unclosed opening tags at end of input.
///
/// **Key Python-faithful change vs Phase A**: unknown tag bodies (e.g. "foobar")
/// are consumed and stored as raw spans — NOT emitted as literal `[foobar]` text.
pub(crate) fn parse_markup(markup: &str) -> (String, Vec<RawSpan>) {
    parse_markup_with_vars(markup, None)
}

/// Parse Textual markup, optionally performing `string.Template`-style variable
/// substitution over the **text** content (mirroring Python `Content.from_markup`
/// with `**variables`).
///
/// Substitution is applied per text token (the runs of plain text *between* tags),
/// exactly as Python's `_to_content` applies `process_text` to text tokens only.
/// Tag bodies (e.g. `[$primary]`) are **not** substituted — matching Python, where
/// only `token.name == "text"` values pass through `Template(...).safe_substitute`.
pub(crate) fn parse_markup_with_vars(
    markup: &str,
    template_variables: Option<&std::collections::HashMap<String, String>>,
) -> (String, Vec<RawSpan>) {
    if !markup.contains('[') {
        // No tags: the whole string is a single text token. Still substitute when
        // variables are provided (Python falls into `to_content` whenever variables
        // are present, even with no `[`).
        let substituted = substitute_text_token(markup, template_variables);
        return (substituted, Vec::new());
    }

    let chars: &[u8] = markup.as_bytes();
    let len = chars.len();

    let mut text = String::with_capacity(markup.len());
    let mut spans: Vec<RawSpan> = Vec::new();

    // Stack of (byte_position_in_text, raw_tag_body, normalized_tag_body)
    // We store the raw tag body — NOT a parsed Style — mirroring Python exactly.
    let mut style_stack: Vec<(usize, String, String)> = Vec::new();

    // Accumulator for the current contiguous text token. We substitute the whole
    // run at once when we reach a tag boundary or EOF, matching Python's
    // token-level `process_text`. This keeps `$name` substitution well-defined
    // across escaped brackets within a single text run.
    let mut pending_text = String::new();
    macro_rules! flush_pending {
        () => {
            if !pending_text.is_empty() {
                let substituted = substitute_text_token(&pending_text, template_variables);
                text.push_str(&substituted);
                pending_text.clear();
            }
        };
    }

    let mut i = 0;
    while i < len {
        // Escaped `\[` → emit literal `[` (still part of the current text token,
        // matching Python's `token.value.replace("\\[", "[")` before substitution)
        if chars[i] == b'\\' && i + 1 < len && chars[i + 1] == b'[' {
            pending_text.push('[');
            i += 2;
            continue;
        }

        // Opening `[`
        if chars[i] == b'[' {
            flush_pending!();
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
                        if let Some((tag_pos, raw_tag, _norm)) = style_stack.pop() {
                            let current_pos = text.len();
                            if tag_pos != current_pos {
                                // Extract meta from raw_tag for metadata-only tags
                                let meta = extract_meta_only(&raw_tag);
                                spans.push(RawSpan {
                                    start: tag_pos,
                                    end: current_pos,
                                    raw_tag,
                                    meta,
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
                                let (tag_pos, raw_tag, _norm) = style_stack.remove(stack_idx);
                                let current_pos = text.len();
                                if tag_pos != current_pos {
                                    let meta = extract_meta_only(&raw_tag);
                                    spans.push(RawSpan {
                                        start: tag_pos,
                                        end: current_pos,
                                        raw_tag,
                                        meta,
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
                        // Empty tag `[ ]` or `[]` → literal (matches Python "blank tag")
                        let literal = format!("[{tag_body}]");
                        text.push_str(&literal);
                    } else if contains_literal_text(tag_trimmed) {
                        // Tag body contains characters that the style tokeniser would
                        // see as text tokens (e.g. embedded `[` or control chars) —
                        // emit as literal text, matching Python's "contains_text" branch.
                        let literal = format!("[{tag_body}]");
                        text.push_str(&literal);
                    } else {
                        // Valid tag candidate: push to stack with raw body.
                        // We do NOT pre-parse the style here — defer to render time.
                        let norm = normalize_tag(tag_trimmed);
                        let pos = text.len();
                        style_stack.push((pos, tag_trimmed.to_string(), norm));
                    }
                }
            } else {
                // No closing `]` found → emit `[` literally and continue
                text.push('[');
                i += 1;
            }
            continue;
        }

        // Regular character → accumulate into the current text token.
        let ch = &markup[i..i + char_len_at(markup, i)];
        pending_text.push_str(ch);
        i += ch.len();
    }

    // Flush any trailing text token before auto-closing spans.
    flush_pending!();

    // Auto-close any unclosed opening tags (Python does this at end-of-input)
    let text_len = text.len();
    if text_len > 0 {
        for (tag_pos, raw_tag, _norm) in style_stack.into_iter().rev() {
            if tag_pos != text_len {
                let meta = extract_meta_only(&raw_tag);
                spans.push(RawSpan {
                    start: tag_pos,
                    end: text_len,
                    raw_tag,
                    meta,
                });
            }
        }
    }

    // Sort spans by start position (Python: `spans.sort(key=itemgetter(0))`)
    spans.sort_by_key(|s| s.start);

    (text, spans)
}

/// Apply `string.Template.safe_substitute` semantics to a single text token.
///
/// Mirrors CPython `string.Template` with its default `delimiter = '$'` and
/// `idpattern = (?a:[_a-z][_a-z0-9]*)` (ASCII, case-insensitive matching):
///
/// - `$$` → literal `$` (escape)
/// - `$name` / `${name}` → replaced by `variables["name"]` when present
/// - identifier = `[A-Za-z_][A-Za-z0-9_]*` (ASCII only)
/// - unknown keys are left **unmodified** (safe substitution — no error)
/// - a `$` not followed by `$`, a valid identifier, or `{ident}` is left as-is
/// - dict lookup is exact-case (`$name` does not match key `Name`)
///
/// When `variables` is `None`, the token is returned unchanged (no allocation
/// when there is nothing to substitute).
fn substitute_text_token(
    token: &str,
    variables: Option<&std::collections::HashMap<String, String>>,
) -> String {
    let Some(variables) = variables else {
        return token.to_string();
    };
    // Fast path: no delimiter present.
    if !token.contains('$') {
        return token.to_string();
    }

    let bytes = token.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(token.len());
    let mut i = 0;

    while i < len {
        if bytes[i] != b'$' {
            // Copy a full UTF-8 char.
            let clen = char_len_at(token, i);
            out.push_str(&token[i..i + clen]);
            i += clen;
            continue;
        }

        // We have a `$` at position i.
        if i + 1 >= len {
            // Trailing `$` → leave as-is.
            out.push('$');
            i += 1;
            continue;
        }

        let next = bytes[i + 1];
        if next == b'$' {
            // `$$` escape → single `$`.
            out.push('$');
            i += 2;
            continue;
        }

        if next == b'{' {
            // Braced: `${ident}`.
            if let Some((ident, after)) = parse_braced_identifier(token, i + 2) {
                match variables.get(ident) {
                    Some(value) => out.push_str(value),
                    None => out.push_str(&token[i..after]), // keep `${ident}` verbatim
                }
                i = after;
                continue;
            }
            // Not a valid `${ident}` form → leave `$` literal, continue from `{`.
            out.push('$');
            i += 1;
            continue;
        }

        // Bare: `$ident`.
        if is_id_start(next) {
            let ident_start = i + 1;
            let mut j = ident_start + 1;
            while j < len && is_id_continue(bytes[j]) {
                j += 1;
            }
            let ident = &token[ident_start..j];
            match variables.get(ident) {
                Some(value) => out.push_str(value),
                None => out.push_str(&token[i..j]), // keep `$ident` verbatim
            }
            i = j;
            continue;
        }

        // `$` followed by something that is not `$`, `{`, or an id-start → literal.
        out.push('$');
        i += 1;
    }

    out
}

/// Parse a braced identifier `ident}` starting at byte index `start` (just after
/// the `{`). Returns `(ident, index_after_closing_brace)` if the content up to the
/// next `}` is a valid ASCII identifier, else `None`.
fn parse_braced_identifier(token: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = token.as_bytes();
    let len = bytes.len();
    if start >= len || !is_id_start(bytes[start]) {
        return None;
    }
    let mut j = start + 1;
    while j < len && is_id_continue(bytes[j]) {
        j += 1;
    }
    // Must be terminated by `}` immediately after the identifier.
    if j < len && bytes[j] == b'}' {
        Some((&token[start..j], j + 1))
    } else {
        None
    }
}

#[inline]
fn is_id_start(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphabetic()
}

#[inline]
fn is_id_continue(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// Check whether a tag body contains characters that would cause Python's
/// markup tokeniser to emit a "text" token (making the tag unparsable as a
/// style expression).  We use a heuristic: a tag body is "literal" if it
/// contains an inner `[` or `]` that would confuse bracket matching.  Normal
/// alphanumeric, `-`, `_`, `=`, `@`, `#`, `$`, `%`, `.`, `(`, `)`, spaces,
/// and `/` are all valid style-token characters.
///
/// This keeps the "fast-literal" gate narrow so that even exotic tokens like
/// `"$primary"`, `"not bold"`, `"auto 20%"`, `"bad on red"`, `"foobar"` all
/// get pushed to the stack (and stored raw), matching Python.
fn contains_literal_text(tag_body: &str) -> bool {
    // Inner unescaped brackets inside the body indicate a genuinely unparsable tag
    tag_body.contains('[') || tag_body.contains(']')
}

/// Extract meta key-value pairs from a raw tag body string for metadata-only tags
/// (e.g. `link=url`, `@click=action`).  This is used when building `RawSpan.meta`
/// so that metadata survives span manipulation even before render-time resolution.
///
/// Mirrors Python `markup.parse_style`'s key/value reader: the value after a
/// `key=` token runs until the next top-level whitespace, but whitespace and
/// commas *inside* parentheses are kept (so `@click=set('a', 'b')` is read as a
/// single value).  This is essential for `@click` actions that carry arguments.
fn extract_meta_only(raw_tag: &str) -> Vec<(String, String)> {
    let mut meta = Vec::new();
    let bytes = raw_tag.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Skip leading whitespace.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        // Read a token up to the next top-level whitespace.
        let token_start = i;
        let mut eq_pos: Option<usize> = None;
        let mut depth = 0i32;
        let mut in_quote: Option<u8> = None;
        while i < bytes.len() {
            let b = bytes[i];
            match in_quote {
                Some(q) if b == q => in_quote = None,
                Some(_) => {}
                None => match b {
                    b'\'' | b'"' => in_quote = Some(b),
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => depth -= 1,
                    b'=' if eq_pos.is_none() => eq_pos = Some(i),
                    _ if b.is_ascii_whitespace() && depth <= 0 => break,
                    _ => {}
                },
            }
            i += 1;
        }
        let token = &raw_tag[token_start..i];
        if let Some(rel_eq) = eq_pos {
            let key = &raw_tag[token_start..rel_eq];
            let value = raw_tag[rel_eq + 1..i].trim_matches('"').trim_matches('\'');
            meta.push((key.to_string(), value.to_string()));
        } else {
            let _ = token;
        }
    }
    meta
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

    // --- substitute_text_token (string.Template.safe_substitute parity) ---

    fn vmap(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_subst_bare_and_braced() {
        let v = vmap(&[("name", "Will")]);
        assert_eq!(substitute_text_token("$name", Some(&v)), "Will");
        assert_eq!(substitute_text_token("${name}", Some(&v)), "Will");
        assert_eq!(
            substitute_text_token("a $name b ${name}", Some(&v)),
            "a Will b Will"
        );
    }

    #[test]
    fn test_subst_dollar_escape() {
        let v = vmap(&[]);
        assert_eq!(substitute_text_token("$$5", Some(&v)), "$5");
        assert_eq!(substitute_text_token("$$$$", Some(&v)), "$$");
    }

    #[test]
    fn test_subst_missing_and_invalid() {
        let v = vmap(&[("1bad", "x"), ("a.b", "y")]);
        assert_eq!(substitute_text_token("$missing", Some(&v)), "$missing");
        assert_eq!(substitute_text_token("$1bad", Some(&v)), "$1bad");
        assert_eq!(substitute_text_token("${a.b}", Some(&v)), "${a.b}");
        // trailing `$`
        assert_eq!(substitute_text_token("end$", Some(&v)), "end$");
        // `$` followed by space
        assert_eq!(substitute_text_token("$ x", Some(&v)), "$ x");
    }

    #[test]
    fn test_subst_underscore_identifier() {
        let v = vmap(&[("_b", "X")]);
        assert_eq!(substitute_text_token("a$_b c", Some(&v)), "aX c");
    }

    #[test]
    fn test_subst_none_and_no_dollar() {
        let v = vmap(&[("name", "Will")]);
        assert_eq!(substitute_text_token("plain text", None), "plain text");
        assert_eq!(substitute_text_token("plain text", Some(&v)), "plain text");
    }

    #[test]
    fn test_subst_non_ascii_identifier_not_matched() {
        // Python default idpattern is ASCII-only; `$café` is not substituted.
        let v = vmap(&[("café", "X")]);
        assert_eq!(substitute_text_token("$café", Some(&v)), "$café");
    }

    #[test]
    fn test_bold_tag() {
        let (text, spans) = parse_markup("[bold]hello[/bold]");
        assert_eq!(text, "hello");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 5);
        assert_eq!(spans[0].raw_tag, "bold");
    }

    #[test]
    fn test_abbreviation_b() {
        let (text, spans) = parse_markup("[b]hello[/b]");
        assert_eq!(text, "hello");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].raw_tag, "b");
        // Resolved style should have bold=true
        let resolved = parse_tag_style(&spans[0].raw_tag).unwrap();
        assert_eq!(resolved.style.bold, Some(true));
    }

    #[test]
    fn test_italic_tag() {
        let (text, spans) = parse_markup("[italic]world[/italic]");
        assert_eq!(text, "world");
        assert_eq!(spans.len(), 1);
        let resolved = parse_tag_style(&spans[0].raw_tag).unwrap();
        assert_eq!(resolved.style.italic, Some(true));
    }

    #[test]
    fn test_link_no_visual_style() {
        // [link=url] must carry meta only — no visual cyan/underline
        let (text, spans) = parse_markup("[link=https://example.com]click me[/link]");
        assert_eq!(text, "click me");
        assert_eq!(spans.len(), 1);
        // Raw tag preserved
        assert_eq!(spans[0].raw_tag, "link=https://example.com");
        // Meta should contain the link url
        let link_meta = spans[0].meta.iter().find(|(k, _)| k == "link");
        assert!(link_meta.is_some(), "link meta must be present");
        assert_eq!(link_meta.unwrap().1, "https://example.com");
        // Resolved visual style should have no fg/underline
        let resolved = parse_tag_style(&spans[0].raw_tag).unwrap();
        assert!(
            resolved.style.fg.is_none(),
            "link tag must not set fg color"
        );
        assert!(
            resolved.style.underline.is_none(),
            "link tag must not set underline"
        );
    }

    #[test]
    fn test_at_click_meta() {
        let (text, spans) = parse_markup("[@click=my_action]click[/]");
        assert_eq!(text, "click");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].raw_tag, "@click=my_action");
        let click_meta = spans[0].meta.iter().find(|(k, _)| k == "@click");
        assert!(click_meta.is_some());
        assert_eq!(click_meta.unwrap().1, "my_action");
    }

    #[test]
    fn test_auto_close_at_end() {
        // Unclosed tag should be auto-closed at end of text
        let (text, spans) = parse_markup("[bold]unclosed");
        assert_eq!(text, "unclosed");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 8);
        assert_eq!(spans[0].raw_tag, "bold");
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
        let bold_span = spans.iter().find(|s| s.raw_tag == "bold");
        let italic_span = spans.iter().find(|s| s.raw_tag == "italic");
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
        assert_eq!(spans[0].raw_tag, "red");
        let resolved = parse_tag_style("red").unwrap();
        assert!(
            resolved.style.fg.is_some(),
            "red tag should set fg color after resolve"
        );
    }

    /// Python-faithful: unrecognised tag like [foobar] is CONSUMED (not literal text).
    /// The tag body is stored as raw_tag; it resolves to null style at render time.
    #[test]
    fn test_unrecognised_tag_is_consumed_not_literal() {
        let (text, spans) = parse_markup("[foobar]test[/foobar]");
        // Tag is consumed — plain text is just "test"
        assert_eq!(
            text, "test",
            "unknown tag must be consumed, not emitted as literal [foobar]test"
        );
        // One span produced, raw_tag = "foobar"
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].raw_tag, "foobar");
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 4);
        // parse_tag_style("foobar") should return Some with null style (unknown token → no-op)
        let resolved = parse_tag_style("foobar");
        assert!(
            resolved.is_some(),
            "parse_tag_style should return Some (had_style=true from consuming foobar)"
        );
        let style = resolved.unwrap().style;
        assert!(style.bold.is_none());
        assert!(style.fg.is_none());
        assert!(style.bg.is_none());
    }

    /// Python-faithful: `[bad on red]y[/]` — mixed unknown+valid tokens.
    /// "bad" is unrecognised but "on red" is valid bg.  Python stores whole tag raw.
    #[test]
    fn test_mixed_unknown_valid_tag_consumed() {
        let (text, spans) = parse_markup("[bad on red]y[/]");
        assert_eq!(text, "y", "tag must be consumed");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].raw_tag, "bad on red");
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
        assert_eq!(spans[0].raw_tag, "bold");
    }

    #[test]
    fn test_on_background_color() {
        let (text, spans) = parse_markup("[on red]text[/]");
        assert_eq!(text, "text");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].raw_tag, "on red");
        // Resolved: "on red" should set bg, not fg
        let resolved = parse_tag_style("on red").unwrap();
        assert!(resolved.style.bg.is_some(), "on <color> should set bg");
        assert!(resolved.style.fg.is_none(), "on <color> must not set fg");
    }

    #[test]
    fn test_foreground_on_background_color() {
        // `white on black` must assign fg=white, bg=black (NOT swapped). The `on`
        // keyword only redirects the NEXT color to the background; the already-
        // parsed `white` stays a foreground. Regression for a markup parser bug
        // that committed the pending color under the post-`on` background flag.
        let resolved = parse_tag_style("white on black").unwrap();
        let white = crate::style::Color::rgb(255, 255, 255);
        let black = crate::style::Color::rgb(0, 0, 0);
        assert_eq!(resolved.style.fg, Some(white), "white must be foreground");
        assert_eq!(resolved.style.bg, Some(black), "black must be background");

        // With additional attributes the colors are still not swapped.
        let resolved = parse_tag_style("r u white on black").unwrap();
        assert_eq!(resolved.style.fg, Some(white));
        assert_eq!(resolved.style.bg, Some(black));
        assert_eq!(resolved.style.reverse, Some(true));
        assert_eq!(resolved.style.underline, Some(true));
    }

    #[test]
    fn test_normalize_tag() {
        assert_eq!(normalize_tag("b"), "bold");
        assert_eq!(normalize_tag("bold"), "bold");
        assert_eq!(normalize_tag("link=url"), "link");
        assert_eq!(normalize_tag("  italic  "), "italic");
    }

    // --- not modifier tests ---

    #[test]
    fn test_not_modifier_bold() {
        // "[not bold]text[/]" → parse_tag_style("not bold") → bold=false
        let parsed = parse_tag_style("not bold").unwrap();
        assert_eq!(
            parsed.style.bold,
            Some(false),
            "not bold should set bold=false"
        );
    }

    #[test]
    fn test_not_modifier_italic() {
        let parsed = parse_tag_style("not italic").unwrap();
        assert_eq!(parsed.style.italic, Some(false));
    }

    // --- deferred resolution tests ---

    #[test]
    fn test_raw_tag_stored_for_bold() {
        // Even a known keyword like "bold" is stored raw (deferred)
        let (_, spans) = parse_markup("[bold]x[/bold]");
        assert_eq!(spans[0].raw_tag, "bold");
        // Resolution happens separately
        let resolved = parse_tag_style(&spans[0].raw_tag).unwrap();
        assert_eq!(resolved.style.bold, Some(true));
    }

    #[test]
    fn test_raw_tag_stored_for_unknown() {
        let (_, spans) = parse_markup("[mytheme-primary]x[/]");
        assert_eq!(spans[0].raw_tag, "mytheme-primary");
        // No parse-time error; null style at default resolve
        let resolved = parse_tag_style(&spans[0].raw_tag).unwrap();
        assert!(resolved.style.bold.is_none());
        assert!(resolved.style.fg.is_none());
    }
}
