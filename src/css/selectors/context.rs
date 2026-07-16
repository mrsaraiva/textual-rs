use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::node_id::NodeId;
use crate::style::Style;

use super::ast::{SelectorMeta, StyleSheet};

thread_local! {
    pub(super) static STYLE_CONTEXT: RefCell<Option<StyleSheet>> = const { RefCell::new(None) };
    pub(super) static STYLE_STACK: RefCell<Vec<Style>> = const { RefCell::new(Vec::new()) };
    pub(super) static SELECTOR_STACK: RefCell<Vec<SelectorMeta>> = const { RefCell::new(Vec::new()) };
    pub(super) static APP_ACTIVE: RefCell<bool> = const { RefCell::new(true) };
    pub(super) static APP_RUNTIME_PSEUDOS: RefCell<AppRuntimePseudos> =
        RefCell::new(AppRuntimePseudos::default());
    pub(super) static COMPUTED_STYLE_CACHE: RefCell<ComputedStyleCache> =
        RefCell::new(ComputedStyleCache::default());
    /// Set of `NodeId`s that match the `:focus-within` pseudo-class.
    ///
    /// Populated by the render pipeline before style resolution: the focused
    /// widget and all of its ancestors are inserted into this set.
    pub(super) static FOCUS_WITHIN_IDS: RefCell<HashSet<NodeId>> =
        RefCell::new(HashSet::new());
    /// Depth of `SELECTOR_STACK` at which the meta of the widget CURRENTLY
    /// executing its `render()` sits (i.e. the stack length right after the
    /// render pipeline pushed the widget's own meta). Explicit live-context
    /// marker for component-class resolution: `resolve_component_style` asks
    /// "is my widget's live meta already on top of the stack" and, when it is,
    /// resolves the component phantom directly against the live stack instead
    /// of re-pushing a mount-seed meta.
    pub(super) static LIVE_WIDGET_META_DEPTH: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
}

pub struct AppActiveGuard(bool);
pub struct AppRuntimePseudosGuard(AppRuntimePseudos);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AppRuntimePseudos {
    pub dark: bool,
    pub inline: bool,
    pub ansi: bool,
    pub nocolor: bool,
}

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

pub fn set_app_runtime_pseudos(pseudos: AppRuntimePseudos) -> AppRuntimePseudosGuard {
    let prev = APP_RUNTIME_PSEUDOS.with(|v| {
        let mut guard = v.borrow_mut();
        let prev = *guard;
        *guard = pseudos;
        prev
    });
    AppRuntimePseudosGuard(prev)
}

impl Drop for AppRuntimePseudosGuard {
    fn drop(&mut self) {
        let prev = self.0;
        APP_RUNTIME_PSEUDOS.with(|v| {
            *v.borrow_mut() = prev;
        });
    }
}

pub(super) fn app_runtime_pseudos() -> AppRuntimePseudos {
    APP_RUNTIME_PSEUDOS.with(|v| *v.borrow())
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

// -- Live widget render context ----------------------------------------------

/// RAII guard restoring the previous live-widget-meta marker on drop.
pub(crate) struct LiveWidgetMetaGuard(Option<usize>);

/// Mark the current top of `SELECTOR_STACK` as the LIVE meta of the widget
/// about to execute `render()`. Called by the render pipeline immediately
/// after it pushes the widget's own meta (see `render_widget_with_meta`).
///
/// This is the explicit (non-heuristic) live-context signal consumed by
/// `resolve_component_style`: the marker matches only while the stack is at
/// exactly this depth, so any nested push (a widget rendering another widget)
/// naturally invalidates it until that nested render marks its own context.
pub(crate) fn mark_live_widget_meta() -> LiveWidgetMetaGuard {
    let depth = SELECTOR_STACK.with(|stack| stack.borrow().len());
    let prev = LIVE_WIDGET_META_DEPTH.with(|cell| cell.replace(Some(depth)));
    LiveWidgetMetaGuard(prev)
}

impl Drop for LiveWidgetMetaGuard {
    fn drop(&mut self) {
        let prev = self.0;
        LIVE_WIDGET_META_DEPTH.with(|cell| cell.set(prev));
    }
}

/// True when the top of `SELECTOR_STACK` is the live meta of the widget
/// currently rendering AND that meta belongs to the CALLING widget (its
/// `type_name` equals `caller_type` or one of `caller_aliases`).
///
/// The identity check is the R7 guard: a widget that renders ANOTHER widget's
/// content inline (e.g. `Footer` calling `FooterKey::render_segments`, or a
/// content-run helper) does so while its OWN live meta is still marked on top.
/// Without the type match, `resolve_component_style(inner, ...)` would misfire
/// and resolve `inner`'s phantom against the OUTER widget's stack. Comparing
/// the top meta's type to the caller keeps the live path to the case Python
/// guarantees: the stack as the CALLING widget's own `render()` sees it.
pub(super) fn live_widget_meta_on_top(caller_type: &str, caller_aliases: &[&str]) -> bool {
    let depth = SELECTOR_STACK.with(|stack| stack.borrow().len());
    if depth == 0 || LIVE_WIDGET_META_DEPTH.with(|cell| cell.get()) != Some(depth) {
        return false;
    }
    SELECTOR_STACK.with(|stack| {
        stack
            .borrow()
            .last()
            .map(|meta| {
                meta.type_name == caller_type
                    || meta.type_aliases.iter().any(|a| a == caller_type)
                    || caller_aliases.iter().any(|a| meta.type_name == *a)
            })
            .unwrap_or(false)
    })
}

// -- Ancestor selector identity ----------------------------------------------

/// Hash of the current ancestor selector-identity chain (the render-time
/// `SELECTOR_STACK`: each ancestor's type/id/classes/pseudo-states).
///
/// This is the render-time analogue of "which stylesheet rules can match my
/// ancestors right now". It changes when an ancestor's classes or pseudo-state
/// (`:focus`, `:hover`, ...) change — the cases where Python's
/// `app.update_styles(node)` cascade re-applies the stylesheet to every
/// descendant and clears each one's cached `visual_style`
/// (`notify_style_update`). It deliberately does NOT cover ancestor resolved
/// style VALUES, so a direct inline mutation (e.g. `styles.background = ...` on
/// an ancestor) leaves it unchanged — matching Python, where inline mutations
/// do not cascade and descendants keep their cached ancestor surface.
///
/// Consumed by the frozen-ancestor-background mechanism in
/// `runtime::render` (`FROZEN_ANCESTOR_BG`).
pub(crate) fn ancestor_selector_fingerprint() -> u64 {
    use std::hash::{Hash, Hasher};
    SELECTOR_STACK.with(|stack| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        stack.borrow().hash(&mut hasher);
        hasher.finish()
    })
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
    /// Active design-token generation the cached entries were resolved against.
    /// A `$token` (e.g. `$panel`) resolves to a concrete `Color` baked into the
    /// cached `Style`; the cache key is theme-independent, so a theme switch
    /// would otherwise return stale colours. Tracking the generation lets us
    /// drop stale entries when the active theme changes (Python `_invalidate_css`).
    theme_generation: u64,
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
        // If the active design theme changed since these entries were resolved,
        // every cached `$token` colour is stale — drop them so this pass
        // re-resolves against the new token map.
        let current_gen = crate::theme::theme_generation();
        if current_gen != self.theme_generation {
            self.theme_generation = current_gen;
            self.entries.clear();
        }
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
