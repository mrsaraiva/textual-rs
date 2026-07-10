use rich_rs::{MetaValue, Segments};

use crate::node_id::{NodeId, node_id_to_ffi};
use crate::renderables::{TextOpacity, Tint};
use crate::style::Style;

pub(crate) fn apply_style_to_segments(
    widget_id: NodeId,
    segments: Segments,
    style: Style,
    parent_style: Option<Style>,
) -> Segments {
    if style.is_empty() {
        return segments;
    }
    let rich_attrs = style.to_rich_without_colors();
    let fallback_bg = crate::style::parse_color_like("$background");
    let parent_bg = crate::css::current_composited_background()
        .or_else(|| parent_style.and_then(|s| s.bg))
        .or(fallback_bg);
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
                    if *value != node_id_to_ffi(widget_id) as i64 {
                        return seg;
                    }
                }
            }

            let mut no_text_style = false;
            if let Some(meta) = seg.meta.as_ref().and_then(|meta| meta.meta.as_ref()) {
                if let Some(MetaValue::Bool(true)) = meta.get("textual:no_style") {
                    return seg;
                }
                if let Some(MetaValue::Bool(true)) = meta.get("textual:no_text_style") {
                    no_text_style = true;
                }
                if let Some(MetaValue::Bool(true)) = meta.get("textual:no_bg") {
                    // We'll clear bgcolor after composing below.
                }
            }
            if !no_text_style {
                if let Some(rich_attrs) = rich_attrs {
                    seg.style = Some(match seg.style {
                        Some(existing) => rich_attrs.combine(&existing),
                        None => rich_attrs,
                    });
                }
            }
            let mut no_bg = false;
            if let Some(meta) = seg.meta.as_ref().and_then(|meta| meta.meta.as_ref()) {
                if let Some(MetaValue::Bool(true)) = meta.get("textual:no_bg") {
                    no_bg = true;
                }
            }

            let mut style_changed = false;
            let mut s = seg.style.unwrap_or_else(rich_rs::Style::new);
            // In composition terms, terminal-default background should behave as transparent
            // so children can inherit parent/widget surfaces.
            let explicit_bg = s.bgcolor.and_then(|bg| {
                if matches!(bg, rich_rs::SimpleColor::Default) {
                    None
                } else {
                    Some(bg)
                }
            });
            let mut under_bg = explicit_bg
                .map(crate::style::color_from_simple)
                .or(parent_bg)
                .unwrap_or(crate::style::Color::rgb(0, 0, 0));

            if !no_bg {
                if explicit_bg.is_none() {
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

            // `background-tint` tints only the widget's OWN surface, mirroring
            // Python's `styles.background.tint(styles.background_tint)` in
            // `DOMNode.rich_style`/`background_colors`: the tint folds into each
            // node's own `background` rule, not blanket over every segment the
            // widget emits. Child/component renderables that carry their own
            // opaque bg (e.g. a Switch slider's `$panel-darken-2`) must NOT be
            // re-tinted by the parent widget's tint — doing so double-tints them
            // (byte01/02 slider #0b1922 vs #000f18). The widget's own surface is
            // the set of cells painting `style.bg` (its `background` rule) over
            // the inherited parent surface; cells whose bg equals that surface
            // (both the inherited fill we set above and any explicit surface
            // fill the layout emits, e.g. the Switch `padding: 0 2` cells) get
            // the tint, while cells with a different opaque bg keep their color.
            if !no_bg {
                if let Some(tint) = style.background_tint {
                    // The widget's own surface color: its `background` rule
                    // composited over the inherited parent surface. `None` when
                    // the widget has no `background` rule — matching Python,
                    // where tinting a transparent `styles.background` is a no-op.
                    let own_surface_bg = style.bg.map(|bg| {
                        bg.flatten_over(
                            parent_bg.unwrap_or_else(|| crate::style::Color::rgb(0, 0, 0)),
                        )
                    });
                    if let (Some(bg_simple), Some(surface)) = (s.bgcolor, own_surface_bg) {
                        let bg = crate::style::color_from_simple(bg_simple);
                        if bg == surface {
                            let blended = Tint::<()>::blend_color_with_percent(
                                bg,
                                tint.color,
                                tint.percent,
                            );
                            let flat = blended.flatten_over(under_bg);
                            under_bg = flat;
                            s.bgcolor = Some(flat.to_simple_opaque());
                            style_changed = true;
                        }
                    }
                }
            }
            let text_opacity = style.text_opacity.map(|value| value as f32 / 100.0);
            // Only stamp the widget's resolved foreground onto segments carrying a
            // visible glyph. Whitespace-only fill (padding, content-area extend,
            // blank rows) must keep fg = terminal-default unless it was given an
            // explicit fg at construction — mirroring Python Textual's `to_strip`,
            // where glyph cells use the full style but pad cells use
            // `style.background_style` (bg only). The App/Screen default
            // `color: $foreground` therefore reaches text glyphs but not the fill.
            let has_glyph = seg.text.chars().any(|c| !c.is_whitespace());
            // text-opacity: 0% — mirror Python `TextOpacity.process_segments`
            // (opacity == 0 branch): every cell becomes a blank with only the
            // background set (`from_color(bgcolor=style.bgcolor)`), so the glyph run
            // is replaced by spaces of equal cell width and the foreground is
            // dropped entirely (fg = terminal-default). Applies to glyph cells AND
            // to fg-bearing fill cells (the vertical-extend rows carry visual_style
            // fg from the widget render), matching Python's per-line filter.
            if matches!(text_opacity, Some(o) if o == 0.0) {
                if has_glyph {
                    let width = rich_rs::cell_len(&seg.text);
                    seg.text = " ".repeat(width).into();
                }
                if s.color.is_some() {
                    s.color = None;
                    style_changed = true;
                }
                if style_changed || seg.style.is_some() {
                    seg.style = Some(s);
                }
                return seg;
            }
            // Preserve per-segment foregrounds unless unset.
            if s.color.is_none() && has_glyph {
                let bg_for_text = s
                    .bgcolor
                    .map(crate::style::color_from_simple)
                    .unwrap_or(under_bg);

                if let Some(fg) = style.fg {
                    let mut fg = fg;
                    if let Some(opacity) = text_opacity {
                        fg = TextOpacity::<()>::apply_alpha(fg, opacity);
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
                    // Composite the contrast color using the fractional alpha
                    // directly (Python `bg + contrast.with_alpha(a)` keeps the
                    // float), avoiding u8 alpha quantization drift.
                    let contrast = crate::style::contrast_text(bg_for_text);
                    let flat = contrast.blend_over_float(bg_for_text, effective_alpha);
                    s.color = Some(flat.to_simple_opaque());
                    style_changed = true;
                }
            } else if let (Some(opacity), Some(existing)) = (text_opacity, s.color) {
                let bg_for_text = s
                    .bgcolor
                    .map(crate::style::color_from_simple)
                    .unwrap_or(under_bg);
                let existing = crate::style::color_from_simple(existing);
                let flat = TextOpacity::<()>::blend_foreground_over_background(
                    existing,
                    bg_for_text,
                    opacity,
                );
                s.color = Some(flat.to_simple_opaque());
                style_changed = true;
            }
            if let Some(tint) = style.tint {
                if let Some(bg) = s.bgcolor {
                    let bg = crate::style::color_from_simple(bg);
                    let blended =
                        Tint::<()>::blend_color_with_percent(bg, tint.color, tint.percent);
                    s.bgcolor = Some(blended.to_simple_opaque());
                    style_changed = true;
                }
                if let Some(fg) = s.color {
                    let fg = crate::style::color_from_simple(fg);
                    let blended =
                        Tint::<()>::blend_color_with_percent(fg, tint.color, tint.percent);
                    s.color = Some(blended.to_simple_opaque());
                    style_changed = true;
                }
            }
            if style_changed || seg.style.is_some() {
                seg.style = Some(s);
            }
            seg
        })
        .collect()
}

/// Rust analog of Python's always-on `ANSIToTruecolor` line filter
/// (textual/filter.py, applied to every rendered widget line in
/// `_styles_cache.render_line`): replace ANSI-indexed segment colors
/// (`Standard`/`EightBit`) with their truecolor equivalents so indexed
/// content — e.g. rich renderables such as `rich_rs::markdown::Markdown` —
/// paints the exact same RGB cells as Python. Standard colors map through the
/// active ANSI terminal theme (MONOKAI when dark, ALABASTER when light);
/// 8-bit colors map through rich's fixed 256-color palette.
///
/// Mirroring Python's `enabled=not app.ansi_color`, the pass is skipped when
/// the app runs in native-ANSI mode (`:ansi` runtime pseudo).
///
/// NOTE: the dim pre-blend half of Python's filter (`dim_color` + strip dim)
/// is NOT ported here — that is the tracked global-dim follow-up; widgets that
/// need it today pre-blend locally (see `option_list.rs`).
pub(crate) fn apply_ansi_truecolor_to_segments(segments: Segments) -> Segments {
    let pseudos = super::context::app_runtime_pseudos();
    if pseudos.ansi {
        return segments;
    }
    let dark = pseudos.dark;
    segments
        .into_iter()
        .map(|mut seg| {
            if seg.control.is_some() {
                return seg;
            }
            let Some(mut style) = seg.style else {
                return seg;
            };
            let mut changed = false;
            if let Some(color) = style.color {
                if let Some(rgb) = crate::style::ansi_simple_to_truecolor(color, dark) {
                    style.color = Some(rgb);
                    changed = true;
                }
            }
            if let Some(bgcolor) = style.bgcolor {
                if let Some(rgb) = crate::style::ansi_simple_to_truecolor(bgcolor, dark) {
                    style.bgcolor = Some(rgb);
                    changed = true;
                }
            }
            if changed {
                seg.style = Some(style);
            }
            seg
        })
        .collect()
}

#[cfg(test)]
mod ansi_truecolor_tests {
    use super::apply_ansi_truecolor_to_segments;
    use rich_rs::{Segment, Segments, SimpleColor};

    fn styled(color: Option<SimpleColor>, bg: Option<SimpleColor>) -> Segments {
        let mut style = rich_rs::Style::new();
        style.color = color;
        style.bgcolor = bg;
        let mut segs = Segments::new();
        segs.push(Segment::styled("x", style));
        segs
    }

    fn first_style(segments: Segments) -> rich_rs::Style {
        segments.into_iter().next().unwrap().style.unwrap()
    }

    /// Python `ANSIToTruecolor` with the MONOKAI dark theme: ANSI magenta
    /// (Standard 5, e.g. rich markdown blockquote/h2) must paint #f4005f —
    /// exactly what Python Textual outputs for the same content.
    #[test]
    fn standard_ansi_color_maps_through_monokai_when_dark() {
        let _guard = super::super::context::set_app_runtime_pseudos(
            super::super::context::AppRuntimePseudos {
                dark: true,
                ..Default::default()
            },
        );
        let out = first_style(apply_ansi_truecolor_to_segments(styled(
            Some(SimpleColor::Standard(5)),
            None,
        )));
        assert_eq!(out.color, Some(SimpleColor::Rgb { r: 244, g: 0, b: 95 }));
    }

    /// Light theme (ALABASTER): magenta slot 5 is rgb(122, 62, 157).
    #[test]
    fn standard_ansi_color_maps_through_alabaster_when_light() {
        let _guard = super::super::context::set_app_runtime_pseudos(
            super::super::context::AppRuntimePseudos {
                dark: false,
                ..Default::default()
            },
        );
        let out = first_style(apply_ansi_truecolor_to_segments(styled(
            Some(SimpleColor::Standard(5)),
            None,
        )));
        assert_eq!(
            out.color,
            Some(SimpleColor::Rgb {
                r: 122,
                g: 62,
                b: 157
            })
        );
    }

    /// Truecolor and Default colors pass through untouched; the `:ansi`
    /// native-ANSI mode disables the filter (Python `enabled=not ansi_color`).
    #[test]
    fn rgb_default_and_native_ansi_are_untouched() {
        let _guard = super::super::context::set_app_runtime_pseudos(
            super::super::context::AppRuntimePseudos {
                dark: true,
                ..Default::default()
            },
        );
        let rgb = SimpleColor::Rgb { r: 1, g: 2, b: 3 };
        let out = first_style(apply_ansi_truecolor_to_segments(styled(
            Some(rgb),
            Some(SimpleColor::Default),
        )));
        assert_eq!(out.color, Some(rgb));
        assert_eq!(out.bgcolor, Some(SimpleColor::Default));

        let _guard = super::super::context::set_app_runtime_pseudos(
            super::super::context::AppRuntimePseudos {
                dark: true,
                ansi: true,
                ..Default::default()
            },
        );
        let out = first_style(apply_ansi_truecolor_to_segments(styled(
            Some(SimpleColor::Standard(5)),
            None,
        )));
        assert_eq!(out.color, Some(SimpleColor::Standard(5)));
    }
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
    let parent_bg = crate::css::current_composited_background()
        .or_else(|| parent_style.and_then(|style| style.bg))
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
                // Python parity: background opacity is applied TWICE — once in
                // widget.background_colors (bg = parent.blend(widget_bg, opacity)) and then
                // again in _apply_opacity (blends those pre-composited segments at opacity).
                // Net result: parent.blend(parent.blend(widget_bg, opacity), opacity).
                let intermediate = TextOpacity::<()>::apply_alpha(bg, opacity).flatten_over(parent_bg);
                let flat_bg = TextOpacity::<()>::apply_alpha(intermediate, opacity).flatten_over(parent_bg);
                style.bgcolor = Some(flat_bg.to_simple_opaque());
                style_changed = true;
            }

            if let Some(fg) = original_fg {
                let fg_source = fg;
                // Python parity: fg is only processed once — background_colors only composites
                // the bg, not the fg. _apply_opacity then applies fg once. So fg gets ONE blend.
                let fg = TextOpacity::<()>::apply_alpha(fg, opacity);
                let mut flat_fg = fg.flatten_over(parent_bg);
                if let (Some(src_bg), Some(dst_bg)) = (
                    original_bg,
                    style.bgcolor.map(crate::style::color_from_simple),
                ) {
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
