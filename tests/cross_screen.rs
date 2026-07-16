//! Cross-screen widget access (design note "mount_and_cross_screen", Gap 7).
//!
//! Phase B1: the App-level synchronous surface. `&mut App` outside dispatch
//! holds no tree borrow, so `ScreenRef`-addressed queries and mutations are
//! direct: `query_on` / `query_one_on` / `with_widget_mut_on` / `screen_tree`.
//!
//! Phase B2: the handler-level deferred surface. A handler runs while the
//! runtime holds a live `&mut` borrow of the dispatching tree (the dispatch
//! live-borrow invariant), so cross-screen access from handler context rides
//! the deferred command queue: `WidgetCtx::query_one_on` /
//! `ScreenMessageCtx::query_one_on` enqueue a tree-scoped target resolved at
//! drain time, and the `update_via` closure runs against the resolved widget
//! in its OWNING tree.
//!
//! Screens own separate arena trees; before this surface, a pushed modal made
//! every widget beneath it unreachable (queries resolve against the active
//! tree only). These tests pin the screen-addressing rules: `AppRoot` reaches
//! the base tree under any stack, `Name` resolves the topmost match (Python
//! `get_screen` semantics), `Tree` is exact and never survives a pop.

use std::sync::{Arc, Mutex};

use rich_rs::{Console, ConsoleOptions, Segments};
use textual::prelude::*;

/// Base app: a single `#log` Static on the app-root tree.
struct BaseApp;

impl TextualApp for BaseApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("base-initial").id("log"))
    }
}

/// A named, stacked screen carrying one Static whose id and text are its own.
struct NamedScreen {
    screen_name: &'static str,
    body_id: &'static str,
    body_text: &'static str,
}

impl Screen for NamedScreen {
    fn name(&self) -> &str {
        self.screen_name
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(VerticalGroup::new().with_child(Static::new(self.body_text).id(self.body_id)))
    }
}

fn main_screen() -> NamedScreen {
    NamedScreen {
        screen_name: "main",
        body_id: "main-log",
        body_text: "main-initial",
    }
}

fn modal_screen() -> NamedScreen {
    NamedScreen {
        screen_name: "modal",
        body_id: "modal-label",
        body_text: "modal-initial",
    }
}

/// Read a Static's text on any live tree (background trees included) through
/// the public surface: `screen_tree` + `Handle::read_in`.
fn static_text_on(app: &App, screen: ScreenRef<'_>, selector: &str) -> Option<String> {
    let tree = app.screen_tree(screen)?;
    let node = app.query_one_on(screen, selector).ok()?;
    Handle::<Static>::resolve(tree, node)
        .ok()?
        .read_in(tree, |s| s.text().to_string())
        .ok()
}

/// `query_on` / `query_one_on` scope resolution while two screens are stacked:
/// `Active` sees the top modal, `AppRoot` reaches the base tree, `Name`/`Tree`
/// reach the middle screen, and misses are clean errors (`Unmounted` for a
/// dead screen ref, `NoMatch` for a missing selector).
#[test]
fn query_on_resolves_approot_name_and_tree_while_screens_stacked() {
    run_test(BaseApp, |pilot| {
        pilot.app_mut().push_screen(Box::new(main_screen()));
        pilot.pause()?;
        let main_tree_id = pilot
            .app()
            .screen_tree(ScreenRef::Active)
            .expect("main screen tree")
            .tree_id();
        pilot.app_mut().push_screen(Box::new(modal_screen()));
        pilot.pause()?;

        let app = pilot.app();
        // The unscoped surface is active-tree-only: the base #log is invisible.
        assert!(
            app.query_one("#log").is_err(),
            "unscoped query_one must not see the base tree under a modal"
        );
        // Active: the top modal's tree.
        assert!(app.query_one_on(ScreenRef::Active, "#modal-label").is_ok());
        // AppRoot: the base tree, regardless of the stack.
        assert!(app.query_one_on(ScreenRef::AppRoot, "#log").is_ok());
        // Name: the (non-active) middle screen by Screen::name().
        assert!(
            app.query_one_on(ScreenRef::Name("main"), "#main-log")
                .is_ok()
        );
        // Tree: exact tree id of the middle screen.
        assert!(
            app.query_one_on(ScreenRef::Tree(main_tree_id), "#main-log")
                .is_ok()
        );
        // Scopes do not leak into each other.
        assert!(
            matches!(
                app.query_one_on(ScreenRef::Name("main"), "#modal-label"),
                Err(QueryError::NoMatch)
            ),
            "the modal's widget must not resolve on the main screen's tree"
        );
        // A screen name that matches nothing is Unmounted, not NoMatch.
        assert!(
            matches!(
                app.query_one_on(ScreenRef::Name("nope"), "#log"),
                Err(QueryError::Unmounted)
            ),
            "an unknown screen name must resolve to Unmounted"
        );
        Ok(())
    })
    .unwrap();
}

/// `with_widget_mut_on` mutates a background tree synchronously from App-level
/// (Pilot) context: the base `#log` and the middle screen's `#main-log` both
/// update while the modal stays on top, and the base update is visible in the
/// rendered frame once the stack is popped.
#[test]
fn with_widget_mut_on_updates_background_trees_synchronously() {
    run_test(BaseApp, |pilot| {
        pilot.app_mut().push_screen(Box::new(main_screen()));
        pilot.pause()?;
        pilot.app_mut().push_screen(Box::new(modal_screen()));
        pilot.pause()?;

        // Synchronous cross-screen mutation, return value passed through.
        let out = pilot.app_mut().with_widget_mut_on::<Static, _>(
            ScreenRef::AppRoot,
            "#log",
            |s, _ctx| {
                s.update("base-updated");
                42
            },
        );
        assert_eq!(
            out.ok(),
            Some(42),
            "with_widget_mut_on must apply and return"
        );

        pilot
            .app_mut()
            .with_widget_mut_on::<Static, _>(ScreenRef::Name("main"), "#main-log", |s, _ctx| {
                s.update("main-updated");
            })
            .expect("named-screen mutation must apply");

        // State changed immediately (no flush needed), observable while the
        // modal is still the active screen.
        assert_eq!(
            static_text_on(pilot.app(), ScreenRef::AppRoot, "#log").as_deref(),
            Some("base-updated")
        );
        assert_eq!(
            static_text_on(pilot.app(), ScreenRef::Name("main"), "#main-log").as_deref(),
            Some("main-updated")
        );
        // The modal's own widget was never touched.
        assert_eq!(
            static_text_on(pilot.app(), ScreenRef::Active, "#modal-label").as_deref(),
            Some("modal-initial")
        );

        // A wrong downcast is a clean NoMatch, not a panic.
        assert!(
            matches!(
                pilot.app_mut().with_widget_mut_on::<Button, _>(
                    ScreenRef::AppRoot,
                    "#log",
                    |_b, _ctx| {}
                ),
                Err(QueryError::NoMatch)
            ),
            "a downcast miss must be NoMatch"
        );

        // Pop back to the base tree: the mutation must be visible in the frame.
        pilot.app_mut().pop_screen();
        pilot.pause()?;
        pilot.app_mut().pop_screen();
        pilot.pause()?;
        let frame = pilot.app().frame_plain_text();
        assert!(
            frame.contains("base-updated"),
            "popped-to frame must show the cross-screen update, got:\n{frame}"
        );
        Ok(())
    })
    .unwrap();
}

/// Name collisions resolve to the TOPMOST matching screen (top-down search,
/// the Python `get_screen` semantic); the deeper same-named screen stays
/// addressable by its exact tree id.
#[test]
fn name_collision_resolves_topmost_screen() {
    run_test(BaseApp, |pilot| {
        pilot.app_mut().push_screen(Box::new(NamedScreen {
            screen_name: "detail",
            body_id: "tag",
            body_text: "first-detail",
        }));
        pilot.pause()?;
        let first_tree_id = pilot
            .app()
            .screen_tree(ScreenRef::Name("detail"))
            .expect("first detail tree")
            .tree_id();

        pilot.app_mut().push_screen(Box::new(NamedScreen {
            screen_name: "detail",
            body_id: "tag",
            body_text: "second-detail",
        }));
        pilot.pause()?;

        let named_tree_id = pilot
            .app()
            .screen_tree(ScreenRef::Name("detail"))
            .expect("named detail tree")
            .tree_id();
        let active_tree_id = pilot
            .app()
            .screen_tree(ScreenRef::Active)
            .expect("active tree")
            .tree_id();
        assert_eq!(
            named_tree_id, active_tree_id,
            "Name must resolve to the topmost matching screen"
        );
        assert_ne!(named_tree_id, first_tree_id);

        // A Name-addressed mutation lands on the topmost match only.
        pilot
            .app_mut()
            .with_widget_mut_on::<Static, _>(ScreenRef::Name("detail"), "#tag", |s, _ctx| {
                s.update("second-updated");
            })
            .expect("name-addressed mutation must apply");
        assert_eq!(
            static_text_on(pilot.app(), ScreenRef::Name("detail"), "#tag").as_deref(),
            Some("second-updated")
        );
        assert_eq!(
            static_text_on(pilot.app(), ScreenRef::Tree(first_tree_id), "#tag").as_deref(),
            Some("first-detail"),
            "the deeper same-named screen must be untouched"
        );
        Ok(())
    })
    .unwrap();
}

/// A popped screen's tree id resolves to nothing: `screen_tree` is `None` and
/// the query surface degrades to `Unmounted`, never to a different tree.
#[test]
fn popped_screen_tree_id_resolves_to_nothing() {
    run_test(BaseApp, |pilot| {
        pilot.app_mut().push_screen(Box::new(modal_screen()));
        pilot.pause()?;
        let modal_tree_id = pilot
            .app()
            .screen_tree(ScreenRef::Active)
            .expect("modal tree")
            .tree_id();

        pilot.app_mut().pop_screen();
        pilot.pause()?;

        let app = pilot.app();
        assert!(app.screen_tree(ScreenRef::Tree(modal_tree_id)).is_none());
        assert!(matches!(
            app.query_on(ScreenRef::Tree(modal_tree_id), "#modal-label"),
            Err(QueryError::Unmounted)
        ));
        assert!(matches!(
            app.query_one_on(ScreenRef::Name("modal"), "#modal-label"),
            Err(QueryError::Unmounted)
        ));
        // And mutation attempts are clean errors too.
        assert!(matches!(
            pilot.app_mut().with_widget_mut_on::<Static, _>(
                ScreenRef::Tree(modal_tree_id),
                "#modal-label",
                |_s, _ctx| {}
            ),
            Err(QueryError::Unmounted)
        ));
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// Phase B2: handler-level deferred cross-screen updates
// ---------------------------------------------------------------------------

/// The motivating port pattern: a modal whose handler live-updates a widget on
/// the screen beneath it. `ScreenMessageCtx::query_one_on` + `update_via_screen`.
struct UpdaterModal;

impl Screen for UpdaterModal {
    fn name(&self) -> &str {
        "updater-modal"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(VerticalGroup::new().with_child(Button::new("Update base").id("do-update")))
    }

    fn on_button_pressed(
        &mut self,
        pressed: &ButtonPressed,
        _control: NodeId,
        ctx: &mut ScreenMessageCtx,
    ) {
        if pressed.button_id.as_deref() == Some("do-update") {
            let query = ctx.query_one_on::<Static>(ScreenRef::Name("main"), "#main-log");
            query.update_via_screen(ctx, |s, wctx| {
                s.update("updated-from-modal");
                wctx.request_repaint();
            });
            ctx.set_handled();
        }
    }
}

/// Modal-updates-base (design note section 2.6, the headline pattern): while
/// the modal is still on top, its handler's deferred cross-screen update lands
/// on the named screen beneath; after popping, the frame shows it.
#[test]
fn modal_handler_updates_named_screen_beneath_deferred() {
    run_test(BaseApp, |pilot| {
        pilot.app_mut().push_screen(Box::new(main_screen()));
        pilot.pause()?;
        pilot.app_mut().push_screen(Box::new(UpdaterModal));
        pilot.pause()?;

        pilot.click("#do-update")?;
        pilot.pause()?;

        // The modal is still up; the base ("main") screen's widget changed.
        assert_eq!(pilot.app().screen_count(), 2, "modal must still be on top");
        assert_eq!(
            static_text_on(pilot.app(), ScreenRef::Name("main"), "#main-log").as_deref(),
            Some("updated-from-modal"),
            "the deferred cross-screen update must land while the modal is up"
        );
        // The modal's own tree was not touched by the whole-tree-rooted query.
        assert!(
            pilot
                .app()
                .query_one_on(ScreenRef::Active, "#main-log")
                .is_err(),
            "the update must not have leaked a widget into the modal tree"
        );

        // Pop the modal: the revealed frame shows the update.
        pilot.app_mut().pop_screen();
        pilot.pause()?;
        let frame = pilot.app().frame_plain_text();
        assert!(
            frame.contains("updated-from-modal"),
            "revealed frame must show the cross-screen update, got:\n{frame}"
        );
        Ok(())
    })
    .unwrap();
}

/// A focusable widget inside a pushed screen whose key handler updates the
/// APP-ROOT tree via `WidgetCtx::query_one_on` (the widget-handler half of the
/// Phase B2 surface).
struct KeyUpdater;

impl Widget for KeyUpdater {
    fn render(&self, _c: &Console, _o: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "KeyUpdater"
    }

    fn focusable(&self) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, ctx: &mut WidgetCtx) {
        if matches!(event, Event::Key(_)) {
            let query = ctx.query_one_on::<Static>(ScreenRef::AppRoot, "#log");
            query.update_via(ctx, |s, wctx| {
                s.update("updated-by-key");
                wctx.request_repaint();
            });
            ctx.set_handled();
        }
    }
}

struct KeyModal;

impl Screen for KeyModal {
    fn name(&self) -> &str {
        "key-modal"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(VerticalGroup::new().with_child(KeyUpdater))
    }
}

#[test]
fn widget_key_handler_updates_app_root_deferred() {
    run_test(BaseApp, |pilot| {
        pilot.app_mut().push_screen(Box::new(KeyModal));
        pilot.pause()?;

        pilot.press(&["u"])?;
        pilot.pause()?;

        assert_eq!(pilot.app().screen_count(), 1, "modal must still be on top");
        assert_eq!(
            static_text_on(pilot.app(), ScreenRef::AppRoot, "#log").as_deref(),
            Some("updated-by-key"),
            "WidgetCtx::query_one_on must update the app-root tree while covered"
        );

        pilot.app_mut().pop_screen();
        pilot.pause()?;
        let frame = pilot.app().frame_plain_text();
        assert!(
            frame.contains("updated-by-key"),
            "revealed frame must show the update, got:\n{frame}"
        );
        Ok(())
    })
    .unwrap();
}

/// Base app whose `#log` sits below the modal dialog area, so the underlay
/// text is visible through a translucent modal.
struct OffsetLogApp;

impl TextualApp for OffsetLogApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new("underlay-initial").id("log"))
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet("#log { margin-top: 6; }");
        Ok(())
    }
}

/// A translucent modal (ModalScreen default background) with a small top-left
/// dialog carrying the update button, leaving the underlay rows visible.
struct DimUpdaterModal;

impl Screen for DimUpdaterModal {
    fn name(&self) -> &str {
        "dim-updater"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(VerticalGroup::new().with_child(Button::new("Go").id("dim-update")))
    }

    fn css(&self) -> Option<&str> {
        Some("VerticalGroup { width: 12; height: 3; }")
    }

    fn on_button_pressed(
        &mut self,
        pressed: &ButtonPressed,
        _control: NodeId,
        ctx: &mut ScreenMessageCtx,
    ) {
        if pressed.button_id.as_deref() == Some("dim-update") {
            let query = ctx.query_one_on::<Static>(ScreenRef::AppRoot, "#log");
            query.update_via_screen(ctx, |s, wctx| {
                s.update("underlay-updated");
                wctx.request_repaint();
            });
            ctx.set_handled();
        }
    }
}

/// Visible-underlay repaint (design note section 2.6): a cross-screen update
/// beneath a TRANSLUCENT modal repaints in the composited frame immediately,
/// without popping the modal (the compositor re-renders every visible layer).
#[test]
fn translucent_modal_underlay_repaints_without_popping() {
    run_test(OffsetLogApp, |pilot| {
        pilot.resize(40, 12)?;
        pilot.app_mut().push_screen(Box::new(DimUpdaterModal));
        pilot.pause()?;

        // Precondition: the underlay text shows through the translucent modal.
        let before = pilot.app().frame_plain_text();
        assert!(
            before.contains("underlay-initial"),
            "underlay text must be visible through the translucent modal, got:\n{before}"
        );

        pilot.click("#dim-update")?;
        pilot.pause()?;

        assert_eq!(pilot.app().screen_count(), 1, "modal must still be up");
        let after = pilot.app().frame_plain_text();
        assert!(
            after.contains("underlay-updated"),
            "the composited frame must repaint the underlay update while the \
             modal is still up, got:\n{after}"
        );
        assert!(
            !after.contains("underlay-initial"),
            "the stale underlay text must be gone from the frame"
        );
        Ok(())
    })
    .unwrap();
}

/// A modal whose handler enqueues a cross-screen command against its OWN tree
/// (by exact tree id) and dismisses itself in the same handler, plus one
/// against a screen name that never existed. Whatever the pop/flush
/// interleaving, nothing panics and the command against the dead/unknown
/// scope is dropped with a log (the drop-never-panic contract).
struct SelfTargetModal {
    own_tree: Arc<Mutex<u64>>,
}

impl Screen for SelfTargetModal {
    fn name(&self) -> &str {
        "self-target"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(
            VerticalGroup::new()
                .with_child(Static::new("self-static").id("self-static"))
                .with_child(Button::new("Bye").id("dismiss-btn")),
        )
    }

    fn on_button_pressed(
        &mut self,
        pressed: &ButtonPressed,
        _control: NodeId,
        ctx: &mut ScreenMessageCtx,
    ) {
        if pressed.button_id.as_deref() == Some("dismiss-btn") {
            let own_tree = *self.own_tree.lock().unwrap();
            // Targets its own (about-to-pop) tree by exact id.
            let self_query = ctx.query_one_on::<Static>(ScreenRef::Tree(own_tree), "#self-static");
            self_query.update_via_screen(ctx, |s, _| s.update("self-touched"));
            // Targets a screen that never existed: must drop, never panic.
            let ghost_query = ctx.query_one_on::<Static>(ScreenRef::Name("ghost"), "#anything");
            ghost_query.update_via_screen(ctx, |s, _| s.update("never-lands"));
            ctx.dismiss_none();
        }
    }
}

#[test]
fn screen_popped_or_unknown_scope_drops_without_panic() {
    run_test(BaseApp, |pilot| {
        let own_tree = Arc::new(Mutex::new(0u64));
        pilot.app_mut().push_screen(Box::new(SelfTargetModal {
            own_tree: own_tree.clone(),
        }));
        pilot.pause()?;
        *own_tree.lock().unwrap() = pilot
            .app()
            .screen_tree(ScreenRef::Active)
            .expect("modal tree")
            .tree_id();

        // Dismiss + enqueue in the same handler: no panic across pop/flush.
        pilot.click("#dismiss-btn")?;
        pilot.pause()?;

        assert_eq!(pilot.app().screen_count(), 0, "modal must have dismissed");
        // The app is fully functional afterwards: base tree intact + rendered.
        assert_eq!(
            static_text_on(pilot.app(), ScreenRef::Active, "#log").as_deref(),
            Some("base-initial")
        );
        let frame = pilot.app().frame_plain_text();
        assert!(
            frame.contains("base-initial"),
            "base frame must render after the dismissal, got:\n{frame}"
        );
        Ok(())
    })
    .unwrap();
}
