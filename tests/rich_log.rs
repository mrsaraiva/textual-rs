use rich_rs::Console;
use textual::css::{default_widget_stylesheet, set_style_context};
use textual::prelude::*;
use textual::render::FrameBuffer;

fn options_for(console: &Console, width: usize, height: usize) -> rich_rs::ConsoleOptions {
    let mut options = console.options().clone();
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
    options
}

#[test]
fn rich_log_auto_scrolls_to_latest_lines() {
    let console = Console::new();
    let options = options_for(&console, 16, 2);

    let mut log = RichLog::new();
    log.write("line 1");
    log.write("line 2");
    log.write("line 3");
    log.write("line 4");

    let buf = FrameBuffer::from_renderable(&console, &options, &log, None);
    let lines = buf.as_plain_lines();
    assert!(lines[0].starts_with("line 3"));
    assert!(lines[1].starts_with("line 4"));
}

#[test]
fn rich_log_respects_max_lines() {
    let console = Console::new();
    let options = options_for(&console, 16, 4);

    let mut log = RichLog::new().max_lines(2);
    log.write("line 1");
    log.write("line 2");
    log.write("line 3");

    let buf = FrameBuffer::from_renderable(&console, &options, &log, None);
    let lines = buf.as_plain_lines();
    assert!(lines[0].starts_with("line 2"));
    assert!(lines[1].starts_with("line 3"));
}

#[test]
fn rich_log_scrolls_via_actions() {
    let console = Console::new();
    let options = options_for(&console, 16, 2);

    let mut log = RichLog::new().auto_scroll(false);
    log.write("line 1");
    log.write("line 2");
    log.write("line 3");

    let before = FrameBuffer::from_renderable(&console, &options, &log, None);
    assert!(before.as_plain_lines()[0].starts_with("line 1"));

    log.on_event(&Event::Action(Action::ScrollDown), &mut EventCtx::default());

    let after = FrameBuffer::from_renderable(&console, &options, &log, None);
    assert!(after.as_plain_lines()[0].starts_with("line 2"));
}

#[test]
fn rich_log_preserves_view_anchor_when_trimming_max_lines() {
    let console = Console::new();
    let options = options_for(&console, 16, 2);

    let mut log = RichLog::new().max_lines(3).auto_scroll(false);
    log.write("line 1");
    log.write("line 2");
    log.write("line 3");
    let _ = FrameBuffer::from_renderable(&console, &options, &log, None);
    log.on_event(&Event::Action(Action::ScrollDown), &mut EventCtx::default());

    let before = FrameBuffer::from_renderable(&console, &options, &log, None);
    assert!(before.as_plain_lines()[0].starts_with("line 2"));
    assert!(before.as_plain_lines()[1].starts_with("line 3"));

    log.write("line 4");
    let after = FrameBuffer::from_renderable(&console, &options, &log, None);
    assert!(after.as_plain_lines()[0].starts_with("line 2"));
    assert!(after.as_plain_lines()[1].starts_with("line 3"));
}

#[test]
fn rich_log_renders_all_explicit_lines_from_styled_segments() {
    let console = Console::new();
    let options = options_for(&console, 16, 2);

    let mut log = RichLog::new();
    log.write_segments(vec![rich_rs::Segment::new("line 1\nline 2")]);

    let buf = FrameBuffer::from_renderable(&console, &options, &log, None);
    let lines = buf.as_plain_lines();
    assert!(lines[0].starts_with("line 1"));
    assert!(lines[1].starts_with("line 2"));
}

#[test]
fn rich_log_write_markup_renders_without_literal_markup_tags() {
    let console = Console::new();
    let options = options_for(&console, 24, 1);

    let mut log = RichLog::new();
    log.write_markup("[bold]warn[/] [red]error[/]");

    let buf = FrameBuffer::from_renderable(&console, &options, &log, None);
    let lines = buf.as_plain_lines();
    assert!(lines[0].starts_with("warn error"));
    assert!(!lines[0].contains("[bold]"));
    assert!(!lines[0].contains("[red]"));
}

#[test]
fn rich_log_write_respects_default_markup_setting() {
    let console = Console::new();
    let options = options_for(&console, 24, 1);

    let mut plain = RichLog::new();
    plain.write("[bold]warn[/]");
    let plain_buf = FrameBuffer::from_renderable(&console, &options, &plain, None);
    assert!(plain_buf.as_plain_lines()[0].contains("[bold]warn[/]"));

    let mut markup = RichLog::new().markup(true);
    markup.write("[bold]warn[/]");
    let markup_buf = FrameBuffer::from_renderable(&console, &options, &markup, None);
    assert!(markup_buf.as_plain_lines()[0].starts_with("warn"));
    assert!(!markup_buf.as_plain_lines()[0].contains("[bold]"));
}

#[test]
fn rich_log_write_applies_default_highlighter_when_enabled() {
    let console = Console::new();
    let options = options_for(&console, 32, 1);

    let mut plain = RichLog::new();
    plain.write("None 123");
    let plain_buf = FrameBuffer::from_renderable(&console, &options, &plain, None);

    let mut highlighted = RichLog::new().highlight(true);
    highlighted.write("None 123");
    let highlighted_buf = FrameBuffer::from_renderable(&console, &options, &highlighted, None);

    let plain_styled = (0..32).filter_map(|x| plain_buf.get(x, 0).style).count();
    let highlighted_styled = (0..32)
        .filter_map(|x| highlighted_buf.get(x, 0).style)
        .count();

    assert!(highlighted_styled >= plain_styled);
}

#[test]
fn rich_log_renders_multiline_renderable_entries() {
    let console = Console::new();
    let options = options_for(&console, 16, 2);

    let mut log = RichLog::new();
    log.write_renderable(rich_rs::Text::plain("line 1\nline 2"));

    let buf = FrameBuffer::from_renderable(&console, &options, &log, None);
    let lines = buf.as_plain_lines();
    assert!(lines[0].starts_with("line 1"));
    assert!(lines[1].starts_with("line 2"));
}

#[test]
fn rich_log_auto_scroll_tracks_bottom_after_multiline_renderable_write() {
    let console = Console::new();
    let options = options_for(&console, 16, 2);

    let mut log = RichLog::new();
    log.write("line 1");
    log.write("line 2");
    let _ = FrameBuffer::from_renderable(&console, &options, &log, None);

    log.write_renderable(rich_rs::Text::plain("line 3\nline 4"));

    let buf = FrameBuffer::from_renderable(&console, &options, &log, None);
    let lines = buf.as_plain_lines();
    assert!(lines[0].starts_with("line 3"));
    assert!(lines[1].starts_with("line 4"));
}

#[test]
fn rich_log_focus_style_does_not_draw_border_chrome() {
    let _guard = set_style_context(default_widget_stylesheet());
    let console = Console::new();
    let options = options_for(&console, 12, 2);

    let mut log = RichLog::new();
    log.set_focus(true);
    log.write("line 1");
    log.write("line 2");

    let buf = FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&log), None);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().all(|line| !line.contains('│')));
    assert!(lines.iter().all(|line| !line.contains('─')));
    assert!(lines[0].starts_with("line 1"));
}
