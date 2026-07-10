//! Regression: translucent foreground composition must flatten over the REAL
//! composed ancestor surface, not a widget-locally guessed `$background`.
//!
//! stopwatch03 cluster (201 colour cells): a `Digits` inside a `$boost`
//! container (`background: $boost` = white@0.04 over the Screen's `#121212`
//! = `#1b1b1b`) with `color: $foreground-muted` (`#E0E0E0` @ 0.6) must render
//! its glyphs `#919191` — Python flattens the muted fg over the boosted
//! surface. `Digits::render` used to pre-flatten via `Style::to_rich()`,
//! whose base is `$background` (`#121212`), yielding `#8d8d8d`. Colors are
//! now owned by the generic segment-composition pass, which uses the actual
//! ancestor-composited background.

use rich_rs::Console;
use textual::css::{StyleSheet, default_widget_stylesheet, set_style_context};
use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame_with_stylesheet};

const CSS: &str = r#"
Horizontal {
    background: $boost;
    height: 5;
    margin: 1;
    padding: 1;
}
Static {
    color: $foreground-muted;
}
Digits {
    color: $foreground-muted;
}
"#;

fn render_case(child: impl Widget + 'static) -> textual::render::FrameBuffer {
    let mut stylesheet = default_widget_stylesheet();
    stylesheet.extend(&StyleSheet::parse(CSS));
    let _guard = set_style_context(stylesheet.clone());

    let mut root = AppRoot::new().with_child(Horizontal::new().with_child(child));
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should build");
    render_tree_to_frame_with_stylesheet(&mut tree, &mut root, &console, 40, 10, stylesheet)
}

fn find_glyph(buf: &textual::render::FrameBuffer, glyph: char) -> (usize, usize) {
    buf.as_plain_lines()
        .iter()
        .enumerate()
        .find_map(|(y, line)| {
            line.char_indices()
                .find(|&(_, c)| c == glyph)
                .map(|(byte_x, _)| (line[..byte_x].chars().count(), y))
        })
        .unwrap_or_else(|| panic!("expected {glyph:?} to render"))
}

#[test]
fn digits_muted_fg_flattens_over_boosted_surface() {
    let buf = render_case(Digits::new("00:00"));
    let (x, y) = find_glyph(&buf, '╭');
    let style = buf.get(x, y).style.expect("styled cell");
    // Python: boost surface #1b1b1b, fg-muted over it = #919191 (NOT #8d8d8d,
    // which is the muted fg flattened over the raw $background).
    assert_eq!(
        style.bgcolor,
        Some(rich_rs::SimpleColor::Rgb { r: 27, g: 27, b: 27 }),
        "digit cell background must be the boost-composited surface"
    );
    assert_eq!(
        style.color,
        Some(rich_rs::SimpleColor::Rgb { r: 145, g: 145, b: 145 }),
        "muted fg must flatten over the boosted surface (#919191), got: {:?}",
        style.color
    );
}

#[test]
fn static_muted_fg_flattens_over_boosted_surface() {
    let buf = render_case(Static::new("00:00"));
    let (x, y) = find_glyph(&buf, '0');
    let style = buf.get(x, y).style.expect("styled cell");
    assert_eq!(
        style.bgcolor,
        Some(rich_rs::SimpleColor::Rgb { r: 27, g: 27, b: 27 }),
        "text cell background must be the boost-composited surface"
    );
    assert_eq!(
        style.color,
        Some(rich_rs::SimpleColor::Rgb { r: 145, g: 145, b: 145 }),
        "muted fg must flatten over the boosted surface (#919191), got: {:?}",
        style.color
    );
}
