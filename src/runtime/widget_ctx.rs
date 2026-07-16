//! `WidgetCtx` query/update surface (WidgetCtx build, sub-step 2).
//!
//! The [`WidgetCtx`](crate::event::WidgetCtx) type lives in `event/mod.rs` (it
//! carries the event-scoped flags and `DerefMut`s to the reactive recording
//! surface). This module adds the *cross-node* surface — `query_one`, the
//! deferred [`WidgetQuery`], `add_class`/`remove_class`, and `Handle::update_via`
//! — which all enqueue [`WidgetCommand`]s applied later by the shared flush
//! (`src/runtime/commands.rs`).
//!
//! Why deferred: a handler runs while the runtime holds a live `&mut` borrow of
//! the widget tree (`routing.rs:169-182`), so it cannot resolve a *different*
//! node or mutate it in place. `query_one` therefore does NOT resolve at call
//! time — it captures a [`CommandTarget`] resolved at drain, when no tree borrow
//! is held (and after any earlier command in the flush has mounted its target).

use std::any::{Any, TypeId};
use std::marker::PhantomData;
use std::time::Duration;

use super::TimerHandle;
use super::commands::{
    CommandTarget, TimerTick, WidgetCommand, WidgetTimerCallback, alloc_widget_timer_id,
    enqueue_widget_command,
};
use crate::event::WidgetCtx;
use crate::handle::Handle;
use crate::widgets::Widget;

/// A deferred, typed single-widget query returned by [`WidgetCtx::query_one`] /
/// [`WidgetCtx::query_one_id`]. Its target is resolved at drain time; call
/// [`WidgetQuery::update_via`] to enqueue an update against the resolved widget.
///
/// `WidgetQuery` vs [`Handle`]: a `WidgetQuery` is an **unresolved command
/// target** — a selector/type rooted at the querying widget, resolved when the
/// flush drains (the tree is borrowed during the handler, so it *cannot* resolve
/// now). A [`Handle`] is a **resolved identity** — a concrete `(NodeId, tree_id)`
/// obtained earlier (e.g. from a `HandleSlot`). Both expose `update_via`; the
/// `Handle` form skips the drain-time selector match.
pub struct WidgetQuery<W: Widget> {
    target: CommandTarget,
    _marker: PhantomData<fn() -> W>,
}

impl<W: Widget> WidgetQuery<W> {
    fn new(target: CommandTarget) -> Self {
        Self {
            target,
            _marker: PhantomData,
        }
    }

    /// Enqueue a deferred update of the queried widget. During the shared flush
    /// the target is resolved, downcast to `W`, and the closure runs with the
    /// resolved `&mut W` plus a fresh [`WidgetCtx`]. Returns `()` — this is
    /// deferred; do not read widget state back from the handler (read-then-decide
    /// inside the closure instead).
    ///
    /// WARNING: messages posted from inside the closure (via the fresh
    /// `WidgetCtx`) are **not routed** until the `PostUp` command lands — the
    /// flush currently debug-logs and drops them.
    ///
    /// `_ctx` is a **deliberate capability token**: requiring a `&mut WidgetCtx`
    /// proves the caller is inside a handler (the only place cross-node updates
    /// are legal) and reserves room to scope the enqueue to the caller later. It
    /// is intentionally unused today — do not "clean it up".
    pub fn update_via<F>(self, _ctx: &mut WidgetCtx, f: F)
    where
        F: FnOnce(&mut W, &mut WidgetCtx) + Send + 'static,
    {
        enqueue_widget_command(WidgetCommand::UpdateWidget {
            target: self.target,
            apply: make_update_apply::<W, F>(f),
        });
    }
}

impl<'a> WidgetCtx<'a> {
    /// Query the single descendant of this widget whose concrete type is `W`
    /// (Python `self.query_one(W)`), resolved at drain time. Rooted at this
    /// widget's node — descendants only, not self.
    ///
    /// Matches the **exact concrete Rust type** `W` by `TypeId` — there is no
    /// subclass/supertrait/style-alias matching (a `Button` query never matches a
    /// user newtype wrapping a Button). Use [`query_one_id`](Self::query_one_id)
    /// for CSS-selector matching.
    pub fn query_one<W: Widget>(&self) -> WidgetQuery<W> {
        WidgetQuery::new(CommandTarget::TypeMatch {
            root: self.node_id(),
            ty: TypeId::of::<W>(),
        })
    }

    /// Query the single descendant matching a CSS selector (e.g. `"#disp"`),
    /// resolved at drain time. Rooted at this widget's node.
    pub fn query_one_id<W: Widget>(&self, selector: &str) -> WidgetQuery<W> {
        WidgetQuery::new(CommandTarget::Selector {
            root: self.node_id(),
            sel: selector.to_string(),
        })
    }

    /// Add a CSS class to this widget's own node (Python `self.add_class(name)`).
    ///
    /// RA2.3: enqueues a deferred [`WidgetCommand::AddClass`] applied by the
    /// shared flush (`tree.add_class` + layout invalidation) — the ONE deferred
    /// mechanism, replacing the RA2.2-interim `EventCtx`/`DispatchOutcome`
    /// class-op side-channel. Because both the live loop and headless pump run the
    /// command flush before render, the visible result is identical to the former
    /// `absorb_outcome` path. This inherent method shadows `ReactiveCtx::add_class`
    /// (reachable via `Deref`, which only sets reactive flags).
    pub fn add_class(&mut self, class: &str) {
        enqueue_widget_command(WidgetCommand::AddClass {
            target: self_node_target(self.node_id()),
            class: class.to_string(),
        });
    }

    /// Remove a CSS class from this widget's own node (command-queue path).
    pub fn remove_class(&mut self, class: &str) {
        enqueue_widget_command(WidgetCommand::RemoveClass {
            target: self_node_target(self.node_id()),
            class: class.to_string(),
        });
    }

    /// Add (when `on`) or remove `class` on this widget's own node (command queue).
    ///
    /// Footgun closer: shadows `ReactiveCtx::set_class` (reachable via `Deref`),
    /// which only sets reactive flags — this keeps every WidgetCtx class op on the
    /// one command-queue path.
    pub fn set_class(&mut self, on: bool, class: &str) {
        if on {
            self.add_class(class);
        } else {
            self.remove_class(class);
        }
    }

    /// Add a CSS class to an arbitrary node (command-queue path). Footgun closer
    /// for `ReactiveCtx::add_class_to`.
    pub fn add_class_to(&mut self, node: crate::node_id::NodeId, class: &str) {
        enqueue_widget_command(WidgetCommand::AddClass {
            target: self_node_target(node),
            class: class.to_string(),
        });
    }

    /// Remove a CSS class from an arbitrary node (command-queue path). Footgun
    /// closer for `ReactiveCtx::remove_class_from`.
    pub fn remove_class_from(&mut self, node: crate::node_id::NodeId, class: &str) {
        enqueue_widget_command(WidgetCommand::RemoveClass {
            target: self_node_target(node),
            class: class.to_string(),
        });
    }

    /// Apply a closure to this widget's own inline styles (Python
    /// `widget.styles.<prop> = v`). Deferred: enqueues a
    /// [`WidgetCommand::UpdateStyles`] applied by the shared flush against the
    /// arena node record. This is the post-mount inline-style write path — the
    /// widget's node seed is drained at mount, so mutating the seed after mount is
    /// invisible; route style writes here so they reach layout/render (retires the
    /// former inline-style write-through staging hook).
    pub fn update_styles<F>(&mut self, f: F)
    where
        F: FnOnce(&mut crate::widgets::WidgetStyles) + Send + 'static,
    {
        enqueue_widget_command(WidgetCommand::UpdateStyles {
            target: self_node_target(self.node_id()),
            apply: Box::new(f),
        });
    }

    /// Register a **widget-owned** repeating interval timer on this widget's node
    /// (Python `self.set_interval`). The callback receives the concrete widget
    /// `&mut W` (downcast at fire) and a fresh `WidgetCtx`, so a reactive `set_*`
    /// inside it flows to that node's watchers. With `paused = true` it starts
    /// paused; control it via the returned [`TimerHandle`]'s
    /// `pause()`/`resume()`/`stop()`.
    ///
    /// Runs on the SAME `TimerRuntime` as app timers, so `Pilot::advance_clock`
    /// drives it deterministically. The timer is purged when its node unmounts.
    /// Registration is deferred (enqueued, applied by the next flush); the handle
    /// is valid immediately (its id is pre-allocated).
    pub fn set_interval<W, F>(&mut self, interval: Duration, paused: bool, mut f: F) -> TimerHandle
    where
        W: Widget,
        F: FnMut(&mut W, &mut WidgetCtx, TimerTick) + Send + 'static,
    {
        let timer_id = alloc_widget_timer_id();
        let callback: WidgetTimerCallback =
            Box::new(move |widget: &mut dyn Widget, wctx: &mut WidgetCtx, tick: TimerTick| {
                match (widget as &mut dyn Any).downcast_mut::<W>() {
                    Some(concrete) => f(concrete, wctx, tick),
                    None => crate::debug::debug_render(&format!(
                        "[widget-timer] fire downcast miss: node is not {}",
                        std::any::type_name::<W>()
                    )),
                }
            });
        enqueue_widget_command(WidgetCommand::RegisterTimer {
            node: self.node_id(),
            timer_id,
            interval,
            paused,
            repeat: None,
            callback,
        });
        TimerHandle::from_id(timer_id)
    }

    /// Schedule a **widget-owned one-shot timer** on this widget's node
    /// (Python `self.set_timer`): `f` runs exactly once, `delay` after
    /// registration, with the concrete widget `&mut W` (downcast at fire) and a
    /// fresh `WidgetCtx`. This is the public counterpart of
    /// [`set_interval`](Self::set_interval) for one-shot work; no
    /// `event_ctx_mut()` reach-through is needed.
    ///
    /// Runs on the SAME `TimerRuntime` as app timers, so `Pilot::advance_clock`
    /// drives it deterministically. The returned [`TimerHandle`] can
    /// `pause()`/`resume()`/`stop()` it before it fires; after the single fire
    /// the timer is dropped. The timer is purged if its node unmounts first.
    /// Registration is deferred (enqueued, applied by the next flush); the
    /// handle is valid immediately (its id is pre-allocated).
    pub fn set_timer<W, F>(&mut self, delay: Duration, f: F) -> TimerHandle
    where
        W: Widget,
        F: FnOnce(&mut W, &mut WidgetCtx, TimerTick) + Send + 'static,
    {
        let timer_id = alloc_widget_timer_id();
        // The runtime callback type is `FnMut` (repeating timers); adapt the
        // one-shot `FnOnce` through an `Option` take. The runtime never calls
        // it twice (`repeat = Some(1)` removes the timer at its first fire).
        let mut once = Some(f);
        let callback: WidgetTimerCallback =
            Box::new(move |widget: &mut dyn Widget, wctx: &mut WidgetCtx, tick: TimerTick| {
                let Some(f) = once.take() else {
                    return;
                };
                match (widget as &mut dyn Any).downcast_mut::<W>() {
                    Some(concrete) => f(concrete, wctx, tick),
                    None => crate::debug::debug_render(&format!(
                        "[widget-timer] one-shot fire downcast miss: node is not {}",
                        std::any::type_name::<W>()
                    )),
                }
            });
        enqueue_widget_command(WidgetCommand::RegisterTimer {
            node: self.node_id(),
            timer_id,
            interval: delay,
            paused: false,
            repeat: Some(1),
            callback,
        });
        TimerHandle::from_id(timer_id)
    }
}

impl<W: Widget> Handle<W> {
    /// Enqueue a deferred update of the handled widget via a `WidgetCtx`. Unlike
    /// [`Handle::update`] (immediate, `&mut App`), this defers to the shared
    /// flush so it is callable from inside a handler while the tree is borrowed.
    /// Target is the handle's already-resolved node.
    ///
    /// Same caveats as [`WidgetQuery::update_via`]: closure-posted messages are
    /// not routed yet (PostUp), and `_ctx` is a deliberate capability token
    /// (proves handler context) — intentionally unused, do not remove.
    pub fn update_via<F>(self, _ctx: &mut WidgetCtx, f: F)
    where
        F: FnOnce(&mut W, &mut WidgetCtx) + Send + 'static,
    {
        enqueue_widget_command(WidgetCommand::UpdateWidget {
            // Stamp the handle's carried tree identity: screens own separate
            // trees whose slotmap keys collide, so an unstamped NodeId drained
            // while a different tree is active would mutate an unrelated
            // widget. A stamped target whose tree is not the active tree at
            // drain is dropped with a debug log (cross-screen apply is a
            // deferred feature).
            target: CommandTarget::Node {
                node: self.node_id(),
                tree: Some(self.tree_id()),
            },
            apply: make_update_apply::<W, F>(f),
        });
    }
}

/// Node target for ctx-path enqueues (self / arbitrary-node class ops, style
/// writes): stamped with the DISPATCHING tree's identity when inside a
/// dispatch scope, so the command cannot alias a same-keyed node of a
/// different tree if the active tree changes before the drain. Outside a
/// dispatch scope (`None`) it resolves against the active tree at drain,
/// exactly as before the tree-stamp.
fn self_node_target(node: crate::node_id::NodeId) -> CommandTarget {
    CommandTarget::Node {
        node,
        tree: crate::runtime::dispatch_ctx::dispatch_tree_id(),
    }
}

/// Wrap a typed closure into the erased `apply` the flush runs, capturing the
/// concrete type `W` AT ENQUEUE and downcasting the resolved widget to it AT
/// DRAIN (the proven `with_widget_mut_as` pattern). A downcast miss (the target
/// resolved to a different concrete type) logs loudly and drops — a user bug,
/// never a panic.
fn make_update_apply<W, F>(f: F) -> super::commands::WidgetApply
where
    W: Widget,
    F: FnOnce(&mut W, &mut WidgetCtx) + Send + 'static,
{
    Box::new(move |widget: &mut dyn Widget, wctx: &mut WidgetCtx| {
        match (widget as &mut dyn Any).downcast_mut::<W>() {
            Some(concrete) => f(concrete, wctx),
            None => crate::debug::debug_render(&format!(
                "[widget-command] UpdateWidget downcast miss: resolved target is not {}",
                std::any::type_name::<W>()
            )),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::node_id::NodeId;
    use crate::runtime::commands::take_widget_commands;
    use crate::runtime::types::PendingInvalidation;
    use crate::widget_tree::WidgetTree;
    use rich_rs::{Console, ConsoleOptions, Segments};

    /// Minimal widget carrying observable state.
    struct Probe {
        value: u32,
    }

    impl Widget for Probe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            "Probe"
        }
    }

    fn build_probe_tree(value: u32) -> (WidgetTree, NodeId) {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(Probe { value }));
        (tree, root)
    }

    fn test_app_with_tree(tree: WidgetTree) -> crate::runtime::App {
        let mut app = crate::runtime::App::new().expect("app should initialize for runtime tests");
        app.widget_tree = Some(tree);
        app
    }

    fn flush_commands(app: &mut crate::runtime::App) {
        let mut pending = PendingInvalidation::default();
        for cmd in take_widget_commands() {
            app.apply_widget_command(cmd, &mut pending);
        }
    }

    fn probe_value(tree: &WidgetTree, node: NodeId) -> u32 {
        let n = tree.get(node).expect("probe node present");
        (n.widget.as_ref() as &dyn Any)
            .downcast_ref::<Probe>()
            .expect("probe downcast")
            .value
    }

    /// Aliasing regression (design note section 2.1 / Phase B0): a `Handle`
    /// from tree A must NOT mutate tree B's widget at the same slotmap key when
    /// B is the tree commands resolve against at drain time. Before the
    /// tree-stamp fix, `Handle::update_via` dropped the handle's `tree_id` and
    /// the command passed `contains()` against the unrelated tree.
    #[test]
    fn handle_update_via_does_not_alias_across_trees() {
        let _ = take_widget_commands();

        // Tree A (NOT installed in the app) and a handle to its root.
        let (tree_a, root_a) = build_probe_tree(1);
        let handle_a = crate::handle::Handle::<Probe>::resolve(&tree_a, root_a).unwrap();

        // Tree B, independently built: same insert order, same slotmap key.
        let (tree_b, root_b) = build_probe_tree(2);
        assert_eq!(
            handle_a.node_id(),
            root_b,
            "precondition: independent trees allocate identical NodeIds"
        );

        // B is the active tree at drain time.
        let mut app = test_app_with_tree(tree_b);

        let mut ev = EventCtx::default();
        let mut wctx = WidgetCtx::new(handle_a.node_id(), &mut ev);
        handle_a.update_via(&mut wctx, |p, _| p.value = 99);
        drop(wctx);

        flush_commands(&mut app);

        let tree = app.widget_tree.as_ref().unwrap();
        assert_eq!(
            probe_value(tree, root_b),
            2,
            "tree B's widget must not be mutated by a handle from tree A"
        );
    }

    /// The stamped path still applies when the handle's owning tree IS the tree
    /// commands resolve against (the normal same-tree case).
    #[test]
    fn handle_update_via_applies_in_owning_tree() {
        let _ = take_widget_commands();

        let (tree_a, root_a) = build_probe_tree(1);
        let handle_a = crate::handle::Handle::<Probe>::resolve(&tree_a, root_a).unwrap();
        let mut app = test_app_with_tree(tree_a);

        let mut ev = EventCtx::default();
        let mut wctx = WidgetCtx::new(handle_a.node_id(), &mut ev);
        handle_a.update_via(&mut wctx, |p, _| p.value = 99);
        drop(wctx);

        flush_commands(&mut app);

        let tree = app.widget_tree.as_ref().unwrap();
        assert_eq!(probe_value(tree, root_a), 99);
    }

    struct ModalScreenStub;

    impl crate::screen::Screen for ModalScreenStub {
        fn name(&self) -> &str {
            "modal-stub"
        }

        fn compose(&self) -> Box<dyn Widget> {
            Box::new(Probe { value: 500 })
        }
    }

    /// A stamped target whose owning tree is still alive in the app but NOT the
    /// active tree (a screen is pushed on top) is dropped: neither the owning
    /// tree nor the screen's same-keyed node is mutated. Cross-screen APPLY is
    /// Phase B2; B0 only guarantees no aliasing.
    #[test]
    fn handle_update_via_drops_while_other_screen_active() {
        let _ = take_widget_commands();

        let (tree_a, root_a) = build_probe_tree(1);
        let tree_a_id = tree_a.tree_id();
        let handle_a = crate::handle::Handle::<Probe>::resolve(&tree_a, root_a).unwrap();
        let mut app = test_app_with_tree(tree_a);

        app.push_screen(Box::new(ModalScreenStub));
        // Screen build may enqueue its own commands; flush them first.
        flush_commands(&mut app);
        assert_ne!(
            app.tree_by_id(tree_a_id)
                .map(crate::widget_tree::WidgetTree::tree_id),
            None,
            "precondition: tree A is alive in the app"
        );

        let mut ev = EventCtx::default();
        let mut wctx = WidgetCtx::new(handle_a.node_id(), &mut ev);
        handle_a.update_via(&mut wctx, |p, _| p.value = 99);
        drop(wctx);

        flush_commands(&mut app);

        // Owning (non-active) tree untouched: the command was dropped, not
        // applied cross-screen.
        let tree_a_ref = app.tree_by_id(tree_a_id).expect("tree A alive");
        assert_eq!(probe_value(tree_a_ref, root_a), 1);
        // The active screen tree's same-keyed Probe (if any) untouched too.
        let active = app.active_widget_tree().expect("active screen tree");
        assert_ne!(active.tree_id(), tree_a_id);
        if let Some(n) = active.get(handle_a.node_id())
            && let Some(p) = (n.widget.as_ref() as &dyn Any).downcast_ref::<Probe>()
        {
            assert_eq!(p.value, 500, "screen tree widget must not be aliased");
        }
    }

    /// Back-compat pin: an unstamped (`tree: None`) Node target resolves
    /// against the active tree exactly as before the tree-stamp.
    #[test]
    fn unstamped_node_target_resolves_against_active_tree() {
        let _ = take_widget_commands();

        let (tree, root) = build_probe_tree(1);
        let mut app = test_app_with_tree(tree);

        crate::runtime::commands::enqueue_widget_command(
            crate::runtime::commands::WidgetCommand::AddClass {
                target: crate::runtime::commands::CommandTarget::Node {
                    node: root,
                    tree: None,
                },
                class: "active".to_string(),
            },
        );
        flush_commands(&mut app);

        let tree = app.widget_tree.as_ref().unwrap();
        assert!(tree.has_class(root, "active"));
    }

    /// The ctx enqueue paths stamp the DISPATCHING tree's id when inside a
    /// dispatch scope (and `None` outside one).
    #[test]
    fn ctx_class_ops_stamp_dispatching_tree() {
        let _ = take_widget_commands();

        let (_tree, root) = build_probe_tree(1);

        // Outside any dispatch scope: unstamped (active-tree resolution).
        let mut ev = EventCtx::default();
        let mut wctx = WidgetCtx::new(root, &mut ev);
        wctx.add_class("outside");
        drop(wctx);

        // Inside a dispatch scope: stamped with the dispatching tree's id.
        {
            let _guard = crate::runtime::dispatch_ctx::set_dispatch_tree(4242);
            let mut ev = EventCtx::default();
            let mut wctx = WidgetCtx::new(root, &mut ev);
            wctx.add_class("inside");
        }

        let stamps: Vec<Option<u64>> = take_widget_commands()
            .into_iter()
            .map(|cmd| match cmd {
                crate::runtime::commands::WidgetCommand::AddClass {
                    target: crate::runtime::commands::CommandTarget::Node { tree, .. },
                    ..
                } => tree,
                _ => panic!("expected AddClass Node commands"),
            })
            .collect();
        assert_eq!(stamps, vec![None, Some(4242)]);
    }
}
