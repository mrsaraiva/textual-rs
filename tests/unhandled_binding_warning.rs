//! Regression: a matched `BindingDecl` whose action no dispatch layer handles
//! (source-node `execute_action`, app-root `execute_action`, and the
//! `on_app_unhandled_action` fallback all decline) must be REPORTED, not
//! silently swallowed. Authors otherwise believe the binding is wired when it
//! does nothing.
//!
//! Python parity: `App._dispatch_action` logs
//! `<action> ... has no target. Could not find methods '_action_x'/'action_x'`
//! via `log.system` (`app.py`). Rust mirrors that with a warning on the input
//! debug channel (`TEXTUAL_DEBUG_INPUT_FILE`) plus a bounded test-observable
//! buffer drained by `runtime::take_unhandled_binding_reports()`. Default
//! runtime behavior (fall through to raw key dispatch) is unchanged.

use textual::compose;
use textual::prelude::*;
use textual::runtime::take_unhandled_binding_reports;

struct UnwiredBindingApp;

impl TextualApp for UnwiredBindingApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(compose![Static::new("body").id("body")])
    }
    fn bindings(&self) -> Vec<BindingDecl> {
        // Nothing implements this action: no execute_action arm, no
        // action_registry entry, no on_app_action_str override.
        vec![BindingDecl::new("x", "totally_unwired", "Does nothing")]
    }
}

#[test]
fn matched_binding_with_no_action_handler_is_reported() {
    UnwiredBindingApp
        .run_test(|pilot| {
            let _ = take_unhandled_binding_reports();
            pilot.press(&["x"])?;
            let reports = take_unhandled_binding_reports();
            assert!(
                reports.iter().any(|r| r.contains("totally_unwired")),
                "a matched binding whose action nothing handles must be reported \
                 (got reports: {reports:?})"
            );
            Ok(())
        })
        .unwrap();
}

struct WiredBindingApp;

impl TextualApp for WiredBindingApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(compose![Static::new("body").id("body")])
    }
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("x", "wired", "Handled by the app")]
    }
    fn on_app_action_str(
        &mut self,
        _app: &mut App,
        action: &str,
        ctx: &mut textual::event::WidgetCtx,
    ) {
        if action == "wired" {
            ctx.set_handled();
        }
    }
}

#[test]
fn handled_binding_is_not_reported() {
    WiredBindingApp
        .run_test(|pilot| {
            let _ = take_unhandled_binding_reports();
            pilot.press(&["x"])?;
            let reports = take_unhandled_binding_reports();
            assert!(
                reports.is_empty(),
                "a handled binding must not be reported (got: {reports:?})"
            );
            Ok(())
        })
        .unwrap();
}
