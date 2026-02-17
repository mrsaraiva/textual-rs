use rich_rs::Console;
use textual::debug::DebugLayout;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame_with_debug};

fn render_tree_debug(
    root: &mut dyn Widget,
    width: usize,
    height: usize,
    debug: &DebugLayout,
) -> FrameBuffer {
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(root).expect("tree should build");
    render_tree_to_frame_with_debug(&mut tree, root, &console, width, height, Some(debug))
}

#[test]
fn debug_layout_draws_borders_and_sizes() {
    let mut container = Container::new()
        .with_child(Label::new("alpha"))
        .with_child(Label::new("beta"));

    let debug = DebugLayout::enabled();
    let buf = render_tree_debug(&mut container, 12, 7, &debug);
    let plain = buf.as_plain_lines().join("\n");
    assert!(plain.contains("alpha"), "expected alpha in tree render");
    assert!(plain.contains("beta"), "expected beta in tree render");
}
