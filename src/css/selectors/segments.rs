use rich_rs::{MetaValue, Segments};

use crate::style::Style;
use crate::widgets::WidgetId;

pub(crate) fn apply_style_to_segments(
    widget_id: WidgetId,
    segments: Segments,
    style: Style,
    parent_style: Option<Style>,
) -> Segments {
    if style.is_empty() {
        return segments;
    }
    let rich_attrs = style.to_rich_without_colors();
    let fallback_bg = crate::style::parse_color_like("$background");
    let parent_bg = parent_style.and_then(|s| s.bg).or(fallback_bg);
    segments
        .into_iter()
        .map(|mut seg| {
            if seg.control.is_some() {
                return seg;
            }

            // Only apply this widget's resolved style to segments that originate from this widget.
            // Child widgets render their own styles already (including inherited properties), and
            // parent widgets should not overwrite them during this pass.
            if let Some(meta) = seg.meta.as_ref().and_then(|meta| meta.meta.as_ref()) {
                if let Some(MetaValue::Int(value)) = meta.get("textual:widget_id") {
                    if *value != widget_id.as_u64() as i64 {
                        return seg;
                    }
                }
            }

            let rich_attrs = rich_attrs;
            if let Some(meta) = seg.meta.as_ref().and_then(|meta| meta.meta.as_ref()) {
                if let Some(MetaValue::Bool(true)) = meta.get("textual:no_style") {
                    return seg;
                }
                if let Some(MetaValue::Bool(true)) = meta.get("textual:no_bg") {
                    // We'll clear bgcolor after composing below.
                }
            }
            if let Some(rich_attrs) = rich_attrs {
                seg.style = Some(match seg.style {
                    Some(existing) => rich_attrs.combine(&existing),
                    None => rich_attrs,
                });
            }
            let mut no_bg = false;
            if let Some(meta) = seg.meta.as_ref().and_then(|meta| meta.meta.as_ref()) {
                if let Some(MetaValue::Bool(true)) = meta.get("textual:no_bg") {
                    no_bg = true;
                }
            }

            let mut style_changed = false;
            let mut s = seg.style.unwrap_or_else(rich_rs::Style::new);
            let mut under_bg = s
                .bgcolor
                .map(crate::style::color_from_simple)
                .or(parent_bg)
                .unwrap_or(crate::style::Color::rgb(0, 0, 0));

            if !no_bg {
                if s.bgcolor.is_none() {
                    // Preserve per-segment backgrounds (e.g. DataTable row/cell backgrounds,
                    // Input selection/cursor). When a segment has no explicit background:
                    // - apply this widget's own `bg` if present, flattened over parent bg
                    // - otherwise keep the parent surface color so transparent children
                    //   visually inherit container background during composition.
                    let effective_bg = if let Some(bg) = style.bg {
                        bg.flatten_over(under_bg)
                    } else {
                        under_bg
                    };
                    under_bg = effective_bg;
                    s.bgcolor = Some(effective_bg.to_simple_opaque());
                    style_changed = true;
                }
            } else if s.bgcolor.is_some() {
                s.bgcolor = None;
                style_changed = true;
            }

            if let Some(tint) = style.background_tint {
                if let Some(bg) = s.bgcolor {
                    let bg = crate::style::color_from_simple(bg);
                    let blended = crate::style::blend_colors(bg, tint.color, tint.percent);
                    let flat = blended.flatten_over(under_bg);
                    under_bg = flat;
                    s.bgcolor = Some(flat.to_simple_opaque());
                    style_changed = true;
                }
            }
            if let Some(tint) = style.tint {
                if let Some(bg) = s.bgcolor {
                    let bg = crate::style::color_from_simple(bg);
                    let blended = crate::style::blend_colors(bg, tint.color, tint.percent);
                    s.bgcolor = Some(blended.to_simple_opaque());
                    style_changed = true;
                }
            }

            let text_opacity = style.text_opacity.map(|value| value as f32 / 100.0);
            // Preserve per-segment foregrounds unless unset.
            if s.color.is_none() {
                let bg_for_text = s
                    .bgcolor
                    .map(crate::style::color_from_simple)
                    .unwrap_or(under_bg);

                if let Some(fg) = style.fg {
                    let mut fg = fg;
                    if let Some(opacity) = text_opacity {
                        fg.a = ((fg.a as f32) * opacity).round().clamp(0.0, 255.0) as u8;
                    }
                    let flat = fg.flatten_over(bg_for_text);
                    s.color = Some(flat.to_simple_opaque());
                    style_changed = true;
                } else if let Some(auto) = style.fg_auto {
                    let auto_alpha = auto.alpha();
                    let effective_alpha = if let Some(opacity) = text_opacity {
                        (auto_alpha * opacity).clamp(0.0, 1.0)
                    } else {
                        auto_alpha
                    };
                    let contrast =
                        crate::style::contrast_text(bg_for_text).with_alpha(effective_alpha);
                    let flat = contrast.flatten_over(bg_for_text);
                    s.color = Some(flat.to_simple_opaque());
                    style_changed = true;
                }
            } else if let (Some(opacity), Some(existing)) = (text_opacity, s.color) {
                let bg_for_text = s
                    .bgcolor
                    .map(crate::style::color_from_simple)
                    .unwrap_or(under_bg);
                let mut existing = crate::style::color_from_simple(existing);
                existing.a = ((existing.a as f32) * opacity).round().clamp(0.0, 255.0) as u8;
                let flat = existing.flatten_over(bg_for_text);
                s.color = Some(flat.to_simple_opaque());
                style_changed = true;
            }
            if style_changed || seg.style.is_some() {
                seg.style = Some(s);
            }
            seg
        })
        .collect()
}

pub(crate) fn apply_widget_opacity_to_segments(
    segments: Segments,
    opacity_percent: u8,
    parent_style: Option<Style>,
) -> Segments {
    if opacity_percent >= 100 {
        return segments;
    }
    let opacity = (opacity_percent as f32 / 100.0).clamp(0.0, 1.0);
    let fallback_bg = crate::style::parse_color_like("$background");
    let parent_bg = parent_style
        .and_then(|style| style.bg)
        .or(fallback_bg)
        .unwrap_or(crate::style::Color::rgb(0, 0, 0));

    segments
        .into_iter()
        .map(|mut seg| {
            if seg.control.is_some() {
                return seg;
            }
            let mut style_changed = false;
            let mut style = seg.style.unwrap_or_else(rich_rs::Style::new);
            let original_bg = style.bgcolor.map(crate::style::color_from_simple);
            let original_fg = style.color.map(crate::style::color_from_simple);

            if let Some(bg) = original_bg {
                let mut bg = bg;
                bg.a = ((bg.a as f32) * opacity).round().clamp(0.0, 255.0) as u8;
                let flat_bg = bg.flatten_over(parent_bg);
                style.bgcolor = Some(flat_bg.to_simple_opaque());
                style_changed = true;
            }

            if let Some(fg) = original_fg {
                let fg_source = fg;
                let mut fg = fg;
                fg.a = ((fg.a as f32) * opacity).round().clamp(0.0, 255.0) as u8;
                let mut flat_fg = fg.flatten_over(parent_bg);
                if let (Some(src_bg), Some(dst_bg)) =
                    (original_bg, style.bgcolor.map(crate::style::color_from_simple))
                {
                    if src_bg == fg_source {
                        flat_fg = dst_bg;
                    }
                }
                style.color = Some(flat_fg.to_simple_opaque());
                style_changed = true;
            }

            if style_changed || seg.style.is_some() {
                seg.style = Some(style);
            }
            seg
        })
        .collect()
}
