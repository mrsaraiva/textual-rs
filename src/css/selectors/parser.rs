use std::time::Duration;

use crate::style::{
    Align, BorderEdge, BorderType, BoxSizing, Constrain, ContentAlign, Display, Dock, Hatch,
    HorizontalAlign, Keyline, KeylineType, Layout, Margin, Offset, OffsetValue, Overflow,
    OverlayMode, Pointer, Position, PropertyTransition, Scalar, ScrollbarGutter,
    ScrollbarVisibility, Split, Style, StyleProperty, TextAlign, TextOverflow, TextStyleFlags,
    TextWrap, Tint, TransitionTiming, VerticalAlign, Visibility, parse_auto_color_like,
    parse_color_like, resolve_text_style_token_flags,
};

use super::ast::{Combinator, PseudoClass, SelectorChain, StyleRule, StyleSelector, StyleSheet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CssParseIssueKind {
    UnterminatedBlock,
    InvalidSelector,
    UnexpectedToken,
    UnsupportedAtRule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CssParseIssue {
    kind: CssParseIssueKind,
    message: String,
    snippet: String,
    offset: usize,
}

impl StyleSheet {
    pub fn parse(input: &str) -> Self {
        let (sheet, issues) = parse_with_issues(input);
        emit_parse_issues(&issues);
        sheet
    }
}

fn parse_with_issues(input: &str) -> (StyleSheet, Vec<CssParseIssue>) {
    let mut sheet = StyleSheet::new();
    let mut issues = Vec::new();
    let mut pos = 0usize;

    while pos < input.len() {
        let Some(rel_open) = input[pos..].find('{') else {
            break;
        };
        let open = pos + rel_open;
        let selector_text = input[pos..open].trim();
        let Some(close) = find_matching_brace(input, open) else {
            issues.push(CssParseIssue {
                kind: CssParseIssueKind::UnterminatedBlock,
                message: "unterminated block".to_string(),
                snippet: snippet_of(input, open, input.len()),
                offset: open,
            });
            break;
        };
        let body = &input[open + 1..close];
        if selector_text.is_empty() {
            issues.push(CssParseIssue {
                kind: CssParseIssueKind::UnexpectedToken,
                message: "missing selector before block".to_string(),
                snippet: snippet_of(input, open, close + 1),
                offset: open,
            });
            pos = close + 1;
            continue;
        }
        if selector_text.starts_with('@') {
            issues.push(CssParseIssue {
                kind: CssParseIssueKind::UnsupportedAtRule,
                message: format!("unsupported at-rule: {selector_text}"),
                snippet: snippet_of(input, pos, close + 1),
                offset: pos,
            });
            pos = close + 1;
            continue;
        }
        let selectors = split_selector_groups(selector_text);
        if selectors.is_empty() {
            issues.push(CssParseIssue {
                kind: CssParseIssueKind::InvalidSelector,
                message: format!("invalid selector list: {selector_text}"),
                snippet: snippet_of(input, pos, close + 1),
                offset: pos,
            });
            pos = close + 1;
            continue;
        }
        parse_rule_block(&mut sheet, &selectors, body, pos, input, &mut issues);
        pos = close + 1;
    }

    (sheet, issues)
}

fn parse_rule_block(
    sheet: &mut StyleSheet,
    selectors: &[String],
    body: &str,
    base_offset: usize,
    source: &str,
    issues: &mut Vec<CssParseIssue>,
) {
    let split = split_block_body(body, base_offset, source, issues);
    let style = parse_style_body(&split.declarations);
    if !style.is_empty() {
        append_style_rules(sheet, selectors, style, issues, base_offset, source);
    }

    for nested in split.nested_rules {
        if nested.selector.starts_with('@') {
            issues.push(CssParseIssue {
                kind: CssParseIssueKind::UnsupportedAtRule,
                message: format!("unsupported at-rule: {}", nested.selector),
                snippet: snippet_of(source, nested.selector_offset, nested.block_end),
                offset: nested.selector_offset,
            });
            continue;
        }
        let expanded = expand_nested_selectors(selectors, &nested.selector);
        if expanded.is_empty() {
            issues.push(CssParseIssue {
                kind: CssParseIssueKind::InvalidSelector,
                message: format!("invalid nested selector: {}", nested.selector),
                snippet: snippet_of(source, nested.selector_offset, nested.block_end),
                offset: nested.selector_offset,
            });
            continue;
        }
        parse_rule_block(
            sheet,
            &expanded,
            &nested.body,
            nested.selector_offset,
            source,
            issues,
        );
    }
}

fn append_style_rules(
    sheet: &mut StyleSheet,
    selectors: &[String],
    style: Style,
    issues: &mut Vec<CssParseIssue>,
    base_offset: usize,
    source: &str,
) {
    for selector in selectors {
        if let Some(selector_chain) = parse_selector_chain(selector) {
            sheet.rules.push(StyleRule {
                selector_chain,
                style: style.clone(),
            });
        } else {
            issues.push(CssParseIssue {
                kind: CssParseIssueKind::InvalidSelector,
                message: format!("invalid selector: {selector}"),
                snippet: snippet_of(
                    source,
                    base_offset,
                    (base_offset + selector.len()).min(source.len()),
                ),
                offset: base_offset,
            });
        }
    }
}

#[derive(Debug)]
struct NestedRuleBlock {
    selector: String,
    body: String,
    selector_offset: usize,
    block_end: usize,
}

#[derive(Debug, Default)]
struct SplitBlockBody {
    declarations: String,
    nested_rules: Vec<NestedRuleBlock>,
}

fn split_block_body(
    body: &str,
    base_offset: usize,
    source: &str,
    issues: &mut Vec<CssParseIssue>,
) -> SplitBlockBody {
    let mut out = SplitBlockBody::default();
    let mut cursor = 0usize;
    let mut idx = 0usize;

    while idx < body.len() {
        let Some(rel_open) = body[idx..].find('{') else {
            break;
        };
        let open = idx + rel_open;
        let Some(close) = find_matching_brace(body, open) else {
            issues.push(CssParseIssue {
                kind: CssParseIssueKind::UnterminatedBlock,
                message: "unterminated nested block".to_string(),
                snippet: snippet_of(source, base_offset + open, base_offset + body.len()),
                offset: base_offset + open,
            });
            break;
        };

        let segment = &body[cursor..open];
        let (declarations, selector_fragment) = split_segment_for_nested_selector(segment);
        if !declarations.trim().is_empty() {
            out.declarations.push_str(declarations);
            if !declarations.trim_end().ends_with(';') {
                out.declarations.push(';');
            }
            out.declarations.push('\n');
        }
        let selector = selector_fragment.trim();
        if selector.is_empty() {
            issues.push(CssParseIssue {
                kind: CssParseIssueKind::UnexpectedToken,
                message: "missing nested selector before block".to_string(),
                snippet: snippet_of(source, base_offset + open, base_offset + close + 1),
                offset: base_offset + open,
            });
        } else {
            out.nested_rules.push(NestedRuleBlock {
                selector: selector.to_string(),
                body: body[open + 1..close].to_string(),
                selector_offset: base_offset
                    + cursor
                    + segment.len().saturating_sub(selector.len()),
                block_end: base_offset + close + 1,
            });
        }

        cursor = close + 1;
        idx = close + 1;
    }

    let tail = body[cursor..].trim();
    if !tail.is_empty() {
        out.declarations.push_str(tail);
    }
    out
}

fn split_segment_for_nested_selector(segment: &str) -> (&str, &str) {
    match segment.rfind(';') {
        Some(idx) => (&segment[..=idx], &segment[idx + 1..]),
        None => ("", segment),
    }
}

fn split_selector_groups(selector: &str) -> Vec<String> {
    selector
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn expand_nested_selectors(parents: &[String], nested_selector: &str) -> Vec<String> {
    let nested_groups = split_selector_groups(nested_selector);
    if nested_groups.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for parent in parents {
        for nested in &nested_groups {
            let combined = if nested.contains('&') {
                nested.replace('&', parent)
            } else {
                format!("{parent} {nested}")
            };
            let combined = combined.trim();
            if !combined.is_empty() {
                out.push(combined.to_string());
            }
        }
    }
    out
}

fn find_matching_brace(input: &str, open: usize) -> Option<usize> {
    if input.as_bytes().get(open) != Some(&b'{') {
        return None;
    }
    let mut depth = 1usize;
    for (i, b) in input.as_bytes().iter().enumerate().skip(open + 1) {
        match *b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn snippet_of(source: &str, start: usize, end: usize) -> String {
    if source.is_empty() {
        return String::new();
    }
    let safe_start = start.min(source.len());
    let safe_end = end.min(source.len()).max(safe_start);
    source[safe_start..safe_end]
        .replace('\n', " ")
        .replace('\r', " ")
        .trim()
        .chars()
        .take(120)
        .collect()
}

fn emit_parse_issues(issues: &[CssParseIssue]) {
    if issues.is_empty() {
        return;
    }
    let summary = format!(
        "[css][parse] {} issue(s) encountered while parsing stylesheet",
        issues.len()
    );
    eprintln!("{summary}");
    crate::debug::debug_style(&summary);
    for issue in issues {
        let line = format!(
            "[css][parse][{:?}] at byte {}: {} | {}",
            issue.kind, issue.offset, issue.message, issue.snippet
        );
        eprintln!("{line}");
        crate::debug::debug_style(&line);
    }
}

pub(crate) fn parse_selector_list(selector: &str) -> Vec<SelectorChain> {
    let mut groups = Vec::new();
    for group in selector.split(',') {
        let group = group.trim();
        if group.is_empty() {
            continue;
        }
        if let Some(chain) = parse_selector_chain(group) {
            groups.push(chain);
        }
    }
    groups
}

fn parse_selector(selector: &str) -> Option<StyleSelector> {
    let selector = selector.trim();
    if selector.is_empty() {
        return None;
    }

    let mut type_name: Option<String> = None;
    let mut id: Option<String> = None;
    let mut classes: Vec<String> = Vec::new();
    let mut pseudos: Vec<PseudoClass> = Vec::new();

    let mut chars = selector.chars().peekable();
    let mut current = String::new();
    let mut mode: Option<char> = None; // '#', '.', ':', or None for type

    let mut flush = |mode: Option<char>, token: &str| {
        if token.is_empty() {
            return;
        }
        match mode {
            None => {
                if type_name.is_none() {
                    type_name = Some(token.to_string());
                }
            }
            Some('#') => id = Some(token.to_string()),
            Some('.') => classes.push(token.to_string()),
            Some(':') => {
                // Ignore any `:pseudo(...)` forms for now.
                let name = token
                    .split('(')
                    .next()
                    .unwrap_or(token)
                    .trim()
                    .to_lowercase();
                let pseudo = match name.as_str() {
                    "disabled" => Some(PseudoClass::Disabled),
                    "focus" | "focused" => Some(PseudoClass::Focus),
                    "blur" => Some(PseudoClass::Blur),
                    "focus-within" | "focus_within" => Some(PseudoClass::FocusWithin),
                    "hover" => Some(PseudoClass::Hover),
                    "active" => Some(PseudoClass::Active),
                    "dark" => Some(PseudoClass::Dark),
                    "light" => Some(PseudoClass::Light),
                    "inline" => Some(PseudoClass::Inline),
                    "ansi" => Some(PseudoClass::Ansi),
                    "nocolor" => Some(PseudoClass::NoColor),
                    "can-focus" | "can_focus" => Some(PseudoClass::CanFocus),
                    "even" => Some(PseudoClass::Even),
                    "odd" => Some(PseudoClass::Odd),
                    "first-child" | "first_child" => Some(PseudoClass::FirstChild),
                    "last-child" | "last_child" => Some(PseudoClass::LastChild),
                    _ => None,
                };
                if let Some(pseudo) = pseudo {
                    pseudos.push(pseudo);
                }
            }
            _ => {}
        }
    };

    while let Some(ch) = chars.next() {
        match ch {
            '#' | '.' | ':' => {
                flush(mode, current.trim());
                current.clear();
                mode = Some(ch);
            }
            _ => current.push(ch),
        }
    }

    flush(mode, current.trim());

    let mut selector = StyleSelector::default();
    if let Some(type_name) = type_name {
        // "*" is the universal selector — matches any type.
        if type_name != "*" {
            selector = StyleSelector::new(type_name);
        }
    }
    if let Some(id) = id {
        selector = selector.id(id);
    }
    for class in classes {
        selector = selector.class(class);
    }
    for pseudo in pseudos {
        selector = selector.pseudo(pseudo);
    }
    Some(selector)
}

fn parse_selector_chain(selector: &str) -> Option<SelectorChain> {
    let mut tokens: Vec<String> = Vec::new();
    let mut buf = String::new();
    for ch in selector.chars() {
        match ch {
            '>' => {
                if !buf.trim().is_empty() {
                    tokens.push(buf.trim().to_string());
                }
                tokens.push(">".to_string());
                buf.clear();
            }
            c if c.is_whitespace() => {
                if !buf.trim().is_empty() {
                    tokens.push(buf.trim().to_string());
                    buf.clear();
                }
            }
            _ => buf.push(ch),
        }
    }
    if !buf.trim().is_empty() {
        tokens.push(buf.trim().to_string());
    }

    let mut parts = Vec::new();
    let mut combinators = Vec::new();
    let mut pending: Option<Combinator> = None;
    for token in tokens {
        if token == ">" {
            pending = Some(Combinator::Child);
            continue;
        }
        if let Some(selector) = parse_selector(&token) {
            if !parts.is_empty() {
                combinators.push(pending.unwrap_or(Combinator::Descendant));
            }
            parts.push(selector);
            pending = None;
        }
    }

    if parts.is_empty() {
        return None;
    }

    Some(SelectorChain { parts, combinators })
}

/// Strip a trailing `!important` annotation (case-insensitive) from a CSS value.
///
/// Returns `(clean_value, is_important)`.
fn strip_important(value: &str) -> (&str, bool) {
    let trimmed = value.trim();
    let len = trimmed.len();
    // "important" = 9 chars; minimum input with `!important` is 10 chars.
    if len < 10 {
        return (value, false);
    }
    // Use `get()` for safe slicing — avoids panic on multi-byte char boundaries.
    let suffix = match trimmed.get(len.saturating_sub(9)..) {
        Some(s) if s.eq_ignore_ascii_case("important") => s,
        _ => return (value, false),
    };
    // The suffix is ASCII "important", so `len - 9` is a valid char boundary.
    let _ = suffix;
    let before = trimmed[..len - 9].trim_end();
    if let Some(rest) = before.strip_suffix('!') {
        return (rest.trim(), true);
    }
    (value, false)
}

/// Map a CSS property key to the [`StyleProperty`] variants it affects.
///
/// Returns an empty slice for unknown keys.
fn importance_properties_for_key(key: &str) -> &'static [StyleProperty] {
    match key {
        "fg" | "color" => &[StyleProperty::Fg],
        "bg" | "background" => &[StyleProperty::Bg],
        "width" => &[StyleProperty::Width],
        "height" => &[StyleProperty::Height],
        "min-width" => &[StyleProperty::MinWidth],
        "max-width" => &[StyleProperty::MaxWidth],
        "min-height" => &[StyleProperty::MinHeight],
        "max-height" => &[StyleProperty::MaxHeight],
        "padding" => &[StyleProperty::Padding],
        "layout" => &[StyleProperty::Layout],
        "display" => &[StyleProperty::Display],
        "visibility" => &[StyleProperty::Visibility],
        "overflow" => &[
            StyleProperty::Overflow,
            StyleProperty::OverflowX,
            StyleProperty::OverflowY,
        ],
        "overflow-x" => &[StyleProperty::OverflowX],
        "overflow-y" => &[StyleProperty::OverflowY],
        "dock" => &[StyleProperty::Dock],
        "margin" => &[StyleProperty::Margin],
        "bold" => &[StyleProperty::Bold],
        "dim" => &[StyleProperty::Dim],
        "italic" => &[StyleProperty::Italic],
        "underline" => &[StyleProperty::Underline],
        "tint" => &[StyleProperty::Tint],
        "background-tint" => &[StyleProperty::BackgroundTint],
        "text-opacity" => &[StyleProperty::TextOpacity],
        "opacity" => &[StyleProperty::Opacity],
        "line-pad" => &[StyleProperty::Padding],
        "transition-duration" => &[StyleProperty::TransitionDuration],
        "transition-delay" => &[StyleProperty::TransitionDelay],
        "transition-timing-function" => &[StyleProperty::TransitionTiming],
        "border-top" => &[StyleProperty::BorderTop],
        "border-right" => &[StyleProperty::BorderRight],
        "border-bottom" => &[StyleProperty::BorderBottom],
        "border-left" => &[StyleProperty::BorderLeft],
        "border" => &[
            StyleProperty::BorderTop,
            StyleProperty::BorderRight,
            StyleProperty::BorderBottom,
            StyleProperty::BorderLeft,
        ],
        "grid-size-columns" => &[StyleProperty::GridSizeColumns],
        "grid-size-rows" => &[StyleProperty::GridSizeRows],
        "grid-size" => &[StyleProperty::GridSizeColumns, StyleProperty::GridSizeRows],
        "grid-columns" => &[StyleProperty::GridColumns],
        "grid-rows" => &[StyleProperty::GridRows],
        "grid-gutter-horizontal" => &[StyleProperty::GridGutterHorizontal],
        "grid-gutter-vertical" => &[StyleProperty::GridGutterVertical],
        "grid-gutter" => &[
            StyleProperty::GridGutterHorizontal,
            StyleProperty::GridGutterVertical,
        ],
        "text-align" => &[StyleProperty::TextAlign],
        "content-align" => &[StyleProperty::ContentAlign],
        "align" | "align-horizontal" | "align-vertical" => &[StyleProperty::Align],
        "offset" | "offset-x" | "offset-y" => &[StyleProperty::Offset],
        "layer" => &[StyleProperty::Layer],
        "layers" => &[StyleProperty::Layers],
        "constrain" => &[StyleProperty::Constrain],
        "pointer" => &[StyleProperty::Pointer],
        // --- P2 CSS gap properties ---
        "position" => &[StyleProperty::Position],
        "box-sizing" => &[StyleProperty::BoxSizing],
        "split" => &[StyleProperty::Split],
        "padding-top" => &[StyleProperty::PaddingTop],
        "padding-right" => &[StyleProperty::PaddingRight],
        "padding-bottom" => &[StyleProperty::PaddingBottom],
        "padding-left" => &[StyleProperty::PaddingLeft],
        "margin-top" => &[StyleProperty::MarginTop],
        "margin-right" => &[StyleProperty::MarginRight],
        "margin-bottom" => &[StyleProperty::MarginBottom],
        "margin-left" => &[StyleProperty::MarginLeft],
        "outline" => &[
            StyleProperty::OutlineTop,
            StyleProperty::OutlineRight,
            StyleProperty::OutlineBottom,
            StyleProperty::OutlineLeft,
        ],
        "outline-top" => &[StyleProperty::OutlineTop],
        "outline-right" => &[StyleProperty::OutlineRight],
        "outline-bottom" => &[StyleProperty::OutlineBottom],
        "outline-left" => &[StyleProperty::OutlineLeft],
        "border-title-align" => &[StyleProperty::BorderTitleAlign],
        "border-subtitle-align" => &[StyleProperty::BorderSubtitleAlign],
        "border-title-color" => &[StyleProperty::BorderTitleColor],
        "border-title-background" => &[StyleProperty::BorderTitleBackground],
        "border-title-style" => &[StyleProperty::BorderTitleStyle],
        "border-subtitle-color" => &[StyleProperty::BorderSubtitleColor],
        "border-subtitle-background" => &[StyleProperty::BorderSubtitleBackground],
        "border-subtitle-style" => &[StyleProperty::BorderSubtitleStyle],
        "scrollbar-color" => &[StyleProperty::ScrollbarColor],
        "scrollbar-color-hover" => &[StyleProperty::ScrollbarColorHover],
        "scrollbar-color-active" => &[StyleProperty::ScrollbarColorActive],
        "scrollbar-background" => &[StyleProperty::ScrollbarBackground],
        "scrollbar-background-hover" => &[StyleProperty::ScrollbarBackgroundHover],
        "scrollbar-background-active" => &[StyleProperty::ScrollbarBackgroundActive],
        "scrollbar-corner-color" => &[StyleProperty::ScrollbarCornerColor],
        "scrollbar-gutter" => &[StyleProperty::ScrollbarGutter],
        "scrollbar-size" => &[StyleProperty::ScrollbarSize],
        "scrollbar-size-horizontal" => &[StyleProperty::ScrollbarSizeHorizontal],
        "scrollbar-size-vertical" => &[StyleProperty::ScrollbarSizeVertical],
        "scrollbar-visibility" => &[StyleProperty::ScrollbarVisibility],
        "text-wrap" => &[StyleProperty::TextWrapProp],
        "text-overflow" => &[StyleProperty::TextOverflowProp],
        "link-color" => &[StyleProperty::LinkColor],
        "link-background" => &[StyleProperty::LinkBackground],
        "link-style" => &[StyleProperty::LinkStyleProp],
        "link-color-hover" => &[StyleProperty::LinkColorHover],
        "link-background-hover" => &[StyleProperty::LinkBackgroundHover],
        "link-style-hover" => &[StyleProperty::LinkStyleHover],
        "row-span" => &[StyleProperty::RowSpan],
        "column-span" => &[StyleProperty::ColumnSpan],
        "hatch" => &[StyleProperty::HatchProp],
        "overlay" => &[StyleProperty::OverlayProp],
        "keyline" => &[StyleProperty::KeylineProp],
        "constrain-x" => &[StyleProperty::ConstrainX],
        "constrain-y" => &[StyleProperty::ConstrainY],
        "expand" => &[StyleProperty::ExpandProp],
        _ => &[],
    }
}

/// Reset any `Option<T>` CSS property to `None` (the `initial` keyword).
/// Returns `true` if the property was recognized and reset.
fn apply_initial(style: &mut Style, key: &str, is_important: bool) -> bool {
    macro_rules! reset {
        ($field:ident, $prop:expr) => {{
            style.$field = None;
            if is_important {
                style.importance.set($prop);
            }
            true
        }};
    }
    macro_rules! reset_border {
        ($field:ident, $prop:expr) => {{
            style.$field = BorderEdge::Unset;
            if is_important {
                style.importance.set($prop);
            }
            true
        }};
    }
    match key {
        // --- Colors (already handled inline, but included for completeness) ---
        "fg" | "color" => {
            style.fg = None;
            style.fg_auto = None;
            if is_important {
                style.importance.set(StyleProperty::Fg);
            }
            true
        }
        "bg" | "background" => reset!(bg, StyleProperty::Bg),
        // --- Scalars ---
        "width" => reset!(width, StyleProperty::Width),
        "height" => reset!(height, StyleProperty::Height),
        "min-width" => reset!(min_width, StyleProperty::MinWidth),
        "max-width" => reset!(max_width, StyleProperty::MaxWidth),
        "min-height" => reset!(min_height, StyleProperty::MinHeight),
        "max-height" => reset!(max_height, StyleProperty::MaxHeight),
        // --- Spacing ---
        "padding" => reset!(padding, StyleProperty::Padding),
        "margin" => reset!(margin, StyleProperty::Margin),
        "padding-top" => reset!(padding_top, StyleProperty::PaddingTop),
        "padding-right" => reset!(padding_right, StyleProperty::PaddingRight),
        "padding-bottom" => reset!(padding_bottom, StyleProperty::PaddingBottom),
        "padding-left" => reset!(padding_left, StyleProperty::PaddingLeft),
        "margin-top" => reset!(margin_top, StyleProperty::MarginTop),
        "margin-right" => reset!(margin_right, StyleProperty::MarginRight),
        "margin-bottom" => reset!(margin_bottom, StyleProperty::MarginBottom),
        "margin-left" => reset!(margin_left, StyleProperty::MarginLeft),
        // --- Layout / display ---
        "layout" => reset!(layout, StyleProperty::Layout),
        "display" => reset!(display, StyleProperty::Display),
        "visibility" => reset!(visibility, StyleProperty::Visibility),
        "overflow" => {
            style.overflow = None;
            style.overflow_x = None;
            style.overflow_y = None;
            if is_important {
                style.importance.set(StyleProperty::Overflow);
                style.importance.set(StyleProperty::OverflowX);
                style.importance.set(StyleProperty::OverflowY);
            }
            true
        }
        "overflow-x" => reset!(overflow_x, StyleProperty::OverflowX),
        "overflow-y" => reset!(overflow_y, StyleProperty::OverflowY),
        "dock" => reset!(dock, StyleProperty::Dock),
        "position" => reset!(position, StyleProperty::Position),
        "box-sizing" => reset!(box_sizing, StyleProperty::BoxSizing),
        "split" => reset!(split, StyleProperty::Split),
        // --- Alignment / offset ---
        "text-align" => reset!(text_align, StyleProperty::TextAlign),
        "content-align" => reset!(content_align, StyleProperty::ContentAlign),
        "align" => reset!(align, StyleProperty::Align),
        "offset" => reset!(offset, StyleProperty::Offset),
        "pointer" => reset!(pointer, StyleProperty::Pointer),
        // --- Constrain ---
        "constrain" => {
            style.constrain = None;
            style.constrain_x = None;
            style.constrain_y = None;
            if is_important {
                style.importance.set(StyleProperty::Constrain);
                style.importance.set(StyleProperty::ConstrainX);
                style.importance.set(StyleProperty::ConstrainY);
            }
            true
        }
        "constrain-x" => reset!(constrain_x, StyleProperty::ConstrainX),
        "constrain-y" => reset!(constrain_y, StyleProperty::ConstrainY),
        // --- Text style flags ---
        "bold" => reset!(bold, StyleProperty::Bold),
        "dim" => reset!(dim, StyleProperty::Dim),
        "italic" => reset!(italic, StyleProperty::Italic),
        "underline" => reset!(underline, StyleProperty::Underline),
        "reverse" => reset!(reverse, StyleProperty::Reverse),
        "strike" | "strikethrough" => reset!(strike, StyleProperty::Strike),
        "text-opacity" => reset!(text_opacity, StyleProperty::TextOpacity),
        "opacity" => reset!(opacity, StyleProperty::Opacity),
        // --- Tint ---
        "tint" => reset!(tint, StyleProperty::Tint),
        "background-tint" => reset!(background_tint, StyleProperty::BackgroundTint),
        // --- Border title/subtitle ---
        "border-title-align" => reset!(border_title_align, StyleProperty::BorderTitleAlign),
        "border-subtitle-align" => {
            reset!(border_subtitle_align, StyleProperty::BorderSubtitleAlign)
        }
        "border-title-color" => reset!(border_title_color, StyleProperty::BorderTitleColor),
        "border-title-background" => {
            reset!(
                border_title_background,
                StyleProperty::BorderTitleBackground
            )
        }
        "border-title-style" => reset!(border_title_style, StyleProperty::BorderTitleStyle),
        "border-subtitle-color" => {
            reset!(border_subtitle_color, StyleProperty::BorderSubtitleColor)
        }
        "border-subtitle-background" => {
            reset!(
                border_subtitle_background,
                StyleProperty::BorderSubtitleBackground
            )
        }
        "border-subtitle-style" => {
            reset!(border_subtitle_style, StyleProperty::BorderSubtitleStyle)
        }
        // --- Border edges ---
        "border-top" => reset_border!(border_top, StyleProperty::BorderTop),
        "border-right" => reset_border!(border_right, StyleProperty::BorderRight),
        "border-bottom" => reset_border!(border_bottom, StyleProperty::BorderBottom),
        "border-left" => reset_border!(border_left, StyleProperty::BorderLeft),
        // --- Outline edges ---
        "outline-top" => reset_border!(outline_top, StyleProperty::OutlineTop),
        "outline-right" => reset_border!(outline_right, StyleProperty::OutlineRight),
        "outline-bottom" => reset_border!(outline_bottom, StyleProperty::OutlineBottom),
        "outline-left" => reset_border!(outline_left, StyleProperty::OutlineLeft),
        // --- Scrollbar ---
        "scrollbar-color" => reset!(scrollbar_color, StyleProperty::ScrollbarColor),
        "scrollbar-color-hover" => {
            reset!(scrollbar_color_hover, StyleProperty::ScrollbarColorHover)
        }
        "scrollbar-color-active" => {
            reset!(scrollbar_color_active, StyleProperty::ScrollbarColorActive)
        }
        "scrollbar-background" => {
            reset!(scrollbar_background, StyleProperty::ScrollbarBackground)
        }
        "scrollbar-background-hover" => {
            reset!(
                scrollbar_background_hover,
                StyleProperty::ScrollbarBackgroundHover
            )
        }
        "scrollbar-background-active" => {
            reset!(
                scrollbar_background_active,
                StyleProperty::ScrollbarBackgroundActive
            )
        }
        "scrollbar-corner-color" => {
            reset!(scrollbar_corner_color, StyleProperty::ScrollbarCornerColor)
        }
        "scrollbar-gutter" => reset!(scrollbar_gutter, StyleProperty::ScrollbarGutter),
        "scrollbar-size" => reset!(scrollbar_size, StyleProperty::ScrollbarSize),
        "scrollbar-size-horizontal" => {
            reset!(
                scrollbar_size_horizontal,
                StyleProperty::ScrollbarSizeHorizontal
            )
        }
        "scrollbar-size-vertical" => {
            reset!(
                scrollbar_size_vertical,
                StyleProperty::ScrollbarSizeVertical
            )
        }
        "scrollbar-visibility" => {
            reset!(scrollbar_visibility, StyleProperty::ScrollbarVisibility)
        }
        // --- Text wrap / overflow ---
        "text-wrap" => reset!(text_wrap, StyleProperty::TextWrapProp),
        "text-overflow" => reset!(text_overflow, StyleProperty::TextOverflowProp),
        // --- Link styling ---
        "link-color" => reset!(link_color, StyleProperty::LinkColor),
        "link-background" => reset!(link_background, StyleProperty::LinkBackground),
        "link-style" => reset!(link_style, StyleProperty::LinkStyleProp),
        "link-color-hover" => reset!(link_color_hover, StyleProperty::LinkColorHover),
        "link-background-hover" => {
            reset!(link_background_hover, StyleProperty::LinkBackgroundHover)
        }
        "link-style-hover" => reset!(link_style_hover, StyleProperty::LinkStyleHover),
        // --- Grid ---
        "grid-size-columns" => reset!(grid_size_columns, StyleProperty::GridSizeColumns),
        "grid-size-rows" => reset!(grid_size_rows, StyleProperty::GridSizeRows),
        "grid-columns" => reset!(grid_columns, StyleProperty::GridColumns),
        "grid-rows" => reset!(grid_rows, StyleProperty::GridRows),
        "grid-gutter-horizontal" => {
            reset!(grid_gutter_horizontal, StyleProperty::GridGutterHorizontal)
        }
        "grid-gutter-vertical" => {
            reset!(grid_gutter_vertical, StyleProperty::GridGutterVertical)
        }
        // --- Grid child placement ---
        "row-span" => reset!(row_span, StyleProperty::RowSpan),
        "column-span" => reset!(column_span, StyleProperty::ColumnSpan),
        // --- Advanced ---
        "hatch" => reset!(hatch, StyleProperty::HatchProp),
        "overlay" => reset!(overlay, StyleProperty::OverlayProp),
        "keyline" => reset!(keyline, StyleProperty::KeylineProp),
        "expand" => reset!(expand, StyleProperty::ExpandProp),
        "layer" => reset!(layer, StyleProperty::Layer),
        "layers" => reset!(layers, StyleProperty::Layers),
        // --- Transitions ---
        "transitions" | "transition" => reset!(transitions, StyleProperty::TransitionsProp),
        _ => false,
    }
}

pub(super) fn parse_style_body(body: &str) -> Style {
    let mut style = Style::new();
    for decl in body.split(';') {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        let mut parts = decl.splitn(2, ':');
        let key = parts.next().unwrap_or("").trim().to_lowercase();
        let raw_value = parts.next().unwrap_or("").trim();
        let (value, is_important) = strip_important(raw_value);
        // Universal `initial` keyword support: reset any property to None.
        if value.eq_ignore_ascii_case("initial")
            && apply_initial(&mut style, key.as_str(), is_important)
        {
            continue;
        }
        // Track whether this arm handles importance itself (for shorthands
        // like `text-style` and `transition` that set multiple sub-properties).
        let mut handled_importance = false;
        match key.as_str() {
            "fg" | "color" => {
                if let Some(auto) = parse_auto_color_like(value) {
                    style = style.fg_auto(auto);
                } else if let Some((color, alpha)) = parse_color_like_with_alpha(value) {
                    let color = match alpha {
                        Some(p) => color.with_alpha(p as f32 / 100.0),
                        None => color,
                    };
                    style = style.fg(color);
                }
            }
            "bg" | "background" => {
                if let Some((color, alpha)) = parse_color_like_with_alpha(value) {
                    let color = match alpha {
                        Some(p) => color.with_alpha(p as f32 / 100.0),
                        None => color,
                    };
                    style = style.bg(color);
                }
            }
            "width" => {
                if let Some(scalar) = parse_scalar(value) {
                    style = style.width(scalar);
                }
            }
            "height" => {
                if let Some(scalar) = parse_scalar(value) {
                    style = style.height(scalar);
                }
            }
            "min-width" => {
                if let Some(scalar) = parse_scalar(value) {
                    style = style.min_width(scalar);
                }
            }
            "max-width" => {
                if let Some(scalar) = parse_scalar(value) {
                    style = style.max_width(scalar);
                }
            }
            "min-height" => {
                if let Some(scalar) = parse_scalar(value) {
                    style = style.min_height(scalar);
                }
            }
            "max-height" => {
                if let Some(scalar) = parse_scalar(value) {
                    style = style.max_height(scalar);
                }
            }
            "padding" => {
                if let Some(spacing) = parse_spacing(value) {
                    style.padding = Some(spacing);
                }
            }
            "layout" => {
                style.layout = parse_layout(value);
            }
            "display" => {
                style.display = parse_display(value);
            }
            "visibility" => {
                style.visibility = parse_visibility(value);
            }
            "overflow" => {
                let tokens: Vec<&str> = value.split_whitespace().collect();
                match tokens.len() {
                    1 => {
                        let v = parse_overflow(tokens[0]);
                        style.overflow = v;
                        style.overflow_x = v;
                        style.overflow_y = v;
                    }
                    2 => {
                        style.overflow_x = parse_overflow(tokens[0]);
                        style.overflow_y = parse_overflow(tokens[1]);
                    }
                    _ => {}
                }
            }
            "overflow-x" => {
                style.overflow_x = parse_overflow(value);
            }
            "overflow-y" => {
                style.overflow_y = parse_overflow(value);
            }
            "dock" => {
                style.dock = parse_dock(value);
            }
            "margin" => {
                if let Some(margin) = parse_margin(value) {
                    style = style.margin(margin);
                }
            }
            "bold" => {
                if let Some(val) = parse_bool(value) {
                    style = style.bold(val);
                }
            }
            "dim" => {
                if let Some(val) = parse_bool(value) {
                    style = style.dim(val);
                }
            }
            "italic" => {
                if let Some(val) = parse_bool(value) {
                    style = style.italic(val);
                }
            }
            "underline" => {
                if let Some(val) = parse_bool(value) {
                    style = style.underline(val);
                }
            }
            "strike" | "strikethrough" => {
                if let Some(val) = parse_bool(value) {
                    style.strike = Some(val);
                    if is_important {
                        style.importance.set(StyleProperty::Strike);
                    }
                }
            }
            "tint" => {
                if let Some(tint) = parse_tint(value) {
                    style.tint = Some(tint);
                }
            }
            "background-tint" => {
                if let Some(tint) = parse_tint(value) {
                    style.background_tint = Some(tint);
                }
            }
            "text-opacity" => {
                if let Some(percent) = parse_opacity_percent(value) {
                    style = style.text_opacity(percent);
                }
            }
            "opacity" => {
                if let Some(percent) = parse_opacity_percent(value) {
                    style = style.opacity(percent);
                }
            }
            "text-style" => {
                // Shorthand: only mark sub-properties that are actually set.
                handled_importance = true;
                parse_text_style_shorthand_into_style(&mut style, value, is_important);
            }
            "line-pad" => {
                if let Ok(value) = value.parse::<u16>() {
                    style.line_pad = Some(value);
                }
            }
            "transition" => {
                // Shorthand: only mark sub-properties that are actually set.
                handled_importance = true;
                // P2-36: parse per-property transitions (comma-separated).
                let items: Vec<&str> = value.split(',').collect();
                let mut per_property = Vec::new();
                for (idx, item) in items.iter().enumerate() {
                    let item = item.trim();
                    if item.is_empty() {
                        continue;
                    }
                    // Try to extract a property name (first non-duration/non-timing token).
                    let mut prop_name: Option<String> = None;
                    let mut dur: Option<std::time::Duration> = None;
                    let mut del: Option<std::time::Duration> = None;
                    let mut tim: Option<TransitionTiming> = None;
                    for token in item.split_whitespace() {
                        if dur.is_none() {
                            if let Some(d) = parse_duration(token) {
                                dur = Some(d);
                                continue;
                            }
                        } else if del.is_none() {
                            if let Some(d) = parse_duration(token) {
                                del = Some(d);
                                continue;
                            }
                        }
                        if tim.is_none() {
                            if let Some(t) = parse_transition_timing(token) {
                                tim = Some(t);
                                continue;
                            }
                        }
                        if prop_name.is_none() {
                            prop_name = Some(token.to_string());
                        }
                    }
                    let duration = dur.unwrap_or(std::time::Duration::from_millis(250));
                    let timing = tim.unwrap_or(TransitionTiming::Linear);
                    let delay = del.unwrap_or(std::time::Duration::ZERO);
                    if let Some(name) = prop_name {
                        per_property.push(PropertyTransition {
                            property: name,
                            duration,
                            timing,
                            delay,
                        });
                    }
                    // First item: set global transition fields only for values
                    // explicitly present in the declaration (backward compat).
                    if idx == 0 {
                        if let Some(d) = dur {
                            style = style.transition_duration(d);
                            if is_important {
                                style.importance.set(StyleProperty::TransitionDuration);
                            }
                        }
                        if let Some(d) = del {
                            style = style.transition_delay(d);
                            if is_important {
                                style.importance.set(StyleProperty::TransitionDelay);
                            }
                        }
                        if let Some(t) = tim {
                            style = style.transition_timing(t);
                            if is_important {
                                style.importance.set(StyleProperty::TransitionTiming);
                            }
                        }
                    }
                }
                if !per_property.is_empty() {
                    style.transitions = Some(per_property);
                    if is_important {
                        style.importance.set(StyleProperty::TransitionsProp);
                    }
                }
            }
            "transition-duration" => {
                if let Some(duration) = parse_duration(value) {
                    style = style.transition_duration(duration);
                }
            }
            "transition-delay" => {
                if let Some(delay) = parse_duration(value) {
                    style = style.transition_delay(delay);
                }
            }
            "transition-timing-function" => {
                if let Some(timing) = parse_transition_timing(value) {
                    style = style.transition_timing(timing);
                }
            }
            "border-top" => {
                if let Some(edge) = parse_border_edge(value) {
                    style.border_top = edge;
                }
            }
            "border-right" => {
                if let Some(edge) = parse_border_edge(value) {
                    style.border_right = edge;
                }
            }
            "border-bottom" => {
                if let Some(edge) = parse_border_edge(value) {
                    style.border_bottom = edge;
                }
            }
            "border-left" => {
                if let Some(edge) = parse_border_edge(value) {
                    style.border_left = edge;
                }
            }
            "border" => {
                if let Some(edges) = parse_border_shorthand(value) {
                    style.border_top = edges.0;
                    style.border_right = edges.1;
                    style.border_bottom = edges.2;
                    style.border_left = edges.3;
                }
            }
            "grid-size-columns" => {
                if let Ok(n) = value.trim().parse::<u16>() {
                    style.grid_size_columns = Some(n);
                }
            }
            "grid-size-rows" => {
                if let Ok(n) = value.trim().parse::<u16>() {
                    style.grid_size_rows = Some(n);
                }
            }
            "grid-size" => {
                let parts: Vec<&str> = value.split_whitespace().collect();
                match parts.len() {
                    1 => {
                        if let Ok(cols) = parts[0].parse::<u16>() {
                            style.grid_size_columns = Some(cols);
                            style.grid_size_rows = Some(0);
                        }
                    }
                    2 => {
                        if let (Ok(cols), Ok(rows)) =
                            (parts[0].parse::<u16>(), parts[1].parse::<u16>())
                        {
                            style.grid_size_columns = Some(cols);
                            style.grid_size_rows = Some(rows);
                        }
                    }
                    _ => {}
                }
            }
            "grid-columns" => {
                let parsed: Vec<Option<Scalar>> = value
                    .split_whitespace()
                    .map(|token| parse_scalar(token))
                    .collect();
                if !parsed.is_empty() && parsed.iter().all(|s| s.is_some()) {
                    style.grid_columns = Some(parsed.into_iter().map(|s| s.unwrap()).collect());
                }
            }
            "grid-rows" => {
                let parsed: Vec<Option<Scalar>> = value
                    .split_whitespace()
                    .map(|token| parse_scalar(token))
                    .collect();
                if !parsed.is_empty() && parsed.iter().all(|s| s.is_some()) {
                    style.grid_rows = Some(parsed.into_iter().map(|s| s.unwrap()).collect());
                }
            }
            "grid-gutter-horizontal" => {
                if let Ok(n) = value.trim().parse::<u16>() {
                    style.grid_gutter_horizontal = Some(n);
                }
            }
            "grid-gutter-vertical" => {
                if let Ok(n) = value.trim().parse::<u16>() {
                    style.grid_gutter_vertical = Some(n);
                }
            }
            "grid-gutter" => {
                let parts: Vec<&str> = value.split_whitespace().collect();
                match parts.len() {
                    1 => {
                        if let Ok(v) = parts[0].parse::<u16>() {
                            style.grid_gutter_horizontal = Some(v);
                            style.grid_gutter_vertical = Some(v);
                        }
                    }
                    2 => {
                        if let (Ok(h), Ok(v)) = (parts[0].parse::<u16>(), parts[1].parse::<u16>()) {
                            style.grid_gutter_horizontal = Some(h);
                            style.grid_gutter_vertical = Some(v);
                        }
                    }
                    _ => {}
                }
            }
            "layer" => {
                let name = value.trim();
                if !name.is_empty() {
                    style.layer = Some(name.to_string());
                }
            }
            "layers" => {
                let names: Vec<String> = value
                    .split_whitespace()
                    .filter(|t| !t.is_empty())
                    .map(|t| t.to_string())
                    .collect();
                if !names.is_empty() {
                    style.layers = Some(names);
                }
            }
            "text-align" => {
                style.text_align = parse_text_align(value);
            }
            "content-align" => {
                if let Some(ca) = parse_content_align(value) {
                    style.content_align = Some(ca);
                }
            }
            "align" => {
                if let Some(a) = parse_align(value) {
                    style.align = Some(a);
                }
            }
            "align-horizontal" => {
                if let Some(h) = parse_horizontal_align(value) {
                    let existing = style.align.unwrap_or(Align {
                        horizontal: HorizontalAlign::Left,
                        vertical: VerticalAlign::Top,
                    });
                    style.align = Some(Align {
                        horizontal: h,
                        vertical: existing.vertical,
                    });
                }
            }
            "align-vertical" => {
                if let Some(v) = parse_vertical_align(value) {
                    let existing = style.align.unwrap_or(Align {
                        horizontal: HorizontalAlign::Left,
                        vertical: VerticalAlign::Top,
                    });
                    style.align = Some(Align {
                        horizontal: existing.horizontal,
                        vertical: v,
                    });
                }
            }
            "content-align-horizontal" => {
                if let Some(h) = parse_horizontal_align(value) {
                    let existing = style.content_align.unwrap_or(ContentAlign {
                        horizontal: HorizontalAlign::Left,
                        vertical: VerticalAlign::Top,
                    });
                    style.content_align = Some(ContentAlign {
                        horizontal: h,
                        vertical: existing.vertical,
                    });
                }
            }
            "content-align-vertical" => {
                if let Some(v) = parse_vertical_align(value) {
                    let existing = style.content_align.unwrap_or(ContentAlign {
                        horizontal: HorizontalAlign::Left,
                        vertical: VerticalAlign::Top,
                    });
                    style.content_align = Some(ContentAlign {
                        horizontal: existing.horizontal,
                        vertical: v,
                    });
                }
            }
            "offset" => {
                if let Some(o) = parse_offset(value) {
                    style.offset = Some(o);
                }
            }
            "offset-x" => {
                if let Some(x) = parse_offset_value(value.trim()) {
                    let existing = style.offset.unwrap_or_default();
                    style.offset = Some(Offset { x, y: existing.y });
                }
            }
            "offset-y" => {
                if let Some(y) = parse_offset_value(value.trim()) {
                    let existing = style.offset.unwrap_or_default();
                    style.offset = Some(Offset { x: existing.x, y });
                }
            }
            "constrain" => {
                let parse_constrain = |t: &str| match t.to_lowercase().as_str() {
                    "none" => Some(Constrain::None),
                    "inside" => Some(Constrain::Inside),
                    "inflect" => Some(Constrain::Inflect),
                    _ => None,
                };
                let tokens: Vec<&str> = value.split_whitespace().collect();
                match tokens.len() {
                    1 => {
                        let v = parse_constrain(tokens[0]);
                        style.constrain = v;
                        style.constrain_x = v;
                        style.constrain_y = v;
                    }
                    2 => {
                        style.constrain_x = parse_constrain(tokens[0]);
                        style.constrain_y = parse_constrain(tokens[1]);
                    }
                    _ => {}
                }
            }
            "pointer" => {
                style.pointer = match value.trim().to_lowercase().as_str() {
                    "default" => Some(Pointer::Default),
                    "pointer" => Some(Pointer::Pointer),
                    "text" => Some(Pointer::Text),
                    "not-allowed" => Some(Pointer::NotAllowed),
                    _ => None,
                };
            }
            // --- P2 CSS gap properties (P2-24..P2-36) ---
            "position" => {
                style.position = match value.trim().to_lowercase().as_str() {
                    "relative" => Some(Position::Relative),
                    "absolute" => Some(Position::Absolute),
                    _ => None,
                };
            }
            "box-sizing" => {
                style.box_sizing = match value.trim().to_lowercase().as_str() {
                    "content-box" => Some(BoxSizing::ContentBox),
                    "border-box" => Some(BoxSizing::BorderBox),
                    _ => None,
                };
            }
            "split" => {
                style.split = match value.trim().to_lowercase().as_str() {
                    "top" => Some(Split::Top),
                    "right" => Some(Split::Right),
                    "bottom" => Some(Split::Bottom),
                    "left" => Some(Split::Left),
                    _ => None,
                };
            }
            // P2-27: individual spacing sides (stored as per-side fields, not on aggregate)
            "padding-top" => {
                if let Ok(v) = value.trim().parse::<u16>() {
                    style.padding_top = Some(v);
                }
            }
            "padding-right" => {
                if let Ok(v) = value.trim().parse::<u16>() {
                    style.padding_right = Some(v);
                }
            }
            "padding-bottom" => {
                if let Ok(v) = value.trim().parse::<u16>() {
                    style.padding_bottom = Some(v);
                }
            }
            "padding-left" => {
                if let Ok(v) = value.trim().parse::<u16>() {
                    style.padding_left = Some(v);
                }
            }
            "margin-top" => {
                if let Ok(v) = value.trim().parse::<u16>() {
                    style.margin_top = Some(v);
                }
            }
            "margin-right" => {
                if let Ok(v) = value.trim().parse::<u16>() {
                    style.margin_right = Some(v);
                }
            }
            "margin-bottom" => {
                if let Ok(v) = value.trim().parse::<u16>() {
                    style.margin_bottom = Some(v);
                }
            }
            "margin-left" => {
                if let Ok(v) = value.trim().parse::<u16>() {
                    style.margin_left = Some(v);
                }
            }
            // P2-28: outline
            "outline" => {
                if let Some(edges) = parse_border_shorthand(value) {
                    style.outline_top = edges.0;
                    style.outline_right = edges.1;
                    style.outline_bottom = edges.2;
                    style.outline_left = edges.3;
                }
            }
            "outline-top" => {
                if let Some(edge) = parse_border_edge(value) {
                    style.outline_top = edge;
                }
            }
            "outline-right" => {
                if let Some(edge) = parse_border_edge(value) {
                    style.outline_right = edge;
                }
            }
            "outline-bottom" => {
                if let Some(edge) = parse_border_edge(value) {
                    style.outline_bottom = edge;
                }
            }
            "outline-left" => {
                if let Some(edge) = parse_border_edge(value) {
                    style.outline_left = edge;
                }
            }
            // P2-29: border title/subtitle styling
            "border-title-align" => {
                style.border_title_align = parse_horizontal_align(value);
            }
            "border-subtitle-align" => {
                style.border_subtitle_align = parse_horizontal_align(value);
            }
            "border-title-color" => {
                if let Some(color) = parse_color_like(value) {
                    style.border_title_color = Some(color);
                }
            }
            "border-title-background" => {
                if let Some(color) = parse_color_like(value) {
                    style.border_title_background = Some(color);
                }
            }
            "border-title-style" => {
                style.border_title_style = parse_text_style_flags(value);
            }
            "border-subtitle-color" => {
                if let Some(color) = parse_color_like(value) {
                    style.border_subtitle_color = Some(color);
                }
            }
            "border-subtitle-background" => {
                if let Some(color) = parse_color_like(value) {
                    style.border_subtitle_background = Some(color);
                }
            }
            "border-subtitle-style" => {
                style.border_subtitle_style = parse_text_style_flags(value);
            }
            // P2-30: scrollbar CSS
            "scrollbar-color" => {
                if let Some(color) = parse_color_like(value) {
                    style.scrollbar_color = Some(color);
                }
            }
            "scrollbar-color-hover" => {
                if let Some(color) = parse_color_like(value) {
                    style.scrollbar_color_hover = Some(color);
                }
            }
            "scrollbar-color-active" => {
                if let Some(color) = parse_color_like(value) {
                    style.scrollbar_color_active = Some(color);
                }
            }
            "scrollbar-background" => {
                if let Some(color) = parse_color_like(value) {
                    style.scrollbar_background = Some(color);
                }
            }
            "scrollbar-background-hover" => {
                if let Some(color) = parse_color_like(value) {
                    style.scrollbar_background_hover = Some(color);
                }
            }
            "scrollbar-background-active" => {
                if let Some(color) = parse_color_like(value) {
                    style.scrollbar_background_active = Some(color);
                }
            }
            "scrollbar-corner-color" => {
                if let Some(color) = parse_color_like(value) {
                    style.scrollbar_corner_color = Some(color);
                }
            }
            "scrollbar-gutter" => {
                style.scrollbar_gutter = match value.trim().to_lowercase().as_str() {
                    "auto" => Some(ScrollbarGutter::Auto),
                    "stable" => Some(ScrollbarGutter::Stable),
                    _ => None,
                };
            }
            "scrollbar-size" => {
                if let Ok(n) = value.trim().parse::<u16>() {
                    style.scrollbar_size = Some(n);
                }
            }
            "scrollbar-size-horizontal" => {
                if let Ok(n) = value.trim().parse::<u16>() {
                    style.scrollbar_size_horizontal = Some(n);
                }
            }
            "scrollbar-size-vertical" => {
                if let Ok(n) = value.trim().parse::<u16>() {
                    style.scrollbar_size_vertical = Some(n);
                }
            }
            "scrollbar-visibility" => {
                style.scrollbar_visibility = match value.trim().to_lowercase().as_str() {
                    "auto" => Some(ScrollbarVisibility::Auto),
                    "hidden" => Some(ScrollbarVisibility::Hidden),
                    "visible" => Some(ScrollbarVisibility::Visible),
                    _ => None,
                };
            }
            // P2-31: text-wrap, text-overflow
            "text-wrap" => {
                style.text_wrap = match value.trim().to_lowercase().as_str() {
                    "wrap" => Some(TextWrap::Wrap),
                    "nowrap" | "no-wrap" => Some(TextWrap::NoWrap),
                    _ => None,
                };
            }
            "text-overflow" => {
                style.text_overflow = match value.trim().to_lowercase().as_str() {
                    "clip" => Some(TextOverflow::Clip),
                    "fold" => Some(TextOverflow::Fold),
                    "ellipsis" => Some(TextOverflow::Ellipsis),
                    _ => None,
                };
            }
            // P2-32: link styling
            "link-color" => {
                if let Some(color) = parse_color_like(value) {
                    style.link_color = Some(color);
                }
            }
            "link-background" => {
                if let Some(color) = parse_color_like(value) {
                    style.link_background = Some(color);
                }
            }
            "link-style" => {
                style.link_style = parse_link_style_value(value);
            }
            "link-color-hover" => {
                if let Some(color) = parse_color_like(value) {
                    style.link_color_hover = Some(color);
                }
            }
            "link-background-hover" => {
                if let Some(color) = parse_color_like(value) {
                    style.link_background_hover = Some(color);
                }
            }
            "link-style-hover" => {
                style.link_style_hover = parse_link_style_value(value);
            }
            // P2-33: grid child placement
            "row-span" => {
                if let Ok(n) = value.trim().parse::<u16>() {
                    style.row_span = Some(n);
                }
            }
            "column-span" => {
                if let Ok(n) = value.trim().parse::<u16>() {
                    style.column_span = Some(n);
                }
            }
            // P2-34: hatch, overlay, keyline
            "hatch" => {
                let parts: Vec<&str> = value.split_whitespace().collect();
                if parts.len() >= 2 {
                    let ch = parts[0].chars().next();
                    // Remaining tokens form the color value.
                    let color_str = parts[1..].join(" ");
                    if let (Some(character), Some(color)) = (ch, parse_color_like(&color_str)) {
                        style.hatch = Some(Hatch { character, color });
                    }
                }
            }
            "overlay" => {
                style.overlay = match value.trim().to_lowercase().as_str() {
                    "none" => Some(OverlayMode::None),
                    "screen" => Some(OverlayMode::Screen),
                    _ => None,
                };
            }
            "keyline" => {
                let parts: Vec<&str> = value.split_whitespace().collect();
                if parts.len() >= 2 {
                    let keyline_type = match parts[0].to_lowercase().as_str() {
                        "none" => Some(KeylineType::None),
                        "thin" => Some(KeylineType::Thin),
                        "heavy" => Some(KeylineType::Heavy),
                        "double" => Some(KeylineType::Double),
                        _ => None,
                    };
                    let color_str = parts[1..].join(" ");
                    if let (Some(kt), Some(color)) = (keyline_type, parse_color_like(&color_str)) {
                        style.keyline = Some(Keyline {
                            keyline_type: kt,
                            color,
                        });
                    }
                }
            }
            // P2-35: constrain-x, constrain-y, expand
            "constrain-x" => {
                style.constrain_x = match value.trim().to_lowercase().as_str() {
                    "none" => Some(Constrain::None),
                    "inside" => Some(Constrain::Inside),
                    "inflect" => Some(Constrain::Inflect),
                    _ => None,
                };
            }
            "constrain-y" => {
                style.constrain_y = match value.trim().to_lowercase().as_str() {
                    "none" => Some(Constrain::None),
                    "inside" => Some(Constrain::Inside),
                    "inflect" => Some(Constrain::Inflect),
                    _ => None,
                };
            }
            "expand" => {
                if let Some(val) = parse_bool(value) {
                    style.expand = Some(val);
                }
            }
            _ => {}
        }
        // For non-shorthand properties, apply importance generically.
        if is_important && !handled_importance {
            for prop in importance_properties_for_key(&key) {
                style.importance.set(*prop);
            }
        }
    }
    style
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_margin(value: &str) -> Option<Margin> {
    parse_spacing(value)
}

fn parse_spacing(value: &str) -> Option<crate::style::Spacing> {
    let parts: Vec<&str> = value
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .collect();
    let nums: Vec<u16> = parts.iter().filter_map(|part| part.parse().ok()).collect();
    match nums.len() {
        1 => Some(crate::style::Spacing::all(nums[0])),
        2 => Some(crate::style::Spacing::vertical_horizontal(nums[0], nums[1])),
        3 => Some(crate::style::Spacing::new(
            nums[0], nums[1], nums[2], nums[1],
        )),
        4 => Some(crate::style::Spacing::new(
            nums[0], nums[1], nums[2], nums[3],
        )),
        _ => None,
    }
}

fn parse_border_edge(value: &str) -> Option<BorderEdge> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(BorderEdge::None);
    }
    let mut tokens = value.split_whitespace().filter(|t| !t.is_empty());
    let first = tokens.next()?;
    let (border_type, rest_tokens): (BorderType, Vec<&str>) = match first.to_lowercase().as_str() {
        "tall" => (BorderType::Tall, tokens.collect()),
        "block" => (BorderType::Block, tokens.collect()),
        "heavy" => (BorderType::Heavy, tokens.collect()),
        "solid" => (BorderType::Solid, tokens.collect()),
        "outer" => (BorderType::Outer, tokens.collect()),
        "hkey" => (BorderType::HKey, tokens.collect()),
        "vkey" => (BorderType::VKey, tokens.collect()),
        // If the first token isn't a border type, treat it as a color token and default
        // to `solid`.
        _ => (
            BorderType::Solid,
            std::iter::once(first).chain(tokens).collect(),
        ),
    };
    let mut color: Option<crate::style::Color> = None;
    let mut alpha_percent: Option<u8> = None;
    for token in rest_tokens {
        if let Some(raw) = token.strip_suffix('%') {
            if let Ok(v) = raw.parse::<u8>() {
                alpha_percent = Some(v.min(100));
                continue;
            }
        }
        if let Some(c) = parse_color_like(token) {
            color = Some(c);
        }
    }
    let mut color = color?;
    if let Some(p) = alpha_percent {
        color = color.with_alpha(p as f32 / 100.0);
    }
    Some(BorderEdge::Edge { border_type, color })
}

fn parse_border_shorthand(value: &str) -> Option<(BorderEdge, BorderEdge, BorderEdge, BorderEdge)> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some((
            BorderEdge::None,
            BorderEdge::None,
            BorderEdge::None,
            BorderEdge::None,
        ));
    }
    let mut tokens = value.split_whitespace().filter(|t| !t.is_empty());
    let kind = tokens.next()?.to_lowercase();
    let border_type = match kind.as_str() {
        "block" => BorderType::Block,
        "solid" => BorderType::Solid,
        "heavy" => BorderType::Heavy,
        "tall" => BorderType::Tall,
        "outer" => BorderType::Outer,
        "hkey" => BorderType::HKey,
        "vkey" => BorderType::VKey,
        _ => return None,
    };
    let mut color: Option<crate::style::Color> = None;
    let mut alpha_percent: Option<u8> = None;
    for token in tokens {
        if let Some(raw) = token.strip_suffix('%') {
            if let Ok(v) = raw.parse::<u8>() {
                alpha_percent = Some(v.min(100));
                continue;
            }
        }
        if let Some(c) = parse_color_like(token) {
            color = Some(c);
        }
    }
    let mut color = color?;
    if let Some(p) = alpha_percent {
        color = color.with_alpha(p as f32 / 100.0);
    }
    let edge = BorderEdge::Edge { border_type, color };
    Some((edge, edge, edge, edge))
}

fn parse_tint(value: &str) -> Option<Tint> {
    // Format: "<color> <percent>%" (percent is optional, defaults to 0).
    let mut color: Option<crate::style::Color> = None;
    let mut percent: Option<u8> = None;
    for token in value.split_whitespace().filter(|t| !t.is_empty()) {
        if let Some(raw) = token.strip_suffix('%') {
            if let Ok(v) = raw.parse::<u8>() {
                percent = Some(v);
                continue;
            }
        }
        if let Some(c) = parse_color_like(token) {
            color = Some(c);
        }
    }
    Some(Tint::new(color?, percent.unwrap_or(0)))
}

fn parse_opacity_percent(value: &str) -> Option<u8> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(raw) = value.strip_suffix('%') {
        return raw.trim().parse::<u8>().ok().map(|v| v.min(100));
    }
    let value: f32 = value.parse().ok()?;
    if value.is_sign_negative() {
        return Some(0);
    }
    if value > 1.0 {
        return Some((value.round() as i32).clamp(0, 100) as u8);
    }
    Some((value * 100.0).round().clamp(0.0, 100.0) as u8)
}

/// Parse a single transition shorthand item (backward-compat helper).
#[allow(dead_code)]
pub(super) fn parse_transition_shorthand(
    value: &str,
) -> Option<(Option<Duration>, Option<Duration>, Option<TransitionTiming>)> {
    // Parse only the first transition item in a comma-separated declaration.
    // Example: "offset 300ms ease-in-out 50ms".
    let first_item = value.split(',').next()?.trim();
    if first_item.is_empty() {
        return None;
    }
    let mut duration: Option<Duration> = None;
    let mut delay: Option<Duration> = None;
    let mut timing: Option<TransitionTiming> = None;

    for token in first_item.split_whitespace() {
        if duration.is_none() {
            if let Some(parsed) = parse_duration(token) {
                duration = Some(parsed);
                continue;
            }
        } else if delay.is_none() {
            if let Some(parsed) = parse_duration(token) {
                delay = Some(parsed);
                continue;
            }
        }
        if timing.is_none() {
            timing = parse_transition_timing(token);
        }
    }

    Some((duration, delay, timing))
}

pub(super) fn parse_duration(value: &str) -> Option<Duration> {
    let token = value.trim().to_lowercase();
    if token.is_empty() {
        return None;
    }
    if let Some(raw) = token.strip_suffix("ms") {
        let ms: f64 = raw.trim().parse().ok()?;
        if ms.is_sign_negative() {
            return None;
        }
        return Some(Duration::from_secs_f64(ms / 1000.0));
    }
    if let Some(raw) = token.strip_suffix('s') {
        let secs: f64 = raw.trim().parse().ok()?;
        if secs.is_sign_negative() {
            return None;
        }
        return Some(Duration::from_secs_f64(secs));
    }
    None
}

fn parse_scalar(value: &str) -> Option<Scalar> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.eq_ignore_ascii_case("auto") {
        return Some(Scalar::Auto);
    }
    if let Some(raw) = value.strip_suffix('%') {
        return raw.trim().parse::<f32>().ok().map(Scalar::Percent);
    }
    if let Some(raw) = value.strip_suffix("fr") {
        return raw.trim().parse::<f32>().ok().map(Scalar::Fraction);
    }
    if let Some(raw) = value.strip_suffix("vw") {
        return raw.trim().parse::<f32>().ok().map(Scalar::ViewWidth);
    }
    if let Some(raw) = value.strip_suffix("vh") {
        return raw.trim().parse::<f32>().ok().map(Scalar::ViewHeight);
    }
    value.parse::<u16>().ok().map(Scalar::Cells)
}

fn parse_layout(value: &str) -> Option<Layout> {
    match value.trim().to_lowercase().as_str() {
        "horizontal" => Some(Layout::Horizontal),
        "vertical" => Some(Layout::Vertical),
        "grid" => Some(Layout::Grid),
        _ => None,
    }
}

fn parse_display(value: &str) -> Option<Display> {
    match value.trim().to_lowercase().as_str() {
        "block" => Some(Display::Block),
        "none" => Some(Display::None),
        _ => None,
    }
}

fn parse_visibility(value: &str) -> Option<Visibility> {
    match value.trim().to_lowercase().as_str() {
        "visible" => Some(Visibility::Visible),
        "hidden" => Some(Visibility::Hidden),
        _ => None,
    }
}

fn parse_overflow(value: &str) -> Option<Overflow> {
    match value.trim().to_lowercase().as_str() {
        "auto" => Some(Overflow::Auto),
        "hidden" => Some(Overflow::Hidden),
        "scroll" => Some(Overflow::Scroll),
        _ => None,
    }
}

fn parse_dock(value: &str) -> Option<Dock> {
    match value.trim().to_lowercase().as_str() {
        "top" => Some(Dock::Top),
        "right" => Some(Dock::Right),
        "bottom" => Some(Dock::Bottom),
        "left" => Some(Dock::Left),
        _ => None,
    }
}

fn parse_text_align(value: &str) -> Option<TextAlign> {
    match value.trim().to_lowercase().as_str() {
        "left" | "start" => Some(TextAlign::Left),
        "center" => Some(TextAlign::Center),
        "right" | "end" => Some(TextAlign::Right),
        "justify" => Some(TextAlign::Justify),
        _ => None,
    }
}

fn parse_horizontal_align(value: &str) -> Option<HorizontalAlign> {
    match value.trim().to_lowercase().as_str() {
        "left" | "start" => Some(HorizontalAlign::Left),
        "center" => Some(HorizontalAlign::Center),
        "right" | "end" => Some(HorizontalAlign::Right),
        _ => None,
    }
}

fn parse_vertical_align(value: &str) -> Option<VerticalAlign> {
    match value.trim().to_lowercase().as_str() {
        "top" | "start" => Some(VerticalAlign::Top),
        "middle" | "center" => Some(VerticalAlign::Middle),
        "bottom" | "end" => Some(VerticalAlign::Bottom),
        _ => None,
    }
}

fn parse_content_align(value: &str) -> Option<ContentAlign> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let horizontal = parse_horizontal_align(parts[0])?;
    let vertical = parse_vertical_align(parts[1])?;
    Some(ContentAlign {
        horizontal,
        vertical,
    })
}

fn parse_align(value: &str) -> Option<Align> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let horizontal = parse_horizontal_align(parts[0])?;
    let vertical = parse_vertical_align(parts[1])?;
    Some(Align {
        horizontal,
        vertical,
    })
}

fn parse_offset(value: &str) -> Option<Offset> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let x = parse_offset_value(parts[0].trim())?;
    let y = parse_offset_value(parts[1].trim())?;
    Some(Offset { x, y })
}

/// Parse a single offset axis value: either `N` (cells) or `N%` (percent).
fn parse_offset_value(s: &str) -> Option<OffsetValue> {
    let s = s.trim();
    if let Some(raw) = s.strip_suffix('%') {
        raw.parse::<f32>().ok().map(OffsetValue::Percent)
    } else {
        s.parse::<i16>().ok().map(OffsetValue::Cells)
    }
}

/// Parse `bg`/`fg` color value with optional `N%` alpha suffix.
/// Handles `$token N%`, `#hex N%`, `colorname N%`, etc.
fn parse_color_like_with_alpha(value: &str) -> Option<(crate::style::Color, Option<u8>)> {
    // If value has no spaces, fast-path to parse_color_like (no alpha token possible).
    if !value.contains(' ') {
        return parse_color_like(value).map(|c| (c, None));
    }
    // Check if the last whitespace-separated token is a percentage (e.g. `$bg 60%`).
    // If not, try the full value as a color (handles `rgb(210, 210, 210)` etc.).
    let last_token = value.split_whitespace().last().unwrap_or("");
    if let Some(raw) = last_token.strip_suffix('%')
        && let Ok(v) = raw.parse::<u8>()
    {
        // Everything except the last token is the color part.
        let end = value.rfind(last_token).unwrap_or(value.len());
        let color_part = value[..end].trim();
        if let Some(color) = parse_color_like(color_part) {
            return Some((color, Some(v.min(100))));
        }
    }
    // No valid percentage suffix; try the full value as a color.
    parse_color_like(value).map(|c| (c, None))
}

/// Parse link-style value with token resolution and `not` negation support.
fn parse_link_style_value(value: &str) -> Option<TextStyleFlags> {
    let mut flags = TextStyleFlags::default();
    let mut any = false;
    let mut pending_not = false;
    for token in value.split(|c: char| c == ' ' || c == ',' || c == '|') {
        let token = token.trim().to_ascii_lowercase();
        if token.is_empty() {
            continue;
        }
        if token == "not" {
            pending_not = true;
            continue;
        }
        if token == "none" {
            return Some(TextStyleFlags::default());
        }
        if let Some(resolved) = resolve_text_style_token_flags(token.as_str()) {
            let val = !pending_not;
            if resolved.bold {
                flags.bold = val;
                any = true;
            }
            if resolved.dim {
                flags.dim = val;
                any = true;
            }
            if resolved.italic {
                flags.italic = val;
                any = true;
            }
            if resolved.underline {
                flags.underline = val;
                any = true;
            }
            if resolved.reverse {
                flags.reverse = val;
                any = true;
            }
            if resolved.strike {
                flags.strike = val;
                any = true;
            }
        }
        pending_not = false;
    }
    if any { Some(flags) } else { None }
}

fn parse_text_style_flags(value: &str) -> Option<TextStyleFlags> {
    let mut flags = TextStyleFlags::default();
    let mut any = false;
    for token in value.split(|c: char| c == ' ' || c == ',' || c == '|') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        match token.to_lowercase().as_str() {
            "bold" => {
                flags.bold = true;
                any = true;
            }
            "dim" => {
                flags.dim = true;
                any = true;
            }
            "italic" => {
                flags.italic = true;
                any = true;
            }
            "underline" => {
                flags.underline = true;
                any = true;
            }
            "reverse" => {
                flags.reverse = true;
                any = true;
            }
            "strike" | "strikethrough" => {
                flags.strike = true;
                any = true;
            }
            "none" => {
                return Some(TextStyleFlags::default());
            }
            _ => {}
        }
    }
    if any { Some(flags) } else { None }
}

fn parse_text_style_shorthand_into_style(style: &mut Style, value: &str, is_important: bool) {
    let mut pending_not = false;
    for token in value.split(|c: char| c == ' ' || c == ',' || c == '|') {
        let token = token.trim().to_ascii_lowercase();
        if token.is_empty() {
            continue;
        }
        if token == "not" {
            pending_not = true;
            continue;
        }
        if token == "none" {
            // Keep existing behavior: `none` in `text-style` does not force bool fields.
            pending_not = false;
            continue;
        }
        if let Some(flags) = resolve_text_style_token_flags(token.as_str()) {
            let value = !pending_not;
            if flags.bold {
                apply_text_style_flag(style, "bold", value, is_important);
            }
            if flags.dim {
                apply_text_style_flag(style, "dim", value, is_important);
            }
            if flags.italic {
                apply_text_style_flag(style, "italic", value, is_important);
            }
            if flags.underline {
                apply_text_style_flag(style, "underline", value, is_important);
            }
            if flags.reverse {
                apply_text_style_flag(style, "reverse", value, is_important);
            }
            if flags.strike {
                apply_text_style_flag(style, "strike", value, is_important);
            }
        }
        pending_not = false;
    }
}

fn apply_text_style_flag(style: &mut Style, flag: &str, value: bool, is_important: bool) {
    match flag {
        "bold" => {
            style.bold = Some(value);
            if is_important {
                style.importance.set(StyleProperty::Bold);
            }
        }
        "dim" => {
            style.dim = Some(value);
            if is_important {
                style.importance.set(StyleProperty::Dim);
            }
        }
        "italic" => {
            style.italic = Some(value);
            if is_important {
                style.importance.set(StyleProperty::Italic);
            }
        }
        "underline" => {
            style.underline = Some(value);
            if is_important {
                style.importance.set(StyleProperty::Underline);
            }
        }
        "reverse" => {
            style.reverse = Some(value);
            if is_important {
                style.importance.set(StyleProperty::Reverse);
            }
        }
        "strike" | "strikethrough" => {
            style.strike = Some(value);
            if is_important {
                style.importance.set(StyleProperty::Strike);
            }
        }
        _ => {}
    }
}

pub(super) fn parse_transition_timing(value: &str) -> Option<TransitionTiming> {
    match value.trim().to_lowercase().as_str() {
        "linear" => Some(TransitionTiming::Linear),
        "ease" | "ease-in-out" => Some(TransitionTiming::InOutCubic),
        "ease-out" => Some(TransitionTiming::OutCubic),
        "none" => Some(TransitionTiming::None),
        "round" | "step-end" | "steps(1,end)" => Some(TransitionTiming::Round),
        "in-out-cubic" | "in_out_cubic" => Some(TransitionTiming::InOutCubic),
        "out-cubic" | "out_cubic" => Some(TransitionTiming::OutCubic),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Scalar;

    #[test]
    fn parse_grid_size_one_value() {
        let style = parse_style_body("grid-size: 3;");
        assert_eq!(style.grid_size_columns, Some(3));
        assert_eq!(style.grid_size_rows, Some(0));
    }

    #[test]
    fn parse_grid_size_two_values() {
        let style = parse_style_body("grid-size: 3 2;");
        assert_eq!(style.grid_size_columns, Some(3));
        assert_eq!(style.grid_size_rows, Some(2));
    }

    #[test]
    fn parse_grid_columns_scalars() {
        let style = parse_style_body("grid-columns: 1fr 2fr 30;");
        let cols = style.grid_columns.expect("grid_columns should be Some");
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[0], Scalar::Fraction(1.0));
        assert_eq!(cols[1], Scalar::Fraction(2.0));
        assert_eq!(cols[2], Scalar::Cells(30));
    }

    #[test]
    fn parse_grid_rows_scalars() {
        let style = parse_style_body("grid-rows: auto 1fr;");
        let rows = style.grid_rows.expect("grid_rows should be Some");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], Scalar::Auto);
        assert_eq!(rows[1], Scalar::Fraction(1.0));
    }

    #[test]
    fn parse_grid_gutter_one_value() {
        let style = parse_style_body("grid-gutter: 1;");
        assert_eq!(style.grid_gutter_horizontal, Some(1));
        assert_eq!(style.grid_gutter_vertical, Some(1));
    }

    #[test]
    fn parse_grid_gutter_two_values() {
        let style = parse_style_body("grid-gutter: 2 1;");
        assert_eq!(style.grid_gutter_horizontal, Some(2));
        assert_eq!(style.grid_gutter_vertical, Some(1));
    }

    #[test]
    fn parse_grid_gutter_individual() {
        let style = parse_style_body("grid-gutter-horizontal: 3; grid-gutter-vertical: 5;");
        assert_eq!(style.grid_gutter_horizontal, Some(3));
        assert_eq!(style.grid_gutter_vertical, Some(5));
    }

    #[test]
    fn parse_grid_size_individual() {
        let style = parse_style_body("grid-size-columns: 4; grid-size-rows: 2;");
        assert_eq!(style.grid_size_columns, Some(4));
        assert_eq!(style.grid_size_rows, Some(2));
    }

    #[test]
    fn parse_grid_columns_rejects_invalid_token() {
        let style = parse_style_body("grid-columns: 1fr bogus 2fr;");
        assert!(style.grid_columns.is_none());
    }

    #[test]
    fn parse_grid_rows_rejects_invalid_token() {
        let style = parse_style_body("grid-rows: auto nope;");
        assert!(style.grid_rows.is_none());
    }

    // ---- Layer property parsing tests ----

    #[test]
    fn parse_layer_property() {
        let style = parse_style_body("layer: foo;");
        assert_eq!(style.layer.as_deref(), Some("foo"));
    }

    #[test]
    fn parse_layer_property_trims_whitespace() {
        let style = parse_style_body("layer:   bar  ;");
        assert_eq!(style.layer.as_deref(), Some("bar"));
    }

    #[test]
    fn parse_layer_empty_value_is_none() {
        let style = parse_style_body("layer: ;");
        assert!(style.layer.is_none());
    }

    #[test]
    fn parse_layers_property() {
        let style = parse_style_body("layers: base overlay dialog;");
        let layers = style.layers.expect("layers should be Some");
        assert_eq!(layers, vec!["base", "overlay", "dialog"]);
    }

    #[test]
    fn parse_layers_single_name() {
        let style = parse_style_body("layers: default;");
        let layers = style.layers.expect("layers should be Some");
        assert_eq!(layers, vec!["default"]);
    }

    #[test]
    fn parse_layers_empty_value_is_none() {
        let style = parse_style_body("layers: ;");
        assert!(style.layers.is_none());
    }

    #[test]
    fn parse_layer_and_layers_together() {
        let style = parse_style_body("layer: overlay; layers: base overlay;");
        assert_eq!(style.layer.as_deref(), Some("overlay"));
        let layers = style.layers.expect("layers should be Some");
        assert_eq!(layers, vec!["base", "overlay"]);
    }

    #[test]
    fn parse_layer_via_stylesheet() {
        use super::super::ast::StyleSheet;
        let sheet = StyleSheet::parse(".dialog { layer: modal; }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].style.layer.as_deref(), Some("modal"));
    }

    #[test]
    fn parse_layers_via_stylesheet() {
        use super::super::ast::StyleSheet;
        let sheet = StyleSheet::parse("Screen { layers: base overlay; }");
        assert_eq!(sheet.rules.len(), 1);
        let layers = sheet.rules[0]
            .style
            .layers
            .as_ref()
            .expect("layers should be Some");
        assert_eq!(layers, &["base", "overlay"]);
    }

    // -- :focus-within parsing -----------------------------------------------

    #[test]
    fn parse_focus_within_pseudo_class() {
        let chain = parse_selector_chain("Container:focus-within").expect("should parse");
        assert_eq!(chain.parts.len(), 1);
        assert_eq!(chain.parts[0].pseudos(), &[PseudoClass::FocusWithin]);
    }

    #[test]
    fn parse_focus_within_with_underscore() {
        let chain = parse_selector_chain("Container:focus_within").expect("should parse");
        assert_eq!(chain.parts[0].pseudos(), &[PseudoClass::FocusWithin]);
    }

    #[test]
    fn parse_focus_within_combined_with_other_pseudo() {
        let chain = parse_selector_chain("Container:focus-within:dark").expect("should parse");
        assert_eq!(chain.parts.len(), 1);
        assert_eq!(
            chain.parts[0].pseudos(),
            &[PseudoClass::FocusWithin, PseudoClass::Dark]
        );
    }

    #[test]
    fn parse_focus_within_in_selector_chain() {
        let chain = parse_selector_chain("Form:focus-within > Input").expect("should parse");
        assert_eq!(chain.parts.len(), 2);
        assert_eq!(chain.parts[0].pseudos(), &[PseudoClass::FocusWithin]);
        assert!(chain.parts[1].pseudos().is_empty());
    }

    #[test]
    fn parse_app_runtime_bridge_pseudos() {
        let chain = parse_selector_chain("App:blur:inline:ansi:nocolor").expect("should parse");
        assert_eq!(chain.parts.len(), 1);
        assert_eq!(
            chain.parts[0].pseudos(),
            &[
                PseudoClass::Blur,
                PseudoClass::Inline,
                PseudoClass::Ansi,
                PseudoClass::NoColor
            ]
        );
    }

    // -- !important parsing -----------------------------------------------

    #[test]
    fn strip_important_basic() {
        assert_eq!(strip_important("red !important"), ("red", true));
        assert_eq!(strip_important("red"), ("red", false));
        assert_eq!(strip_important("red ! important"), ("red", true));
    }

    #[test]
    fn strip_important_case_insensitive() {
        assert_eq!(strip_important("red !IMPORTANT"), ("red", true));
        assert_eq!(strip_important("red !Important"), ("red", true));
    }

    #[test]
    fn strip_important_no_space_before_bang() {
        assert_eq!(strip_important("0!important"), ("0", true));
    }

    #[test]
    fn strip_important_short_value() {
        // Values shorter than 10 chars without "!important" are left unchanged.
        assert_eq!(strip_important("red"), ("red", false));
        assert_eq!(strip_important(""), ("", false));
    }

    #[test]
    fn parse_color_important_sets_flag() {
        let style = parse_style_body("color: red !important;");
        assert!(style.fg.is_some());
        assert!(
            style.importance.get(StyleProperty::Fg),
            "Fg importance should be set"
        );
    }

    #[test]
    fn parse_color_normal_no_flag() {
        let style = parse_style_body("color: red;");
        assert!(style.fg.is_some());
        assert!(
            !style.importance.get(StyleProperty::Fg),
            "Fg importance should NOT be set"
        );
    }

    #[test]
    fn parse_bg_important() {
        let style = parse_style_body("background: #ff0000 !important;");
        assert!(style.bg.is_some());
        assert!(style.importance.get(StyleProperty::Bg));
    }

    #[test]
    fn parse_width_important() {
        let style = parse_style_body("width: 50% !important;");
        assert_eq!(style.width, Some(Scalar::Percent(50.0)));
        assert!(style.importance.get(StyleProperty::Width));
    }

    #[test]
    fn parse_border_shorthand_important() {
        let style = parse_style_body("border: solid red !important;");
        assert!(style.border_top != crate::style::BorderEdge::Unset);
        assert!(style.importance.get(StyleProperty::BorderTop));
        assert!(style.importance.get(StyleProperty::BorderRight));
        assert!(style.importance.get(StyleProperty::BorderBottom));
        assert!(style.importance.get(StyleProperty::BorderLeft));
    }

    #[test]
    fn parse_border_shorthand_heavy() {
        let style = parse_style_body("border: heavy $primary;");
        let expected = crate::style::BorderEdge::Edge {
            border_type: BorderType::Heavy,
            color: crate::style::parse_color_like("$primary").expect("theme token should resolve"),
        };
        assert_eq!(style.border_top, expected);
        assert_eq!(style.border_right, expected);
        assert_eq!(style.border_bottom, expected);
        assert_eq!(style.border_left, expected);
    }

    #[test]
    fn parse_border_side_heavy() {
        let style = parse_style_body("border-left: heavy #336699;");
        assert_eq!(
            style.border_left,
            crate::style::BorderEdge::Edge {
                border_type: BorderType::Heavy,
                color: crate::style::parse_color_like("#336699").expect("hex token should resolve"),
            }
        );
    }

    #[test]
    fn parse_text_style_important_only_marks_set_properties() {
        let style = parse_style_body("text-style: bold italic !important;");
        assert_eq!(style.bold, Some(true));
        assert_eq!(style.italic, Some(true));
        assert!(style.importance.get(StyleProperty::Bold));
        assert!(style.importance.get(StyleProperty::Italic));
        // Dim, Underline, Reverse should NOT be marked important.
        assert!(!style.importance.get(StyleProperty::Dim));
        assert!(!style.importance.get(StyleProperty::Underline));
        assert!(!style.importance.get(StyleProperty::Reverse));
    }

    #[test]
    fn parse_text_style_not_flag_sets_explicit_false() {
        let style = parse_style_body("text-style: not reverse;");
        assert_eq!(style.reverse, Some(false));
    }

    #[test]
    fn parse_text_style_mixed_positive_and_not() {
        let style = parse_style_body("text-style: bold not underline;");
        assert_eq!(style.bold, Some(true));
        assert_eq!(style.underline, Some(false));
    }

    #[test]
    fn parse_text_style_multiple_flags_with_not() {
        let style = parse_style_body("text-style: bold italic not dim;");
        assert_eq!(style.bold, Some(true));
        assert_eq!(style.italic, Some(true));
        assert_eq!(style.dim, Some(false));
    }

    #[test]
    fn parse_text_style_none_keeps_existing_behavior() {
        let style = parse_style_body("text-style: none;");
        assert_eq!(style.bold, None);
        assert_eq!(style.dim, None);
        assert_eq!(style.italic, None);
        assert_eq!(style.underline, None);
        assert_eq!(style.reverse, None);
    }

    #[test]
    fn parse_text_style_token_refs_map_to_default_flags() {
        let style = parse_style_body("text-style: $button-focus-text-style;");
        assert_eq!(style.bold, Some(true));
        assert_eq!(style.reverse, Some(true));

        let style = parse_style_body("text-style: $block-cursor-text-style;");
        assert_eq!(style.bold, Some(true));

        let style = parse_style_body("text-style: $block-cursor-blurred-text-style;");
        assert_eq!(style.bold, None);
        assert_eq!(style.dim, None);
        assert_eq!(style.italic, None);
        assert_eq!(style.underline, None);
        assert_eq!(style.reverse, None);

        let style = parse_style_body("text-style: $input-cursor-text-style;");
        assert_eq!(style.bold, None);
        assert_eq!(style.dim, None);
        assert_eq!(style.italic, None);
        assert_eq!(style.underline, None);
        assert_eq!(style.reverse, None);
    }

    #[test]
    fn parse_multiple_declarations_mixed_importance() {
        let style = parse_style_body("color: red !important; bg: blue;");
        assert!(style.importance.get(StyleProperty::Fg));
        assert!(!style.importance.get(StyleProperty::Bg));
    }

    // -- Full cascade (StyleSheet) importance tests --

    #[test]
    fn cascade_important_wins_over_higher_specificity_normal() {
        use super::super::ast::{SelectorMeta, SelectorStates, StyleSheet};
        // .foo has lower specificity (10) but !important.
        // #bar has higher specificity (100) but normal.
        let sheet = StyleSheet::parse(".foo { color: red !important; } #bar { color: green; }");
        let meta = SelectorMeta {
            type_name: "Widget".to_string(),
            type_aliases: Vec::new(),
            id: Some("bar".to_string()),
            classes: vec!["foo".to_string()],
            states: SelectorStates::default(),
        };
        let style = sheet.style_for_meta(&meta);
        // "red" should win because of !important.
        assert_eq!(style.fg, Some(crate::style::Color::parse("red").unwrap()));
    }

    #[test]
    fn cascade_two_important_higher_specificity_wins() {
        use super::super::ast::{SelectorMeta, SelectorStates, StyleSheet};
        // Both rules have !important; higher specificity (#bar = 100) wins.
        let sheet =
            StyleSheet::parse(".foo { color: red !important; } #bar { color: green !important; }");
        let meta = SelectorMeta {
            type_name: "Widget".to_string(),
            type_aliases: Vec::new(),
            id: Some("bar".to_string()),
            classes: vec!["foo".to_string()],
            states: SelectorStates::default(),
        };
        let style = sheet.style_for_meta(&meta);
        assert_eq!(style.fg, Some(crate::style::Color::parse("green").unwrap()));
    }

    #[test]
    fn cascade_source_order_breaks_tie_same_specificity_and_importance() {
        use super::super::ast::{SelectorMeta, SelectorStates, StyleSheet};
        // Two class selectors, same specificity (10), both normal — later wins.
        let sheet = StyleSheet::parse(".a { color: red; } .b { color: green; }");
        let meta = SelectorMeta {
            type_name: "Widget".to_string(),
            type_aliases: Vec::new(),
            id: None,
            classes: vec!["a".to_string(), "b".to_string()],
            states: SelectorStates::default(),
        };
        let style = sheet.style_for_meta(&meta);
        assert_eq!(style.fg, Some(crate::style::Color::parse("green").unwrap()));
    }

    // ---- text-align parsing tests ----

    #[test]
    fn parse_text_align_center() {
        let style = parse_style_body("text-align: center;");
        assert_eq!(style.text_align, Some(crate::style::TextAlign::Center));
    }

    #[test]
    fn parse_text_align_right() {
        let style = parse_style_body("text-align: right;");
        assert_eq!(style.text_align, Some(crate::style::TextAlign::Right));
    }

    #[test]
    fn parse_text_align_left() {
        let style = parse_style_body("text-align: left;");
        assert_eq!(style.text_align, Some(crate::style::TextAlign::Left));
    }

    #[test]
    fn parse_text_align_justify() {
        let style = parse_style_body("text-align: justify;");
        assert_eq!(style.text_align, Some(crate::style::TextAlign::Justify));
    }

    #[test]
    fn parse_text_align_start_maps_to_left() {
        let style = parse_style_body("text-align: start;");
        assert_eq!(style.text_align, Some(crate::style::TextAlign::Left));
    }

    #[test]
    fn parse_text_align_end_maps_to_right() {
        let style = parse_style_body("text-align: end;");
        assert_eq!(style.text_align, Some(crate::style::TextAlign::Right));
    }

    #[test]
    fn parse_text_align_case_insensitive() {
        let style = parse_style_body("text-align: CENTER;");
        assert_eq!(style.text_align, Some(crate::style::TextAlign::Center));
    }

    #[test]
    fn parse_text_align_important() {
        let style = parse_style_body("text-align: center !important;");
        assert_eq!(style.text_align, Some(crate::style::TextAlign::Center));
        assert!(style.importance.get(StyleProperty::TextAlign));
    }

    // ---- content-align parsing tests ----

    #[test]
    fn parse_content_align_center_middle() {
        let style = parse_style_body("content-align: center middle;");
        let ca = style.content_align.expect("content_align should be Some");
        assert_eq!(ca.horizontal, crate::style::HorizontalAlign::Center);
        assert_eq!(ca.vertical, crate::style::VerticalAlign::Middle);
    }

    #[test]
    fn parse_content_align_left_top() {
        let style = parse_style_body("content-align: left top;");
        let ca = style.content_align.expect("content_align should be Some");
        assert_eq!(ca.horizontal, crate::style::HorizontalAlign::Left);
        assert_eq!(ca.vertical, crate::style::VerticalAlign::Top);
    }

    #[test]
    fn parse_content_align_right_bottom() {
        let style = parse_style_body("content-align: right bottom;");
        let ca = style.content_align.expect("content_align should be Some");
        assert_eq!(ca.horizontal, crate::style::HorizontalAlign::Right);
        assert_eq!(ca.vertical, crate::style::VerticalAlign::Bottom);
    }

    #[test]
    fn parse_content_align_important() {
        let style = parse_style_body("content-align: center middle !important;");
        assert!(style.content_align.is_some());
        assert!(style.importance.get(StyleProperty::ContentAlign));
    }

    #[test]
    fn parse_content_align_single_value_rejected() {
        // content-align requires exactly two values.
        let style = parse_style_body("content-align: center;");
        assert!(style.content_align.is_none());
    }

    // ---- align parsing tests ----

    #[test]
    fn parse_align_center_middle() {
        let style = parse_style_body("align: center middle;");
        let a = style.align.expect("align should be Some");
        assert_eq!(a.horizontal, crate::style::HorizontalAlign::Center);
        assert_eq!(a.vertical, crate::style::VerticalAlign::Middle);
    }

    #[test]
    fn parse_align_left_top() {
        let style = parse_style_body("align: left top;");
        let a = style.align.expect("align should be Some");
        assert_eq!(a.horizontal, crate::style::HorizontalAlign::Left);
        assert_eq!(a.vertical, crate::style::VerticalAlign::Top);
    }

    #[test]
    fn parse_align_right_bottom() {
        let style = parse_style_body("align: right bottom;");
        let a = style.align.expect("align should be Some");
        assert_eq!(a.horizontal, crate::style::HorizontalAlign::Right);
        assert_eq!(a.vertical, crate::style::VerticalAlign::Bottom);
    }

    #[test]
    fn parse_align_important() {
        let style = parse_style_body("align: center middle !important;");
        assert!(style.align.is_some());
        assert!(style.importance.get(StyleProperty::Align));
    }

    #[test]
    fn parse_align_single_value_rejected() {
        let style = parse_style_body("align: center;");
        assert!(style.align.is_none());
    }

    // ---- offset parsing tests ----

    #[test]
    fn parse_offset_basic() {
        let style = parse_style_body("offset: 1 2;");
        let o = style.offset.expect("offset should be Some");
        assert_eq!(o.x, OffsetValue::Cells(1));
        assert_eq!(o.y, OffsetValue::Cells(2));
    }

    #[test]
    fn parse_offset_negative() {
        let style = parse_style_body("offset: -5 3;");
        let o = style.offset.expect("offset should be Some");
        assert_eq!(o.x, OffsetValue::Cells(-5));
        assert_eq!(o.y, OffsetValue::Cells(3));
    }

    #[test]
    fn parse_offset_zero() {
        let style = parse_style_body("offset: 0 0;");
        let o = style.offset.expect("offset should be Some");
        assert_eq!(o.x, OffsetValue::Cells(0));
        assert_eq!(o.y, OffsetValue::Cells(0));
    }

    #[test]
    fn parse_offset_important() {
        let style = parse_style_body("offset: 1 2 !important;");
        assert!(style.offset.is_some());
        assert!(style.importance.get(StyleProperty::Offset));
    }

    #[test]
    fn parse_offset_single_value_rejected() {
        let style = parse_style_body("offset: 1;");
        assert!(style.offset.is_none());
    }

    // ---- Full stylesheet integration tests ----

    #[test]
    fn parse_stylesheet_with_alignment_properties() {
        use super::super::ast::StyleSheet;
        let css = r#"
            Screen {
                align: center middle;
                content-align: right bottom;
                text-align: justify;
                offset: 3 -1;
            }
        "#;
        let sheet = StyleSheet::parse(css);
        assert_eq!(sheet.rules.len(), 1);
        let style = &sheet.rules[0].style;
        let a = style.align.expect("align");
        assert_eq!(a.horizontal, crate::style::HorizontalAlign::Center);
        assert_eq!(a.vertical, crate::style::VerticalAlign::Middle);
        let ca = style.content_align.expect("content_align");
        assert_eq!(ca.horizontal, crate::style::HorizontalAlign::Right);
        assert_eq!(ca.vertical, crate::style::VerticalAlign::Bottom);
        assert_eq!(style.text_align, Some(crate::style::TextAlign::Justify));
        let o = style.offset.expect("offset");
        assert_eq!(o.x, OffsetValue::Cells(3));
        assert_eq!(o.y, OffsetValue::Cells(-1));
    }

    #[test]
    fn parse_stylesheet_alignment_with_important() {
        use super::super::ast::StyleSheet;
        let css = ".centered { text-align: center !important; align: center middle !important; }";
        let sheet = StyleSheet::parse(css);
        assert_eq!(sheet.rules.len(), 1);
        let style = &sheet.rules[0].style;
        assert_eq!(style.text_align, Some(crate::style::TextAlign::Center));
        assert!(style.importance.get(StyleProperty::TextAlign));
        assert!(style.align.is_some());
        assert!(style.importance.get(StyleProperty::Align));
    }

    // ---- Sub-property parsing tests ----

    #[test]
    fn parse_align_horizontal_only() {
        let style = parse_style_body("align-horizontal: right;");
        let a = style.align.expect("align should be Some");
        assert_eq!(a.horizontal, crate::style::HorizontalAlign::Right);
    }

    #[test]
    fn parse_align_vertical_only() {
        let style = parse_style_body("align-vertical: bottom;");
        let a = style.align.expect("align should be Some");
        assert_eq!(a.vertical, crate::style::VerticalAlign::Bottom);
    }

    #[test]
    fn parse_offset_x_only() {
        let style = parse_style_body("offset-x: 5;");
        let o = style.offset.expect("offset should be Some");
        assert_eq!(o.x, OffsetValue::Cells(5));
        assert_eq!(o.y, OffsetValue::Cells(0));
    }

    #[test]
    fn parse_offset_y_only() {
        let style = parse_style_body("offset-y: -3;");
        let o = style.offset.expect("offset should be Some");
        assert_eq!(o.x, OffsetValue::Cells(0));
        assert_eq!(o.y, OffsetValue::Cells(-3));
    }

    // ---- constrain parsing tests ----

    #[test]
    fn parse_constrain_none() {
        let style = parse_style_body("constrain: none;");
        assert_eq!(style.constrain, Some(crate::style::Constrain::None));
    }

    #[test]
    fn parse_constrain_inside() {
        let style = parse_style_body("constrain: inside;");
        assert_eq!(style.constrain, Some(crate::style::Constrain::Inside));
    }

    #[test]
    fn parse_constrain_inflect() {
        let style = parse_style_body("constrain: inflect;");
        assert_eq!(style.constrain, Some(crate::style::Constrain::Inflect));
    }

    #[test]
    fn parse_constrain_case_insensitive() {
        let style = parse_style_body("constrain: INSIDE;");
        assert_eq!(style.constrain, Some(crate::style::Constrain::Inside));
    }

    #[test]
    fn parse_constrain_unknown_value_is_none() {
        let style = parse_style_body("constrain: bogus;");
        assert_eq!(style.constrain, None);
    }

    #[test]
    fn parse_constrain_important() {
        let style = parse_style_body("constrain: inside !important;");
        assert_eq!(style.constrain, Some(crate::style::Constrain::Inside));
        assert!(style.importance.get(StyleProperty::Constrain));
    }

    #[test]
    fn parse_constrain_via_stylesheet() {
        use super::super::ast::StyleSheet;
        let sheet = StyleSheet::parse("Tooltip { constrain: inflect; }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(
            sheet.rules[0].style.constrain,
            Some(crate::style::Constrain::Inflect)
        );
    }

    // ---- PL-8: universal initial keyword tests ----

    #[test]
    fn parse_initial_min_width() {
        let style = parse_style_body("min-width: 30; min-width: initial;");
        assert!(style.min_width.is_none());
    }

    #[test]
    fn parse_initial_split() {
        let style = parse_style_body("split: right;");
        assert!(style.split.is_some());
        let style2 = parse_style_body("split: initial;");
        assert!(style2.split.is_none());
    }

    #[test]
    fn parse_initial_visibility() {
        let style = parse_style_body("visibility: initial;");
        assert!(style.visibility.is_none());
    }

    #[test]
    fn parse_initial_width() {
        let style = parse_style_body("width: initial;");
        assert!(style.width.is_none());
    }

    #[test]
    fn parse_initial_layout() {
        let style = parse_style_body("layout: initial;");
        assert!(style.layout.is_none());
    }

    #[test]
    fn parse_initial_overflow() {
        let style = parse_style_body("overflow: initial;");
        assert!(style.overflow.is_none());
        assert!(style.overflow_x.is_none());
        assert!(style.overflow_y.is_none());
    }

    // ---- PL-9: two-value overflow tests ----

    #[test]
    fn parse_overflow_two_values() {
        let style = parse_style_body("overflow: hidden auto;");
        assert_eq!(style.overflow_x, Some(crate::style::Overflow::Hidden));
        assert_eq!(style.overflow_y, Some(crate::style::Overflow::Auto));
    }

    #[test]
    fn parse_overflow_single_value_preserved() {
        let style = parse_style_body("overflow: scroll;");
        assert_eq!(style.overflow, Some(crate::style::Overflow::Scroll));
        assert_eq!(style.overflow_x, Some(crate::style::Overflow::Scroll));
        assert_eq!(style.overflow_y, Some(crate::style::Overflow::Scroll));
    }

    #[test]
    fn parse_overflow_scroll_hidden() {
        let style = parse_style_body("overflow: scroll hidden;");
        assert_eq!(style.overflow_x, Some(crate::style::Overflow::Scroll));
        assert_eq!(style.overflow_y, Some(crate::style::Overflow::Hidden));
    }

    // ---- PL-10: Markdown :light/:dark defaults tests ----

    #[test]
    fn parse_markdown_blockquote_light() {
        use super::super::ast::StyleSheet;
        let sheet =
            StyleSheet::parse("MarkdownBlockQuote:light { border-left: outer $text-secondary; }");
        assert_eq!(sheet.rules.len(), 1);
        assert!(sheet.rules[0].style.border_left.is_set());
    }

    #[test]
    fn parse_markdown_fence_dark_defaults() {
        use super::super::ast::StyleSheet;
        let css = r#"MarkdownFence { color: rgb(210, 210, 210); background: black 10%; }"#;
        let sheet = StyleSheet::parse(css);
        assert_eq!(sheet.rules.len(), 1);
        let s = &sheet.rules[0].style;
        assert!(s.fg.is_some());
        assert!(s.bg.is_some());
        // black 10% → alpha = 26 (0.10 * 255 = 25.5, rounded)
        assert_eq!(s.bg.unwrap().a, 26);
    }

    #[test]
    fn parse_markdown_bullet_light() {
        use super::super::ast::StyleSheet;
        let sheet = StyleSheet::parse("MarkdownBullet:light { color: $text-secondary; }");
        assert_eq!(sheet.rules.len(), 1);
        assert!(sheet.rules[0].style.fg.is_some());
    }

    #[test]
    fn parse_markdown_table_light() {
        use super::super::ast::StyleSheet;
        let sheet = StyleSheet::parse("MarkdownTable:light { background: white 30%; }");
        assert_eq!(sheet.rules.len(), 1);
        let bg = sheet.rules[0].style.bg.expect("bg should be Some");
        // white 30% → alpha = 77 (0.30 * 255 = 76.5, rounded)
        assert_eq!(bg.a, 77);
    }

    // ---- PL-1: bg/fg alpha parsing tests ----

    #[test]
    fn parse_bg_with_alpha_token() {
        let style = parse_style_body("bg: $background 60%;");
        let bg = style.bg.expect("bg should be Some");
        // Alpha = 60% → 0.6 * 255 ≈ 153
        assert_eq!(bg.a, 153);
    }

    #[test]
    fn parse_fg_with_alpha_token() {
        let style = parse_style_body("color: $primary 50%;");
        let fg = style.fg.expect("fg should be Some");
        // Alpha = 50% → 0.5 * 255 ≈ 128
        assert_eq!(fg.a, 128);
    }

    // ---- PL-2: constrain shorthand two-value tests ----

    #[test]
    fn parse_constrain_two_values() {
        let style = parse_style_body("constrain: inside inflect;");
        assert_eq!(style.constrain_x, Some(crate::style::Constrain::Inside));
        assert_eq!(style.constrain_y, Some(crate::style::Constrain::Inflect));
    }

    #[test]
    fn parse_constrain_single_sets_both_axes() {
        let style = parse_style_body("constrain: inside;");
        assert_eq!(style.constrain_x, Some(crate::style::Constrain::Inside));
        assert_eq!(style.constrain_y, Some(crate::style::Constrain::Inside));
    }

    // ---- PL-3: percentage offset tests ----

    #[test]
    fn parse_offset_x_percent() {
        let style = parse_style_body("offset-x: -50%;");
        let o = style.offset.expect("offset should be Some");
        assert_eq!(o.x, OffsetValue::Percent(-50.0));
    }

    #[test]
    fn parse_offset_y_percent() {
        let style = parse_style_body("offset-y: 25%;");
        let o = style.offset.expect("offset should be Some");
        assert_eq!(o.y, OffsetValue::Percent(25.0));
    }

    #[test]
    fn parse_offset_mixed_cells_percent() {
        let style = parse_style_body("offset: 5 -50%;");
        let o = style.offset.expect("offset should be Some");
        assert_eq!(o.x, OffsetValue::Cells(5));
        assert_eq!(o.y, OffsetValue::Percent(-50.0));
    }

    // ---- PL-4: missing theme token tests ----

    #[test]
    fn resolve_link_background_token() {
        let c = crate::style::parse_color_like("$link-background");
        assert!(c.is_some(), "$link-background should resolve");
        // Should be transparent
        assert_eq!(c.unwrap().a, 0);
    }

    #[test]
    fn resolve_secondary_muted_token() {
        let c = crate::style::parse_color_like("$secondary-muted");
        assert!(c.is_some(), "$secondary-muted should resolve");
    }

    // ---- PL-5+6: link-style token ref and not keyword tests ----

    #[test]
    fn parse_link_style_token_ref() {
        let style = parse_style_body("link-style: $link-style;");
        let ls = style.link_style.expect("link_style should be Some");
        assert!(ls.underline, "$link-style should set underline");
    }

    #[test]
    fn parse_link_style_hover_token_ref() {
        let style = parse_style_body("link-style-hover: $link-style-hover;");
        let ls = style
            .link_style_hover
            .expect("link_style_hover should be Some");
        assert!(ls.bold, "$link-style-hover should set bold");
    }

    #[test]
    fn parse_link_style_not_keyword() {
        let style = parse_style_body("link-style-hover: bold not underline;");
        let ls = style
            .link_style_hover
            .expect("link_style_hover should be Some");
        assert!(ls.bold);
        assert!(!ls.underline);
    }

    // ---- PL-7: strike / strikethrough tests ----

    #[test]
    fn parse_text_style_strike() {
        let style = parse_style_body("text-style: strike;");
        assert_eq!(style.strike, Some(true));
    }

    #[test]
    fn parse_text_style_not_strike() {
        let style = parse_style_body("text-style: bold not strike;");
        assert_eq!(style.bold, Some(true));
        assert_eq!(style.strike, Some(false));
    }

    #[test]
    fn parse_strike_property() {
        let style = parse_style_body("strike: true;");
        assert_eq!(style.strike, Some(true));
    }

    #[test]
    fn parse_nested_amp_and_descendant_rules() {
        let css = r#"
        Screen {
            color: red;
            &.active {
                bold: true;
            }
            Label {
                underline: true;
            }
        }
        "#;
        let (sheet, issues) = parse_with_issues(css);
        assert!(issues.is_empty(), "unexpected parse issues: {issues:?}");
        assert_eq!(sheet.rules.len(), 3);
        let selectors: Vec<String> = sheet
            .rules
            .iter()
            .map(|r| super::super::debug::selector_chain_string(&r.selector_chain))
            .collect();
        assert!(selectors.iter().any(|s| s == "Screen"));
        assert!(selectors.iter().any(|s| s == "Screen.active"));
        assert!(selectors.iter().any(|s| s == "Screen Label"));
    }

    #[test]
    fn parse_nested_selector_groups_cartesian_expansion() {
        let css = r#"
        Label, Button {
            &.foo, &.bar {
                bold: true;
            }
        }
        "#;
        let (sheet, issues) = parse_with_issues(css);
        assert!(issues.is_empty(), "unexpected parse issues: {issues:?}");
        let selectors: Vec<String> = sheet
            .rules
            .iter()
            .map(|r| super::super::debug::selector_chain_string(&r.selector_chain))
            .collect();
        assert!(selectors.iter().any(|s| s == "Label.foo"));
        assert!(selectors.iter().any(|s| s == "Label.bar"));
        assert!(selectors.iter().any(|s| s == "Button.foo"));
        assert!(selectors.iter().any(|s| s == "Button.bar"));
    }

    #[test]
    fn parse_selector_with_interleaved_pseudo_and_class() {
        let chain =
            parse_selector_chain("Screen:ansi.-screen-suspended").expect("selector should parse");
        assert_eq!(chain.parts().len(), 1);
        let selector = &chain.parts()[0];
        assert_eq!(selector.type_name(), Some("Screen"));
        assert!(
            selector
                .classes()
                .iter()
                .any(|class| class == "-screen-suspended"),
            "expected class -screen-suspended to be preserved"
        );
        assert!(
            selector.pseudos().contains(&PseudoClass::Ansi),
            "expected :ansi pseudo to be parsed"
        );
    }

    #[test]
    fn parse_unsupported_at_rule_records_issue() {
        let css = r#"
        @media (max-width: 20) {
            Label { color: red; }
        }
        Label { underline: true; }
        "#;
        let (sheet, issues) = parse_with_issues(css);
        assert!(
            issues
                .iter()
                .any(|i| matches!(i.kind, CssParseIssueKind::UnsupportedAtRule)),
            "expected unsupported at-rule issue"
        );
        assert_eq!(sheet.rules.len(), 1, "only regular rules should be parsed");
    }
}
