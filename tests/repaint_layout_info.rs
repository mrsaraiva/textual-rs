//! A repaint-only frame (scrolling, a content refresh) leaves layout geometry
//! unchanged, so it must not push layout info (`on_layout`,
//! `set_virtual_content_size`) to every widget again. That whole-tree pass
//! dominated frame time for large documents.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use textual::prelude::*;

struct Probe(Arc<AtomicUsize>);

impl Widget for Probe {
    fn render(&self, _: &rich_rs::Console, _: &rich_rs::ConsoleOptions) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }
    fn on_layout(&mut self, _width: u16, _height: u16) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn style_type(&self) -> &'static str {
        "Probe"
    }
}

struct ProbeApp(Arc<AtomicUsize>);

impl TextualApp for ProbeApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("p", "paint", "Paint"),
            BindingDecl::new("l", "relayout", "Relayout"),
        ]
    }
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Probe(self.0.clone()))
    }
    fn on_app_action_str(&mut self, _app: &mut App, action: &str, ctx: &mut WidgetCtx) {
        match action {
            "paint" => ctx.request_repaint(),
            "relayout" => ctx.request_layout_invalidation(),
            _ => return,
        }
        ctx.set_handled();
    }
}

#[test]
fn layout_info_is_pushed_only_when_layout_changes() {
    let calls = Arc::new(AtomicUsize::new(0));
    ProbeApp(calls.clone())
        .run_test(|pilot| {
            let before = calls.load(Ordering::Relaxed);
            pilot.press(&["p", "p", "p"])?;
            assert_eq!(
                calls.load(Ordering::Relaxed),
                before,
                "repaint-only frames must not call on_layout"
            );

            pilot.press(&["l"])?;
            assert!(
                calls.load(Ordering::Relaxed) > before,
                "a layout invalidation must still call on_layout"
            );
            Ok(())
        })
        .unwrap();
}
