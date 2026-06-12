mod ast;
mod context;
mod debug;
mod matching;
mod parser;
mod resolver;
mod segments;

// Public re-exports (used by `src/css/mod.rs` and external consumers)
pub(crate) use ast::{Combinator, SelectorChain, SelectorMeta};
pub use ast::{PseudoClass, StyleRule, StyleSelector, StyleSheet};
pub use context::{
    AppActiveGuard, AppRuntimePseudos, AppRuntimePseudosGuard, StyleContextGuard, set_app_active,
    set_app_runtime_pseudos, set_style_context,
};
// Re-exported for future use by the event loop / monolithic render path.
#[allow(unused_imports)]
pub use context::{FocusWithinGuard, set_focus_within};
pub(crate) use parser::parse_selector_list;

// Crate-internal re-exports
pub(crate) use resolver::{
    apply_display_visibility_to_tree, begin_style_render_pass, current_composited_background,
    current_parent_style, node_selector_meta, node_selector_meta_from_node, pop_style_context,
    push_style_context, resolve_component_style, resolve_node_style, resolve_style,
    resolve_style_for_meta, selector_meta_component, selector_meta_generic,
    take_layout_affected_style_changes, with_style_stack,
};
pub(crate) use segments::{apply_style_to_segments, apply_widget_opacity_to_segments};

#[cfg(test)]
mod tests {
    use super::parser::{
        parse_duration, parse_style_body, parse_transition_shorthand, parse_transition_timing,
    };
    use super::resolver::{
        begin_style_render_pass, computed_style_cache_stats_for_tests,
        reset_computed_style_cache_for_tests, resolve_style, selector_meta_generic,
        take_layout_affected_style_changes, with_style_stack,
    };
    use super::segments::{apply_style_to_segments, apply_widget_opacity_to_segments};
    use crate::css::{StyleSheet, default_widget_stylesheet};
    use crate::node_id::node_id_from_ffi;
    use crate::style::{Color, Style, TransitionTiming};
    use crate::widgets::{Button, Widget};
    use rich_rs::{Segment, Segments};
    use std::time::Duration;

    struct ProbeWidget {
        classes: Vec<String>,
        style_id: Option<String>,
        focused: bool,
    }

    impl ProbeWidget {
        fn new() -> Self {
            Self {
                classes: Vec::new(),
                style_id: None,
                focused: false,
            }
        }
    }

    impl Widget for ProbeWidget {
        fn render(
            &self,
            _console: &rich_rs::Console,
            _options: &rich_rs::ConsoleOptions,
        ) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            "Probe"
        }

        fn style_id(&self) -> Option<&str> {
            self.style_id.as_deref()
        }

        fn style_classes(&self) -> &[String] {
            &self.classes
        }

        fn has_focus(&self) -> bool {
            self.focused
        }
    }

    #[test]
    fn parses_transition_shorthand_duration_delay_and_timing() {
        let parsed = parse_transition_shorthand("offset 300ms ease-in-out 75ms")
            .expect("transition shorthand should parse");
        assert_eq!(parsed.0, Some(Duration::from_millis(300)));
        assert_eq!(parsed.1, Some(Duration::from_millis(75)));
        assert_eq!(parsed.2, Some(TransitionTiming::InOutCubic));
    }

    #[test]
    fn parses_duration_units() {
        assert_eq!(parse_duration("250ms"), Some(Duration::from_millis(250)));
        assert_eq!(parse_duration("0.5s"), Some(Duration::from_millis(500)));
        assert_eq!(parse_duration("bogus"), None);
    }

    #[test]
    fn parses_transition_timing_aliases() {
        assert_eq!(
            parse_transition_timing("ease-out"),
            Some(TransitionTiming::OutCubic)
        );
        assert_eq!(
            parse_transition_timing("steps(1,end)"),
            Some(TransitionTiming::Round)
        );
        assert_eq!(parse_transition_timing("unknown"), None);
    }

    #[test]
    fn applies_background_to_unstyled_segments() {
        let mut segments = Segments::new();
        segments.push(Segment::new("   "));
        let style = Style::new().bg(Color::parse("#334455").expect("valid color"));
        let styled = apply_style_to_segments(node_id_from_ffi(1), segments, style, None);
        let bg = styled
            .into_iter()
            .next()
            .and_then(|segment| segment.style)
            .and_then(|segment_style| segment_style.bgcolor);
        assert!(
            bg.is_some(),
            "background should be applied to unstyled segments"
        );
    }

    #[test]
    fn default_terminal_background_is_treated_as_transparent_for_composition() {
        let mut segments = Segments::new();
        let seg_style = rich_rs::Style::new().with_bgcolor(rich_rs::SimpleColor::Default);
        segments.push(Segment::styled("x", seg_style));
        let style = Style::new().bg(Color::parse("#334455").expect("valid color"));
        let styled = apply_style_to_segments(node_id_from_ffi(1), segments, style, None);
        let bg = styled
            .into_iter()
            .next()
            .and_then(|segment| segment.style)
            .and_then(|segment_style| segment_style.bgcolor)
            .expect("background should be composed");
        let bg = crate::style::color_from_simple(bg);
        assert_eq!(
            bg,
            Color::parse("#334455").expect("valid color"),
            "default terminal background should be replaced by widget background in composition"
        );
    }

    #[test]
    fn parses_auto_foreground_styles() {
        let style = parse_style_body("fg: auto 87%;");
        assert!(
            style.fg.is_none(),
            "auto fg should not set a concrete color"
        );
        assert_eq!(
            style.fg_auto.map(|auto| auto.alpha_percent),
            Some(87),
            "auto fg percent should be parsed"
        );
    }

    #[test]
    fn parses_auto_foreground_from_text_token() {
        let style = parse_style_body("fg: $button-color-foreground;");
        assert!(
            style.fg.is_none(),
            "token should resolve to auto fg semantics"
        );
        assert_eq!(
            style.fg_auto.map(|auto| auto.alpha_percent),
            Some(87),
            "$button-color-foreground should map to Textual auto text 87%"
        );
    }

    #[test]
    fn auto_foreground_contrasts_against_background() {
        let mut dark_segments = Segments::new();
        dark_segments.push(Segment::new("x"));
        let dark_style = parse_style_body("bg: #121212; fg: auto 87%;");
        let dark = apply_style_to_segments(node_id_from_ffi(1), dark_segments, dark_style, None);
        let dark_fg = dark
            .into_iter()
            .next()
            .and_then(|segment| segment.style)
            .and_then(|segment_style| segment_style.color)
            .expect("auto fg should resolve on dark backgrounds");

        let mut light_segments = Segments::new();
        light_segments.push(Segment::new("x"));
        let light_style = parse_style_body("bg: #f5f5f5; fg: auto 87%;");
        let light = apply_style_to_segments(node_id_from_ffi(1), light_segments, light_style, None);
        let light_fg = light
            .into_iter()
            .next()
            .and_then(|segment| segment.style)
            .and_then(|segment_style| segment_style.color)
            .expect("auto fg should resolve on light backgrounds");

        let dark_fg = crate::style::color_from_simple(dark_fg);
        let light_fg = crate::style::color_from_simple(light_fg);

        assert!(
            dark_fg.r > 180 && dark_fg.g > 180 && dark_fg.b > 180,
            "auto fg should resolve to a light color on dark backgrounds"
        );
        assert!(
            light_fg.r < 80 && light_fg.g < 80 && light_fg.b < 80,
            "auto fg should resolve to a dark color on light backgrounds"
        );
    }

    #[test]
    fn parses_text_opacity() {
        let style = parse_style_body("text-opacity: 0.6;");
        assert_eq!(style.text_opacity, Some(60));

        let style = parse_style_body("text-opacity: 42%;");
        assert_eq!(style.text_opacity, Some(42));
    }

    #[test]
    fn parses_hkey_and_vkey_border_types() {
        let vkey = parse_style_body("border-left: vkey $foreground 30%;");
        assert_eq!(vkey.border_left.edge_type(), "vkey");

        let hkey = parse_style_body("border: hkey $foreground;");
        assert_eq!(hkey.border_top.edge_type(), "hkey");
        assert_eq!(hkey.border_right.edge_type(), "hkey");
        assert_eq!(hkey.border_bottom.edge_type(), "hkey");
        assert_eq!(hkey.border_left.edge_type(), "hkey");
    }

    #[test]
    fn text_opacity_applies_to_existing_foreground() {
        let mut segments = Segments::new();
        let rich_style = rich_rs::Style::new()
            .with_color(crate::style::Color::rgb(255, 255, 255).to_simple_opaque());
        segments.push(Segment::styled("x", rich_style));

        let style = parse_style_body("bg: #000000; text-opacity: 50%;");
        let styled = apply_style_to_segments(node_id_from_ffi(1), segments, style, None);
        let fg = styled
            .into_iter()
            .next()
            .and_then(|segment| segment.style)
            .and_then(|segment_style| segment_style.color)
            .expect("expected foreground color");
        let fg = crate::style::color_from_simple(fg);

        assert!(
            fg.r >= 120 && fg.r <= 136 && fg.g >= 120 && fg.g <= 136 && fg.b >= 120 && fg.b <= 136,
            "text-opacity 50% over black should produce medium gray, got {:?}",
            fg
        );
    }

    #[test]
    fn auto_foreground_uses_tinted_background_for_contrast() {
        let mut segments = Segments::new();
        segments.push(Segment::new("x"));
        let style = parse_style_body("bg: #121212; background-tint: #ffffff 100%; fg: auto 87%;");
        let styled = apply_style_to_segments(node_id_from_ffi(1), segments, style, None);
        let fg = styled
            .into_iter()
            .next()
            .and_then(|segment| segment.style)
            .and_then(|segment_style| segment_style.color)
            .expect("auto fg should resolve");
        let fg = crate::style::color_from_simple(fg);

        assert!(
            fg.r < 80 && fg.g < 80 && fg.b < 80,
            "auto fg should resolve dark after full light tint, got {:?}",
            fg
        );
    }

    #[test]
    fn disabled_primary_button_uses_text_opacity() {
        // Python-aligned: disabled buttons dim via text-opacity inside the variant
        // block (.-style-default:disabled { text-opacity: 60%; }), not widget-level opacity.
        let _guard = super::context::set_style_context(default_widget_stylesheet());
        let enabled = Button::primary("Primary!");
        let disabled = Button::primary("Primary!").disabled(true);

        let enabled_style = resolve_style(&enabled, &selector_meta_generic(&enabled));
        let disabled_style = resolve_style(&disabled, &selector_meta_generic(&disabled));

        assert_eq!(
            enabled_style.fg_auto.map(|value| value.alpha_percent),
            Some(87)
        );
        assert_eq!(
            disabled_style.fg_auto.map(|value| value.alpha_percent),
            Some(87),
            "disabled primary keeps auto-foreground alpha and dims via text opacity"
        );
        assert_eq!(disabled_style.text_opacity, Some(60));
    }

    #[test]
    fn disabled_button_matches_global_disabled_can_focus_opacity_rule() {
        let _guard = super::context::set_style_context(default_widget_stylesheet());
        let enabled = Button::new("Default");
        let disabled = Button::new("Default").disabled(true);

        let enabled_style = resolve_style(&enabled, &selector_meta_generic(&enabled));
        let disabled_style = resolve_style(&disabled, &selector_meta_generic(&disabled));

        assert_eq!(enabled_style.opacity, None);
        assert_eq!(disabled_style.opacity, Some(70));
    }

    #[test]
    fn widget_opacity_dims_background_and_text_together() {
        let original_bg = crate::style::Color::rgb(1, 120, 212);
        let original_fg = crate::style::Color::rgb(221, 237, 249);
        let parent_bg = crate::style::parse_color_like("$background").expect("theme background");
        let mut segments = Segments::new();
        let style = rich_rs::Style::new()
            .with_bgcolor(original_bg.to_simple_opaque())
            .with_color(original_fg.to_simple_opaque());
        segments.push(Segment::styled("x", style));
        let out = apply_widget_opacity_to_segments(segments, 70, None);
        let style = out
            .into_iter()
            .next()
            .and_then(|segment| segment.style)
            .expect("style exists");
        let bg = crate::style::color_from_simple(style.bgcolor.expect("bg"));
        let fg = crate::style::color_from_simple(style.color.expect("fg"));

        let dist = |a: crate::style::Color, b: crate::style::Color| -> i32 {
            let dr = a.r as i32 - b.r as i32;
            let dg = a.g as i32 - b.g as i32;
            let db = a.b as i32 - b.b as i32;
            dr * dr + dg * dg + db * db
        };

        assert!(
            dist(bg, parent_bg) < dist(original_bg, parent_bg),
            "background should move toward parent with opacity"
        );
        assert!(
            dist(fg, bg) < dist(original_fg, original_bg),
            "foreground should move toward local background with opacity"
        );
    }

    #[test]
    fn computed_style_cache_hits_for_stable_widget() {
        reset_computed_style_cache_for_tests();
        let _guard = super::context::set_style_context(StyleSheet::parse(
            "Probe#target.on { fg: #ff00aa; }",
        ));
        let mut widget = ProbeWidget::new();
        widget.style_id = Some("target".to_string());
        widget.classes.push("on".to_string());
        let meta = selector_meta_generic(&widget);

        let first = resolve_style(&widget, &meta);
        let second = resolve_style(&widget, &meta);

        assert_eq!(first.fg, second.fg);
        let (hits, misses) = computed_style_cache_stats_for_tests();
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
    }

    #[test]
    fn computed_style_cache_invalidates_for_ancestor_selector_change() {
        reset_computed_style_cache_for_tests();
        let _guard = super::context::set_style_context(StyleSheet::parse(
            "Probe.panel Probe.child { fg: #00ffaa; }",
        ));

        let mut parent = ProbeWidget::new();
        parent.classes.push("panel".to_string());
        let mut child = ProbeWidget::new();
        child.classes.push("child".to_string());

        let parent_meta = selector_meta_generic(&parent);
        let parent_style = resolve_style(&parent, &parent_meta);
        let child_with_panel = with_style_stack(parent_meta.clone(), parent_style, || {
            resolve_style(&child, &selector_meta_generic(&child))
        });

        parent.classes.clear();
        parent.classes.push("other".to_string());
        let parent_meta_changed = selector_meta_generic(&parent);
        let parent_style_changed = resolve_style(&parent, &parent_meta_changed);
        let child_with_other = with_style_stack(parent_meta_changed, parent_style_changed, || {
            resolve_style(&child, &selector_meta_generic(&child))
        });

        assert_ne!(child_with_panel.fg, child_with_other.fg);
    }

    #[test]
    fn runtime_pseudos_are_driven_by_css_context_state() {
        reset_computed_style_cache_for_tests();
        let _guard =
            super::context::set_style_context(StyleSheet::parse("Probe:inline { bold: true; }"));
        let _active = super::context::set_app_active(true);
        let widget = ProbeWidget::new();

        begin_style_render_pass();
        let style_without_inline = resolve_style(&widget, &selector_meta_generic(&widget));
        assert_ne!(style_without_inline.bold, Some(true));

        let _pseudo_guard =
            super::context::set_app_runtime_pseudos(super::context::AppRuntimePseudos {
                dark: false,
                inline: true,
                ansi: false,
                nocolor: false,
            });
        begin_style_render_pass();
        let style_with_inline = resolve_style(&widget, &selector_meta_generic(&widget));
        assert_eq!(style_with_inline.bold, Some(true));
    }

    #[test]
    fn segment_background_uses_ancestor_surface_when_parent_has_no_bg() {
        reset_computed_style_cache_for_tests();
        let root = ProbeWidget::new();
        let parent = ProbeWidget::new();
        let root_meta = selector_meta_generic(&root);
        let parent_meta = selector_meta_generic(&parent);
        let root_surface = Style::default().bg(Color::rgb(0x22, 0x33, 0x44));
        let parent_transparent = Style::default();

        let styled = with_style_stack(root_meta, root_surface, || {
            with_style_stack(parent_meta, parent_transparent, || {
                apply_style_to_segments(
                    node_id_from_ffi(1),
                    Segments::from(vec![Segment::new("x")]),
                    Style::default().fg(Color::rgb(255, 255, 255)),
                    None,
                )
            })
        });

        let cell_style = styled
            .into_iter()
            .find(|segment| segment.control.is_none())
            .and_then(|segment| segment.style)
            .expect("styled segment");
        assert_eq!(
            cell_style.bgcolor,
            Some(Color::rgb(0x22, 0x33, 0x44).to_simple_opaque()),
            "transparent descendants should compose over ancestor surface instead of terminal default",
        );
    }

    #[test]
    fn layout_affecting_computed_style_change_is_tracked_in_pass() {
        reset_computed_style_cache_for_tests();
        let _guard = super::context::set_style_context(StyleSheet::parse(
            "Probe { min-width: 1; } Probe:focus { min-width: 12; }",
        ));
        let mut widget = ProbeWidget::new();
        widget.focused = false;

        begin_style_render_pass();
        let _ = resolve_style(&widget, &selector_meta_generic(&widget));
        assert!(!take_layout_affected_style_changes());

        widget.focused = true;
        begin_style_render_pass();
        let focused = resolve_style(&widget, &selector_meta_generic(&widget));
        assert_eq!(focused.min_width, Some(crate::style::Scalar::Cells(12)));
        assert!(take_layout_affected_style_changes());
    }

    // -- CSS display / visibility / overflow parsing --------------------------

    #[test]
    fn parses_display_none_and_block() {
        let style = parse_style_body("display: none;");
        assert_eq!(style.display, Some(crate::style::Display::None));

        let style = parse_style_body("display: block;");
        assert_eq!(style.display, Some(crate::style::Display::Block));
    }

    #[test]
    fn parses_visibility_hidden_and_visible() {
        let style = parse_style_body("visibility: hidden;");
        assert_eq!(style.visibility, Some(crate::style::Visibility::Hidden));

        let style = parse_style_body("visibility: visible;");
        assert_eq!(style.visibility, Some(crate::style::Visibility::Visible));
    }

    #[test]
    fn parses_overflow_auto_hidden_scroll() {
        let style = parse_style_body("overflow: auto;");
        assert_eq!(style.overflow, Some(crate::style::Overflow::Auto));
        assert_eq!(style.overflow_x, Some(crate::style::Overflow::Auto));
        assert_eq!(style.overflow_y, Some(crate::style::Overflow::Auto));

        let style = parse_style_body("overflow: hidden;");
        assert_eq!(style.overflow, Some(crate::style::Overflow::Hidden));
        assert_eq!(style.overflow_x, Some(crate::style::Overflow::Hidden));
        assert_eq!(style.overflow_y, Some(crate::style::Overflow::Hidden));

        let style = parse_style_body("overflow: scroll;");
        assert_eq!(style.overflow, Some(crate::style::Overflow::Scroll));
        assert_eq!(style.overflow_x, Some(crate::style::Overflow::Scroll));
        assert_eq!(style.overflow_y, Some(crate::style::Overflow::Scroll));
    }

    #[test]
    fn parses_overflow_x_and_overflow_y() {
        // overflow-x only sets overflow_x, not overflow or overflow_y
        let style = parse_style_body("overflow-x: hidden;");
        assert_eq!(style.overflow, None);
        assert_eq!(style.overflow_x, Some(crate::style::Overflow::Hidden));
        assert_eq!(style.overflow_y, None);

        // overflow-y only sets overflow_y, not overflow or overflow_x
        let style = parse_style_body("overflow-y: scroll;");
        assert_eq!(style.overflow, None);
        assert_eq!(style.overflow_x, None);
        assert_eq!(style.overflow_y, Some(crate::style::Overflow::Scroll));

        // Combined: shorthand + per-axis override
        let style = parse_style_body("overflow: auto; overflow-x: hidden;");
        assert_eq!(style.overflow_x, Some(crate::style::Overflow::Hidden));
        assert_eq!(style.overflow_y, Some(crate::style::Overflow::Auto));
    }

    #[test]
    fn overflow_not_inherited() {
        let parent = parse_style_body("overflow: hidden;");
        let child = Style::new().inherit_from(&parent);
        assert_eq!(child.overflow, None);
        assert_eq!(child.overflow_x, None);
        assert_eq!(child.overflow_y, None);
    }

    #[test]
    fn parses_pointer_property() {
        let style = parse_style_body("pointer: default;");
        assert_eq!(style.pointer, Some(crate::style::Pointer::Default));

        let style = parse_style_body("pointer: pointer;");
        assert_eq!(style.pointer, Some(crate::style::Pointer::Pointer));

        let style = parse_style_body("pointer: text;");
        assert_eq!(style.pointer, Some(crate::style::Pointer::Text));

        let style = parse_style_body("pointer: not-allowed;");
        assert_eq!(style.pointer, Some(crate::style::Pointer::NotAllowed));

        // Unknown value -> None
        let style = parse_style_body("pointer: crosshair;");
        assert_eq!(style.pointer, None);
    }

    #[test]
    fn display_none_in_stylesheet_resolves() {
        let _guard =
            super::context::set_style_context(StyleSheet::parse("Probe.hidden { display: none; }"));
        let mut widget = ProbeWidget::new();
        widget.classes.push("hidden".to_string());
        let meta = selector_meta_generic(&widget);
        let style = resolve_style(&widget, &meta);
        assert_eq!(style.display, Some(crate::style::Display::None));
    }
}
