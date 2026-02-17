use rich_rs::Console;
use textual::css::{default_widget_stylesheet, set_style_context};
use textual::prelude::*;
use textual::render::FrameBuffer;

fn render_buffer_with_size(palette: &CommandPalette, width: usize, height: usize) -> FrameBuffer {
    let _guard = set_style_context(default_widget_stylesheet());
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
    FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(palette), None)
}

fn render_buffer(palette: &CommandPalette) -> FrameBuffer {
    render_buffer_with_size(palette, 72, 14)
}

#[test]
fn command_palette_closed_snapshot() {
    let palette = CommandPalette::new(Label::new("Body content"));
    let buf = render_buffer(&palette);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn command_palette_open_snapshot() {
    let mut palette = CommandPalette::new(Label::new("Body content"));
    let mut ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
    assert!(ctx.handled());

    let buf = render_buffer(&palette);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn command_palette_help_rows_keep_panel_surface_and_dim_style() {
    let mut palette = CommandPalette::new(Label::new("Body content"));
    let mut ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
    assert!(ctx.handled());

    let buf = render_buffer(&palette);

    let lines = buf.as_plain_lines();
    let (help_y, help_x) = lines
        .iter()
        .enumerate()
        .find_map(|(y, line)| {
            line.find("Show help for the focused widget")
                .map(|x| (y, x))
        })
        .expect("help row should be present");
    let help_text = buf.get(help_x, help_y);
    let help_pad = buf.get(buf.width.saturating_sub(2), help_y);

    let text_style = help_text
        .style
        .as_ref()
        .expect("help text cell should carry style");
    let pad_style = help_pad
        .style
        .as_ref()
        .expect("help row padding cell should carry panel style");

    assert_eq!(
        text_style.bgcolor, pad_style.bgcolor,
        "help text and trailing padding should share panel surface background"
    );
    assert_eq!(
        text_style.dim,
        Some(true),
        "help description should use dim text style"
    );
}

#[test]
fn command_palette_placeholder_uses_dim_style() {
    let mut palette = CommandPalette::new(Label::new("Body content"));
    let mut ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
    assert!(ctx.handled());

    let buf = render_buffer(&palette);
    let lines = buf.as_plain_lines();
    let (search_y, search_x) = lines
        .iter()
        .enumerate()
        .find_map(|(y, line)| line.find("Search for commands").map(|x| (y, x)))
        .expect("search placeholder row should be present");
    let placeholder_cell = buf.get(search_x.saturating_add(3), search_y);
    let style = placeholder_cell
        .style
        .as_ref()
        .expect("placeholder cell should have style");
    assert_eq!(
        style.dim,
        Some(true),
        "placeholder text should be rendered dim"
    );
}

#[test]
fn command_palette_search_row_uses_panel_surface_background() {
    let mut palette = CommandPalette::new(Label::new("Body content"));
    palette.set_commands(vec![
        PaletteCommand::new("alpha", "Alpha", "First command"),
        PaletteCommand::new("beta", "Beta", "Second command"),
    ]);
    let mut ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
    assert!(ctx.handled());

    let buf = render_buffer(&palette);
    let lines = buf.as_plain_lines();
    let (search_y, search_x) = lines
        .iter()
        .enumerate()
        .find_map(|(y, line)| line.find("Search for commands").map(|x| (y, x)))
        .expect("search placeholder row should be present");

    let search_style = buf
        .get(search_x.saturating_add(3), search_y)
        .style
        .as_ref()
        .expect("search placeholder should have style");
    let list_bg_style = buf
        .as_plain_lines()
        .iter()
        .enumerate()
        .find_map(|(y, line)| line.find("Beta").map(|x| (y, x)))
        .and_then(|(y, x)| buf.get(x, y).style.as_ref().cloned())
        .expect("list row should have style");

    assert_eq!(
        search_style.bgcolor, list_bg_style.bgcolor,
        "search input row should share the panel/list surface background"
    );
}

#[test]
fn command_palette_unselected_rows_use_panel_surface_background() {
    let mut palette = CommandPalette::new(Label::new("Body content"));
    palette.set_commands(vec![
        PaletteCommand::new("alpha", "Alpha", "First command"),
        PaletteCommand::new("beta", "Beta", "Second command"),
    ]);
    let mut ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
    assert!(ctx.handled());

    let buf = render_buffer(&palette);
    let lines = buf.as_plain_lines();
    let (title_y, title_x) = lines
        .iter()
        .enumerate()
        .find_map(|(y, line)| line.find("Beta").map(|x| (y, x)))
        .expect("title row should be present");
    let (help_y, help_x) = lines
        .iter()
        .enumerate()
        .find_map(|(y, line)| line.find("Second command").map(|x| (y, x)))
        .expect("help row should be present");

    let title_style = buf
        .get(title_x, title_y)
        .style
        .as_ref()
        .expect("title row should have style");
    let help_style = buf
        .get(help_x, help_y)
        .style
        .as_ref()
        .expect("help row should have style");
    assert!(
        title_style.bgcolor.is_some(),
        "title row should carry panel background"
    );
    assert!(
        help_style.bgcolor.is_some(),
        "help row should carry panel background"
    );

    let app_bg = buf.get(10, 0).style.as_ref().and_then(|s| s.bgcolor);
    let title_bg = title_style.bgcolor;
    assert_ne!(
        title_bg, app_bg,
        "palette rows should not reuse the app background color"
    );
}

#[test]
fn command_palette_renders_markup_commands_without_literal_tags() {
    let mut palette = CommandPalette::new(Label::new("Body content"));
    palette.set_commands(vec![PaletteCommand::new(
        "markup",
        "[bold]Deploy[/]",
        "[green]Ship[/] current build",
    )]);
    let mut ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
    assert!(ctx.handled());

    let buf = render_buffer(&palette);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().any(|line| line.contains("Deploy")));
    assert!(lines.iter().any(|line| line.contains("Ship current build")));
    assert!(!lines.iter().any(|line| line.contains("[bold]")));
    assert!(!lines.iter().any(|line| line.contains("[green]")));
}

#[test]
fn command_palette_open_render_handles_small_viewport() {
    let mut palette = CommandPalette::new(Label::new("Body content"));
    palette.on_layout(40, 4);
    let mut ctx = EventCtx::default();
    palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
    assert!(ctx.handled());

    let buf = render_buffer_with_size(&palette, 40, 4);
    let lines = buf.as_plain_lines();
    assert_eq!(lines.len(), 4);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Search for commands"))
    );
}
