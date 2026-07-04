//! Integration tests for the `Welcome` widget.
//!
//! `Welcome` is a compose-based widget: its Button and Markdown container live
//! in the arena tree, not as inline-rendered private fields.  Tests that require
//! visual output use `render_tree_to_frame` to drive the tree render path.
//!
//! Unit tests for compose structure / message routing live in the widget source
//! (`src/widgets/welcome.rs`) because they need access to `pub(crate)` items.

use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn plain_lines(frame: &textual::render::FrameBuffer, w: usize, h: usize) -> Vec<String> {
    (0..h)
        .map(|y| (0..w).map(|x| frame.get(x, y).text.clone()).collect())
        .collect()
}

// ── Rendering ─────────────────────────────────────────────────────────────────

#[test]
fn welcome_renders_title_and_close_button() {
    // Build the arena tree via the full compose pipeline so Welcome's children
    // (Markdown container + Button) are properly mounted and laid out.
    let mut root_widget = AppRoot::new().with_child(Welcome::new());
    let mut tree = build_widget_tree_from_root(&mut root_widget)
        .expect("Welcome must compose a non-empty tree");
    let console = rich_rs::Console::new();
    let frame = render_tree_to_frame(&mut tree, &mut root_widget, &console, 72, 16);
    let lines = plain_lines(&frame, 72, 16);
    assert!(
        lines.iter().any(|l| l.contains("Welcome")),
        "should contain 'Welcome' heading; lines:\n{}",
        lines.join("\n")
    );
    assert!(
        lines.iter().any(|l| l.contains("OK")),
        "should contain 'OK' button; lines:\n{}",
        lines.join("\n")
    );
}

// ── Lifecycle ─────────────────────────────────────────────────────────────────

#[test]
fn welcome_unmount_does_not_panic() {
    let mut welcome = Welcome::new();
    // `on_unmount`/`compose` live on the capability traits (Interactive/Render)
    // AND on Widget (generated); with the prelude bringing both into scope, a
    // bare call is E0034-ambiguous — disambiguate to the capability trait.
    welcome.on_unmount();
}

#[test]
fn welcome_compose_yields_two_top_level_children() {
    // Cross-check accessible from integration tests: compose() returns non-empty.
    // The concrete id values are tested in the unit tests (src/widgets/welcome.rs).
    let mut welcome = Welcome::new();
    let children = welcome.compose();
    assert_eq!(
        children.len(),
        2,
        "Welcome must compose exactly two top-level children"
    );
}
