use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn scroll_view_renders_offset_viewport() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (14, 3);
    options.max_width = 14;
    options.max_height = 3;

    let list = ListView::new(vec![
        "item 1".to_string(),
        "item 2".to_string(),
        "item 3".to_string(),
        "item 4".to_string(),
    ]);
    let mut scroll = ScrollView::new(list).height(3);
    scroll.scroll_to(1);

    let buf = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    insta::assert_snapshot!(buf.debug_dump());
}

#[derive(Debug, Clone)]
struct NoIntrinsicHeightWidget {
    id: WidgetId,
}

impl NoIntrinsicHeightWidget {
    fn new() -> Self {
        Self {
            id: WidgetId::new(),
        }
    }
}

impl Widget for NoIntrinsicHeightWidget {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(
        &self,
        _console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        let mut out = rich_rs::Segments::new();
        let height = options.size.1.max(1);
        for index in 0..height {
            out.push(rich_rs::Segment::new(format!("line {:02}", index)));
            if index + 1 < height {
                out.push(rich_rs::Segment::line());
            }
        }
        out
    }
}

#[test]
fn scroll_view_scrolls_children_without_intrinsic_height() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 3);
    options.max_width = 12;
    options.max_height = 3;

    let mut scroll = ScrollView::new(NoIntrinsicHeightWidget::new()).height(3);

    let before = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    let before_lines = before.as_plain_lines();
    assert!(before_lines[0].starts_with("line 00"));

    scroll.scroll_by(1);
    let after = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    let after_lines = after.as_plain_lines();
    assert!(after_lines[0].starts_with("line 01"));
}

#[derive(Debug, Clone)]
struct StretchWidget {
    id: WidgetId,
}

impl StretchWidget {
    fn new() -> Self {
        Self {
            id: WidgetId::new(),
        }
    }
}

impl Widget for StretchWidget {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(
        &self,
        _console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        let mut out = rich_rs::Segments::new();
        let height = options.size.1.max(1);
        for index in 0..height {
            out.push(rich_rs::Segment::new(format!("stretch {:02}", index)));
            if index + 1 < height {
                out.push(rich_rs::Segment::line());
            }
        }
        out
    }
}

#[test]
fn scroll_view_caps_offset_for_stretch_children() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (14, 3);
    options.max_width = 14;
    options.max_height = 3;

    let mut scroll = ScrollView::new(StretchWidget::new()).height(3);
    let _ = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    scroll.scroll_by(1000);

    let buf = FrameBuffer::from_renderable(&console, &options, &scroll, None);
    let lines = buf.as_plain_lines();
    assert!(
        lines[0].starts_with("stretch 03"),
        "expected capped offset line, got {:?}",
        lines[0]
    );
}
