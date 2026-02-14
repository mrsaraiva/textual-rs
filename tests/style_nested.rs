use rich_rs::Console;
use textual::css::set_style_context;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn nested_descendant_selector_applies() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 1);
    options.max_width = 8;
    options.max_height = 1;

    let css = r#"
    Row {
        Label { underline: true; }
    }
    "#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let row = Row::new().with_child(Label::new("hi"));
    let renderable = WidgetRenderable::new(&row);
    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.underline, Some(true));
}

#[test]
fn nested_amp_class_selector_applies() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (6, 1);
    options.max_width = 6;
    options.max_height = 1;

    let css = r#"
    Label {
        &.notice { bold: true; }
    }
    "#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let label = Node::new(Label::new("hi")).class("notice");
    let renderable = WidgetRenderable::new(&label);
    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.bold, Some(true));
}

#[test]
fn nested_parent_and_child_rules_both_apply() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (6, 1);
    options.max_width = 6;
    options.max_height = 1;

    let css = r#"
    Label {
        color: red;
        &.notice { background: blue; }
    }
    "#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let label = Node::new(Label::new("hi")).class("notice");
    let renderable = WidgetRenderable::new(&label);
    let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);

    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(
        style.color,
        Some(Color::parse("red").expect("parse red").to_simple_opaque())
    );
    assert_eq!(
        style.bgcolor,
        Some(Color::parse("blue").expect("parse blue").to_simple_opaque())
    );
}
