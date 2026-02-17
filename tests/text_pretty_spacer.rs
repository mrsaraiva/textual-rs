use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn label_layout_height_tracks_wrap_width() {
    let mut label = Label::new("123456789");
    label.on_layout(5, 1);
    assert_eq!(label.layout_height(), Some(2));
}

#[test]
fn pretty_switches_to_multiline_when_narrow() {
    let values = vec!["alpha", "beta", "gamma"];
    let mut pretty = Pretty::new(&values);
    pretty.on_layout(10, 6);

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (10, 6);
    options.max_width = 10;
    options.max_height = 6;

    let buffer = FrameBuffer::from_renderable(&console, &options, &pretty, None);
    let lines = buffer.as_plain_lines();
    // With rich_rs::Pretty, the output should expand to multiple lines when narrow
    assert!(
        lines.len() > 1,
        "Expected multi-line output, got: {:?}",
        lines
    );
}

#[test]
fn spacer_reports_intrinsic_width_hint() {
    let spacer = Spacer::new(2).width(8);
    assert_eq!(spacer.content_width(), Some(8));
    assert_eq!(spacer.layout_height(), Some(2));
}

#[test]
fn markdown_layout_height_tracks_wrap_width() {
    let mut markdown = Markdown::new("abcdefghij");
    markdown.on_layout(4, 1);
    assert_eq!(markdown.layout_height(), Some(3));
}

#[test]
fn markdown_has_no_intrinsic_width_hint() {
    let markdown = Markdown::new("# Lady Jessica");
    assert_eq!(markdown.content_width(), None);
}
