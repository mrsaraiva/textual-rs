use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;
use textual::runtime::{build_widget_tree_from_root, render_tree_to_frame};

/// Render a widget through the compose/tree path (the way the runtime renders it),
/// applying the default widget stylesheet, into a FrameBuffer for assertions/snapshots.
///
/// The `Markdown` widget is compose-only — its content lives in composed children that
/// only render when mounted in a widget tree, so tests must render it this way rather
/// than via `FrameBuffer::from_renderable`, which would yield an empty buffer.
fn render_tree(root: &mut dyn Widget, width: usize, height: usize) -> FrameBuffer {
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(root).expect("tree should build");
    render_tree_to_frame(&mut tree, root, &console, width, height)
}

#[test]
fn markdown_renders_basic_blocks() {
    let mut markdown = Markdown::new("# Title\n\n- Alpha\n- Beta\n\n`code`");
    let buf = render_tree(&mut markdown, 26, 9);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn markdown_h1_uses_default_component_style() {
    let mut markdown = Markdown::new("# Heading");
    let buf = render_tree(&mut markdown, 24, 3);

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
    let mut markdown = Markdown::new("# 👩‍🚀 Launch");
    let buf = render_tree(&mut markdown, 28, 3);

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
    let mut markdown = Markdown::new("# Lady Jessica");
    let buf = render_tree(&mut markdown, 32, 3);

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
    let mut markdown = Markdown::new("# Heading wraps nicely");
    let buf = render_tree(&mut markdown, 12, 8);

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

#[test]
fn markdown_layout_height_tracks_rendered_table_height() {
    let mut markdown = Markdown::new(
        "| Name | Type | Default |\n| ---- | ---- | ---- |\n| show_header | bool | True |\n| fixed_rows | int | 0 |",
    );
    markdown.on_layout(48, 6);
    assert!(
        markdown.layout_height().unwrap_or_default() > 4,
        "table rendering should consume more rows than raw markdown source rows"
    );
}
