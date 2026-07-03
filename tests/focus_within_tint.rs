//! Regression: `:focus-within` background-tint must be applied during render.
//!
//! Python Textual applies `Container:focus-within { background-tint: ... }` when
//! the container has a focused descendant (or is itself focused). The Rust render
//! pipeline must install the `:focus-within` node set (`set_focus_within`) so the
//! tint composites onto the container surface. Prior to wiring this, the tint was
//! silently dropped (e.g. focused Collapsible/RadioSet/OptionList surfaces stayed
//! `$surface` instead of the tinted `$surface + $foreground 5%`).

use rich_rs::Console;
use textual::css::{StyleSheet, default_widget_stylesheet, set_app_active, set_style_context};
use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};

#[test]
fn focus_within_applies_background_tint() {
    let _active = set_app_active(true);
    // Container tints its surface when a descendant is focused; the child Input
    // has an explicit surface bg so we can observe the container tint composited
    // onto the child that inherits it.
    let css = r#"
Vertical {
    background: #1e1e1e;
    &:focus-within {
        background-tint: #e0e0e0 5%;
    }
}
"#;
    let mut sheet = default_widget_stylesheet();
    sheet.extend(&StyleSheet::parse(css));
    let _guard = set_style_context(sheet);

    let mut root =
        AppRoot::new().with_child(Vertical::new().with_child(Input::new().with_placeholder("x")));
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(&mut root).expect("tree builds");

    let input_id = tree.query_one("Input").expect("input node");
    tree.set_focus_state(input_id, true);

    let buf = render_tree_to_frame(&mut tree, &mut root, &console, 30, 6);

    // #1e1e1e tinted by #e0e0e0 at 5% => #272727 (Python `Color.tint`).
    let tinted = rich_rs::SimpleColor::Rgb {
        r: 0x27,
        g: 0x27,
        b: 0x27,
    };
    let untinted = rich_rs::SimpleColor::Rgb {
        r: 0x1e,
        g: 0x1e,
        b: 0x1e,
    };

    let mut saw_tinted = false;
    let mut saw_untinted = false;
    for y in 0..6 {
        for x in 0..30 {
            if let Some(bg) = buf.get(x, y).style.and_then(|s| s.bgcolor) {
                if bg == tinted {
                    saw_tinted = true;
                }
                if bg == untinted {
                    saw_untinted = true;
                }
            }
        }
    }

    assert!(
        saw_tinted,
        "expected the :focus-within container surface to be tinted to #272727\n{}",
        buf.debug_dump()
    );
    assert!(
        !saw_untinted,
        "no cell should keep the untinted #1e1e1e surface once :focus-within is active\n{}",
        buf.debug_dump()
    );
}
