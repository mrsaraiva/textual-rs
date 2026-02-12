use std::time::Duration;

use crate::style::{
    BorderEdge, BorderType, Display, Dock, Layout, Margin, Scalar, Style, Tint, TransitionTiming,
    Visibility, parse_auto_color_like, parse_color_like,
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

pub(super) fn parse_style_body(body: &str) -> Style {
    let mut style = Style::new();
    for decl in body.split(';') {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        let mut parts = decl.splitn(2, ':');
        let key = parts.next().unwrap_or("").trim().to_lowercase();
        let value = parts.next().unwrap_or("").trim();
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
                for token in value.split(|c: char| c == ' ' || c == ',' || c == '|') {
                    let token = token.trim();
                    if token.is_empty() {
                        continue;
                    }
                    match token {
                        "bold" => style = style.bold(true),
                        "dim" => style = style.dim(true),
                        "italic" => style = style.italic(true),
                        "underline" => style = style.underline(true),
                        "reverse" => style = style.reverse(true),
                        "$button-focus-text-style" => style = style.reverse(true),
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
                if let Some((duration, delay, timing)) = parse_transition_shorthand(value) {
                    if let Some(duration) = duration {
                        style = style.transition_duration(duration);
                    }
                    if let Some(delay) = delay {
                        style = style.transition_delay(delay);
                    }
                    if let Some(timing) = timing {
                        style = style.transition_timing(timing);
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
                    style.grid_columns =
                        Some(parsed.into_iter().map(|s| s.unwrap()).collect());
                }
            }
            "grid-rows" => {
                let parsed: Vec<Option<Scalar>> = value
                    .split_whitespace()
                    .map(|token| parse_scalar(token))
                    .collect();
                if !parsed.is_empty() && parsed.iter().all(|s| s.is_some()) {
                    style.grid_rows =
                        Some(parsed.into_iter().map(|s| s.unwrap()).collect());
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
                        if let (Ok(h), Ok(v)) =
                            (parts[0].parse::<u16>(), parts[1].parse::<u16>())
                        {
                            style.grid_gutter_horizontal = Some(h);
                            style.grid_gutter_vertical = Some(v);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
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
        3 => Some(crate::style::Spacing::new(nums[0], nums[1], nums[2], nums[1])),
        4 => Some(crate::style::Spacing::new(nums[0], nums[1], nums[2], nums[3])),
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

fn parse_dock(value: &str) -> Option<Dock> {
    match value.trim().to_lowercase().as_str() {
        "top" => Some(Dock::Top),
        "right" => Some(Dock::Right),
        "bottom" => Some(Dock::Bottom),
        "left" => Some(Dock::Left),
        _ => None,
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
}
