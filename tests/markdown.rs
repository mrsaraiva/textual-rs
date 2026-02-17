use rich_rs::Console;
use textual::css::{default_widget_stylesheet, set_style_context};
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn markdown_renders_basic_blocks() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (26, 6);
    options.max_width = 26;
    options.max_height = 6;

    let markdown = Markdown::new("# Title\n\n- Alpha\n- Beta\n\n`code`");

    let buf = FrameBuffer::from_renderable(&console, &options, &markdown, None);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn markdown_h1_uses_default_component_style() {
    let _guard = set_style_context(default_widget_stylesheet());

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 3);
    options.max_width = 24;
    options.max_height = 3;

    let mut markdown = Markdown::new("# Heading");
    markdown.on_layout(24, 3);

    let buf =
        FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&markdown), None);

    let mut heading_pos = None;
    for y in 0..buf.height {
        for x in 0..buf.width {
            let cell = buf.get(x, y);
            if cell.text == "H" {
                heading_pos = Some((x, y, cell.clone()));
                break;
            }
        }
        if heading_pos.is_some() {
            break;
        }
    }
    let (_, heading_row, heading_cell) = heading_pos.expect("expected heading glyph");
    assert_eq!(
        heading_row, 2,
        "MarkdownHeader top margin should offset H1 by 2 rows"
    );
    let style = heading_cell.style.expect("expected heading style");
    assert!(style.color.is_some());
    assert_eq!(style.bold, Some(true));
    assert_ne!(
        style.underline,
        Some(true),
        "h1 text-style token should not force underline"
    );
}

#[test]
fn markdown_heading_style_matches_emoji_heading_text() {
    let _guard = set_style_context(default_widget_stylesheet());

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (28, 3);
    options.max_width = 28;
    options.max_height = 3;

    let mut markdown = Markdown::new("# 👩‍🚀 Launch");
    markdown.on_layout(28, 3);
    let buf =
        FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&markdown), None);

    let mut styled_cell = None;
    for y in 0..buf.height {
        for x in 0..buf.width {
            let cell = buf.get(x, y);
            if cell.text.trim().is_empty() {
                continue;
            }
            if cell.text == "L" {
                styled_cell = Some((y, cell.clone()));
                break;
            }
        }
        if styled_cell.is_some() {
            break;
        }
    }
    let (row, styled_cell) = styled_cell.expect("expected heading text cell");
    assert_eq!(
        row, 2,
        "MarkdownHeader top margin should offset H1 by 2 rows"
    );
    let style = styled_cell.style.expect("expected heading style");
    assert_eq!(style.bold, Some(true));
    assert_ne!(
        style.underline,
        Some(true),
        "h1 text-style token should not force underline"
    );
}

#[test]
fn markdown_h1_content_align_centers_heading_text() {
    let _guard = set_style_context(default_widget_stylesheet());

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (32, 3);
    options.max_width = 32;
    options.max_height = 3;

    let mut markdown = Markdown::new("# Lady Jessica");
    markdown.on_layout(32, 3);
    let buf =
        FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&markdown), None);

    let expected_start = (32 - rich_rs::cell_len("Lady Jessica")) / 2;
    let mut actual = None;
    for y in 0..buf.height {
        for x in 0..buf.width {
            let cell = buf.get(x, y);
            if cell.text == "L" {
                actual = Some((x, y));
                break;
            }
        }
        if actual.is_some() {
            break;
        }
    }
    let (actual_start, row) = actual.expect("expected h1 start cell");
    assert_eq!(
        row, 2,
        "MarkdownHeader top margin should offset H1 by 2 rows"
    );
    assert_eq!(actual_start, expected_start);
}

#[test]
fn markdown_wrapped_h1_keeps_component_style_on_wrapped_lines() {
    let _guard = set_style_context(default_widget_stylesheet());

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 8);
    options.max_width = 12;
    options.max_height = 8;

    let mut markdown = Markdown::new("# Heading wraps nicely");
    markdown.on_layout(12, 8);
    let buf =
        FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&markdown), None);

    let lines = buf.as_plain_lines();
    let wrapped_row = lines
        .iter()
        .position(|line| line.contains("wraps") || line.contains("nicely"))
        .expect("expected wrapped heading row");
    let wrapped_line = lines[wrapped_row].clone();
    assert!(
        wrapped_line.contains("wraps") || wrapped_line.contains("nicely"),
        "expected wrapped heading content on line 2, got {wrapped_line:?}"
    );

    let mut styled_cell = None;
    for x in 0..buf.width {
        let cell = buf.get(x, wrapped_row);
        if cell.text.trim().is_empty() {
            continue;
        }
        styled_cell = Some(cell.clone());
        break;
    }
    let styled_cell = styled_cell.expect("expected styled wrapped heading cell");
    let style = styled_cell
        .style
        .expect("expected style on wrapped heading cell");
    assert_eq!(style.bold, Some(true));
    assert_ne!(
        style.underline,
        Some(true),
        "h1 wrapped lines should keep h1 text-style (bold without underline)"
    );
}
