//! The docked notification stack (`ToastRack`) and its per-toast alignment
//! wrapper (`ToastHolder`).
//!
//! Port of Python `textual.widgets._toast.ToastRack`/`ToastHolder`. The rack is a
//! real widget docked on the screen (`dock: bottom`, `layer: _toastrack`,
//! `align: right bottom`); the app injects it as a system child of the app root
//! (like the scrollbar lanes). `App::notify` records notifications in the app's
//! store and pushes a snapshot into the rack via [`ToastRack::sync`]; the rack
//! reconciles that snapshot into real [`Toast`] child nodes and owns the
//! auto-dismiss timers.
//!
//! Timing lives on the *persistent rack node* (not the ephemeral `Toast`
//! children): the rack registers one widget-owned one-shot timer per
//! notification when it first appears, so a full child recompose (adding a later
//! toast) never resets an earlier toast's countdown. When a timer fires — or the
//! user clicks a toast — a [`NotificationExpired`](crate::message::NotificationExpired)
//! message is posted; the runtime removes the notification from the store and
//! re-syncs the rack, unmounting the toast node for real.

use std::collections::HashMap;

use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::compose::{ChildDecl, ComposeResult};
use crate::message::NotificationExpired;
use crate::runtime::TimerHandle;

use super::toast::{Toast, ToastSeverity};
use super::{NodeSeed, Widget};

/// Immutable description of one notification, pushed from the app's notification
/// store into [`ToastRack::sync`]. Off-tree widgets cannot read `App`, so the
/// full current list is handed over on every change (Python `ToastRack.show`).
#[derive(Debug, Clone)]
pub struct NotificationSnapshot {
    pub id: u64,
    pub title: String,
    pub message: String,
    pub severity: ToastSeverity,
    pub timeout: std::time::Duration,
}

/// One live notification held by the rack, with its rack-owned auto-dismiss
/// timer handle.
#[derive(Debug)]
struct RackEntry {
    id: u64,
    title: Option<String>,
    message: String,
    severity: ToastSeverity,
    /// The auto-dismiss timer for this notification (one-shot). `None` once it
    /// has fired or been stopped.
    timer: Option<TimerHandle>,
}

/// Internal-DOM id for the holder wrapping a given notification's toast (mirrors
/// Python `ToastRack._toast_id`). Stable across recomposes so identity is
/// preserved.
fn toast_holder_id(id: u64) -> String {
    format!("--textual-toast-{id}")
}

/// A container that holds a single toast, controlling its right-alignment within
/// the rack (Python `ToastHolder`). Rebuilt fresh on every rack recompose.
#[derive(Debug, Clone)]
pub struct ToastHolder {
    toast: Toast,
    seed: NodeSeed,
}

impl ToastHolder {
    crate::seed_ident_methods!();

    pub fn new(toast: Toast) -> Self {
        Self {
            toast,
            seed: NodeSeed::default(),
        }
    }
}

impl Widget for ToastHolder {
    fn focusable(&self) -> bool {
        false
    }

    fn compose(&mut self) -> ComposeResult {
        vec![ChildDecl::new(Box::new(self.toast.clone()))]
    }

    /// Chrome-only: the holder is invisible (`visibility: hidden`); the toast
    /// child composites over the screen.
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "ToastHolder"
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for ToastHolder {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

/// The docked notification stack. See the module docs for the ownership model.
#[derive(Debug)]
pub struct ToastRack {
    entries: Vec<RackEntry>,
    seed: NodeSeed,
}

impl Default for ToastRack {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastRack {
    crate::seed_ident_methods!();

    /// Class toggled on the rack node while it holds at least one toast, flipping
    /// its default `display: none` to `display: block` (Python
    /// `ToastRack.display = bool(notifications)`).
    const ACTIVE_CLASS: &'static str = "-active";

    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            seed: NodeSeed {
                classes: vec!["-textual-system".to_string()],
                ..NodeSeed::default()
            },
        }
    }

    /// Number of live toasts (test/observability).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Reconcile the rack's live toasts against the app's current notification
    /// snapshot (Python `ToastRack.show`).
    ///
    /// - Entries present in `snapshot` are kept in snapshot order, preserving
    ///   their still-running auto-dismiss timer.
    /// - New entries get a widget-owned one-shot auto-dismiss timer registered on
    ///   this (persistent) rack node.
    /// - Entries dropped from the snapshot have their timers stopped.
    ///
    /// Then the `-active` display toggle is updated and a child recompose is
    /// requested so the toast child nodes match the entry set.
    pub fn sync(
        &mut self,
        snapshot: Vec<NotificationSnapshot>,
        ctx: &mut crate::event::WidgetCtx,
    ) {
        let mut old: HashMap<u64, RackEntry> =
            self.entries.drain(..).map(|e| (e.id, e)).collect();

        let mut next: Vec<RackEntry> = Vec::with_capacity(snapshot.len());
        for snap in snapshot {
            if let Some(entry) = old.remove(&snap.id) {
                // Keep the existing entry and its running timer untouched — the
                // load-bearing rule: a later toast must not reset this one's
                // countdown.
                next.push(entry);
            } else {
                let id = snap.id;
                let handle =
                    ctx.set_interval::<ToastRack, _>(snap.timeout, false, move |rack, ctx, _tick| {
                        rack.on_auto_dismiss(id, ctx);
                    });
                next.push(RackEntry {
                    id: snap.id,
                    title: (!snap.title.is_empty()).then_some(snap.title),
                    message: snap.message,
                    severity: snap.severity,
                    timer: Some(handle),
                });
            }
        }
        // Stop the timers of any notifications that vanished from the snapshot.
        for (_, mut entry) in old {
            if let Some(handle) = entry.timer.take() {
                handle.stop();
            }
        }

        self.entries = next;
        ctx.set_class(!self.entries.is_empty(), Self::ACTIVE_CLASS);
        ctx.request_recompose();
    }

    /// Auto-dismiss timer fired for notification `id`: make the one-shot stick
    /// (stop the timer) and ask the runtime to remove the notification, which
    /// re-syncs the rack and unmounts the toast node.
    fn on_auto_dismiss(&mut self, id: u64, ctx: &mut crate::event::WidgetCtx) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id)
            && let Some(handle) = entry.timer.take()
        {
            handle.stop();
        }
        ctx.post_message(NotificationExpired { id });
    }
}

impl Widget for ToastRack {
    fn focusable(&self) -> bool {
        false
    }

    fn compose(&mut self) -> ComposeResult {
        self.entries
            .iter()
            .map(|entry| {
                let mut toast =
                    Toast::new(entry.message.clone(), entry.severity).with_notification_id(entry.id);
                if let Some(title) = &entry.title {
                    toast = toast.with_title(title.clone());
                }
                ChildDecl::new(Box::new(ToastHolder::new(toast)))
                    .with_id(&toast_holder_id(entry.id))
            })
            .collect()
    }

    /// Chrome-only: the rack itself is invisible (`visibility: hidden`); only its
    /// toast descendants paint.
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "ToastRack"
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for ToastRack {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
