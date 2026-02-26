use rich_rs::Console;
use textual::css::set_style_context;
use textual::event::{Event, EventCtx, MouseDownEvent, MouseUpEvent};
use textual::node_id::NodeId;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};
use textual::style::Color;

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
    let mut header = Header::new().title("Keys").subtitle("Diagnostics");
    let mut tree = build_widget_tree_from_root(&mut header).expect("tree should build");

    let buf = render_tree_to_frame(&mut tree, &mut header, &console, 40, 1);
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
    assert!(line.contains("│"));
    let left = line.find("Jessica").expect("left binding should exist");
    let palette = line.find("^p").expect("palette key should exist");
    assert!(palette > left);
    assert!(line.contains("palette"));
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
    let enter = line.find("enter").expect("enter key should render");
    let submit = line
        .find("submit")
        .expect("submit description should render");
    assert!(submit > enter);
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
    let _guard = set_style_context(textual::css::default_widget_stylesheet());
    let console = Console::new();
    let options = options_for(&console, 60, 1);
    let non_compact = Footer::new()
        .with_binding("ctrl+q", "quit")
        .with_binding("tab", "next");
    let footer = Footer::new()
        .with_binding("ctrl+q", "quit")
        .with_binding("tab", "next")
        .compact(true);

    let non_compact_buf = FrameBuffer::from_renderable(&console, &options, &non_compact, None);
    let buf = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let line = buf.as_plain_lines()[0].trim_end().to_string();
    let non_compact_line = non_compact_buf.as_plain_lines()[0].trim_end().to_string();
    assert!(line.contains("ctrl+q"));
    assert!(line.contains("quit"));
    assert!(line.contains("tab"));
    assert!(line.contains("next"));
    assert!(line.len() <= non_compact_line.len());
}

#[test]
fn footer_key_type_selector_styles_key_hint_cells() {
    let css = r#"
        Footer {
            bg: #101010;
            color: #ffffff;
        }
        FooterKey .footer-key--key {
            color: #ff8800;
            bg: #202020;
        }
        FooterKey .footer-key--description {
            color: #00ff88;
            bg: #303030;
        }
    "#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let console = Console::new();
    let options = options_for(&console, 32, 1);
    let footer = Footer::new().with_binding("j", "Jessica");
    let buf = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let line = &buf.as_plain_lines()[0];
    let key_x = line.find('j').expect("rendered footer key");
    let desc_x = line.find('J').expect("rendered footer description");

    let expected_key = Color::parse("#ff8800").unwrap().to_simple_opaque();
    let expected_desc = Color::parse("#00ff88").unwrap().to_simple_opaque();
    assert_eq!(
        buf.get(key_x, 0).style.and_then(|style| style.color),
        Some(expected_key)
    );
    assert_eq!(
        buf.get(desc_x, 0).style.and_then(|style| style.color),
        Some(expected_desc)
    );
}

#[test]
fn footer_key_hover_selector_styles_when_mouse_moves_over_binding() {
    let css = r#"
        Footer {
            bg: #101010;
            color: #ffffff;
        }
        FooterKey .footer-key--key {
            color: #999999;
            bg: #202020;
        }
        FooterKey:hover .footer-key--key {
            color: #ff8800;
            bg: #404040;
        }
    "#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let console = Console::new();
    let options = options_for(&console, 32, 1);
    let mut footer = Footer::new().with_binding("j", "Jessica");

    let before = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let line = &before.as_plain_lines()[0];
    let key_x = line.find('j').expect("rendered footer key");
    let normal_color = Color::parse("#999999").unwrap().to_simple_opaque();
    let hover_color = Color::parse("#ff8800").unwrap().to_simple_opaque();
    assert_eq!(
        before.get(key_x, 0).style.and_then(|style| style.color),
        Some(normal_color)
    );

    assert!(footer.on_mouse_move(key_x as u16, 0));
    let after = FrameBuffer::from_renderable(&console, &options, &footer, None);
    assert_eq!(
        after.get(key_x, 0).style.and_then(|style| style.color),
        Some(hover_color)
    );
}

#[test]
fn footer_key_hover_background_applies_across_entire_item() {
    let css = r#"
        Footer {
            bg: #101010;
            color: #ffffff;
        }
        FooterKey {
            bg: #111111;
        }
        FooterKey .footer-key--key {
            color: #aaaaaa;
            bg: transparent;
        }
        FooterKey .footer-key--description {
            color: #bbbbbb;
            bg: transparent;
        }
        FooterKey:hover {
            bg: #404040;
        }
    "#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let console = Console::new();
    let options = options_for(&console, 32, 1);
    let mut footer = Footer::new().with_binding("j", "Jessica");

    let before = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let line = &before.as_plain_lines()[0];
    let desc_x = line.find('J').expect("rendered footer description");
    let base_bg = Color::parse("#111111").unwrap().to_simple_opaque();
    let hover_bg = Color::parse("#404040").unwrap().to_simple_opaque();
    assert_eq!(
        before.get(desc_x, 0).style.and_then(|style| style.bgcolor),
        Some(base_bg)
    );

    assert!(footer.on_mouse_move(desc_x as u16, 0));
    let after = FrameBuffer::from_renderable(&console, &options, &footer, None);
    assert_eq!(
        after.get(desc_x, 0).style.and_then(|style| style.bgcolor),
        Some(hover_bg)
    );
}

#[test]
fn footer_key_hover_applies_to_command_palette_item() {
    let css = r#"
        Footer {
            bg: #101010;
            color: #ffffff;
        }
        FooterKey {
            bg: transparent;
        }
        FooterKey:hover {
            bg: #404040;
        }
    "#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let console = Console::new();
    let width = 64usize;
    let options = options_for(&console, width, 1);
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
    footer.on_layout(width as u16, 1);
    let before = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let line = &before.as_plain_lines()[0];
    let palette_x = line.find('^').expect("palette key should be visible");

    assert!(footer.on_mouse_move(palette_x as u16, 0));
    let after = FrameBuffer::from_renderable(&console, &options, &footer, None);
    let hover_bg = Color::parse("#404040").unwrap().to_simple_opaque();
    assert_eq!(
        after
            .get(palette_x, 0)
            .style
            .and_then(|style| style.bgcolor),
        Some(hover_bg)
    );
}

#[test]
fn footer_paints_full_row_background_when_bindings_change_shape() {
    let css = r#"
        Footer {
            bg: #112233;
            color: #ffffff;
        }
        Footer .footer-key--key {
            bg: #223344;
            color: #ffffff;
        }
        Footer .footer-key--description {
            bg: #334455;
            color: #ffffff;
        }
        Footer .footer-key--command-palette {
            bg: #445566;
            color: #ffffff;
        }
        Footer .footer-key--palette-separator {
            bg: #112233;
            color: #ffffff;
        }
    "#;
    let sheet = StyleSheet::parse(css);
    let _guard = set_style_context(sheet);

    let console = Console::new();
    let width = 60usize;
    let options = options_for(&console, width, 1);
    let mut footer = Footer::new();
    let mut ctx = EventCtx::default();
    footer.on_event(
        &Event::BindingsChanged(vec![
            BindingHint::new("p", "Paul"),
            BindingHint::new("ctrl+p", "palette")
                .with_key_display("^p")
                .with_group("command_palette"),
        ]),
        &mut ctx,
    );
    assert!(ctx.repaint_requested());

    let buf = FrameBuffer::from_renderable(&console, &options, &footer, None);
    for x in 0..width {
        let bg = buf.get(x, 0).style.and_then(|style| style.bgcolor);
        assert!(
            bg.is_some(),
            "footer row cell x={} lost background style; plain_line={:?}",
            x,
            buf.as_plain_lines()[0]
        );
    }
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
    let mut header = Header::new()
        .title("Textual Keys")
        .show_clock(true)
        .time_format("%H:%M:%S");
    let mut tree = build_widget_tree_from_root(&mut header).expect("tree should build");

    let buf = render_tree_to_frame(&mut tree, &mut header, &console, 80, 1);
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
