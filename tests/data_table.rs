use rich_rs::Console;
use slotmap::SlotMap;
use textual::prelude::*;
use textual::reactive::ReactiveCtx;
use textual::render::FrameBuffer;
use textual::runtime::dispatch_ctx::set_dispatch_recipient;

fn make_node_id() -> NodeId {
    let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
    sm.insert(())
}

fn focused_state() -> NodeState {
    NodeState {
        focused: true,
        ..Default::default()
    }
}

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
    table.on_node_state_changed(NodeState::default(), focused_state());
    let mut rctx = ReactiveCtx::new(NodeId::default());
    table.set_selected(1, &mut rctx);

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
    table.on_node_state_changed(NodeState::default(), focused_state());
    let mut rctx = ReactiveCtx::new(NodeId::default());

    table.set_selected(19, &mut rctx);
    let tail = FrameBuffer::from_renderable(&console, &options, &table, None);
    let tail_lines = tail.as_plain_lines();
    assert!(tail_lines.iter().any(|line| line.contains("Row 19")));

    table.set_selected(0, &mut rctx);
    let head = FrameBuffer::from_renderable(&console, &options, &table, None);
    let head_lines = head.as_plain_lines();
    assert!(head_lines.iter().any(|line| line.contains("Row 0")));
}

#[test]
fn data_table_fixed_rows_remain_visible_with_scrolled_selection() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 4);
    options.max_width = 24;
    options.max_height = 4;

    let rows = (0..8)
        .map(|idx| vec![format!("Row {idx}"), idx.to_string()])
        .collect::<Vec<_>>();
    let mut table = DataTable::new(vec!["Name".into(), "Value".into()], rows);
    let mut rctx = ReactiveCtx::new(NodeId::default());
    table.set_fixed_rows(1, &mut rctx);
    table.on_node_state_changed(NodeState::default(), focused_state());
    table.set_selected(7, &mut rctx);

    let buf = FrameBuffer::from_renderable(&console, &options, &table, None);
    let lines = buf.as_plain_lines();
    assert!(lines.iter().any(|line| line.contains("Row 0")));
    assert!(lines.iter().any(|line| line.contains("Row 7")));
}

#[test]
fn data_table_exposes_keyed_row_and_column_lookups() {
    let mut table = DataTable::empty();
    let column = table
        .add_column_with_key("country", "Country")
        .expect("column key should be unique");
    let row = table
        .add_row_with_key("heat-1", vec!["Brazil"])
        .expect("row key should be unique");

    assert_eq!(table.column_index_of(&column), Some(0));
    assert_eq!(table.row_index_of(&row), Some(0));
    assert_eq!(table.cursor_cell_key(), Some((row, column)));
}

#[test]
fn data_table_row_cursor_actions_can_scroll_horizontal_viewport() {
    let mut table = DataTable::new(
        vec![
            "First".into(),
            "Second".into(),
            "Third".into(),
            "Fourth".into(),
        ],
        vec![vec!["a".into(), "b".into(), "c".into(), "d".into()]],
    );
    let _guard = set_dispatch_recipient(make_node_id(), focused_state());
    let mut rctx = ReactiveCtx::new(NodeId::default());
    table.set_cursor_type(CursorType::Row, &mut rctx);
    table.on_layout(12, 4);

    let mut ctx = EventCtx::default();
    table.on_event(&Event::Action(Action::ScrollRight), &mut ctx);
    assert!(ctx.handled());

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (12, 4);
    options.max_width = 12;
    options.max_height = 4;
    let buf = FrameBuffer::from_renderable(&console, &options, &table, None);
    let lines = buf.as_plain_lines();
    assert!(
        lines[0].contains("Second") || lines[0].contains("Third") || lines[0].contains("Fourth")
    );
}
