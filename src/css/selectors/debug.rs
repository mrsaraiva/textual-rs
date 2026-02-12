use std::sync::OnceLock;

use super::ast::{Combinator, PseudoClass, SelectorChain, SelectorMeta};
use crate::style::Style;

pub(super) fn selector_chain_string(chain: &SelectorChain) -> String {
    let mut out = String::new();
    for (idx, part) in chain.parts.iter().enumerate() {
        if idx > 0 {
            let comb = chain.combinators[idx - 1];
            match comb {
                Combinator::Child => out.push_str(" > "),
                Combinator::Descendant => out.push(' '),
            }
        }
        if let Some(name) = &part.type_name {
            out.push_str(name);
        }
        for class in &part.classes {
            out.push('.');
            out.push_str(class);
        }
        if let Some(id) = &part.id {
            out.push('#');
            out.push_str(id);
        }
        for pseudo in &part.pseudos {
            out.push(':');
            match pseudo {
                PseudoClass::Disabled => out.push_str("disabled"),
                PseudoClass::Focus => out.push_str("focus"),
                PseudoClass::Hover => out.push_str("hover"),
                PseudoClass::Active => out.push_str("active"),
                PseudoClass::Dark => out.push_str("dark"),
                PseudoClass::Light => out.push_str("light"),
                PseudoClass::Even => out.push_str("even"),
                PseudoClass::Odd => out.push_str("odd"),
                PseudoClass::FirstChild => out.push_str("first-child"),
                PseudoClass::LastChild => out.push_str("last-child"),
            }
        }
    }
    out
}

pub(super) fn style_debug_matches(meta: &SelectorMeta) -> bool {
    if std::env::var("TEXTUAL_DEBUG_STYLE_FILE").is_err() {
        return false;
    }
    static FILTERS: OnceLock<Vec<String>> = OnceLock::new();
    let filters = FILTERS.get_or_init(|| {
        std::env::var("TEXTUAL_DEBUG_STYLE_FILTER")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(|part| part.trim().to_string())
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    if filters.is_empty() {
        return true;
    }

    let label = style_debug_meta_label(meta).to_lowercase();
    filters.iter().all(|filter| {
        if let Some(value) = filter.strip_prefix("type=") {
            return meta.type_name.eq_ignore_ascii_case(value.trim());
        }
        if let Some(value) = filter.strip_prefix("id=") {
            return meta
                .id
                .as_deref()
                .is_some_and(|id| id.eq_ignore_ascii_case(value.trim()));
        }
        if let Some(value) = filter.strip_prefix("class=") {
            return meta
                .classes
                .iter()
                .any(|class| class.eq_ignore_ascii_case(value.trim()));
        }
        if let Some(value) = filter.strip_prefix("pseudo=") {
            return match value.trim().to_ascii_lowercase().as_str() {
                "disabled" => meta.states.disabled,
                "focus" | "focused" => meta.states.focused,
                "hover" | "hovered" => meta.states.hovered,
                "active" => meta.states.active,
                "dark" => meta.states.dark,
                "light" => !meta.states.dark,
                "even" => meta.states.child_index.map_or(false, |i| i % 2 == 0),
                "odd" => meta.states.child_index.map_or(false, |i| i % 2 == 1),
                "first-child" | "first_child" => meta.states.child_index == Some(0),
                "last-child" | "last_child" => {
                    matches!((meta.states.child_index, meta.states.sibling_count),
                        (Some(idx), Some(count)) if count > 0 && idx == count - 1)
                }
                _ => false,
            };
        }
        label.contains(&filter.to_ascii_lowercase())
    })
}

pub(super) fn style_debug_meta_label(meta: &SelectorMeta) -> String {
    let mut label = meta.type_name.clone();
    if let Some(id) = &meta.id {
        label.push('#');
        label.push_str(id);
    }
    for class in &meta.classes {
        label.push('.');
        label.push_str(class);
    }
    if meta.states.disabled {
        label.push_str(":disabled");
    }
    if meta.states.focused {
        label.push_str(":focus");
    }
    if meta.states.hovered {
        label.push_str(":hover");
    }
    if meta.states.active {
        label.push_str(":active");
    }
    if meta.states.dark {
        label.push_str(":dark");
    }
    if let Some(idx) = meta.states.child_index {
        label.push_str(&format!(":child({})", idx));
    }
    label
}

pub(super) fn style_debug_summary(style: &Style) -> String {
    let fg = style
        .fg
        .map(style_debug_color)
        .unwrap_or_else(|| "-".to_string());
    let fg_auto = style
        .fg_auto
        .map(|value| format!("{}%", value.alpha_percent))
        .unwrap_or_else(|| "-".to_string());
    let bg = style
        .bg
        .map(style_debug_color)
        .unwrap_or_else(|| "-".to_string());
    let tint = style
        .tint
        .map(|value| format!("{}@{}%", style_debug_color(value.color), value.percent))
        .unwrap_or_else(|| "-".to_string());
    let bg_tint = style
        .background_tint
        .map(|value| format!("{}@{}%", style_debug_color(value.color), value.percent))
        .unwrap_or_else(|| "-".to_string());

    format!(
        "fg={} fg_auto={} bg={} bold={:?} dim={:?} italic={:?} underline={:?} reverse={:?} text_opacity={:?} opacity={:?} padding={:?} width={:?} height={:?} min_width={:?} max_width={:?} min_height={:?} max_height={:?} layout={:?} display={:?} visibility={:?} dock={:?} grid_size_columns={:?} grid_size_rows={:?} grid_columns={:?} grid_rows={:?} grid_gutter_h={:?} grid_gutter_v={:?} tint={} bg_tint={}",
        fg,
        fg_auto,
        bg,
        style.bold,
        style.dim,
        style.italic,
        style.underline,
        style.reverse,
        style.text_opacity,
        style.opacity,
        style.padding,
        style.width,
        style.height,
        style.min_width,
        style.max_width,
        style.min_height,
        style.max_height,
        style.layout,
        style.display,
        style.visibility,
        style.dock,
        style.grid_size_columns,
        style.grid_size_rows,
        style.grid_columns,
        style.grid_rows,
        style.grid_gutter_horizontal,
        style.grid_gutter_vertical,
        tint,
        bg_tint,
    )
}

fn style_debug_color(color: crate::style::Color) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        color.r, color.g, color.b, color.a
    )
}
