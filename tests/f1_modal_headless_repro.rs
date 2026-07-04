//! Regression for the ColdApp-Kanban "F1" friction claim: that a pushed modal
//! screen's contents are unreachable by `query_one` / `pilot.click` / auto-focus
//! in the headless `run_test` path.
//!
//! Diagnosis: NOT a runtime bug. A screen's per-screen widget tree is built,
//! mounted, auto-focused, laid out and rendered eagerly at push time
//! (`ScreenStack::push_inner` → `mount_declarations`), and `App::query` /
//! `pilot.click` resolve against `active_widget_tree()` (the top screen). These
//! tests lock that contract: after pushing a modal that mirrors coldapp's
//! `AddTaskScreen` shape (id-bearing fields wrapped in a `VerticalGroup`), the
//! modal's tree is queryable by type and by seed id, focusable, typeable, and
//! clickable — across a trivial root, a centered modal, and a dense app-root.

use rich_rs::{Console, ConsoleOptions, Segments};
use textual::prelude::*;

// A modal root that wraps its id-bearing fields inside a VerticalGroup, exactly
// like coldapp_kanban's AddTaskRoot.
struct ModalRoot;

impl Widget for ModalRoot {
    fn style_type(&self) -> &'static str {
        "ModalRoot"
    }

    fn compose(&mut self) -> ComposeResult {
        vec![ChildDecl::from(
            VerticalGroup::new()
                .with_child(Label::new("Add a new task").with_id("m-title"))
                .with_child(Label::new("Title:"))
                .with_child(Input::new().with_placeholder("Describe").id("foo"))
                .with_child(Label::new("Priority:"))
                .with_child(Button::new("Low").id("p-low"))
                .with_child(Button::new("Med").id("p-med"))
                .with_child(Button::error("High").id("p-high")),
        )]
    }

    fn render(&self, _c: &Console, _o: &ConsoleOptions) -> Segments {
        Segments::new()
    }
}

struct ModalScreen {
    centered: bool,
}

impl Screen for ModalScreen {
    fn name(&self) -> &str {
        "ModalScreen"
    }
    fn compose(&self) -> Box<dyn Widget> {
        Box::new(ModalRoot)
    }
    fn css(&self) -> Option<&str> {
        // Mirror coldapp's add_task.tcss centering.
        if self.centered {
            Some(
                "ModalRoot { align: center middle; background: $surface; \
                 border: thick $primary; width: 60; height: auto; padding: 1 2; } \
                 ModalRoot Input { margin: 1 0; } \
                 ModalRoot Button { margin: 0 1 0 0; }",
            )
        } else {
            None
        }
    }
    fn auto_focus(&self) -> Option<&str> {
        Some("#foo")
    }
    fn on_button_pressed(
        &mut self,
        pressed: &ButtonPressed,
        _control: NodeId,
        ctx: &mut ScreenMessageCtx,
    ) {
        if pressed.button_id.as_deref() == Some("p-high") {
            ctx.dismiss_none();
        }
    }
}

/// App whose root is trivial (or dense, if `dense`), pushing the modal on `a`.
struct ReproApp {
    centered: bool,
    dense: bool,
}

impl TextualApp for ReproApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("a", "add", "Add")]
    }

    fn compose(&mut self) -> AppRoot {
        if self.dense {
            // Fill the whole screen with interactive buttons beneath the modal.
            let mut col = VerticalGroup::new();
            for i in 0..20 {
                col = col.with_child(Button::new(format!("row {i}")).id(format!("bg-{i}")));
            }
            AppRoot::new().with_child(col)
        } else {
            AppRoot::new().with_child(Label::new("base"))
        }
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut WidgetCtx) {
        if action == "add" {
            let centered = self.centered;
            app.push_screen_with_callback(Box::new(ModalScreen { centered }), Box::new(|_| {}));
            ctx.set_handled();
        }
    }
}

fn app(centered: bool, dense: bool) -> ReproApp {
    ReproApp { centered, dense }
}

/// The pushed modal's tree resolves by type and by seed id, and a click on a
/// button inside it routes to the screen's `on_button_pressed` (which dismisses).
#[test]
fn pushed_modal_is_query_and_click_reachable() {
    run_test(app(false, false), |pilot| {
        assert_eq!(pilot.app().screen_count(), 0);
        pilot.press(&["a"])?;
        assert_eq!(pilot.app().screen_count(), 1, "`a` must push the modal");

        // Type queries against the pushed screen tree.
        assert!(pilot.app().query_one("ModalRoot").is_ok(), "screen root style_type");
        assert!(pilot.app().query_one("Input").is_ok(), "Input by type");
        assert!(pilot.app().query_one("#foo").is_ok(), "#foo seed id under a container");
        assert!(pilot.app().query_one("#p-high").is_ok(), "#p-high seed id under a container");

        // Click a button inside the pushed modal → routes to the screen handler.
        pilot.click("#p-high")?;
        assert_eq!(
            pilot.app().screen_count(),
            0,
            "clicking inside the modal must route to the screen handler and dismiss it"
        );
        Ok(())
    })
    .unwrap();
}

/// `auto_focus('#foo')` focuses the modal's Input and typing routes into it.
#[test]
fn pushed_modal_autofocus_and_typing() {
    run_test(app(false, false), |pilot| {
        pilot.press(&["a"])?;
        assert_eq!(pilot.app().screen_count(), 1);

        // If auto_focus worked and key routing reaches the pushed screen, the
        // typed text lands in the Input.
        pilot.press(&["h", "i"])?;
        let value = pilot
            .app_mut()
            .with_query_one_mut_as::<Input, _>("#foo", |i| i.value().to_string())
            .ok();
        assert_eq!(
            value.as_deref(),
            Some("hi"),
            "typing must route into the auto-focused modal Input"
        );
        Ok(())
    })
    .unwrap();
}

/// A centered modal (align: center middle + explicit width) is still reachable.
#[test]
fn centered_pushed_modal_is_reachable() {
    run_test(app(true, false), |pilot| {
        pilot.press(&["a"])?;
        assert_eq!(pilot.app().screen_count(), 1);
        assert!(pilot.app().query_one("#p-high").is_ok(), "#p-high resolves");
        pilot.click("#p-high")?;
        assert_eq!(pilot.app().screen_count(), 0, "centered modal click must dismiss");
        Ok(())
    })
    .unwrap();
}

/// A dense interactive app-root beneath the pushed modal does not prevent a
/// click on the modal's button from routing to the screen handler.
#[test]
fn pushed_modal_reachable_over_dense_root() {
    run_test(app(true, true), |pilot| {
        pilot.press(&["a"])?;
        assert_eq!(pilot.app().screen_count(), 1);
        assert!(pilot.app().query_one("#p-high").is_ok(), "#p-high resolves over a dense root");
        pilot.click("#p-high")?;
        assert_eq!(
            pilot.app().screen_count(),
            0,
            "modal click must dismiss even with a dense app-root beneath"
        );
        Ok(())
    })
    .unwrap();
}
