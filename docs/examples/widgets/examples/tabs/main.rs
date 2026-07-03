/// Port of Python Textual `docs/examples/widgets/tabs.py`.
///
/// Demonstrates the `Tabs` widget:
/// - `a` adds the next name from a rotating list (cycling through NAMES)
/// - `r` removes the currently active tab
/// - `c` clears all tabs
///
/// When a tab is activated, the central label updates to show the tab title.
/// When all tabs are cleared, the label is hidden.
///
/// Python custom actions (`action_add`, `action_remove`, `action_clear`) are
/// implemented here via `on_key_with_app` since Rust's `TextualApp` does not
/// yet support user-defined named actions. Bindings are still declared so the
/// footer shows the hint labels.
use textual::prelude::*;

const NAMES: &[&str] = &[
    "Paul Atreidies",
    "Duke Leto Atreides",
    "Lady Jessica",
    "Gurney Halleck",
    "Baron Vladimir Harkonnen",
    "Glossu Rabban",
    "Chani",
    "Silgar",
];

const CSS: &str = r#"
Tabs {
    dock: top;
}

Screen {
    align: center middle;
}

Label {
    margin: 1 1;
    width: 100%;
    height: 100%;
    background: $panel;
    border: tall $primary;
    content-align: center middle;
}

Label.hidden {
    display: none;
}
"#;

struct TabsApp {
    /// Index into NAMES for the next "add" action.
    next_name_index: usize,
}

impl TabsApp {
    fn new() -> Self {
        Self { next_name_index: 1 }
    }
}

impl TextualApp for TabsApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        // Declare bindings for footer display.  The actual handling is in
        // `on_key_with_app` because "add", "remove", "clear" are not
        // framework built-in actions.
        vec![
            BindingDecl::new("a", "add", "Add tab"),
            BindingDecl::new("r", "remove", "Remove active tab"),
            BindingDecl::new("c", "clear", "Clear tabs"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        // Use widgets directly (no Node wrapper) so that type-selector queries
        // work: `app.with_query_one_mut_as::<Tabs, _>("Tabs", ...)`.
        // Label gets a CSS id via `with_id` so it can be queried by #id.
        let tabs = Tabs::new().with_tab(NAMES[0]);
        let label = Label::new(NAMES[0]).with_id("content-label");
        AppRoot::new()
            .with_child(tabs)
            .with_child(label)
            .with_child(Footer::new())
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut textual::event::WidgetCtx) {
        match key.name() {
            "a" => {
                // Add next name from rotating list. Mutating the tab set changes
                // the composed tab bar, so request a recompose through the ctx
                // (mirrors Python's self-mounting `Tabs.add_tab`).
                let name = NAMES[self.next_name_index % NAMES.len()].to_string();
                self.next_name_index = (self.next_name_index + 1) % NAMES.len();
                if let Ok(tabs) = app.query_one_typed::<Tabs>("Tabs") {
                    let _ = tabs.update(app, |tabs, ctx| {
                        tabs.add_tab(name);
                        ctx.request_recompose();
                    });
                }
                ctx.set_handled();
                ctx.request_repaint();
            }
            "r" => {
                // Remove the active tab.
                let active_id = app
                    .with_query_one_mut_as::<Tabs, _>("Tabs", |tabs| tabs.active())
                    .ok()
                    .flatten();
                if let Some(id) = active_id {
                    if let Ok(tabs) = app.query_one_typed::<Tabs>("Tabs") {
                        let _ = tabs.update(app, |tabs, ctx| {
                            tabs.remove_tab(&id);
                            ctx.request_recompose();
                        });
                    }
                }
                ctx.set_handled();
                ctx.request_repaint();
            }
            "c" => {
                // Clear all tabs.
                if let Ok(tabs) = app.query_one_typed::<Tabs>("Tabs") {
                    let _ = tabs.update(app, |tabs, ctx| {
                        tabs.clear();
                        ctx.request_recompose();
                    });
                }
                ctx.set_handled();
                ctx.request_repaint();
            }
            _ => {}
        }
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        _ctx: &mut textual::event::WidgetCtx,
    ) {
        if let Some(ev) = message.downcast_ref::<TabActivated>() {
            // Update label text and ensure it is visible.
            let title = ev.title.clone();
            let _ = app.with_query_one_mut_as::<Label, _>("#content-label", |label| {
                label.set_text(title);
            });
            let _ = app
                .query_mut("#content-label")
                .map(|q| q.remove_class("hidden"));
        } else if message.downcast_ref::<TabsCleared>().is_some() {
            // Hide the label when there are no tabs.
            let _ = app
                .query_mut("#content-label")
                .map(|q| q.add_class("hidden"));
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(TabsApp::new())
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_app_composes_without_panic() {
        let mut app = TabsApp::new();
        let _root = app.compose();
    }

    #[test]
    fn bindings_declare_add_remove_clear() {
        let app = TabsApp::new();
        let bindings = app.bindings();
        let keys: Vec<(&str, &str)> = bindings
            .iter()
            .map(|b| (b.key.as_str(), b.action.as_str()))
            .collect();
        assert!(keys.iter().any(|(k, _)| *k == "a"), "expected 'a' binding");
        assert!(keys.iter().any(|(k, _)| *k == "r"), "expected 'r' binding");
        assert!(keys.iter().any(|(k, _)| *k == "c"), "expected 'c' binding");
    }

    #[test]
    fn names_list_has_expected_first_entry() {
        assert_eq!(NAMES[0], "Paul Atreidies");
        assert_eq!(NAMES.len(), 8);
    }

    #[test]
    fn next_name_index_cycles() {
        let mut app = TabsApp::new();
        // After 8 additions the index wraps around.
        for _ in 0..NAMES.len() {
            app.next_name_index = (app.next_name_index + 1) % NAMES.len();
        }
        assert_eq!(app.next_name_index, 1);
    }

    /// LIVENESS: pressing `a` runs the `add` handler, appending a new tab to the
    /// `Tabs` widget. We assert on the observable widget state (`tab_count`
    /// 1 -> 2) — the true thing the binding mutates. A dead `a` binding leaves
    /// the count unchanged.
    ///
    /// KNOWN RENDER GAP (DEFERRED): the added tab does NOT appear in the rendered
    /// frame headlessly — the `a` handler calls `request_repaint()` but not
    /// `request_recompose_node(Tabs)`, and `Tabs::add_tab` builds arena children
    /// that need a recompose to become visible (cf. the `progress_bar` ListView
    /// history, which DOES `request_recompose_node` and renders). So
    /// `frame_fingerprint` is unchanged after `a` even though the state advanced.
    /// The binding/state path is live; the tab-bar re-render is the gap.
    #[test]
    fn liveness_add_tab_advances_count() {
        TabsApp::new()
            .run_test(|pilot| {
                let count = |pilot: &Pilot| -> usize {
                    let app = pilot.app();
                    app.query_one_typed::<Tabs>("Tabs")
                        .ok()
                        .and_then(|h| h.read(app, |t| t.tab_count()).ok())
                        .unwrap_or(0)
                };
                assert_eq!(count(pilot), 1, "starts with one tab");
                pilot.press(&["a"])?;
                assert_eq!(count(pilot), 2, "pressing `a` must add a tab");
                Ok(())
            })
            .expect("run_test");
    }
}
