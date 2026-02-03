use std::collections::BTreeMap;
use std::sync::Arc;

use rich_rs::{Console, MetaValue, Renderable, Segment, Segments, StyleMeta};
use textual::render::FrameBuffer;

struct MetaRenderable;

impl Renderable for MetaRenderable {
    fn render(&self, _console: &Console, _options: &rich_rs::ConsoleOptions) -> Segments {
        let mut meta_map = BTreeMap::new();
        meta_map.insert(
            "@click".to_string(),
            MetaValue::Tuple(vec![MetaValue::str("button_1")]),
        );
        let meta = StyleMeta {
            meta: Some(Arc::new(meta_map)),
            ..Default::default()
        };

        let mut segments = Segments::new();
        segments.push(Segment::new_with_meta("OK", meta));
        segments
    }
}

#[test]
fn preserves_meta_in_framebuffer_and_diff() {
    let console = Console::new();
    let mut options = console.options().clone();
    options.size = (4, 1);
    options.max_width = 4;
    options.max_height = 1;

    let next = FrameBuffer::from_renderable(&console, &options, &MetaRenderable, None);
    let prev = FrameBuffer::new(4, 1, None);

    let diff = next.diff_to_segments(&prev);
    let has_meta = diff.iter().any(|seg| seg.meta.is_some());
    assert!(has_meta, "diff should preserve metadata");

    insta::assert_snapshot!(next.debug_dump());
}
