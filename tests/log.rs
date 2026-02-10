use rich_rs::Console;
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
fn log_write_auto_scrolls_to_latest_lines() {
    let console = Console::new();
    let options = options_for(&console, 16, 2);

    let mut log = Log::new();
    log.write("line 1\n");
    log.write("line 2\n");
    log.write("line 3\n");

    let buf = FrameBuffer::from_renderable(&console, &options, &log, None);
    let lines = buf.as_plain_lines();
    assert!(lines[0].starts_with("line 2"));
    assert!(lines[1].starts_with("line 3"));
}

#[test]
fn log_respects_max_lines() {
    let console = Console::new();
    let options = options_for(&console, 16, 2);

    let mut log = Log::new().max_lines(2);
    log.write_line("line 1");
    log.write_line("line 2");
    log.write_line("line 3");

    let buf = FrameBuffer::from_renderable(&console, &options, &log, None);
    let lines = buf.as_plain_lines();
    assert!(lines[0].starts_with("line 2"));
    assert!(lines[1].starts_with("line 3"));
}

#[test]
fn log_write_appends_until_newline() {
    let mut log = Log::new();
    log.write("hel");
    log.write("lo");
    assert_eq!(log.line_count(), 1);
    assert_eq!(log.lines(), ["hello"]);

    log.write("\nnext");
    assert_eq!(log.line_count(), 2);
    assert_eq!(log.lines(), ["hello", "next"]);
}

#[test]
fn log_scrolls_via_actions() {
    let console = Console::new();
    let options = options_for(&console, 16, 2);

    let mut log = Log::new().auto_scroll(false);
    log.write_line("line 1");
    log.write_line("line 2");
    log.write_line("line 3");

    let before = FrameBuffer::from_renderable(&console, &options, &log, None);
    assert!(before.as_plain_lines()[0].starts_with("line 1"));

    let mut ctx = EventCtx::default();
    log.on_event(&Event::Action(Action::ScrollDown), &mut ctx);
    assert!(ctx.handled());

    let after = FrameBuffer::from_renderable(&console, &options, &log, None);
    assert!(after.as_plain_lines()[0].starts_with("line 2"));
}
