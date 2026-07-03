//! Regression: focusing a node that becomes displayed in the SAME frame.
//!
//! A widget handler can, in one pass, reveal a previously `display: none`
//! composed child (by adding a class that flips its CSS `display`) and request
//! focus of that child (`AppFocus { widget_id }`). The class flip is applied by
//! the deferred command flush + layout pass that runs AFTER the handler's
//! dispatch, but the generated `AppFocus` message routes DURING dispatch — so
//! when `action_focus` runs, the target's cached `display` is still stale
//! (`false`) and `set_focus_node` rejects it. Without a retry the child stays
//! unfocused for the frame (the "needs two toggles to focus" bug).
//!
//! This is the framework behavior the arena `Select` overlay depends on (its
//! `SelectOverlay` must be focused the instant `-expanded` is added). The test
//! is intentionally written against GENERIC synthetic widgets, not `Select`, so
//! a future `Select` refactor cannot silently mask a regression here.

#![cfg(test)]

use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::compose::{ChildDecl, ComposeResult};
use crate::event::{Event, WidgetCtx};
use crate::message::AppFocus;
use crate::widgets::{AppRoot, NodeSeed, Widget};
use crate::{App, TextualApp};

const OVERLAY_ID: &str = "spike_overlay";
const PARENT_ID: &str = "spike_parent";

/// A composed child that starts `display: none` and is revealed + focused by
/// its parent in a single pass.
struct SpikeOverlay {
    seed: NodeSeed,
}

impl SpikeOverlay {
    fn new() -> Self {
        Self {
            seed: NodeSeed {
                css_id: Some(OVERLAY_ID.to_string()),
                ..NodeSeed::default()
            },
        }
    }
}

impl Widget for SpikeOverlay {
    fn focusable(&self) -> bool {
        true
    }
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }
    fn style_type(&self) -> &'static str {
        "SpikeOverlay"
    }
    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }
    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

impl Renderable for SpikeOverlay {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

/// Focusable parent: on `o`, reveal + focus its overlay child in one pass.
struct SpikeParent {
    expanded: bool,
    seed: NodeSeed,
}

impl SpikeParent {
    fn new() -> Self {
        Self {
            expanded: false,
            seed: NodeSeed {
                css_id: Some(PARENT_ID.to_string()),
                ..NodeSeed::default()
            },
        }
    }
}

impl Widget for SpikeParent {
    fn focusable(&self) -> bool {
        true
    }
    fn compose(&mut self) -> ComposeResult {
        vec![ChildDecl::new(Box::new(SpikeOverlay::new()))]
    }
    fn on_event(&mut self, event: &Event, ctx: &mut WidgetCtx) {
        if let Event::Key(key) = event {
            if key.code == KeyCode::Char('o') && !self.expanded {
                self.expanded = true;
                // Deferred: applied by the post-dispatch command flush + layout.
                ctx.add_class("-expanded");
                // Generated message: routes during this dispatch, BEFORE the
                // flush above lands — the crux of the same-frame gap.
                ctx.post_message(AppFocus {
                    widget_id: OVERLAY_ID.to_string(),
                });
                ctx.set_handled();
            }
        }
    }
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }
    fn style_type(&self) -> &'static str {
        "SpikeParent"
    }
    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }
    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
    fn layout_height(&self) -> Option<usize> {
        Some(3)
    }
}

impl Renderable for SpikeParent {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

struct SpikeApp;

impl TextualApp for SpikeApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(SpikeParent::new())
    }
    fn configure(&mut self, app: &mut App) -> crate::Result<()> {
        app.load_stylesheet(
            "#spike_overlay { display: none; } \
             SpikeParent.-expanded #spike_overlay { display: block; }",
        );
        Ok(())
    }
}

fn focused_node(app: &App) -> Option<crate::node_id::NodeId> {
    app.active_widget_tree()
        .and_then(super::routing::focused_node_id_tree)
}

#[test]
fn appfocus_of_just_expanded_child_lands_same_frame() {
    crate::run_test(SpikeApp, |pilot| {
        // Deterministically focus the parent, then settle.
        pilot.app_mut().action_focus(PARENT_ID).unwrap();
        pilot.pause()?;
        let parent = pilot.app().query_one(&format!("#{PARENT_ID}")).unwrap();
        assert_eq!(
            focused_node(pilot.app()),
            Some(parent),
            "precondition: parent is focused before the reveal"
        );

        // Reveal + focus the overlay in one keypress.
        pilot.press_key("o")?;

        let overlay = pilot
            .app()
            .query_one(&format!("#{OVERLAY_ID}"))
            .expect("overlay is mounted");
        let tree = pilot.app().active_widget_tree().unwrap();
        assert!(
            tree.is_displayed(overlay),
            "overlay must be displayed after expand"
        );
        assert_eq!(
            focused_node(pilot.app()),
            Some(overlay),
            "overlay must be focused in the same frame it becomes displayed"
        );
        Ok(())
    })
    .unwrap();
}
