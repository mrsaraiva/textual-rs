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
