//! Reactive attribute system for textual-rs.
//!
//! Provides automatic change detection, repaint/layout invalidation, and
//! watcher dispatch for widget fields annotated with `#[reactive]` or `#[var]`.
//!
//! # Overview
//!
//! This module defines the core types that power the reactive system:
//!
//! - [`ReactiveFlags`] — controls what happens when a reactive field changes
//! - [`ReactiveChange`] — records a single field change with old/new values
//! - [`ReactiveCtx`] — context passed to setters, accumulates changes
//! - [`ReactiveWidget`] — trait implemented by `#[derive(Reactive)]`
//!
//! # Usage
//!
//! ```ignore
//! use textual_macros::Reactive;
//!
//! #[derive(Reactive)]
//! struct MyWidget {
//!     #[reactive]
//!     label: String,
//!
//!     #[reactive(layout)]
//!     size: usize,
//!
//!     #[var]
//!     counter: u32,
//! }
//! ```

use crate::node_id::NodeId;
use std::any::Any;
use std::cell::RefCell;

/// Flags controlling what happens when a reactive field changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReactiveFlags {
    /// Request repaint on change (default for `#[reactive]`).
    pub repaint: bool,
    /// Request layout invalidation on change (`#[reactive(layout)]`).
    pub layout: bool,
    /// Call watcher on mount (default for `#[reactive]`, not `#[var]`).
    pub init: bool,
    /// Fire watchers even when old value equals new value.
    ///
    /// Matches Python's `reactive(always_update=True)`. Used for fields like
    /// `Tree.cursor_line` where setting the same value should still trigger
    /// side effects (e.g. scroll-into-view, repaint).
    pub always_update: bool,
}

impl Default for ReactiveFlags {
    fn default() -> Self {
        Self {
            repaint: true,
            layout: false,
            init: true,
            always_update: false,
        }
    }
}

impl ReactiveFlags {
    /// Flags for `#[reactive]`: repaint on change, call watcher on init.
    pub const fn reactive() -> Self {
        Self {
            repaint: true,
            layout: false,
            init: true,
            always_update: false,
        }
    }

    /// Flags for `#[reactive(layout)]`: repaint + layout on change, call watcher on init.
    pub const fn reactive_layout() -> Self {
        Self {
            repaint: true,
            layout: true,
            init: true,
            always_update: false,
        }
    }

    /// Flags for `#[reactive(init = false)]`: repaint on change, no watcher on init.
    pub const fn reactive_no_init() -> Self {
        Self {
            repaint: true,
            layout: false,
            init: false,
            always_update: false,
        }
    }

    /// Flags for `#[reactive(layout, init = false)]`: repaint + layout on change, no watcher on init.
    pub const fn reactive_layout_no_init() -> Self {
        Self {
            repaint: true,
            layout: true,
            init: false,
            always_update: false,
        }
    }

    /// Flags for `#[var]`: no repaint, no layout, no init watcher.
    pub const fn var() -> Self {
        Self {
            repaint: false,
            layout: false,
            init: false,
            always_update: false,
        }
    }

    /// Flags for `reactive(always_update=True)`: repaint on change, call watcher
    /// on init, and fire watchers even when old value equals new value.
    ///
    /// Matches Python's `reactive(always_update=True)` pattern.
    pub const fn reactive_always_update() -> Self {
        Self {
            repaint: true,
            layout: false,
            init: true,
            always_update: true,
        }
    }
}

/// Records a single field change during an event dispatch cycle.
///
/// Stores the field name, flags, and type-erased old/new values so that
/// watcher methods can be called with properly typed arguments.
pub struct ReactiveChange {
    /// The name of the field that changed.
    pub field_name: &'static str,
    /// Flags from the field's reactive annotation.
    pub flags: ReactiveFlags,
    /// The old value before the change, type-erased.
    pub old_value: Box<dyn Any + Send>,
    /// The new value after the change, type-erased.
    pub new_value: Box<dyn Any + Send>,
}

impl std::fmt::Debug for ReactiveChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReactiveChange")
            .field("field_name", &self.field_name)
            .field("flags", &self.flags)
            .field("old_value", &"<type-erased>")
            .field("new_value", &"<type-erased>")
            .finish()
    }
}

/// Context passed to reactive setters. Records changes and provides node identity.
///
/// Widgets receive this via the runtime; they don't construct it themselves.
/// The context accumulates all changes that occurred during an event dispatch
/// cycle, and the runtime drains them afterward to call watchers and request
/// repaint/layout invalidation.
pub struct ReactiveCtx {
    node_id: NodeId,
    changes: Vec<ReactiveChange>,
    repaint_requested: bool,
    layout_requested: bool,
    class_ops: Vec<(NodeId, crate::event::ClassOp)>,
    styles_requested: bool,
}

impl ReactiveCtx {
    /// Create a new reactive context for the given widget node.
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            changes: Vec::new(),
            repaint_requested: false,
            layout_requested: false,
            class_ops: Vec::new(),
            styles_requested: false,
        }
    }

    /// The node identity of the widget that owns this context.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Access the recorded changes.
    pub fn changes(&self) -> &[ReactiveChange] {
        &self.changes
    }

    /// Take all recorded changes, leaving the context empty.
    pub fn take_changes(&mut self) -> Vec<ReactiveChange> {
        std::mem::take(&mut self.changes)
    }

    /// Record a field change. Called by the generated setter methods.
    pub fn record_change(
        &mut self,
        field_name: &'static str,
        flags: ReactiveFlags,
        old_value: Box<dyn Any + Send>,
        new_value: Box<dyn Any + Send>,
    ) {
        if flags.repaint {
            self.repaint_requested = true;
        }
        if flags.layout {
            self.layout_requested = true;
        }
        self.changes.push(ReactiveChange {
            field_name,
            flags,
            old_value,
            new_value,
        });
    }

    /// Whether any change requested a repaint.
    pub fn needs_repaint(&self) -> bool {
        self.repaint_requested
    }

    /// Whether any change requested a layout invalidation.
    pub fn needs_layout(&self) -> bool {
        self.layout_requested
    }

    /// Whether any change/watcher requested style recomputation.
    pub fn needs_styles(&self) -> bool {
        self.styles_requested
    }

    /// Request a repaint without recording a field change (for watcher side effects).
    pub fn request_repaint(&mut self) {
        self.repaint_requested = true;
    }

    /// Request layout invalidation without recording a field change.
    pub fn request_layout(&mut self) {
        self.layout_requested = true;
    }

    /// Request style recomputation (e.g. after CSS class changes via queries).
    pub fn request_styles(&mut self) {
        self.styles_requested = true;
    }

    /// Returns `true` if any changes were recorded.
    pub fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }

    /// Reset repaint/layout flags without touching the recorded changes.
    ///
    /// Used by the app-level reactive bridge after draining changes so that
    /// stale flags from previous hook calls do not accumulate across tick
    /// boundaries.
    pub fn reset_flags(&mut self) {
        self.repaint_requested = false;
        self.layout_requested = false;
        self.styles_requested = false;
    }

    /// Reset the repaint/layout flags (e.g. after the runtime processes them).
    pub fn clear_flags(&mut self) {
        self.repaint_requested = false;
        self.layout_requested = false;
        self.styles_requested = false;
    }

    /// Queue an `Add` class op on this widget's own node.
    pub fn add_class(&mut self, class: &str) {
        let node_id = self.node_id;
        self.class_ops
            .push((node_id, crate::event::ClassOp::Add(class.to_string())));
    }

    /// Queue a `Remove` class op on this widget's own node.
    pub fn remove_class(&mut self, class: &str) {
        let node_id = self.node_id;
        self.class_ops
            .push((node_id, crate::event::ClassOp::Remove(class.to_string())));
    }

    /// Queue an `Add` or `Remove` class op on this widget's own node based on `on`.
    pub fn set_class(&mut self, on: bool, class: &str) {
        if on {
            self.add_class(class);
        } else {
            self.remove_class(class);
        }
    }

    /// Queue an `Add` class op on an arbitrary node.
    pub fn add_class_to(&mut self, node: NodeId, class: &str) {
        self.class_ops
            .push((node, crate::event::ClassOp::Add(class.to_string())));
    }

    /// Queue a `Remove` class op on an arbitrary node.
    pub fn remove_class_from(&mut self, node: NodeId, class: &str) {
        self.class_ops
            .push((node, crate::event::ClassOp::Remove(class.to_string())));
    }

    pub(crate) fn take_class_ops(&mut self) -> Vec<(NodeId, crate::event::ClassOp)> {
        std::mem::take(&mut self.class_ops)
    }
}

impl std::fmt::Debug for ReactiveCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReactiveCtx")
            .field("node_id", &self.node_id)
            .field("changes", &self.changes.len())
            .field("repaint_requested", &self.repaint_requested)
            .field("layout_requested", &self.layout_requested)
            .field("styles_requested", &self.styles_requested)
            .finish()
    }
}

/// Static descriptor for a single reactive field on a widget.
///
/// Generated by the derive macro and returned by `reactive_field_descriptors()`.
/// Used by the runtime for init-phase watcher dispatch and introspection.
#[derive(Debug, Clone, Copy)]
pub struct ReactiveFieldDescriptor {
    /// The field name (e.g. `"label"`, `"size"`).
    pub name: &'static str,
    /// The flags from the field's annotation.
    pub flags: ReactiveFlags,
}

/// Trait implemented by `#[derive(Reactive)]` structs.
///
/// The derive macro generates a `reactive_dispatch` implementation that
/// calls the appropriate `watch_{field}` method for each recorded change
/// (when the field was annotated with `#[reactive(watch)]`).
///
/// Widgets that don't use reactive fields can still implement this trait
/// with the default no-op implementation.
pub trait ReactiveWidget {
    /// Called by the runtime after event dispatch to process recorded changes.
    ///
    /// The default implementation does nothing. The derive macro overrides this
    /// to downcast old/new values and call `watch_{field}` methods.
    fn reactive_dispatch(&mut self, _changes: &[ReactiveChange], _ctx: &mut ReactiveCtx) {
        // Default: no-op. The derive macro generates the real dispatch.
    }

    /// Return static descriptors for all reactive fields on this widget.
    ///
    /// Used by the runtime to decide which fields need init-phase watcher
    /// dispatch on mount. The default returns an empty slice.
    fn reactive_field_descriptors(&self) -> &'static [ReactiveFieldDescriptor] {
        &[]
    }
}

// ── Runtime reactive phase ──────────────────────────────────────────

/// Maximum number of reactive iterations before the runtime considers
/// a cycle and stops processing. Protects against infinite watcher loops
/// where one watcher's side-effect triggers another change ad infinitum.
pub const MAX_REACTIVE_ITERATIONS: usize = 100;

/// Outcome of running the reactive phase for a single widget.
#[derive(Debug, Default)]
pub struct ReactivePhaseResult {
    /// Whether any changes were processed.
    pub had_changes: bool,
    /// Whether any change requested a repaint.
    pub needs_repaint: bool,
    /// Whether any change requested a layout invalidation.
    pub needs_layout: bool,
    /// Number of iterations executed.
    pub iterations: usize,
    /// Whether the iteration limit was hit (potential cycle).
    pub cycle_detected: bool,
    /// Class mutations queued by watcher callbacks during the reactive phase.
    pub class_ops: Vec<(NodeId, crate::event::ClassOp)>,
}

/// Run the reactive phase for a single widget: drain changes, call watchers,
/// and repeat until no new changes are produced (or cycle limit is hit).
///
/// This is the core function called by the event loop after event dispatch.
/// It takes the widget's `ReactiveCtx` (which accumulated changes from setters
/// during event dispatch), drains the changes, calls `reactive_dispatch()`,
/// and iterates if the dispatch produced further changes (e.g. a watcher
/// calling another setter).
pub fn run_reactive_phase(
    widget: &mut dyn ReactiveWidget,
    ctx: &mut ReactiveCtx,
) -> ReactivePhaseResult {
    run_reactive_phase_with_dispatch(ctx, |changes, dispatch_ctx| {
        widget.reactive_dispatch(changes, dispatch_ctx);
    })
}

/// Run the reactive phase with a custom dispatch function.
///
/// This powers runtime integration where watcher dispatch is represented as
/// a queued callback, and also backs [`run_reactive_phase`] for
/// trait-object based widgets.
pub fn run_reactive_phase_with_dispatch(
    ctx: &mut ReactiveCtx,
    mut dispatch: impl FnMut(&[ReactiveChange], &mut ReactiveCtx),
) -> ReactivePhaseResult {
    let mut result = ReactivePhaseResult::default();

    for iteration in 0..MAX_REACTIVE_ITERATIONS {
        if !ctx.has_changes() {
            break;
        }

        result.had_changes = true;
        result.iterations = iteration + 1;

        if ctx.needs_repaint() {
            result.needs_repaint = true;
        }
        if ctx.needs_layout() {
            result.needs_layout = true;
        }

        let changes = ctx.take_changes();
        ctx.clear_flags();
        dispatch(&changes, ctx);
    }

    // Check for cycle: if we hit max iterations and there are still changes.
    if ctx.has_changes() {
        result.cycle_detected = true;
        result.iterations = MAX_REACTIVE_ITERATIONS;
        crate::debug::debug_render(&format!(
            "[reactive] cycle detected: {} iterations exceeded for node {:?}",
            MAX_REACTIVE_ITERATIONS,
            ctx.node_id()
        ));
        // Drain remaining changes to prevent unbounded accumulation.
        let _ = ctx.take_changes();
        ctx.clear_flags();
    }

    // Collect final flags from any remaining state.
    if ctx.needs_repaint() {
        result.needs_repaint = true;
    }
    if ctx.needs_layout() {
        result.needs_layout = true;
    }

    // Drain any class ops queued by watcher callbacks.
    result.class_ops = ctx.take_class_ops();

    result
}

/// Queued reactive work unit drained by the runtime event loop.
///
/// Widgets enqueue entries during event/lifecycle handlers after recording
/// reactive changes in `ctx`; the runtime drains and processes them during the
/// deterministic reactive phase.
pub struct RuntimeReactiveEntry {
    node_id: NodeId,
    ctx: ReactiveCtx,
}

impl RuntimeReactiveEntry {
    pub fn new(node_id: NodeId, ctx: ReactiveCtx) -> Self {
        Self { node_id, ctx }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn run_with_dispatch(
        &mut self,
        dispatch: impl FnMut(&[ReactiveChange], &mut ReactiveCtx),
    ) -> ReactivePhaseResult {
        run_reactive_phase_with_dispatch(&mut self.ctx, dispatch)
    }

    pub fn run_without_dispatch(&mut self) -> ReactivePhaseResult {
        run_reactive_phase_with_dispatch(&mut self.ctx, |_changes, _ctx| {})
    }
}

thread_local! {
    static RUNTIME_REACTIVE_QUEUE: RefCell<Vec<RuntimeReactiveEntry>> = const { RefCell::new(Vec::new()) };
}

/// Enqueue a reactive work item to be processed in the runtime reactive phase.
pub fn enqueue_runtime_reactive_entry(entry: RuntimeReactiveEntry) {
    RUNTIME_REACTIVE_QUEUE.with(|queue| queue.borrow_mut().push(entry));
}

/// Drain all queued runtime reactive work items.
pub fn take_runtime_reactive_entries() -> Vec<RuntimeReactiveEntry> {
    RUNTIME_REACTIVE_QUEUE.with(|queue| std::mem::take(&mut *queue.borrow_mut()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::SlotMap;

    fn make_node_id() -> NodeId {
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    #[test]
    fn reactive_flags_defaults() {
        let flags = ReactiveFlags::default();
        assert!(flags.repaint);
        assert!(!flags.layout);
        assert!(flags.init);
    }

    #[test]
    fn reactive_flags_reactive() {
        let flags = ReactiveFlags::reactive();
        assert!(flags.repaint);
        assert!(!flags.layout);
        assert!(flags.init);
    }

    #[test]
    fn reactive_flags_reactive_layout() {
        let flags = ReactiveFlags::reactive_layout();
        assert!(flags.repaint);
        assert!(flags.layout);
        assert!(flags.init);
    }

    #[test]
    fn reactive_flags_var() {
        let flags = ReactiveFlags::var();
        assert!(!flags.repaint);
        assert!(!flags.layout);
        assert!(!flags.init);
    }

    #[test]
    fn ctx_new_is_empty() {
        let id = make_node_id();
        let ctx = ReactiveCtx::new(id);
        assert_eq!(ctx.node_id(), id);
        assert!(ctx.changes().is_empty());
        assert!(!ctx.needs_repaint());
        assert!(!ctx.needs_layout());
        assert!(!ctx.has_changes());
    }

    #[test]
    fn ctx_record_change_sets_repaint() {
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        ctx.record_change(
            "label",
            ReactiveFlags::reactive(),
            Box::new("old".to_string()),
            Box::new("new".to_string()),
        );
        assert!(ctx.needs_repaint());
        assert!(!ctx.needs_layout());
        assert!(ctx.has_changes());
        assert_eq!(ctx.changes().len(), 1);
        assert_eq!(ctx.changes()[0].field_name, "label");
    }

    #[test]
    fn ctx_record_change_sets_layout() {
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        ctx.record_change(
            "size",
            ReactiveFlags::reactive_layout(),
            Box::new(10_usize),
            Box::new(20_usize),
        );
        assert!(ctx.needs_repaint());
        assert!(ctx.needs_layout());
    }

    #[test]
    fn ctx_var_change_no_repaint() {
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        ctx.record_change(
            "counter",
            ReactiveFlags::var(),
            Box::new(0_u32),
            Box::new(1_u32),
        );
        assert!(!ctx.needs_repaint());
        assert!(!ctx.needs_layout());
        assert!(ctx.has_changes());
    }

    #[test]
    fn ctx_take_changes_drains() {
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        ctx.record_change(
            "a",
            ReactiveFlags::reactive(),
            Box::new(0_i32),
            Box::new(1_i32),
        );
        ctx.record_change(
            "b",
            ReactiveFlags::reactive(),
            Box::new(false),
            Box::new(true),
        );
        let changes = ctx.take_changes();
        assert_eq!(changes.len(), 2);
        assert!(ctx.changes().is_empty());
        // Flags remain set even after draining changes.
        assert!(ctx.needs_repaint());
    }

    #[test]
    fn ctx_clear_flags() {
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        ctx.record_change(
            "x",
            ReactiveFlags::reactive_layout(),
            Box::new(0_i32),
            Box::new(1_i32),
        );
        assert!(ctx.needs_repaint());
        assert!(ctx.needs_layout());
        ctx.clear_flags();
        assert!(!ctx.needs_repaint());
        assert!(!ctx.needs_layout());
    }

    #[test]
    fn ctx_multiple_changes_accumulate() {
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        // First change: var (no repaint)
        ctx.record_change(
            "counter",
            ReactiveFlags::var(),
            Box::new(0_u32),
            Box::new(1_u32),
        );
        assert!(!ctx.needs_repaint());
        // Second change: reactive (repaint)
        ctx.record_change(
            "label",
            ReactiveFlags::reactive(),
            Box::new("a".to_string()),
            Box::new("b".to_string()),
        );
        assert!(ctx.needs_repaint());
        assert_eq!(ctx.changes().len(), 2);
    }

    #[test]
    fn change_old_new_downcast() {
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        ctx.record_change(
            "value",
            ReactiveFlags::reactive(),
            Box::new(42_i32),
            Box::new(99_i32),
        );
        let change = &ctx.changes()[0];
        assert_eq!(*change.old_value.downcast_ref::<i32>().unwrap(), 42);
        assert_eq!(*change.new_value.downcast_ref::<i32>().unwrap(), 99);
    }

    #[test]
    fn reactive_widget_default_is_noop() {
        struct Dummy;
        impl ReactiveWidget for Dummy {}
        let mut dummy = Dummy;
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        // Should not panic.
        dummy.reactive_dispatch(&[], &mut ctx);
    }

    #[test]
    fn change_debug_impl() {
        let change = ReactiveChange {
            field_name: "test",
            flags: ReactiveFlags::reactive(),
            old_value: Box::new(1_i32),
            new_value: Box::new(2_i32),
        };
        let debug_str = format!("{:?}", change);
        assert!(debug_str.contains("test"));
        assert!(debug_str.contains("type-erased"));
    }

    #[test]
    fn reactive_flags_always_update() {
        let flags = ReactiveFlags::reactive_always_update();
        assert!(flags.repaint);
        assert!(!flags.layout);
        assert!(flags.init);
        assert!(flags.always_update);
    }

    #[test]
    fn reactive_flags_default_not_always_update() {
        assert!(!ReactiveFlags::reactive().always_update);
        assert!(!ReactiveFlags::var().always_update);
        assert!(!ReactiveFlags::reactive_layout().always_update);
    }

    #[test]
    fn ctx_request_styles_sets_flag_and_clears() {
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        assert!(!ctx.needs_styles());
        ctx.request_styles();
        assert!(ctx.needs_styles());
        ctx.clear_flags();
        assert!(!ctx.needs_styles());

        ctx.request_styles();
        assert!(ctx.needs_styles());
        ctx.reset_flags();
        assert!(!ctx.needs_styles());
    }

    #[test]
    fn ctx_request_repaint_layout_without_change() {
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        assert!(!ctx.needs_repaint());
        assert!(!ctx.needs_layout());
        assert!(!ctx.has_changes());

        ctx.request_repaint();
        assert!(ctx.needs_repaint());
        assert!(!ctx.has_changes());

        ctx.request_layout();
        assert!(ctx.needs_layout());
        assert!(!ctx.has_changes());
    }

    #[test]
    fn reactive_widget_default_with_app_delegates() {
        // Verifies that a struct with only reactive_dispatch overridden
        // receives calls correctly through the default impl path.
        struct SimpleWidget {
            dispatch_called: bool,
        }
        impl ReactiveWidget for SimpleWidget {
            fn reactive_dispatch(
                &mut self,
                changes: &[ReactiveChange],
                _ctx: &mut ReactiveCtx,
            ) {
                if !changes.is_empty() {
                    self.dispatch_called = true;
                }
            }
        }

        let id = make_node_id();
        let flags = ReactiveFlags::reactive();
        let changes = vec![ReactiveChange {
            field_name: "x",
            flags,
            old_value: Box::new(0_i32),
            new_value: Box::new(1_i32),
        }];
        let mut ctx = ReactiveCtx::new(id);
        let mut w = SimpleWidget { dispatch_called: false };
        w.reactive_dispatch(&changes, &mut ctx);
        assert!(w.dispatch_called);
    }
}
