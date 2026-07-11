//! Regression: click-to-focus (Python `Screen._forward_event`, MouseDown).
//!
//! Python focuses the first focusable widget in the ancestry of the widget
//! under a mouse press BEFORE the widget receives the event
//! (`get_focusable_widget_at` + `set_focus(..., scroll_visible=False)`), and a
//! press on a widget with no focusable ancestor leaves focus untouched. The
//! runtime had NO pointer-focus at all: focus only ever moved via Tab/actions,
//! so (e.g.) clicking a Button while an Input was focused left the Input's
//! `:focus` border painted — the `events/prevent` parity gap.

#![cfg(test)]

use crate::widgets::{AppRoot, Button, Input, Static};
use crate::{App, TextualApp};

struct ClickFocusApp;

impl TextualApp for ClickFocusApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Input::new().id("field"))
            .with_child(Button::new("Clear").id("clear"))
            .with_child(Static::new("just a label").id("label"))
    }
}

fn focused_node(app: &App) -> Option<crate::node_id::NodeId> {
    app.active_widget_tree()
        .and_then(super::routing::focused_node_id_tree)
}

#[test]
fn mouse_down_moves_focus_to_clicked_focusable_widget() {
    crate::run_test(ClickFocusApp, |pilot| {
        pilot.app_mut().action_focus("field").unwrap();
        pilot.pause()?;
        let input = pilot.app().query_one("#field").unwrap();
        let button = pilot.app().query_one("#clear").unwrap();
        assert_eq!(
            focused_node(pilot.app()),
            Some(input),
            "precondition: the Input holds focus"
        );

        // Clicking the Button must BLUR the Input and focus the Button
        // (Python `Screen._forward_event` order: focus moves before the widget
        // handles the press).
        pilot.click("#clear")?;
        assert_eq!(
            focused_node(pilot.app()),
            Some(button),
            "clicking a focusable widget must move focus to it"
        );

        // Clicking a NON-focusable widget with no focusable ancestor leaves
        // the current focus untouched (Python only refocuses when
        // `get_focusable_widget_at` finds a focusable node).
        pilot.click("#label")?;
        assert_eq!(
            focused_node(pilot.app()),
            Some(button),
            "clicking a non-focusable widget must not steal focus"
        );
        Ok(())
    })
    .unwrap();
}
