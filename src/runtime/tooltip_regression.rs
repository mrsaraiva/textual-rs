//! End-to-end regression tests for the system tooltip on the shared widget path.
//!
//! The tooltip is a `position: absolute; overlay: screen` bubble mounted once by
//! the runtime on every screen (`mount_system_tooltip`). On hover the runtime
//! reads the hovered widget's `tooltip()`, sets the bubble's text + owner
//! ([`Tooltip::apply_system_state`]) and stores the mouse-relative anchor as the
//! node's `absolute_offset`. CSS `offset-x: -50%` then centers the bubble on the
//! anchor and the `overlay: screen` deferred-paint pass floats it at the top z of
//! the screen, constrained into the frame (`constrain: inside inflect`). No
//! widget-local FrameBuffer compositor is involved — the retired `tooltip_frame`
//! / `overlay_origin` path is gone.
//!
//! These tests pin the behaviours the design turns on:
//! 1. hovering a widget with a `tooltip()` surfaces the bubble text on screen;
//! 2. moving off the widget clears it;
//! 3. the bubble is placed as a real absolutely-positioned node (its
//!    `absolute_offset` reflects the hover anchor) rather than baked geometry.

#![cfg(test)]

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::widgets::{AppRoot, NodeSeed, SYSTEM_TOOLTIP_STYLE_ID, Widget};
use crate::{App, TextualApp};

const HOST_ID: &str = "tip_host";
const TIP: &str = "Fear is the mind-killer";

/// A hoverable widget that advertises a tooltip via the `tooltip()` hook (the
/// same hook `.with_tooltip(...)` feeds in real apps).
struct TipHost {
    seed: NodeSeed,
}

impl TipHost {
    fn new() -> Self {
        Self {
            seed: NodeSeed {
                css_id: Some(HOST_ID.to_string()),
                ..NodeSeed::default()
            },
        }
    }
}

impl Widget for TipHost {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        vec![Segment::from("hover me")].into()
    }
    fn tooltip(&self) -> Option<String> {
        Some(TIP.to_string())
    }
    fn mouse_interactive(&self) -> bool {
        true
    }
    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
    fn style_type(&self) -> &'static str {
        "TipHost"
    }
    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }
    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for TipHost {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

struct TipApp;

impl TextualApp for TipApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(TipHost::new())
    }
}

fn frame_shows(app: &App, needle: &str) -> bool {
    app.frame
        .as_plain_lines()
        .iter()
        .any(|line| line.contains(needle))
}

fn tooltip_absolute_offset(app: &App) -> Option<(i32, i32)> {
    let id = app.get_widget_by_id(SYSTEM_TOOLTIP_STYLE_ID).ok()?;
    app.active_widget_tree()?.get(id)?.absolute_offset
}

#[test]
fn hovering_widget_surfaces_and_clears_tooltip_bubble() {
    crate::run_test(TipApp, |pilot| {
        pilot.pause()?;
        assert!(
            !frame_shows(pilot.app(), TIP),
            "tooltip must be hidden before any hover"
        );
        assert_eq!(
            tooltip_absolute_offset(pilot.app()),
            None,
            "no anchor before hover"
        );

        // Hover the host: the runtime reads its tooltip(), shows the bubble and
        // anchors it at the hover point.
        pilot.hover(&format!("#{HOST_ID}"))?;
        pilot.pause()?;
        assert!(
            frame_shows(pilot.app(), TIP),
            "tooltip bubble must render on screen while hovering the host"
        );
        assert!(
            tooltip_absolute_offset(pilot.app()).is_some(),
            "the bubble must be anchored via the node's absolute_offset (not baked geometry)"
        );

        // Move the pointer off the host (to an empty cell): the bubble clears.
        pilot.move_to(0, 23)?;
        pilot.pause()?;
        assert!(
            !frame_shows(pilot.app(), TIP),
            "tooltip must clear once the pointer leaves the host"
        );
        assert_eq!(
            tooltip_absolute_offset(pilot.app()),
            None,
            "anchor must be cleared when the bubble hides"
        );
        Ok(())
    })
    .unwrap();
}
