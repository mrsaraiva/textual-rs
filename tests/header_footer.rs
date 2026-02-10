use rich_rs::Console;
use textual::event::{Event, EventCtx, MouseUpEvent};
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
fn header_renders_title_and_subtitle() {
    let console = Console::new();
    let options = options_for(&console, 40, 1);
    let header = Header::new().title("Keys").subtitle("Diagnostics");

    let buf = FrameBuffer::from_renderable(&console, &options, &header, None);
    let line = &buf.as_plain_lines()[0];
    assert!(line.contains("Keys"));
    assert!(line.contains("Diagnostics"));
}

#[test]
fn footer_renders_bindings() {
    let console = Console::new();
    let options = options_for(&console, 60, 1);
    let footer = Footer::new()
        .with_binding("ctrl+q", "quit")
        .with_binding("tab", "next");

    let buf = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let line = &buf.as_plain_lines()[0];
    assert!(line.contains("ctrl+q"));
    assert!(line.contains("quit"));
    assert!(line.contains("tab"));
    assert!(line.contains("next"));
}

#[test]
fn footer_updates_from_bindings_changed_event() {
    let console = Console::new();
    let options = options_for(&console, 60, 1);
    let mut footer = Footer::new();
    let mut ctx = EventCtx::default();
    footer.on_event(
        &Event::BindingsChanged(vec![
            BindingHint::new("tab", "next").hidden(true),
            BindingHint::new("j", "Jessica"),
            BindingHint::new("ctrl+p", "palette").with_key_display("^p"),
        ]),
        &mut ctx,
    );
    assert!(ctx.repaint_requested());

    let buf = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let line = &buf.as_plain_lines()[0];
    assert!(!line.contains("next"));
    assert!(line.contains("Jessica"));
    assert!(line.contains("^p"));
    assert!(line.contains("palette"));
}

#[test]
fn footer_docks_command_palette_binding_to_right_slot() {
    let console = Console::new();
    let options = options_for(&console, 48, 1);
    let mut footer = Footer::new();
    let mut ctx = EventCtx::default();
    footer.on_event(
        &Event::BindingsChanged(vec![
            BindingHint::new("j", "Jessica"),
            BindingHint::new("ctrl+p", "palette")
                .with_key_display("^p")
                .with_group("command_palette"),
        ]),
        &mut ctx,
    );
    assert!(ctx.repaint_requested());

    let buf = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let line = &buf.as_plain_lines()[0];
    assert!(line.contains("Jessica"));
    assert!(line.trim_end().ends_with("^p palette"));
}

#[test]
fn footer_groups_consecutive_bindings_with_same_group() {
    let console = Console::new();
    let options = options_for(&console, 80, 1);
    let mut footer = Footer::new();
    let mut ctx = EventCtx::default();
    footer.on_event(
        &Event::BindingsChanged(vec![
            BindingHint::new("left", "move left").with_group("Move"),
            BindingHint::new("right", "move right").with_group("Move"),
            BindingHint::new("enter", "submit"),
        ]),
        &mut ctx,
    );
    assert!(ctx.repaint_requested());

    let buf = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let line = &buf.as_plain_lines()[0];
    assert!(line.contains("left"));
    assert!(line.contains("right"));
    assert!(line.contains("Move"));
    assert!(!line.contains("move left"));
    assert!(!line.contains("move right"));
    assert!(line.contains("enter submit"));
}

#[test]
fn footer_compact_mode_tightens_spacing() {
    let console = Console::new();
    let options = options_for(&console, 60, 1);
    let footer = Footer::new()
        .with_binding("ctrl+q", "quit")
        .with_binding("tab", "next")
        .compact(true);

    let buf = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let line = &buf.as_plain_lines()[0];
    assert!(line.starts_with("ctrl+q quit tab next"));
}

#[test]
fn header_mouse_up_toggles_tall_outside_icon() {
    let mut header = Header::new().title("Textual Keys");
    let mut ctx = EventCtx::default();
    header.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(header.id()),
            screen_x: 20,
            screen_y: 0,
            x: 20,
            y: 0,
        }),
        &mut ctx,
    );

    assert!(ctx.handled());
    assert_eq!(header.layout_height(), Some(3));
    assert!(header.style_classes().iter().any(|class| class == "-tall"));
}

#[test]
fn header_icon_click_does_not_toggle_tall() {
    let mut header = Header::new().title("Textual Keys");
    let mut ctx = EventCtx::default();
    header.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(header.id()),
            screen_x: 1,
            screen_y: 0,
            x: 1,
            y: 0,
        }),
        &mut ctx,
    );

    assert!(ctx.handled());
    assert_eq!(header.layout_height(), Some(1));
    assert!(!header.style_classes().iter().any(|class| class == "-tall"));
}

#[test]
fn header_can_render_clock() {
    let console = Console::new();
    let options = options_for(&console, 80, 1);
    let header = Header::new()
        .title("Textual Keys")
        .show_clock(true)
        .time_format("%H:%M:%S");

    let buf = FrameBuffer::from_renderable(&console, &options, &header, None);
    let line = &buf.as_plain_lines()[0];
    assert!(line.contains(":"));
}
