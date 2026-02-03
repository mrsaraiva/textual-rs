use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::widget::set_style_context;

#[test]
fn descendant_selectors_match() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 1);
    options.max_width = 8;
    options.max_height = 1;

    let css = ".panel Label { underline: true; }";
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let row = Node::new(Row::new().with_child(Label::new("hi"))).class("panel");
    let renderable = WidgetRenderable::new(&row);
    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.underline, Some(true));
}

#[test]
fn selector_groups_apply_to_multiple() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (6, 1);
    options.max_width = 6;
    options.max_height = 1;

    let css = "Label, .note { bold: true; }";
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let label = Node::new(Label::new("hi")).class("note");
    let renderable = WidgetRenderable::new(&label);
    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.bold, Some(true));
}
