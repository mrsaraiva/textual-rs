//! Cross-screen widget access, App-level synchronous surface (Phase B1 of the
//! mount-and-cross-screen design, section 2.3 Tier 1).
//!
//! Every pushed screen owns a completely separate arena tree
//! (`ScreenEntry.widget_tree`), and the default query/command surface resolves
//! against ONE tree, the active one (`App::active_widget_tree`). This module
//! adds screen-addressed variants: a [`ScreenRef`] names a tree (active, app
//! root, screen name, or exact `WidgetTree::tree_id`), and the `*_on` methods
//! query/mutate that tree directly.
//!
//! Why this tier is synchronous: `&mut App` outside dispatch holds no tree
//! borrow, it owns every tree, so reads and writes here are direct (Python
//! parity: `app.get_screen("main").query_one("#log")` is a plain method call).
//! Handler context can never do this (the dispatch live-borrow invariant); the
//! handler-side counterpart is the deferred `WidgetCtx::query_one_on` /
//! `ScreenMessageCtx::query_one_on` surface (Phase B2, scoped
//! `CommandTarget`s), which stays on the command queue.

use std::any::Any;

use super::{App, DomQuery};
use crate::event::WidgetCtx;
use crate::node_id::NodeId;
use crate::widget_tree::{QueryError, WidgetTree};
use crate::widgets::Widget;

/// How a screen (widget tree) is addressed by the cross-screen APIs.
///
/// Resolution rules:
/// - [`Active`](Self::Active): the top screen-stack entry's tree, else the app
///   root tree (exactly [`App::query`]'s implicit scope).
/// - [`AppRoot`](Self::AppRoot): the base app tree, regardless of any pushed
///   screens.
/// - [`Name`](Self::Name): the topmost stacked screen whose [`crate::screen::Screen::name`]
///   or mode name (from `switch_mode`) equals the given string; name
///   collisions resolve to the topmost match (the Python `get_screen`
///   semantic). NOTE: the default `Screen::name()` is `"Screen"`, so name
///   addressing is only as reliable as the port's naming discipline; prefer
///   [`Tree`](Self::Tree) when a tree id is available.
/// - [`Tree`](Self::Tree): the live tree with this exact
///   [`WidgetTree::tree_id`] (process-unique, collision-free), searched over
///   the screen stack plus the app root. A popped screen's id resolves to
///   nothing, never to a different screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenRef<'a> {
    /// The active tree (top screen, else app root).
    Active,
    /// The base app-root tree, even while screens are pushed on top.
    AppRoot,
    /// The topmost stacked screen with this `Screen::name()` or mode name.
    Name(&'a str),
    /// The tree with this exact `WidgetTree::tree_id()`.
    Tree(u64),
}

impl App {
    /// Resolve a [`ScreenRef`] to the `tree_id` of a live tree, or `None` when
    /// no live tree matches (e.g. the named screen was popped).
    pub(crate) fn resolve_screen_ref(&self, screen: ScreenRef<'_>) -> Option<u64> {
        match screen {
            ScreenRef::Active => self.active_widget_tree().map(WidgetTree::tree_id),
            ScreenRef::AppRoot => self.widget_tree.as_ref().map(WidgetTree::tree_id),
            ScreenRef::Name(name) => self.screen_stack.find_tree_id_by_name(name),
            ScreenRef::Tree(id) => self.tree_by_id(id).map(WidgetTree::tree_id),
        }
    }

    /// Resolve a screen reference to its widget tree (Python
    /// `app.get_screen(name)` / `app.screen`). `None` when no live tree
    /// matches.
    pub fn screen_tree(&self, screen: ScreenRef<'_>) -> Option<&WidgetTree> {
        let tree_id = self.resolve_screen_ref(screen)?;
        self.tree_by_id(tree_id)
    }

    /// Mutable variant of [`Self::screen_tree`].
    pub fn screen_tree_mut(&mut self, screen: ScreenRef<'_>) -> Option<&mut WidgetTree> {
        let tree_id = self.resolve_screen_ref(screen)?;
        self.tree_by_id_mut(tree_id)
    }

    /// Query nodes on the addressed screen's tree with a CSS selector, same
    /// shape as [`App::query`] but screen-scoped. Python:
    /// `app.get_screen("main").query("...")`.
    ///
    /// `Err(QueryError::Unmounted)` when the screen reference resolves to no
    /// live tree (e.g. the named screen was popped).
    pub fn query_on(
        &self,
        screen: ScreenRef<'_>,
        selector: &str,
    ) -> std::result::Result<DomQuery, QueryError> {
        Self::validate_selector(selector)?;
        let tree = self.screen_tree(screen).ok_or(QueryError::Unmounted)?;
        tree.query(selector).map(DomQuery::from_nodes)
    }

    /// Query the first node matching `selector` on the addressed screen's tree
    /// (screen-scoped [`App::query_one`]). Python:
    /// `app.get_screen("main").query_one("#log")`.
    pub fn query_one_on(
        &self,
        screen: ScreenRef<'_>,
        selector: &str,
    ) -> std::result::Result<NodeId, QueryError> {
        self.query_on(screen, selector)?.first()
    }

    /// Query one widget on the addressed screen's tree and mutate it
    /// synchronously, downcast to `W`, with a fresh [`WidgetCtx`]. Python:
    /// `app.get_screen("main").query_one("#log", Log).write(...)`.
    ///
    /// Routes through the shared scoped-node-update path
    /// (`run_on_node_widget_r_in`), so the update converges identically to the
    /// deferred command path: dispatch-recipient guard, owning-tree stamp for
    /// commands enqueued from the closure, reactive fixpoint when the tree is
    /// the active one, and EventCtx absorption. A mutation on a non-active
    /// tree requests a full relayout/repaint so the compositor repaints every
    /// visible layer (an update behind a translucent screen shows immediately;
    /// behind an opaque screen it is state-only until reveal, both matching
    /// Python).
    ///
    /// First-cut caveats for non-active trees (Phase B3 follow-ups): reactive
    /// `watch_*` dispatch and messages posted from the closure are dropped
    /// with a debug log; direct widget mutation, class ops, and repaint are
    /// fully applied.
    ///
    /// Errors: `Unmounted` when the screen reference resolves to no live
    /// tree, `NoMatch` when the selector misses or the matched widget is not
    /// a `W`.
    pub fn with_widget_mut_on<W: Widget + 'static, R>(
        &mut self,
        screen: ScreenRef<'_>,
        selector: &str,
        f: impl FnOnce(&mut W, &mut WidgetCtx) -> R,
    ) -> std::result::Result<R, QueryError> {
        let tree_id = self
            .resolve_screen_ref(screen)
            .ok_or(QueryError::Unmounted)?;
        let node = {
            let tree = self.tree_by_id(tree_id).ok_or(QueryError::Unmounted)?;
            DomQuery::from_nodes(tree.query(selector)?).first()?
        };
        let mut pending = crate::runtime::types::PendingInvalidation::default();
        let result = self
            .run_on_node_widget_r_in(
                Some(tree_id),
                node,
                |widget, ctx| {
                    (widget as &mut dyn Any)
                        .downcast_mut::<W>()
                        .map(|w| f(w, ctx))
                },
                &mut pending,
            )
            .flatten();
        // This entry point has no live frame `pending` to merge into (same as
        // `with_widget_mut`): promote any absorbed layout/style invalidation to
        // the loop-level force flag so the next iteration relayouts, and any
        // content invalidation to a full clear-equivalent repaint.
        if pending.flags.layout || pending.flags.style {
            self.pending_force_relayout = true;
        } else if pending.is_dirty() {
            self.request_query_refresh(&[node]);
        }
        result.ok_or(QueryError::NoMatch)
    }
}
