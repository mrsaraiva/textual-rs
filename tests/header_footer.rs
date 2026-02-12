use rich_rs::Console;
use textual::event::{Event, EventCtx, MouseDownEvent, MouseUpEvent};
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
fn footer_applies_deferred_bindings_on_focus_gain() {
    let console = Console::new();
    let options = options_for(&console, 60, 1);
    let mut footer = Footer::new();

    let mut unfocus_ctx = EventCtx::default();
    footer.on_event(&Event::AppFocus(false), &mut unfocus_ctx);

    let mut bindings_ctx = EventCtx::default();
    footer.on_event(
        &Event::BindingsChanged(vec![
            BindingHint::new("ctrl+p", "palette").with_key_display("^p"),
        ]),
        &mut bindings_ctx,
    );
    assert!(!bindings_ctx.repaint_requested());

    let before_focus = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let before_focus_line = &before_focus.as_plain_lines()[0];
    assert!(!before_focus_line.contains("^p"));
    assert!(!before_focus_line.contains("palette"));

    let mut focus_ctx = EventCtx::default();
    footer.on_event(&Event::AppFocus(true), &mut focus_ctx);
    assert!(focus_ctx.repaint_requested());

    let after_focus = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let after_focus_line = &after_focus.as_plain_lines()[0];
    assert!(after_focus_line.contains("^p"));
    assert!(after_focus_line.contains("palette"));
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
    let mut down_ctx = EventCtx::default();
    header.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 20,
            screen_y: 0,
            x: 20,
            y: 0,
        }),
        &mut down_ctx,
    );
    assert!(down_ctx.handled());

    let mut ctx = EventCtx::default();
    header.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(NodeId::default()),
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
    let mut down_ctx = EventCtx::default();
    header.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 1,
            screen_y: 0,
            x: 1,
            y: 0,
        }),
        &mut down_ctx,
    );
    assert!(down_ctx.handled());

    let mut ctx = EventCtx::default();
    header.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(NodeId::default()),
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

#[test]
fn header_cross_region_press_release_is_noop() {
    let mut header = Header::new().title("Textual Keys");
    let id = NodeId::default();
    let mut down_ctx = EventCtx::default();
    header.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: id,
            screen_x: 1,
            screen_y: 0,
            x: 1,
            y: 0,
        }),
        &mut down_ctx,
    );
    assert!(down_ctx.handled());

    let mut up_ctx = EventCtx::default();
    header.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(id),
            screen_x: 20,
            screen_y: 0,
            x: 20,
            y: 0,
        }),
        &mut up_ctx,
    );
    assert!(up_ctx.handled());
    assert_eq!(header.layout_height(), Some(1));
}
