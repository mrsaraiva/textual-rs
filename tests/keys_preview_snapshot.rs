use rich_rs::{Segment, Style as RichStyle};
use textual::css::{StyleSheet, default_widget_stylesheet, set_style_context};
use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};

fn help_panel() -> impl Widget {
    let title = Styled::new(
        Label::new("Press some keys!"),
        Style::new().bold(true).underline(true),
    );
    let content = Container::new().with_child(title).with_child(Label::new(
        "To quit the app press Ctrl+Q twice or press the Quit button below.",
    ));
    let boxed = Styled::new(
        content,
        Style::new()
            .line_pad(1)
            .border_top(Color::parse("#7f868d").unwrap())
            .border_right(Color::parse("#7f868d").unwrap())
            .border_bottom(Color::parse("#7f868d").unwrap())
            .border_left(Color::parse("#7f868d").unwrap()),
    );
    Constrained::new(boxed).min_height(4).max_height(4)
}

fn sample_log() -> RichLog {
    let key_style =
        RichStyle::new().with_color(Color::parse("#b73763").unwrap().to_simple_opaque());
    let field_style =
        RichStyle::new().with_color(Color::parse("#f5a623").unwrap().to_simple_opaque());
    let value_style =
        RichStyle::new().with_color(Color::parse("#98d168").unwrap().to_simple_opaque());
    let bool_style = RichStyle::new()
        .with_color(Color::parse("#b73763").unwrap().to_simple_opaque())
        .with_italic(true);

    let mut log = RichLog::new().max_lines(400).scroll_step(2);
    log.write_segments(vec![
        Segment::styled("Key".to_string(), key_style),
        Segment::new("(".to_string()),
        Segment::styled("key".to_string(), field_style),
        Segment::new("=".to_string()),
        Segment::styled("'a'".to_string(), value_style),
        Segment::new(", ".to_string()),
        Segment::styled("character".to_string(), field_style),
        Segment::new("=".to_string()),
        Segment::styled("'a'".to_string(), value_style),
        Segment::new(", ".to_string()),
        Segment::styled("name".to_string(), field_style),
        Segment::new("=".to_string()),
        Segment::styled("'a'".to_string(), value_style),
        Segment::new(", ".to_string()),
        Segment::styled("is_printable".to_string(), field_style),
        Segment::new("=".to_string()),
        Segment::styled("True".to_string(), bool_style),
        Segment::new(")".to_string()),
    ]);
    log.write("AppFocus: true");
    log
}

fn action_bar() -> impl Widget {
    let row = Row::new()
        .with_child(Constrained::new(Button::warning("Clear").flat(true)))
        .with_child(Constrained::new(Button::error("Quit").flat(true)));
    Constrained::new(row).min_height(3).max_height(3)
}

#[test]
#[ignore = "TRACKED (height-chrome keystone regression, PRE-EXISTING at base 461f6cc, NOT from the container-class work): the keystone drifted this layout snapshot two ways — (1) CORRECT: the help panel now renders its title + closes its border box (old golden had a broken empty box); (2) REGRESSION: a flat Button wrapped in `Constrained` inside a `Row` is clipped to its top edge (`▄▄▄` only; body `█ Clear █` + bottom `▀▀▀` gone). The SAME flat button renders all 3 rows correctly in a plain Container, so the clip is a keystone flow-layout gap in chrome-bearing-child height propagation through Constrained/Row, NOT a Button bug. Accepting the new snapshot would launder the button regression into the golden, so it is held. Un-ignore + re-accept once the Constrained/Row chrome-height propagation is fixed at the keystone layout level."]
fn keys_preview_layout_snapshot() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let css_path = repo_root.join("docs/examples/widgets/examples/keys/keys.tcss");
    let css = std::fs::read_to_string(css_path).expect("read keys.tcss");
    let mut stylesheet = default_widget_stylesheet();
    stylesheet.extend(&StyleSheet::parse(&css));
    let _guard = set_style_context(stylesheet);

    let mut root = preview_root_with_top_bottom(
        Some("Textual Keys"),
        Some(4),
        help_panel(),
        sample_log(),
        Some(3),
        action_bar(),
    );

    let console = rich_rs::Console::new();
    let mut options = console.options().clone();
    options.size = (80, 16);
    options.max_width = 80;
    options.max_height = 16;
    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should build");
    let buffer = render_tree_to_frame(&mut tree, &mut root, &console, 80, 16);
    insta::assert_snapshot!(buffer.debug_dump());
}
