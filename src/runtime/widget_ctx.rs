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

    /// Enqueue an add-class on this widget's own node, applied by the flush
    /// (Python `self.add_class(name)`).
    pub fn add_class(&mut self, class: &str) {
        enqueue_widget_command(WidgetCommand::AddClass {
            target: CommandTarget::Node(self.node_id()),
            class: class.to_string(),
        });
    }

    /// Enqueue a remove-class on this widget's own node, applied by the flush.
    pub fn remove_class(&mut self, class: &str) {
        enqueue_widget_command(WidgetCommand::RemoveClass {
            target: CommandTarget::Node(self.node_id()),
            class: class.to_string(),
        });
    }

    /// Enqueue an add (when `on`) or remove of `class` on this widget's own node.
    ///
    /// Footgun closer: `ReactiveCtx::set_class` (reachable via `Deref`) queues the
    /// op on the reactive ctx instead of the command queue that
    /// [`add_class`](Self::add_class)/[`remove_class`](Self::remove_class) use —
    /// this inherent shadow keeps every WidgetCtx class op on the ONE command path.
    pub fn set_class(&mut self, on: bool, class: &str) {
        if on {
            self.add_class(class);
        } else {
            self.remove_class(class);
        }
    }

    /// Enqueue an add-class on an arbitrary node (resolved at drain). Footgun
    /// closer for `ReactiveCtx::add_class_to` (routes through the command queue).
    pub fn add_class_to(&mut self, node: crate::node_id::NodeId, class: &str) {
        enqueue_widget_command(WidgetCommand::AddClass {
            target: CommandTarget::Node(node),
            class: class.to_string(),
        });
    }

    /// Enqueue a remove-class on an arbitrary node (resolved at drain). Footgun
    /// closer for `ReactiveCtx::remove_class_from`.
    pub fn remove_class_from(&mut self, node: crate::node_id::NodeId, class: &str) {
        enqueue_widget_command(WidgetCommand::RemoveClass {
            target: CommandTarget::Node(node),
            class: class.to_string(),
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
            target: CommandTarget::Node(self.node_id()),
            apply: make_update_apply::<W, F>(f),
        });
    }
}

/// Wrap a typed closure into the erased `apply` the flush runs, capturing the
/// concrete type `W` AT ENQUEUE and downcasting the resolved widget to it AT
/// DRAIN (the proven `with_widget_mut_as` pattern). A downcast miss (the target
/// resolved to a different concrete type) logs loudly and drops — a user bug,
/// never a panic.
fn make_update_apply<W, F>(f: F) -> Box<dyn FnOnce(&mut dyn Widget, &mut WidgetCtx) + Send>
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
