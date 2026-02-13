use std::time::Duration;

use crate::style::{
    Align, BorderEdge, BorderType, Constrain, ContentAlign, Display, Dock, HorizontalAlign, Layout,
    Margin, Offset, Overflow, Pointer, Scalar, Style, StyleProperty, TextAlign, Tint,
    TransitionTiming, VerticalAlign, Visibility, parse_auto_color_like, parse_color_like,
};

use super::ast::{Combinator, PseudoClass, SelectorChain, StyleRule, StyleSelector, StyleSheet};

impl StyleSheet {
    pub fn parse(input: &str) -> Self {
        let mut sheet = StyleSheet::new();
        let mut rest = input;
        while let Some(start) = rest.find('{') {
            let selector = rest[..start].trim();
            let after = &rest[start + 1..];
            let end = match after.find('}') {
                Some(pos) => pos,
                None => break,
            };
            let body = &after[..end];
            let style = parse_style_body(body);
            if !style.is_empty() {
                for selector_chain in parse_selector_list(selector) {
                    sheet.rules.push(StyleRule {
                        selector_chain,
                        style: style.clone(),
                    });
                }
            }
            rest = &after[end + 1..];
        }
        sheet
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

    // Split off pseudo-classes (`Button:disabled`, `.foo:focus`, etc).
    let mut pseudo_parts = selector.split(':');
    let base_selector = pseudo_parts.next().unwrap_or("").trim();
    let pseudos: Vec<PseudoClass> = pseudo_parts
        .filter_map(|part| {
            let name = part.trim();
            if name.is_empty() {
                return None;
            }
            // Ignore any `:pseudo(...)` forms for now.
            let name = name.split('(').next().unwrap_or(name).trim().to_lowercase();
            match name.as_str() {
                "disabled" => Some(PseudoClass::Disabled),
                "focus" | "focused" => Some(PseudoClass::Focus),
                "focus-within" | "focus_within" => Some(PseudoClass::FocusWithin),
                "hover" => Some(PseudoClass::Hover),
                "active" => Some(PseudoClass::Active),
                "dark" => Some(PseudoClass::Dark),
                "light" => Some(PseudoClass::Light),
                "even" => Some(PseudoClass::Even),
                "odd" => Some(PseudoClass::Odd),
                "first-child" | "first_child" => Some(PseudoClass::FirstChild),
                "last-child" | "last_child" => Some(PseudoClass::LastChild),
                _ => None,
            }
        })
        .collect();

    let mut type_name: Option<String> = None;
    let mut id: Option<String> = None;
    let mut classes: Vec<String> = Vec::new();

    let mut chars = base_selector.chars().peekable();
    let mut current = String::new();
    let mut mode: Option<char> = None;

    while let Some(ch) = chars.next() {
        match ch {
            '#' | '.' => {
                if mode.is_none() && !current.is_empty() {
                    type_name = Some(current.clone());
                } else if let Some(mode) = mode {
                    match mode {
                        '#' => id = Some(current.clone()),
                        '.' => classes.push(current.clone()),
                        _ => {}
                    }
                }
                current.clear();
                mode = Some(ch);
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        match mode {
            None => type_name = Some(current),
            Some('#') => id = Some(current),
            Some('.') => classes.push(current),
            _ => {}
        }
    }

    let mut selector = StyleSelector::default();
    if let Some(type_name) = type_name {
        selector = StyleSelector::new(type_name);
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
        _ => &[],
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
        // Track whether this arm handles importance itself (for shorthands
        // like `text-style` and `transition` that set multiple sub-properties).
        let mut handled_importance = false;
        match key.as_str() {
            "fg" | "color" => {
                if let Some(auto) = parse_auto_color_like(value) {
                    style = style.fg_auto(auto);
                } else if let Some(color) = parse_color_like(value) {
                    style = style.fg(color);
                }
            }
            "bg" | "background" => {
                if let Some(color) = parse_color_like(value) {
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
                let v = parse_overflow(value);
                style.overflow = v;
                style.overflow_x = v;
                style.overflow_y = v;
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
                for token in value.split(|c: char| c == ' ' || c == ',' || c == '|') {
                    let token = token.trim();
                    if token.is_empty() {
                        continue;
                    }
                    match token {
                        "bold" => {
                            style = style.bold(true);
                            if is_important {
                                style.importance.set(StyleProperty::Bold);
                            }
                        }
                        "dim" => {
                            style = style.dim(true);
                            if is_important {
                                style.importance.set(StyleProperty::Dim);
                            }
                        }
                        "italic" => {
                            style = style.italic(true);
                            if is_important {
                                style.importance.set(StyleProperty::Italic);
                            }
                        }
                        "underline" => {
                            style = style.underline(true);
                            if is_important {
                                style.importance.set(StyleProperty::Underline);
                            }
                        }
                        "reverse" | "$button-focus-text-style" => {
                            style = style.reverse(true);
                            if is_important {
                                style.importance.set(StyleProperty::Reverse);
                            }
                        }
                        _ => {}
                    }
                }
            }
            "line-pad" => {
                if let Ok(value) = value.parse() {
                    style = style.line_pad(value);
                }
            }
            "transition" => {
                // Shorthand: only mark sub-properties that are actually set.
                handled_importance = true;
                if let Some((duration, delay, timing)) = parse_transition_shorthand(value) {
                    if let Some(duration) = duration {
                        style = style.transition_duration(duration);
                        if is_important {
                            style.importance.set(StyleProperty::TransitionDuration);
                        }
                    }
                    if let Some(delay) = delay {
                        style = style.transition_delay(delay);
                        if is_important {
                            style.importance.set(StyleProperty::TransitionDelay);
                        }
                    }
                    if let Some(timing) = timing {
                        style = style.transition_timing(timing);
                        if is_important {
                            style.importance.set(StyleProperty::TransitionTiming);
                        }
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
                if let Ok(x) = value.trim().parse::<i16>() {
                    let existing = style.offset.unwrap_or(Offset { x: 0, y: 0 });
                    style.offset = Some(Offset { x, y: existing.y });
                }
            }
            "offset-y" => {
                if let Ok(y) = value.trim().parse::<i16>() {
                    let existing = style.offset.unwrap_or(Offset { x: 0, y: 0 });
                    style.offset = Some(Offset { x: existing.x, y });
                }
            }
            "constrain" => {
                style.constrain = match value.trim().to_lowercase().as_str() {
                    "none" => Some(Constrain::None),
                    "inside" => Some(Constrain::Inside),
                    "inflect" => Some(Constrain::Inflect),
                    _ => None,
                };
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
    let x = parts[0].trim().parse::<i16>().ok()?;
    let y = parts[1].trim().parse::<i16>().ok()?;
    Some(Offset { x, y })
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
        assert_eq!(o.x, 1);
        assert_eq!(o.y, 2);
    }

    #[test]
    fn parse_offset_negative() {
        let style = parse_style_body("offset: -5 3;");
        let o = style.offset.expect("offset should be Some");
        assert_eq!(o.x, -5);
        assert_eq!(o.y, 3);
    }

    #[test]
    fn parse_offset_zero() {
        let style = parse_style_body("offset: 0 0;");
        let o = style.offset.expect("offset should be Some");
        assert_eq!(o.x, 0);
        assert_eq!(o.y, 0);
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
        assert_eq!(o.x, 3);
        assert_eq!(o.y, -1);
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
        assert_eq!(o.x, 5);
        assert_eq!(o.y, 0);
    }

    #[test]
    fn parse_offset_y_only() {
        let style = parse_style_body("offset-y: -3;");
        let o = style.offset.expect("offset should be Some");
        assert_eq!(o.x, 0);
        assert_eq!(o.y, -3);
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
}
