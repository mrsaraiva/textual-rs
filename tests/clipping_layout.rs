use rich_rs::Console;
use textual::prelude::*;

#[test]
fn container_clips_to_viewport_height() {
    let console = Console::new();

    let mut root = Container::new();
    root.push(Label::new("line1"));
    root.push(Label::new("line2"));
    root.push(Label::new("line3"));
    root.push(Label::new("line4"));

    let mut tree = build_widget_tree_from_root(&mut root).expect("tree should build");
    let buf = render_tree_to_frame(&mut tree, &mut root, &console, 10, 3);
    insta::assert_snapshot!(buf.debug_dump());
}
