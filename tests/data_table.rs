use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn data_table_renders_header_and_rows() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (16, 4);
    options.max_width = 16;
    options.max_height = 4;

    let mut table = DataTable::new(
        vec!["Name".into(), "Value".into()],
        vec![
            vec!["Alpha".into(), "1".into()],
            vec!["Beta".into(), "2".into()],
        ],
    );
    table.set_focus(true);
    table.set_selected(1);

    let buf = FrameBuffer::from_renderable(&console, &options, &table, None);
    insta::assert_snapshot!(buf.debug_dump());
}

#[test]
fn data_table_keeps_selected_row_visible_after_large_jumps() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (20, 4);
    options.max_width = 20;
    options.max_height = 4;

    let rows = (0..20)
        .map(|idx| vec![format!("Row {idx}"), idx.to_string()])
        .collect::<Vec<_>>();
    let mut table = DataTable::new(vec!["Name".into(), "Value".into()], rows);
    table.set_focus(true);

    table.set_selected(19);
    let tail = FrameBuffer::from_renderable(&console, &options, &table, None);
    let tail_lines = tail.as_plain_lines();
    assert!(tail_lines.iter().any(|line| line.contains("Row 19")));

    table.set_selected(0);
    let head = FrameBuffer::from_renderable(&console, &options, &table, None);
    let head_lines = head.as_plain_lines();
    assert!(head_lines.iter().any(|line| line.contains("Row 0")));
}
