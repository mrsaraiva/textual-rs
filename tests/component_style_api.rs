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

// ── Phantom identity (Python virtual-node semantics) ────────────────────────

/// G2a regression: a `CheckerBoard { color: ... }` TYPE rule must NOT leak
/// into the component style (Python's virtual node is typeless).
#[test]
fn widget_type_rule_does_not_leak_into_component_style() {
    let _guard = set_style_context(StyleSheet::parse(
        "CheckerBoard { color: #ff0000; } CheckerBoard .checkerboard--white-square { background: #A5BAC9; }",
    ));
    let board = CheckerBoard;
    let style = resolve_component_style(&board, &["checkerboard--white-square"]);
    assert_eq!(
        style.fg, None,
        "type rules must not match the typeless component phantom"
    );
    assert_eq!(style.bg, Some(Color::parse("#A5BAC9").unwrap()));
}

/// G2b regression: a `Widget { color: ... }` UNIVERSAL rule must NOT match a
/// component phantom (Python's bare DOMNode is not matched by Widget rules).
#[test]
fn widget_universal_rule_does_not_leak_into_component_style() {
    let _guard = set_style_context(StyleSheet::parse("Widget { color: #ff0000; }"));
    let board = CheckerBoard;
    let style = resolve_component_style(&board, &["checkerboard--white-square"]);
    assert_eq!(
        style.fg, None,
        "Widget universal rules must not match component phantoms"
    );
}

/// G2c regression: a compound `CheckerBoard.checkerboard--white-square` rule
/// (type + component class on ONE selector) must not style the part — the
/// phantom is typeless, exactly like Python's virtual node.
#[test]
fn type_class_compound_rule_does_not_style_component() {
    let _guard = set_style_context(StyleSheet::parse(
        "CheckerBoard.checkerboard--white-square { background: #00ff00; }",
    ));
    let board = CheckerBoard;
    let style = resolve_component_style(&board, &["checkerboard--white-square"]);
    assert_eq!(style.bg, None);
}

/// Negative pseudos match the stateless phantom (Python parity): `.part:blur`
/// and `.part:light` style the part; positive `.part:focus` does not.
#[test]
fn stateless_phantom_matches_negative_pseudos_only() {
    let _guard = set_style_context(StyleSheet::parse(
        r#"
        .checkerboard--white-square:blur { color: #ff0000; }
        .checkerboard--white-square:light { background: #00ff00; }
        .checkerboard--black-square:focus { color: #0000ff; }
        "#,
    ));
    let board = CheckerBoard;
    let white = resolve_component_style(&board, &["checkerboard--white-square"]);
    assert_eq!(white.fg, Some(Color::parse("#ff0000").unwrap()));
    assert_eq!(white.bg, Some(Color::parse("#00ff00").unwrap()));
    let black = resolve_component_style(&board, &["checkerboard--black-square"]);
    assert_eq!(black.fg, None, ".part:focus must never match the phantom");
}

// ── Multi-name semantics (D4) ────────────────────────────────────────────────

/// Merged form (Python `get_component_styles(*names)`): per-name resolution
/// combined in ARGUMENT order — a later name wins even against a rule of
/// higher specificity for an earlier name.
#[test]
fn merged_multi_name_later_name_wins_regardless_of_specificity() {
    let _guard = set_style_context(StyleSheet::parse(
        r#"
        CheckerBoard > .checkerboard--white-square { color: #ff0000; background: #111111; }
        .checkerboard--black-square { color: #0000ff; }
        "#,
    ));
    let board = CheckerBoard;
    let merged = textual::css::resolve_component_style_merged(
        &board,
        &["checkerboard--white-square", "checkerboard--black-square"],
    );
    assert_eq!(
        merged.fg,
        Some(Color::parse("#0000ff").unwrap()),
        "later argument must win over the earlier name's higher-specificity rule"
    );
    assert_eq!(
        merged.bg,
        Some(Color::parse("#111111").unwrap()),
        "properties only set by the earlier name are kept"
    );
}

/// Compound form (state-marker usage): all names on ONE phantom, so compound
/// `.a.b` rules match — and they do NOT match under the merged form.
#[test]
fn compound_form_matches_compound_class_rules() {
    let _guard = set_style_context(StyleSheet::parse(
        ".checkerboard--white-square.-active { color: #00ff00; }",
    ));
    let board = CheckerBoard;
    let compound = resolve_component_style(&board, &["checkerboard--white-square", "-active"]);
    assert_eq!(compound.fg, Some(Color::parse("#00ff00").unwrap()));

    let merged = textual::css::resolve_component_style_merged(
        &board,
        &["checkerboard--white-square", "-active"],
    );
    assert_eq!(
        merged.fg, None,
        "merged form resolves per name, so compound rules never match"
    );
}

/// Partial form: only sheet-set properties, no inherit-from-parent step.
#[test]
fn partial_form_returns_only_sheet_set_properties() {
    let _guard = set_style_context(StyleSheet::parse(
        "CheckerBoard .checkerboard--white-square { background: #A5BAC9; }",
    ));
    let board = CheckerBoard;
    let partial =
        textual::css::resolve_component_style_partial(&board, &["checkerboard--white-square"]);
    assert_eq!(partial.bg, Some(Color::parse("#A5BAC9").unwrap()));
    assert_eq!(partial.fg, None);
}

// ── Declaration validation (D1) ──────────────────────────────────────────────

/// An undeclared name fired through the validated trait method asserts in
/// debug builds (Python raises `KeyError`).
#[test]
#[cfg_attr(debug_assertions, should_panic(expected = "not declared in component_classes"))]
fn undeclared_component_name_fails_debug_validation() {
    let _guard = set_style_context(StyleSheet::parse(""));
    let board = CheckerBoard;
    let _ = board.get_component_styles("checkerboard--no-such-part");
    // In release builds the resolution proceeds (logged via the style debug
    // facility); nothing to assert here.
    #[cfg(debug_assertions)]
    unreachable!();
}
