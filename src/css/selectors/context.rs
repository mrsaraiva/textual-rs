use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use crate::node_id::NodeId;
use crate::style::Style;

use super::ast::{SelectorMeta, StyleSheet};

thread_local! {
    pub(super) static STYLE_CONTEXT: RefCell<Option<StyleSheet>> = RefCell::new(None);
    pub(super) static STYLE_STACK: RefCell<Vec<Style>> = RefCell::new(Vec::new());
    pub(super) static SELECTOR_STACK: RefCell<Vec<SelectorMeta>> = RefCell::new(Vec::new());
    pub(super) static APP_ACTIVE: RefCell<bool> = RefCell::new(true);
    pub(super) static COMPUTED_STYLE_CACHE: RefCell<ComputedStyleCache> =
        RefCell::new(ComputedStyleCache::default());
    pub(super) static THEME_DARK: Cell<bool> = Cell::new(false);
    /// Set of `NodeId`s that match the `:focus-within` pseudo-class.
    ///
    /// Populated by the render pipeline before style resolution: the focused
    /// widget and all of its ancestors are inserted into this set.
    pub(super) static FOCUS_WITHIN_IDS: RefCell<HashSet<NodeId>> =
        RefCell::new(HashSet::new());
}

pub struct AppActiveGuard(bool);

pub fn set_app_active(active: bool) -> AppActiveGuard {
    let prev = APP_ACTIVE.with(|v| {
        let mut guard = v.borrow_mut();
        let prev = *guard;
        *guard = active;
        prev
    });
    AppActiveGuard(prev)
}

impl Drop for AppActiveGuard {
    fn drop(&mut self) {
        let prev = self.0;
        APP_ACTIVE.with(|v| {
            *v.borrow_mut() = prev;
        });
    }
}

pub(super) fn app_is_active() -> bool {
    APP_ACTIVE.with(|v| *v.borrow())
}

pub struct ThemeDarkGuard(bool);

pub fn set_theme_dark(dark: bool) -> ThemeDarkGuard {
    let prev = THEME_DARK.with(|v| {
        let prev = v.get();
        v.set(dark);
        prev
    });
    ThemeDarkGuard(prev)
}

impl Drop for ThemeDarkGuard {
    fn drop(&mut self) {
        let prev = self.0;
        THEME_DARK.with(|v| v.set(prev));
    }
}

pub(super) fn theme_is_dark() -> bool {
    THEME_DARK.with(|v| v.get())
}

// -- Focus-within context ---------------------------------------------------

/// RAII guard that restores the previous `:focus-within` set on drop.
pub struct FocusWithinGuard(HashSet<NodeId>);

/// Set the `:focus-within` node set for the current render pass.
///
/// `ids` should contain the focused node's `NodeId` plus every ancestor's
/// `NodeId`.  Returns a guard that restores the previous set on drop.
pub fn set_focus_within(ids: HashSet<NodeId>) -> FocusWithinGuard {
    let prev = FOCUS_WITHIN_IDS.with(|v| {
        let mut guard = v.borrow_mut();
        std::mem::replace(&mut *guard, ids)
    });
    FocusWithinGuard(prev)
}

impl Drop for FocusWithinGuard {
    fn drop(&mut self) {
        let prev = std::mem::take(&mut self.0);
        FOCUS_WITHIN_IDS.with(|v| {
            *v.borrow_mut() = prev;
        });
    }
}

/// Check whether a `NodeId` is in the current `:focus-within` set.
pub(super) fn is_focus_within(node: NodeId) -> bool {
    FOCUS_WITHIN_IDS.with(|v| v.borrow().contains(&node))
}

pub struct StyleContextGuard(Option<StyleSheet>);

pub fn set_style_context(stylesheet: StyleSheet) -> StyleContextGuard {
    COMPUTED_STYLE_CACHE.with(|cache| cache.borrow_mut().set_stylesheet(stylesheet.clone()));
    let prev = STYLE_CONTEXT.with(|ctx| ctx.borrow_mut().replace(stylesheet));
    StyleContextGuard(prev)
}

impl Drop for StyleContextGuard {
    fn drop(&mut self) {
        let prev = self.0.take();
        STYLE_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = prev;
        });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ComputedStyleKey {
    pub(super) meta: SelectorMeta,
    pub(super) ancestors: Vec<SelectorMeta>,
    pub(super) parent_style: Option<Style>,
    pub(super) inline_style: Option<Style>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ComputedStyleCacheStats {
    pub(super) hits: u64,
    pub(super) misses: u64,
}

#[derive(Debug, Clone)]
pub(super) struct CachedComputedStyle {
    pub(super) key: ComputedStyleKey,
    pub(super) resolved: Style,
}

#[derive(Debug, Default)]
pub(super) struct ComputedStyleCache {
    stylesheet: Option<StyleSheet>,
    entries: HashMap<NodeId, CachedComputedStyle>,
    stats: ComputedStyleCacheStats,
    layout_affected_change_in_pass: bool,
}

impl ComputedStyleCache {
    fn set_stylesheet(&mut self, stylesheet: StyleSheet) {
        if self.stylesheet.as_ref() == Some(&stylesheet) {
            return;
        }
        self.stylesheet = Some(stylesheet);
        self.entries.clear();
        self.layout_affected_change_in_pass = false;
    }

    pub(super) fn begin_render_pass(&mut self) {
        self.layout_affected_change_in_pass = false;
    }

    pub(super) fn take_layout_affected_change(&mut self) -> bool {
        let changed = self.layout_affected_change_in_pass;
        self.layout_affected_change_in_pass = false;
        changed
    }

    pub(super) fn get(&mut self, widget_id: NodeId, key: &ComputedStyleKey) -> Option<Style> {
        if let Some(entry) = self.entries.get(&widget_id) {
            if &entry.key == key {
                self.stats.hits = self.stats.hits.saturating_add(1);
                return Some(entry.resolved.clone());
            }
        }
        self.stats.misses = self.stats.misses.saturating_add(1);
        None
    }

    pub(super) fn prior_resolved(&self, widget_id: NodeId) -> Option<Style> {
        self.entries
            .get(&widget_id)
            .map(|entry| entry.resolved.clone())
    }

    pub(super) fn store(
        &mut self,
        widget_id: NodeId,
        key: ComputedStyleKey,
        resolved: Style,
        layout_affected_changed: bool,
    ) {
        if layout_affected_changed {
            self.layout_affected_change_in_pass = true;
        }
        self.entries
            .insert(widget_id, CachedComputedStyle { key, resolved });
    }

    #[cfg(test)]
    pub(super) fn reset_for_tests(&mut self) {
        self.entries.clear();
        self.stats = ComputedStyleCacheStats::default();
        self.layout_affected_change_in_pass = false;
    }

    #[cfg(test)]
    pub(super) fn stats_for_tests(&self) -> ComputedStyleCacheStats {
        self.stats
    }
}
