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

use super::commands::{CommandTarget, WidgetCommand, enqueue_widget_command};
use crate::event::WidgetCtx;
use crate::handle::Handle;
use crate::widgets::Widget;

/// A deferred, typed single-widget query returned by [`WidgetCtx::query_one`] /
/// [`WidgetCtx::query_one_id`]. Its target is resolved at drain time; call
/// [`WidgetQuery::update_via`] to enqueue an update against the resolved widget.
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
}

impl<W: Widget> Handle<W> {
    /// Enqueue a deferred update of the handled widget via a `WidgetCtx`. Unlike
    /// [`Handle::update`] (immediate, `&mut App`), this defers to the shared
    /// flush so it is callable from inside a handler while the tree is borrowed.
    /// Target is the handle's already-resolved node.
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
