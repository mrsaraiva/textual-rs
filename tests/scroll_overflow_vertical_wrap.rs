//! Regression test for the box-model VERTICAL-WRAP fix (c15-scrollroots, root #3).
//!
//! On a host that allows horizontal overflow (`Screen { overflow: auto auto }`), an
//! EXPLICIT `width: auto` child (a `Label`, whose DEFAULT_CSS sets `width: auto`)
//! must be laid out at its FULL unwrapped content width. If it instead wraps to the
//! narrow viewport, its `height: auto` counts the extra wrapped rows and inflates
//! the host's virtual content height — which shifts the scrollbar thumb and paints a
//! spurious thumb fragment in the body. Python keeps the long line unwrapped and
//! clips / h-scrolls it (`_resolve.resolve_box_models` measures content width
//! unconstrained; the compositor never re-wraps the child).
//!
//! Mirrors `docs/examples/styles/scrollbar_corner_color.py`: a Label whose first
//! line is far wider than the viewport, followed by many short rows. The fix is
//! asserted by the rendered scrollbar geometry — without it, the inflated virtual
//! height paints a vertical-thumb fragment several rows up from where it belongs.

use rich_rs::Console;
use textual::css::{StyleSheet, default_widget_stylesheet};
use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame_with_stylesheet};

const THUMB_GLYPHS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█', '▎', '▌', '▊'];

#[test]
fn long_line_auto_width_label_does_not_inflate_scroll_geometry() {
    let console = Console::new();
    let mut sheet = default_widget_stylesheet();
    sheet.extend(&StyleSheet::parse("Screen { overflow: auto auto; }"));

    // Long first line (200 cells, far wider than the 40-col viewport) followed by
    // many short lines that overflow the viewport vertically. The long line must NOT
    // wrap; its auto-height must remain 1 row.
    let mut text = "L".repeat(200);
    for _ in 0..40 {
        text.push_str("\nshort");
    }

    let mut root = AppRoot::new().with_child(Label::new(text));

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree builds");
    let buf = render_tree_to_frame_with_stylesheet(&mut tree, &mut root, &console, 40, 12, sheet);
    let rows = buf.as_plain_lines();

    // The horizontal scrollbar occupies the LAST row only. No interior body row
    // (rows 1..=10, the "short" lines) may contain a scrollbar thumb block glyph:
    // a thumb fragment there is the signature of the inflated-virtual-height bug
    // (the vertical thumb is mis-sized/mis-placed because the long line wrapped).
    for (i, row) in rows.iter().enumerate().take(rows.len().saturating_sub(1)).skip(1) {
        let stray_thumb = row.chars().any(|c| THUMB_GLYPHS.contains(&c));
        assert!(
            !stray_thumb,
            "interior body row {i} contains a stray scrollbar thumb glyph — the \
             long auto-width line wrapped to the narrow viewport and inflated the \
             host virtual height (root #3 regression). row={row:?}"
        );
    }
}
