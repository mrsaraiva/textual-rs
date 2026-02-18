use rich_rs::{Console, Segment, Style as RichStyle};
use textual::prelude::*;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};

#[derive(Clone, Copy)]
struct PlainText {
    text: &'static str,
    style: RichStyle,
    preserve_underlay: bool,
}

impl PlainText {
    fn transparent(text: &'static str) -> Self {
        Self {
            text,
            style: RichStyle::new(),
            preserve_underlay: true,
        }
    }

    fn with_bg(text: &'static str, bg: Color) -> Self {
        Self {
            text,
            style: RichStyle::new().with_bgcolor(bg.to_simple_opaque()),
            preserve_underlay: false,
        }
    }
}

impl Widget for PlainText {
    fn render(&self, _console: &Console, _options: &rich_rs::ConsoleOptions) -> rich_rs::Segments {
        vec![Segment::styled(self.text, self.style)].into()
    }

    fn preserve_underlay(&self) -> bool {
        self.preserve_underlay
    }
}

struct BackgroundFill {
    child: Box<dyn Widget>,
    style: RichStyle,
    child_extracted: bool,
}

impl BackgroundFill {
    fn new(child: impl Widget + 'static, bg: Color) -> Self {
        Self {
            child: Box::new(child),
            style: RichStyle::new().with_bgcolor(bg.to_simple_opaque()),
            child_extracted: false,
        }
    }
}

impl Widget for BackgroundFill {
    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.child_extracted {
            return Vec::new();
        }
        self.child_extracted = true;
        vec![std::mem::replace(&mut self.child, Box::new(Spacer::new(1)))]
    }

    fn render(&self, _console: &Console, options: &rich_rs::ConsoleOptions) -> rich_rs::Segments {
        let width = options.size.0.max(1);
        vec![Segment::styled(" ".repeat(width), self.style)].into()
    }
}

fn render_widget(
    root: &mut dyn Widget,
    width: usize,
    height: usize,
) -> textual::render::FrameBuffer {
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(root).expect("tree should exist");
    render_tree_to_frame(&mut tree, root, &console, width, height)
}

#[test]
fn inherited_styles_apply_to_children() {
    let green = Color::parse("green").expect("parse green");
    let child = PlainText::transparent("hello");
    let mut styled = Styled::new(child, Style::new().fg(green));

    let buf = render_widget(&mut styled, 10, 1);
    let cell = buf.get(0, 0);
    let style = cell.style.expect("style should be present");
    assert_eq!(style.color, Some(green.to_simple_opaque()));
}

#[test]
fn transparent_child_composes_parent_background() {
    let blue = Color::parse("blue").expect("parse blue");
    let child = PlainText::transparent("hello");
    let mut styled = BackgroundFill::new(child, blue);

    let buf = render_widget(&mut styled, 10, 1);
    let cell = buf.get(6, 0);
    assert_eq!(
        cell.style.and_then(|s| s.bgcolor),
        Some(blue.to_simple_opaque()),
        "transparent child should preserve parent background on uncovered cells"
    );
}

#[test]
fn child_background_overrides_parent_background() {
    let green = Color::parse("green").expect("parse green");
    let blue = Color::parse("blue").expect("parse blue");

    let child = PlainText::with_bg("hello", green);
    let mut styled = Styled::new(child, Style::new().bg(blue));

    let buf = render_widget(&mut styled, 10, 1);
    let cell = buf.get(0, 0);
    assert_eq!(
        cell.style.and_then(|s| s.bgcolor),
        Some(green.to_simple_opaque()),
        "child explicit background should win over parent background"
    );
}
