//! Verification for the public component-class style API (KEYSTONE 1f).
//!
//! Custom widgets can declare `COMPONENT_CLASSES` (via
//! [`Widget::component_classes`]) and resolve them from CSS at render time via
//! [`Widget::get_component_styles`] / [`Widget::get_component_rich_style`],
//! mirroring Python `Widget.get_component_styles` / `get_component_rich_style`.
//!
//! This is the fundamental the `checker02` / `checker03` / `checker04` demo
//! ports were working around by hardcoding `#A5BAC9` / `#004578` / `darkred`.

use rich_rs::{Console, ConsoleOptions, Segments};
use textual::css::{resolve_component_style, set_style_context, StyleSheet};
use textual::prelude::*;
use textual::style::Color;

/// A minimal custom widget that declares two component classes, exactly like
/// the `CheckerBoard` demo widgets.
struct CheckerBoard;

impl Widget for CheckerBoard {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "CheckerBoard"
    }

    fn component_classes(&self) -> &[&'static str] {
        &["checkerboard--white-square", "checkerboard--black-square"]
    }
}

const CSS: &str = r#"
CheckerBoard .checkerboard--white-square {
    background: #A5BAC9;
}
CheckerBoard .checkerboard--black-square {
    background: #004578;
}
"#;

/// The free public function resolves a component class to the CSS-declared
/// background colour (no hardcoding).
#[test]
fn resolve_component_style_reads_css_background() {
    let _guard = set_style_context(StyleSheet::parse(CSS));
    let board = CheckerBoard;

    let white = resolve_component_style(&board, &["checkerboard--white-square"]);
    let black = resolve_component_style(&board, &["checkerboard--black-square"]);

    assert_eq!(
        white.bg,
        Some(Color::parse("#A5BAC9").unwrap()),
        "white square should resolve to the CSS-declared #A5BAC9"
    );
    assert_eq!(
        black.bg,
        Some(Color::parse("#004578").unwrap()),
        "black square should resolve to the CSS-declared #004578"
    );
}

/// The `Widget` trait method (Python parity: `get_component_styles`) resolves
/// the same colours via the declared component class name.
#[test]
fn get_component_styles_trait_method_reads_css() {
    let _guard = set_style_context(StyleSheet::parse(CSS));
    let board = CheckerBoard;

    assert_eq!(
        board.get_component_styles("checkerboard--white-square").bg,
        Some(Color::parse("#A5BAC9").unwrap())
    );
    assert_eq!(
        board.get_component_styles("checkerboard--black-square").bg,
        Some(Color::parse("#004578").unwrap())
    );
}

/// `get_component_rich_style` (Python parity) produces a ready-to-paint Rich
/// style carrying the CSS background colour.
#[test]
fn get_component_rich_style_produces_paintable_bg() {
    let _guard = set_style_context(StyleSheet::parse(CSS));
    let board = CheckerBoard;

    let white = board
        .get_component_rich_style("checkerboard--white-square")
        .expect("white square should yield a paintable rich style");
    let black = board
        .get_component_rich_style("checkerboard--black-square")
        .expect("black square should yield a paintable rich style");

    let white_bg = white.bgcolor.expect("white rich style should carry a bgcolor");
    let black_bg = black.bgcolor.expect("black rich style should carry a bgcolor");

    assert_eq!(
        white_bg,
        Color::parse("#A5BAC9").unwrap().to_simple_opaque(),
        "white rich style bgcolor should match CSS #A5BAC9"
    );
    assert_eq!(
        black_bg,
        Color::parse("#004578").unwrap().to_simple_opaque(),
        "black rich style bgcolor should match CSS #004578"
    );
    assert_ne!(white_bg, black_bg, "the two squares must differ");
}

/// A custom widget can override a component class colour purely from CSS (the
/// whole point of the API): swap the white square to red and observe it.
#[test]
fn component_class_colour_is_css_overridable() {
    let _guard = set_style_context(StyleSheet::parse(
        "CheckerBoard .checkerboard--white-square { background: #ff0000; }",
    ));
    let board = CheckerBoard;

    assert_eq!(
        board.get_component_styles("checkerboard--white-square").bg,
        Some(Color::parse("#ff0000").unwrap()),
        "component-class colour must follow CSS, proving it is not hardcoded"
    );
}

/// The declared component-class set is exposed (Python `COMPONENT_CLASSES`).
#[test]
fn component_classes_are_declared() {
    let board = CheckerBoard;
    assert_eq!(
        board.component_classes(),
        &["checkerboard--white-square", "checkerboard--black-square"]
    );
}
