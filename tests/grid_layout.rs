use rich_rs::Console;
use textual::prelude::*;
use textual::render::FrameBuffer;

#[test]
fn grid_renders_cells() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (8, 4);
    options.max_width = 8;
    options.max_height = 4;

    let mut grid = Grid::new(2, 2);
    grid.set(0, 0, Label::new("a"));
    grid.set(0, 1, Label::new("b"));
    grid.set(1, 0, Label::new("c"));
    grid.set(1, 1, Label::new("d"));

    let buf = FrameBuffer::from_renderable(&console, &options, &grid, None);
    insta::assert_snapshot!(buf.debug_dump());
}
