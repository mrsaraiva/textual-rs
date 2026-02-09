mod ast;
mod context;
mod debug;
mod matching;
mod parser;
mod resolver;
mod segments;

// Public re-exports (used by `src/css/mod.rs` and external consumers)
pub use ast::{StyleRule, StyleSelector, StyleSheet};
pub use context::{AppActiveGuard, StyleContextGuard, set_app_active, set_style_context};

// Crate-internal re-exports
pub(crate) use resolver::{
    current_parent_style, resolve_component_style, resolve_component_style_with_id, resolve_style,
    resolve_style_for_meta, selector_meta_component, selector_meta_generic, with_style_stack,
};
pub(crate) use segments::{apply_style_to_segments, apply_widget_opacity_to_segments};

#[cfg(test)]
mod tests {
    use super::parser::{parse_duration, parse_style_body, parse_transition_shorthand, parse_transition_timing};
    use super::resolver::{resolve_style, selector_meta_generic};
    use super::segments::{apply_style_to_segments, apply_widget_opacity_to_segments};
    use crate::css::default_widget_stylesheet;
    use crate::style::{Color, Style, TransitionTiming};
    use crate::widgets::{Button, WidgetId};
    use rich_rs::{Segment, Segments};
    use std::time::Duration;

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
        let styled = apply_style_to_segments(WidgetId::from_u64(1), segments, style, None);
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
    fn parses_auto_foreground_styles() {
        let style = parse_style_body("fg: auto 87%;");
        assert!(style.fg.is_none(), "auto fg should not set a concrete color");
        assert_eq!(
            style.fg_auto.map(|auto| auto.alpha_percent),
            Some(87),
            "auto fg percent should be parsed"
        );
    }

    #[test]
    fn parses_auto_foreground_from_text_token() {
        let style = parse_style_body("fg: $button-color-foreground;");
        assert!(style.fg.is_none(), "token should resolve to auto fg semantics");
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
        let dark = apply_style_to_segments(WidgetId::from_u64(1), dark_segments, dark_style, None);
        let dark_fg = dark
            .into_iter()
            .next()
            .and_then(|segment| segment.style)
            .and_then(|segment_style| segment_style.color)
            .expect("auto fg should resolve on dark backgrounds");

        let mut light_segments = Segments::new();
        light_segments.push(Segment::new("x"));
        let light_style = parse_style_body("bg: #f5f5f5; fg: auto 87%;");
        let light =
            apply_style_to_segments(WidgetId::from_u64(1), light_segments, light_style, None);
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
    fn text_opacity_applies_to_existing_foreground() {
        let mut segments = Segments::new();
        let rich_style =
            rich_rs::Style::new().with_color(crate::style::Color::rgb(255, 255, 255).to_simple_opaque());
        segments.push(Segment::styled("x", rich_style));

        let style = parse_style_body("bg: #000000; text-opacity: 50%;");
        let styled = apply_style_to_segments(WidgetId::from_u64(1), segments, style, None);
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
        let styled = apply_style_to_segments(WidgetId::from_u64(1), segments, style, None);
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
    fn disabled_primary_button_uses_text_and_widget_opacity() {
        let _guard = super::context::set_style_context(default_widget_stylesheet());
        let enabled = Button::primary("Primary!");
        let disabled = Button::primary("Primary!").disabled(true);

        let enabled_style = resolve_style(&enabled, &selector_meta_generic(&enabled));
        let disabled_style = resolve_style(&disabled, &selector_meta_generic(&disabled));

        assert_eq!(enabled_style.fg_auto.map(|value| value.alpha_percent), Some(87));
        assert_eq!(
            disabled_style.fg_auto.map(|value| value.alpha_percent),
            Some(87),
            "disabled primary keeps auto-foreground alpha and dims via text/widget opacity"
        );
        assert_eq!(disabled_style.text_opacity, Some(60));
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
}
