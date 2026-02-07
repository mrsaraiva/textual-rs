use rich_rs::{Console, Segment, Style};
use textual::render::FrameBuffer;
use textual::style::parse_color_like;
use textual::widgets::{Constrained, Horizontal, Widget, WidgetId, WidgetRenderable};

struct Swatch {
    id: WidgetId,
    text: &'static str,
    style: Style,
}

impl Swatch {
    fn new(text: &'static str, style: Style) -> Self {
        Self {
            id: WidgetId::new(),
            text,
            style,
        }
    }
}

impl Widget for Swatch {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, _console: &Console, _options: &rich_rs::ConsoleOptions) -> rich_rs::Segments {
        vec![Segment::styled(self.text, self.style)].into()
    }
}

#[test]
fn transparent_segments_keep_default_framebuffer_background() {
    let bg = parse_color_like("#123456").expect("valid color");
    let default_style = Style::new().with_bgcolor(bg.to_simple_opaque());
    let lines = vec![vec![Segment::new("    ")]];
    let framebuffer = FrameBuffer::from_lines(&lines, 4, 1, Some(default_style));

    for x in 0..4 {
        let cell = framebuffer.get(x, 0);
        let cell_style = cell.style.expect("default style should be preserved");
        assert_eq!(cell_style.bgcolor, Some(bg.to_simple_opaque()));
    }
}

#[test]
fn horizontal_trailing_space_does_not_inherit_last_child_background() {
    let base_bg = parse_color_like("#0b0e13").expect("valid color");
    let first_bg = parse_color_like("#203040").expect("valid color");
    let last_bg = parse_color_like("#804020").expect("valid color");
    let base_style = Style::new().with_bgcolor(base_bg.to_simple_opaque());

    let row = Horizontal::new()
        .with_child(
            Constrained::new(Swatch::new(
                "AAAA",
                Style::new().with_bgcolor(first_bg.to_simple_opaque()),
            ))
            .min_width(4)
            .max_width(4),
        )
        .with_child(
            Constrained::new(Swatch::new(
                "BBBB",
                Style::new().with_bgcolor(last_bg.to_simple_opaque()),
            ))
            .min_width(4)
            .max_width(4),
        );

    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (20, 1);
    options.max_width = 20;
    options.max_height = 1;

    let framebuffer = FrameBuffer::from_renderable(
        &console,
        &options,
        &WidgetRenderable::new(&row),
        Some(base_style),
    );

    // Row content is 8 cells wide; trailing viewport area must remain on base background.
    for x in 8..20 {
        let cell = framebuffer.get(x, 0);
        let cell_style = cell
            .style
            .expect("base style should be preserved in trailing area");
        assert_eq!(cell_style.bgcolor, Some(base_bg.to_simple_opaque()));
    }
}
