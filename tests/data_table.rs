use rich_rs::Console;
use slotmap::SlotMap;
use textual::prelude::*;
use textual::event::EventCtx;
use textual::reactive::ReactiveCtx;
use textual::render::FrameBuffer;
use textual::runtime::dispatch_ctx::set_dispatch_recipient;
use textual::widgets::SortKey;

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

/// Extract (r, g, b) from a rendered cell's `SimpleColor` foreground.
fn simple_rgb(color: rich_rs::SimpleColor) -> (u8, u8, u8) {
    match color {
        rich_rs::SimpleColor::Rgb { r, g, b } => (r, g, b),
        other => panic!("expected truecolor RGB foreground, got {other:?}"),
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

    // Snapshot meta: `textual:no_style` is expected on two regions, matching
    // Python's component-class semantics (cells composed to FINAL colours must
    // not be re-tinted by the widget-level `DataTable:focus { background-tint }`
    // pass):
    // - row y=0: the header (`.datatable--header` carries its OWN tint rule),
    // - y=2 x=0..=6: the padded cursor cell (default cursor type is Cell and
    //   `set_selected(1)` puts the cursor on (1, 0) = the "Beta" name cell;
    //   Python paints it opaque `$block-cursor-background` with no extra tint).
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
    { let mut __w = textual::event::WidgetCtx::__from_dispatch(textual::node_id::NodeId::default(), &mut ctx); table.on_event(&Event::Action(Action::ScrollRight), &mut __w) };
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

// ── Cell-model verification (keystone: renderable Content cells + key-fn sort) ──

/// A styled `Content` cell renders its foreground color + italic. Mirrors Python
/// `Text("…", style="italic #03AC13")` in `data_table_renderables`.
#[test]
fn data_table_styled_cell_renders_color_and_italic() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 4);
    options.max_width = 24;
    options.max_height = 4;

    let mut table = DataTable::empty();
    table.add_columns(["a"]);
    // Single italic green cell. Right-justified to also exercise alignment.
    table.add_row_cells(vec![
        DataTableCell::markup("[italic #03AC13]Go").with_align(TextAlign::Right),
    ]);
    table.set_show_header(false, &mut ReactiveCtx::new(NodeId::default()));

    let buf = FrameBuffer::from_renderable(&console, &options, &table, None);

    // Find a cell whose glyph is 'G' or 'o' (the styled content) and check its style.
    let mut found = false;
    'outer: for y in 0..4 {
        for x in 0..24 {
            let cell = buf.get(x, y);
            if cell.text == "G" || cell.text == "o" {
                let style = cell.style.as_ref().expect("styled glyph must carry a style");
                assert_eq!(
                    style.italic,
                    Some(true),
                    "styled cell glyph should be italic"
                );
                let color = style.color.expect("styled glyph must have a fg color");
                assert_eq!(
                    simple_rgb(color),
                    (0x03, 0xAC, 0x13),
                    "styled cell glyph fg should be #03AC13"
                );
                found = true;
                break 'outer;
            }
        }
    }
    assert!(found, "expected to find the styled cell glyph in the render");
}

/// Numeric-key sort orders rows numerically (so 10 comes after 2, not before).
#[test]
fn data_table_sort_numeric_key_orders_correctly() {
    let mut table = DataTable::empty();
    table.add_columns(["lane", "swimmer"]);
    table.add_row(vec!["10", "Darren"]);
    table.add_row(vec!["2", "Michael"]);
    table.add_row(vec!["1", "Aleksandr"]);

    // Single-column numeric sort by lane.
    table.sort(0, false);
    let lanes: Vec<&str> = (0..3).map(|r| table.get_cell(r, 0).unwrap()).collect();
    assert_eq!(lanes, vec!["1", "2", "10"], "numeric sort: 1 < 2 < 10");

    // Reverse.
    table.sort(0, true);
    let lanes_rev: Vec<&str> = (0..3).map(|r| table.get_cell(r, 0).unwrap()).collect();
    assert_eq!(lanes_rev, vec!["10", "2", "1"]);

    // Key-function sort: by last value parsed, here last char of swimmer name.
    table.sort_by(&[1], false, |vals| {
        SortKey::str(vals[0].chars().last().unwrap_or(' ').to_string())
    });
    // Aleksandr->r, Darren->n, Michael->l  => l < n < r
    let names: Vec<&str> = (0..3).map(|r| table.get_cell(r, 1).unwrap()).collect();
    assert_eq!(names, vec!["Michael", "Darren", "Aleksandr"]);
}

/// A custom key over multiple columns (average of two numeric columns, then last
/// name) sorts faithfully — mirrors `data_table_sort`'s "sort by average time".
#[test]
fn data_table_sort_average_key_over_multiple_columns() {
    let mut table = DataTable::empty();
    table.add_columns(["swimmer", "t1", "t2"]);
    table.add_row(vec!["Zoe Adams", "60", "60"]); // avg 60
    table.add_row(vec!["Amy Carter", "50", "50"]); // avg 50
    table.add_row(vec!["Bob Brown", "55", "45"]); // avg 50

    table.sort_by(&[0, 1, 2], false, |vals| {
        let name = vals[0];
        let scores: Vec<f64> = vals[1..]
            .iter()
            .filter_map(|s| s.parse::<f64>().ok())
            .collect();
        let avg = scores.iter().sum::<f64>() / scores.len() as f64;
        let last = name.split_whitespace().last().unwrap_or("").to_string();
        SortKey::tuple([SortKey::number(avg), SortKey::str(last)])
    });

    // avg ties (Amy 50, Bob 50) broken by last name: Brown < Carter; then Adams (60).
    let names: Vec<&str> = (0..3).map(|r| table.get_cell(r, 0).unwrap()).collect();
    assert_eq!(names, vec!["Bob Brown", "Amy Carter", "Zoe Adams"]);
}

/// Multi-column sort (no key): order by first column, ties broken by second.
#[test]
fn data_table_multi_column_sort() {
    let mut table = DataTable::empty();
    table.add_columns(["group", "lane"]);
    table.add_row(vec!["b", "1"]);
    table.add_row(vec!["a", "10"]);
    table.add_row(vec!["a", "2"]);

    table.sort_by_columns(&[0, 1], false);

    let rows: Vec<(&str, &str)> = (0..3)
        .map(|r| (table.get_cell(r, 0).unwrap(), table.get_cell(r, 1).unwrap()))
        .collect();
    // group a before b; within a, lane 2 before 10 (numeric).
    assert_eq!(rows, vec![("a", "2"), ("a", "10"), ("b", "1")]);
}

/// A styled row label is preserved as `Content` and contributes its width to the
/// label column. Mirrors `data_table_labels` (`label=Text(...)`).
#[test]
fn data_table_styled_row_label_renders() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (24, 4);
    options.max_width = 24;
    options.max_height = 4;

    let mut table = DataTable::empty();
    table.add_columns(["x"]);
    table.add_row_labeled(vec!["v"], Content::from_markup("[#B0FC38 italic]7"));
    table.set_show_header(false, &mut ReactiveCtx::new(NodeId::default()));

    let buf = FrameBuffer::from_renderable(&console, &options, &table, None);

    let mut found = false;
    'outer: for y in 0..4 {
        for x in 0..24 {
            let cell = buf.get(x, y);
            if cell.text == "7" {
                let style = cell.style.as_ref().expect("label glyph must carry a style");
                assert_eq!(style.italic, Some(true), "label should be italic");
                let color = style.color.expect("label must have a fg color");
                assert_eq!(simple_rgb(color), (0xB0, 0xFC, 0x38));
                found = true;
                break 'outer;
            }
        }
    }
    assert!(found, "expected styled row label glyph in render");
}
