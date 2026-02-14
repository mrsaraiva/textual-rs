use rich_rs::Console;
use textual::css::set_style_context;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::style::AutoColor;
use textual::style::parse_color_like;

#[test]
fn stylesheet_parser_applies_rules() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (6, 1);
    options.max_width = 6;
    options.max_height = 1;

    let css = r#"
Label { fg: red; bold: true; }
#hero { underline: true; }
.notice { bg: blue; }
"#;

    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let label = Node::new(Label::new("hi")).id("hero").class("notice");
    let renderable = WidgetRenderable::new(&label);
    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    let red = Color::parse("red").expect("parse red");
    let blue = Color::parse("blue").expect("parse blue");
    assert_eq!(style.bold, Some(true));
    assert_eq!(style.underline, Some(true));
    assert_eq!(style.color, Some(red.to_simple_opaque()));
    assert_eq!(style.bgcolor, Some(blue.to_simple_opaque()));
}

#[test]
fn rgba_background_is_composited_over_base_background() {
    use rich_rs::Console;
    use textual::css::set_style_context;
    use textual::render::FrameBuffer;
    use textual::style::parse_color_like;

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (1, 1);
    options.max_width = 1;
    options.max_height = 1;

    let css = r#"
Label { bg: rgba(255,0,0,0.5); }
"#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let label = Label::new("x");
    let renderable = WidgetRenderable::new(&label);
    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    let base = parse_color_like("$background").expect("parse $background");
    let expected = Color::rgba(255, 0, 0, 128).flatten_over(base);
    assert_eq!(style.bgcolor, Some(expected.to_simple_opaque()));
}

#[test]
fn stylesheet_parser_text_style_tokens_and_not_semantics() {
    let css = r#"
Label {
    text-style: $button-focus-text-style not underline;
}
Input {
    text-style: $input-cursor-text-style;
}
    "#;
    let sheet = StyleSheet::parse(css);
    let rules = sheet.rules();
    assert_eq!(rules.len(), 1);

    let label_style = &rules[0].style();
    assert_eq!(label_style.bold, Some(true));
    assert_eq!(label_style.reverse, Some(true));
    assert_eq!(label_style.underline, Some(false));
}

#[test]
fn parse_color_like_supports_transparent_and_ansi_names() {
    use textual::style::parse_color_like;

    assert_eq!(
        parse_color_like("transparent"),
        Some(Color::rgba(0, 0, 0, 0))
    );
    assert_eq!(
        parse_color_like("ansi_default"),
        Some(Color::rgba(0, 0, 0, 0))
    );
    assert_eq!(parse_color_like("ansi_black"), Some(Color::rgb(0, 0, 0)));
    assert_eq!(
        parse_color_like("ansi_bright_white"),
        Some(Color::rgb(255, 255, 255))
    );
}

#[test]
fn stylesheet_parser_markdown_heading_tokens_resolve_in_parse_flow() {
    let css = r#"
Label {
    bg: $markdown-h1-background;
    fg: $markdown-h1-color;
    text-style: $markdown-h1-text-style;
}
"#;
    let sheet = StyleSheet::parse(css);
    let rules = sheet.rules();
    assert_eq!(rules.len(), 1);

    let style = &rules[0].style();
    assert_eq!(style.bg, Some(Color::rgba(0, 0, 0, 0)));
    assert_eq!(style.fg, parse_color_like("$markdown-h1-color"));
    assert_eq!(style.bold, Some(true));
    assert_eq!(style.dim, None);
    assert_eq!(style.italic, None);
    assert_eq!(style.underline, None);
}

#[test]
fn stylesheet_parser_markdown_h6_text_style_token_maps_to_dim() {
    let css = r#"
Label {
    text-style: $markdown-h6-text-style;
}
"#;
    let sheet = StyleSheet::parse(css);
    let rules = sheet.rules();
    assert_eq!(rules.len(), 1);

    let style = &rules[0].style();
    assert_eq!(style.dim, Some(true));
    assert_eq!(style.bold, None);
    assert_eq!(style.italic, None);
    assert_eq!(style.underline, None);
    assert_eq!(style.reverse, None);
}

#[test]
fn stylesheet_parser_color_auto_percent_sets_fg_auto() {
    let css = r#"
Label {
    color: auto 90%;
}
"#;
    let sheet = StyleSheet::parse(css);
    let style = sheet.rules()[0].style();
    assert_eq!(style.fg, None);
    assert_eq!(style.fg_auto, Some(AutoColor::new(90)));
}

#[test]
fn stylesheet_parser_fg_auto_percent_sets_fg_auto() {
    let css = r#"
Label {
    fg: auto 50%;
}
"#;
    let sheet = StyleSheet::parse(css);
    let style = sheet.rules()[0].style();
    assert_eq!(style.fg, None);
    assert_eq!(style.fg_auto, Some(AutoColor::new(50)));
}
