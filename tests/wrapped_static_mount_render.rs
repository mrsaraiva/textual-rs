//! Regression: a wrapper widget that delegates `render()` to an inner `Static`
//! and populates that `Static` in `on_mount()` must paint its text.
//!
//! Root cause (cycle-3 "wrapstatic"): children declared via `compose()` /
//! `with_child()` are *extracted* out of their parent widget and re-homed as
//! arena-tree nodes. The runtime build path drained the initial `Mount`
//! lifecycle events and discarded them, so those extracted nodes never received
//! `on_mount()`. A `Static` wrapper that calls `update()` in `on_mount` (the
//! `Hello(Static)` pattern from `docs/examples/guide/widgets/hello04..06`) was
//! left with empty content and rendered a blank content box.
//!
//! The render *delegation* itself was always correct — content set in `new()`
//! renders fine. The gap was purely the missing per-node `on_mount()` call,
//! now fired by `WidgetTree::fire_mount_callbacks` during tree build.

use textual::prelude::*;
use textual::runtime::render_tree_to_frame;
use textual::widget_tree::WidgetTree;

/// Mirrors the `Hello(Static)` wrapper from the hello04/05/06 demos: an inner
/// `Static` whose content is set in `on_mount`, with `render()` delegated to it.
struct Hello {
    inner: Static,
    mounted: bool,
}

impl Hello {
    fn new() -> Self {
        Self {
            inner: Static::new(""),
            mounted: false,
        }
    }
}

impl Widget for Hello {
    fn style_type(&self) -> &'static str {
        "Hello"
    }

    fn style_type_aliases(&self) -> &[&'static str] {
        &["Static"]
    }

    fn render(
        &self,
        console: &rich_rs::Console,
        options: &rich_rs::ConsoleOptions,
    ) -> rich_rs::Segments {
        self.inner.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    fn on_mount(&mut self) {
        self.mounted = true;
        self.inner.update("Hello, World!");
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.inner.take_node_seed()
    }
}

fn dump_frame(width: usize, height: usize, frame: &textual::render::FrameBuffer) -> String {
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            out.push_str(&frame.get(x, y).text);
        }
        out.push('\n');
    }
    out
}

#[test]
fn wrapped_static_content_set_in_on_mount_renders() {
    let mut tree = WidgetTree::new();
    let root = tree.set_root(Box::new(AppRoot::new()));
    tree.mount(root, Box::new(Hello::new()));

    // Mirror the runtime build path: fire on_mount on freshly-mounted nodes.
    tree.fire_mount_callbacks(root);

    let console = rich_rs::Console::new();
    let mut root_widget = AppRoot::new();
    let (w, h) = (24, 3);
    let frame = render_tree_to_frame(&mut tree, &mut root_widget, &console, w, h);
    let dump = dump_frame(w, h, &frame);

    assert!(
        dump.contains("Hello, World!") || dump.contains("World"),
        "wrapped Static text set in on_mount must paint; got:\n{dump}"
    );
}

#[test]
fn without_firing_mount_callbacks_content_is_empty() {
    // Negative control: skipping the mount-callback firing reproduces the
    // original bug (empty content box), proving the fix is load-bearing.
    let mut tree = WidgetTree::new();
    let root = tree.set_root(Box::new(AppRoot::new()));
    tree.mount(root, Box::new(Hello::new()));
    // Intentionally do NOT call fire_mount_callbacks.

    let console = rich_rs::Console::new();
    let mut root_widget = AppRoot::new();
    let (w, h) = (24, 3);
    let frame = render_tree_to_frame(&mut tree, &mut root_widget, &console, w, h);
    let dump = dump_frame(w, h, &frame);

    assert!(
        !dump.contains("World"),
        "without on_mount the wrapped Static is empty (negative control); got:\n{dump}"
    );
}
