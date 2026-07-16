//! Shared surface-compositing for component-class styles.
//!
//! Converts a resolved component [`Style`] into a ready-to-paint
//! [`rich_rs::Style`] with the exact colour math Python's
//! `Widget.get_component_rich_style` performs, generalised from the per-widget
//! fragments that used to live in `input_chrome.rs` / `text_area.rs` /
//! `tree/render.rs` / `radio_button.rs`:
//!
//! 1. fold a `background-tint` carried ON the component style into its
//!    background (`Tint::blend_color_with_percent`, mirroring Python
//!    `background += styles.background.tint(styles.background_tint)`);
//! 2. flatten a (possibly semi-transparent) component `background` over the
//!    widget's effective composited surface — never via style inheritance
//!    (`bg` is not inherited; render-time composition invariant);
//! 3. resolve the foreground: a concrete `fg` flattens over the
//!    under-background; an `auto <pct>%` foreground resolves the contrast
//!    colour of the under-background at fractional alpha, matching Python's
//!    `background.get_contrast_text(alpha)`;
//! 4. fold `text-opacity` into the foreground (Python `widget.py`
//!    `get_component_rich_style`: `foreground = background + foreground *
//!    text_opacity`).
//!
//! The under-surface comes from the render-time seam
//! ([`crate::render_context::composited_background`] /
//! `current_composited_background`), with an off-tree fallback that resolves
//! the widget's own stateless style (including its `background-tint`).

use crate::style::{Color, Style, parse_color_like};
use crate::widgets::Widget;

/// The widget's painted surface background: the live composited background
/// during a tree render (state-aware, including any `:focus`
/// `background-tint`), or a stateless off-tree fallback with any
/// `background-tint` applied by hand.
pub(crate) fn component_surface_bg<W: Widget + ?Sized>(widget: &W) -> Color {
    let fallback_bg = parse_color_like("$background").unwrap_or(Color::rgb(0, 0, 0));
    super::resolver::current_composited_background().unwrap_or_else(|| {
        // Off-tree callers (unit tests without a live style stack): resolve
        // statelessly and apply any `background-tint` by hand.
        let base_meta = super::resolver::selector_meta_generic(widget);
        let base_style = super::resolver::resolve_style(widget, &base_meta);
        let mut bg = match base_style.bg {
            Some(bg) if bg.a <= 0.0 => fallback_bg,
            Some(bg) => bg,
            None => fallback_bg,
        };
        if let Some(tint) = base_style.background_tint {
            bg = crate::renderables::Tint::<()>::blend_color_with_percent(
                bg,
                tint.color,
                tint.percent,
            );
        }
        bg
    })
}

/// Convert a resolved component [`Style`] to a paintable [`rich_rs::Style`],
/// composited over `surface` (see [`component_surface_bg`]).
///
/// Returns `None` when the style carries no paintable attributes (no fg/bg
/// and no text attributes) — equivalent to an empty Rich style.
pub(crate) fn component_style_to_rich(style: &Style, surface: Color) -> Option<rich_rs::Style> {
    let attrs = style.to_rich_without_colors();
    let mut rich = attrs.clone().unwrap_or_default();
    let mut has_paint = attrs.is_some();
    let mut under_bg = surface;

    // 1+2. Fold the component's own background-tint into its bg, then flatten
    // over the surface (flatten first, tint the flattened result — same order
    // as `current_composited_background`).
    if let Some(bg) = style.bg {
        if bg.a > 0.0 {
            let mut flat = bg.flatten_over(under_bg);
            if let Some(tint) = style.background_tint {
                flat = crate::renderables::Tint::<()>::blend_color_with_percent(
                    flat,
                    tint.color,
                    tint.percent,
                );
            }
            under_bg = flat;
            rich = rich.with_bgcolor(flat.to_simple_opaque());
            has_paint = true;
        }
    }

    // 3. Foreground: concrete fg flattens over the under-background; `auto
    // <pct>%` resolves contrast at fractional alpha.
    let mut fg_flat: Option<Color> = None;
    if let Some(fg) = style.fg {
        if fg.a > 0.0 {
            fg_flat = Some(fg.flatten_over(under_bg));
        }
    } else if let Some(auto) = style.fg_auto {
        let contrast = crate::style::contrast_text(under_bg);
        fg_flat = Some(contrast.blend_over_float(under_bg, auto.alpha()));
    }

    // 4. text-opacity folds the foreground toward the under-background
    // (Python: `foreground = background + foreground * text_opacity`).
    if let (Some(fg), Some(opacity)) = (fg_flat, style.text_opacity) {
        if opacity < 100 {
            fg_flat = Some(fg.blend_over_float(under_bg, f32::from(opacity) / 100.0));
        }
    }

    if let Some(fg) = fg_flat {
        rich = rich.with_color(fg.to_simple_opaque());
        has_paint = true;
    }

    if has_paint { Some(rich) } else { None }
}

/// One-call form: resolve the widget's component class(es) (compound form,
/// see [`super::resolver::resolve_component_style`]) and composite over the
/// widget's effective surface. Backs `Widget::get_component_rich_style`.
pub(crate) fn resolve_component_rich_style<W: Widget + ?Sized>(
    widget: &W,
    classes: &[&str],
) -> Option<rich_rs::Style> {
    let style = super::resolver::resolve_component_style(widget, classes);
    let surface = component_surface_bg(widget);
    component_style_to_rich(&style, surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::StyleSheet;
    use rich_rs::Segments;

    struct Board;

    impl Widget for Board {
        fn render(
            &self,
            _console: &rich_rs::Console,
            _options: &rich_rs::ConsoleOptions,
        ) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            "CheckerBoard"
        }

        fn component_classes(&self) -> &[&'static str] {
            &["part"]
        }
    }

    /// Build a live-looking meta for the Board: real id, a runtime class, and
    /// interaction states (the arena-node shape seed metas cannot carry).
    fn live_board_meta(focused: bool, hovered: bool) -> super::super::ast::SelectorMeta {
        super::super::ast::SelectorMeta {
            type_name: "CheckerBoard".to_string(),
            type_aliases: Vec::new(),
            id: Some("the-id".to_string()),
            classes: vec!["the-class".to_string()],
            states: super::super::ast::SelectorStates {
                focused,
                hovered,
                ..Default::default()
            },
            component_phantom: false,
        }
    }

    /// Resolve `part` under a live render context (widget meta on top of the
    /// selector stack + the live marker), the way `render_widget_with_meta`
    /// sets it up.
    fn resolve_part_live(meta: super::super::ast::SelectorMeta) -> Style {
        super::super::resolver::with_style_stack(meta, Style::new(), || {
            let _live = super::super::context::mark_live_widget_meta();
            super::super::resolver::resolve_component_style(&Board, &["part"])
        })
    }

    /// D3: with the widget's live meta on the stack (id + runtime class +
    /// `:focus`), id-, class- and pseudo-qualified parent rules all match.
    #[test]
    fn live_stack_id_class_and_pseudo_qualified_rules_match() {
        let _guard = super::super::context::set_style_context(StyleSheet::parse(
            r#"
            #the-id > .part { color: #ff0000; }
            CheckerBoard.the-class > .part { background: #0000ff; }
            CheckerBoard:focus > .part { text-style: bold; }
            "#,
        ));
        let style = resolve_part_live(live_board_meta(true, false));
        assert_eq!(
            style.fg,
            Some(Color::parse("#ff0000").unwrap()),
            "id-qualified parent rule must match via the live stack"
        );
        assert_eq!(
            style.bg,
            Some(Color::parse("#0000ff").unwrap()),
            "class-qualified parent rule must match via the live stack"
        );
        assert_eq!(
            style.bold,
            Some(true),
            "pseudo-qualified parent rule must match via the live stack"
        );
    }

    /// R7 regression: an OUTER widget (`Wrapper`) renders `Board`'s content
    /// inline while the OUTER live meta is still marked on top. Resolving
    /// `Board`'s component must NOT use the outer stack: it must fall back to
    /// pushing `Board`'s own seed meta so `CheckerBoard > .part` still matches
    /// (Footer -> FooterKey inline-render shape). Without the identity guard
    /// this resolved `.part` against the `Wrapper` stack and lost the rule.
    #[test]
    fn inline_nested_render_does_not_misfire_live_context() {
        struct Wrapper;
        impl Widget for Wrapper {
            fn render(
                &self,
                _c: &rich_rs::Console,
                _o: &rich_rs::ConsoleOptions,
            ) -> Segments {
                Segments::new()
            }
            fn style_type(&self) -> &'static str {
                "Wrapper"
            }
        }
        let _guard = super::super::context::set_style_context(StyleSheet::parse(
            "CheckerBoard > .part { color: #ff0000; }",
        ));
        // Wrapper's meta is marked live and on top (as if mid-render).
        let wrapper_meta = super::super::ast::SelectorMeta::new(
            "Wrapper".to_string(),
            None,
            Vec::new(),
        );
        let style = super::super::resolver::with_style_stack(wrapper_meta, Style::new(), || {
            let _live = super::super::context::mark_live_widget_meta();
            // Board resolves its OWN component inline under Wrapper's render.
            super::super::resolver::resolve_component_style(&Board, &["part"])
        });
        assert_eq!(
            style.fg,
            Some(Color::parse("#ff0000").unwrap()),
            "inline-nested render must fall back to the caller's own seed context"
        );
    }

    /// D3 fallback: with NO live context, the seed meta is pushed so
    /// type-qualified rules still resolve (off-tree/unit-test contexts).
    #[test]
    fn off_tree_fallback_resolves_type_qualified_rules() {
        let _guard = super::super::context::set_style_context(StyleSheet::parse(
            "CheckerBoard > .part { color: #00ff00; }",
        ));
        let style = super::super::resolver::resolve_component_style(&Board, &["part"]);
        assert_eq!(style.fg, Some(Color::parse("#00ff00").unwrap()));
    }

    /// G2d pinned BOTH directions: `.part:hover` must NOT match a hovered
    /// widget's part (positive pseudos never match the stateless phantom) ...
    #[test]
    fn part_positive_pseudo_does_not_match_hovered_widget() {
        let _guard = super::super::context::set_style_context(StyleSheet::parse(
            ".part:hover { color: #ff0000; }",
        ));
        let style = resolve_part_live(live_board_meta(false, true));
        assert_eq!(
            style.fg, None,
            ".part:hover must not match even when the widget itself is hovered"
        );
    }

    /// ... while the NEGATIVE pseudos `.part:blur` / `.part:light` MUST keep
    /// matching (stateless-node semantics, identical in Python).
    #[test]
    fn part_negative_pseudos_keep_matching() {
        let _guard = super::super::context::set_style_context(StyleSheet::parse(
            ".part:blur { color: #ff0000; } .part:light { background: #00ff00; }",
        ));
        // Even under a FOCUSED widget, the phantom itself is stateless.
        let style = resolve_part_live(live_board_meta(true, false));
        assert_eq!(
            style.fg,
            Some(Color::parse("#ff0000").unwrap()),
            ".part:blur must match the stateless phantom"
        );
        assert_eq!(
            style.bg,
            Some(Color::parse("#00ff00").unwrap()),
            ".part:light must match the stateless phantom (light default)"
        );
    }

    /// G2b: `Widget { ... }` universal rules must not match the phantom, even
    /// though they match every real widget meta.
    #[test]
    fn widget_universal_rule_does_not_match_phantom() {
        let _guard = super::super::context::set_style_context(StyleSheet::parse(
            "Widget { color: #ff0000; }",
        ));
        let style = resolve_part_live(live_board_meta(false, false));
        assert_eq!(
            style.fg, None,
            "Widget universal rules must not reach component phantoms"
        );
    }

    #[test]
    fn alpha_background_flattens_over_surface() {
        // 50% white over black surface -> mid gray.
        let style = Style::new().bg(Color::rgba(255, 255, 255, 128));
        let rich = component_style_to_rich(&style, Color::rgb(0, 0, 0))
            .expect("bg should be paintable");
        let bg = rich.bgcolor.expect("bgcolor set");
        let bg = crate::style::color_from_simple(bg);
        assert!(
            bg.r > 100 && bg.r < 155,
            "50% white over black should be mid gray, got {bg:?}"
        );
        assert_eq!(bg.r, bg.g);
        assert_eq!(bg.g, bg.b);
    }

    #[test]
    fn component_carried_background_tint_folds_into_bg() {
        // Opaque black bg + 100% white tint -> white.
        let mut style = Style::new().bg(Color::rgb(0, 0, 0));
        style.background_tint = Some(crate::style::Tint {
            color: Color::rgb(255, 255, 255),
            percent: 100,
        });
        let rich = component_style_to_rich(&style, Color::rgb(10, 10, 10))
            .expect("bg should be paintable");
        let bg = crate::style::color_from_simple(rich.bgcolor.expect("bgcolor set"));
        assert!(
            bg.r > 250 && bg.g > 250 && bg.b > 250,
            "100% white tint over black bg should be white, got {bg:?}"
        );
    }

    #[test]
    fn auto_foreground_contrasts_against_under_background() {
        // `auto 38%` on a dark surface -> light-ish fg blended at 38%.
        let mut style = Style::new();
        style.fg_auto = Some(crate::style::AutoColor { alpha_percent: 100 });
        let rich = component_style_to_rich(&style, Color::rgb(10, 10, 10))
            .expect("auto fg should be paintable");
        let fg = crate::style::color_from_simple(rich.color.expect("color set"));
        assert!(
            fg.r > 180 && fg.g > 180 && fg.b > 180,
            "full-alpha auto fg on dark surface should be light, got {fg:?}"
        );
    }

    #[test]
    fn text_opacity_folds_foreground_toward_surface() {
        let style = Style::new()
            .fg(Color::rgb(255, 255, 255))
            .text_opacity(50);
        let rich = component_style_to_rich(&style, Color::rgb(0, 0, 0))
            .expect("fg should be paintable");
        let fg = crate::style::color_from_simple(rich.color.expect("color set"));
        assert!(
            fg.r > 100 && fg.r < 155,
            "50% text-opacity white over black should be mid gray, got {fg:?}"
        );
    }

    #[test]
    fn empty_style_yields_none() {
        assert!(component_style_to_rich(&Style::new(), Color::rgb(0, 0, 0)).is_none());
    }
}
