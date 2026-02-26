use crate::css::{StyleRule, StyleSheet};
use crate::event::{AnimationRequest, BindingHint, InvalidationFlags};
use crate::message::MessageEvent;
use crate::node_id::{NodeId, node_id_from_ffi};
use crate::render::{DirtyRegion, FrameBuffer};
use crate::widgets::{ToastSeverity, border_spacing_from_style};
use crate::worker::WorkerRequest;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x0: u16,
    pub(crate) y0: u16,
    pub(crate) x1: u16,
    pub(crate) y1: u16,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct HitTestMap {
    pub(crate) bounds: HashMap<NodeId, Rect>,
}

impl HitTestMap {
    pub(crate) fn from_frame(frame: &FrameBuffer) -> Self {
        let mut out = HitTestMap::default();
        for (id, rect) in frame.owner_bounds() {
            let wid = node_id_from_ffi(id as u64);
            out.bounds.insert(
                wid,
                Rect {
                    x0: rect.x0,
                    y0: rect.y0,
                    x1: rect.x1,
                    y1: rect.y1,
                },
            );
        }
        out
    }

    pub(crate) fn rect(&self, id: NodeId) -> Option<Rect> {
        self.bounds.get(&id).copied()
    }

    /// Translate screen coordinates to content-local coordinates for `target`.
    ///
    /// Computes local coordinates from the node bounding rect.
    ///
    /// This map stores frame-composited bounds. For CSS-inset-aware coordinate
    /// translation, prefer `NodeHitTestMap::content_local_coords`.
    pub(crate) fn content_local_coords(
        &self,
        target: NodeId,
        screen_x: u16,
        screen_y: u16,
    ) -> (u16, u16) {
        let Some(rect) = self.rect(target) else {
            return (0, 0);
        };
        (
            screen_x.saturating_sub(rect.x0),
            screen_y.saturating_sub(rect.y0),
        )
    }
}

// ---------------------------------------------------------------------------
// NodeHitTestMap (P1-12: arena NodeId-keyed hit-test map)
// ---------------------------------------------------------------------------

/// Hit-test map keyed by arena `NodeId`.
///
/// This is the primary hit-test map for arena-tree coordinate translation.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct NodeHitTestMap {
    pub(crate) bounds: HashMap<NodeId, Rect>,
}

impl NodeHitTestMap {
    /// Look up the bounding rectangle for a given `NodeId`.
    pub(crate) fn rect(&self, id: NodeId) -> Option<Rect> {
        self.bounds.get(&id).copied()
    }

    /// Translate screen coordinates to content-local coordinates for `target`.
    ///
    /// Mirrors [`HitTestMap::content_local_coords`] but uses the arena tree
    /// instead of recursive `visit_children_mut`. Will be called when the
    /// runtime uses `NodeHitTestMap` for tree-based coordinate translation.
    #[allow(dead_code)]
    pub(crate) fn content_local_coords(
        &self,
        tree: &mut crate::widget_tree::WidgetTree,
        target: NodeId,
        screen_x: u16,
        screen_y: u16,
    ) -> (u16, u16) {
        let Some(rect) = self.rect(target) else {
            return (0, 0);
        };

        let (inset_x, inset_y) = if let Some(node) = tree.get(target) {
            let meta = crate::css::selector_meta_generic_with_classes(
                node.widget.as_ref(),
                node.classes.iter().cloned(),
            );
            let resolved = crate::css::resolve_style(node.widget.as_ref(), &meta);
            let line_pad = resolved.line_pad.unwrap_or(0) as usize;
            let (top, _bottom, left, _right) = border_spacing_from_style(&resolved);
            (left.saturating_add(line_pad) as u16, top as u16)
        } else {
            (0, 0)
        };

        let origin_x = rect.x0.saturating_add(inset_x);
        let origin_y = rect.y0.saturating_add(inset_y);
        (
            screen_x.saturating_sub(origin_x),
            screen_y.saturating_sub(origin_y),
        )
    }
}

impl From<&HitTestMap> for NodeHitTestMap {
    fn from(hit_test: &HitTestMap) -> Self {
        Self {
            bounds: hit_test.bounds.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BindingHintEntry {
    pub(crate) key: crate::event::KeyBind,
    pub(crate) hint: BindingHint,
}

#[derive(Debug, Clone)]
pub(crate) struct AppNotification {
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) severity: ToastSeverity,
    pub(crate) expires_at: Instant,
}

impl AppNotification {
    pub(crate) fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        severity: ToastSeverity,
        timeout: Duration,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            severity,
            expires_at: Instant::now() + timeout,
        }
    }
}

pub(crate) const DEFAULT_NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const TOAST_GAP_ROWS: usize = 1;
pub(crate) const TOAST_SIDE_MARGIN: usize = 2;

pub(crate) struct StylesheetWatcher {
    pub(crate) path: PathBuf,
    pub(crate) last_modified: Option<std::time::SystemTime>,
    pub(crate) last_css: String,
    pub(crate) interval: Duration,
    pub(crate) last_checked: Instant,
}

#[derive(Debug, Clone)]
pub(crate) struct StylesheetReload {
    pub(crate) previous: StyleSheet,
    pub(crate) next: StyleSheet,
    pub(crate) changed_rules: Vec<StyleRule>,
    pub(crate) layout_affected: bool,
}

pub(crate) const SYNC_START: &str = "\x1b[?2026h";
pub(crate) const SYNC_END: &str = "\x1b[?2026l";

#[derive(Debug, Clone, Default)]
pub struct DispatchOutcome {
    pub handled: bool,
    pub repaint_requested: bool,
    pub invalidation: InvalidationFlags,
    pub stop_requested: bool,
    pub messages: Vec<MessageEvent>,
    pub animation_requests: Vec<AnimationRequest>,
    /// Worker spawn requests emitted by widgets during this dispatch cycle.
    pub worker_requests: Vec<WorkerRequest>,
    /// Nodes whose children should be recomposed after dispatch.
    pub recompose_nodes: Vec<NodeId>,
    /// True when at least one handler called `prevent_default()` on the envelope
    /// during message dispatch, signalling the default action should be skipped.
    ///
    /// **Note:** Currently widgets receive `&MessageEvent` (not `&mut MessageEnvelope`),
    /// so this flag cannot be set from widget code yet. It is wired end-to-end
    /// in preparation for a future Widget trait update that passes envelopes
    /// directly to `on_message()`.
    pub default_prevented: bool,
}

impl DispatchOutcome {
    pub fn should_repaint(&self) -> bool {
        self.handled || self.repaint_requested || self.invalidation.content
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DirtyRegions {
    full: bool,
    regions: Vec<Rect>,
}

impl DirtyRegions {
    pub(crate) fn invalidate_all(&mut self) {
        self.full = true;
        self.regions.clear();
    }

    pub(crate) fn invalidate_rect(&mut self, rect: Rect) {
        if self.full {
            return;
        }
        self.regions.push(rect);
    }

    pub(crate) fn is_full(&self) -> bool {
        self.full
    }

    pub(crate) fn as_render_regions(
        &self,
        width: usize,
        height: usize,
    ) -> Option<Vec<DirtyRegion>> {
        if self.full {
            return None;
        }
        if self.regions.is_empty() {
            return Some(Vec::new());
        }
        if self.regions.len() > 64 {
            return None;
        }

        let max_x = width.saturating_sub(1) as u16;
        let max_y = height.saturating_sub(1) as u16;
        let mut out = Vec::new();
        for rect in &self.regions {
            if width == 0 || height == 0 {
                continue;
            }
            let mut x0 = rect.x0.min(max_x);
            let mut y0 = rect.y0.min(max_y);
            let mut x1 = rect.x1.min(max_x);
            let mut y1 = rect.y1.min(max_y);
            if x0 > x1 || y0 > y1 {
                continue;
            }
            // Expand by one cell on each side to keep wide-character continuations safe.
            x0 = x0.saturating_sub(1);
            y0 = y0.saturating_sub(1);
            x1 = x1.saturating_add(1).min(max_x);
            y1 = y1.saturating_add(1).min(max_y);
            out.push(DirtyRegion {
                x0: x0 as usize,
                y0: y0 as usize,
                x1: x1 as usize,
                y1: y1 as usize,
            });
        }
        Some(out)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PendingInvalidation {
    pub(crate) flags: InvalidationFlags,
    pub(crate) content_regions: DirtyRegions,
}

impl PendingInvalidation {
    pub(crate) fn is_dirty(&self) -> bool {
        self.flags.content || self.content_regions.is_full()
    }

    pub(crate) fn request_full_content(&mut self) {
        self.flags.content = true;
        self.content_regions.invalidate_all();
    }

    pub(crate) fn request_widget_rect(&mut self, hit_test: &HitTestMap, id: NodeId) {
        self.flags.content = true;
        if self.content_regions.is_full() {
            return;
        }
        if let Some(rect) = hit_test.rect(id) {
            self.content_regions.invalidate_rect(rect);
            return;
        }
        // If the hit-test map has no bounds for the target, region-scoped diff
        // would produce an empty update while still advancing the internal
        // framebuffer, causing visible flicker on subsequent diffs. Fall back to
        // full-content invalidation to keep terminal/frame state synchronized.
        self.content_regions.invalidate_all();
    }

    pub(crate) fn request_flags(&mut self, flags: InvalidationFlags) {
        self.flags.merge(flags);
        if flags.layout || flags.style {
            self.content_regions.invalidate_all();
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct SegmentStreamStats {
    pub(crate) controls: usize,
    pub(crate) home: usize,
    pub(crate) clear: usize,
    pub(crate) carriage_return: usize,
    pub(crate) cursor_moves: usize,
    pub(crate) move_to: usize,
    pub(crate) text_segments: usize,
    pub(crate) text_bytes: usize,
    pub(crate) newline_text: usize,
    pub(crate) touch_last_col: usize,
    pub(crate) overflow_right: usize,
    pub(crate) max_cursor_x: usize,
    pub(crate) max_cursor_y: usize,
}

pub(crate) fn resize_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("TEXTUAL_DEBUG_RESIZE_TRACE")
            .ok()
            .map(|value| {
                let normalized = value.trim().to_ascii_lowercase();
                !(normalized.is_empty()
                    || normalized == "0"
                    || normalized == "false"
                    || normalized == "off"
                    || normalized == "no")
            })
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::{DirtyRegions, HitTestMap, NodeHitTestMap, PendingInvalidation, Rect};
    use crate::node_id::node_id_from_ffi;
    use std::collections::HashMap;

    #[test]
    fn dirty_regions_expand_by_one_cell() {
        let mut dirty = DirtyRegions::default();
        dirty.invalidate_rect(Rect {
            x0: 2,
            y0: 1,
            x1: 2,
            y1: 1,
        });
        let regions = dirty
            .as_render_regions(10, 5)
            .expect("should remain region-scoped");
        assert_eq!(regions.len(), 1);
        let region = regions[0];
        assert_eq!((region.x0, region.y0, region.x1, region.y1), (1, 0, 3, 2));
    }

    #[test]
    fn dirty_regions_fall_back_to_full_after_many_rects() {
        let mut dirty = DirtyRegions::default();
        for _ in 0..70 {
            dirty.invalidate_rect(Rect {
                x0: 1,
                y0: 1,
                x1: 1,
                y1: 1,
            });
        }
        assert!(
            dirty.as_render_regions(20, 10).is_none(),
            "many disjoint regions should fall back to full diff"
        );
    }

    #[test]
    fn node_hit_test_map_from_hit_test_copies_bounds() {
        let id_a = node_id_from_ffi(1);
        let id_b = node_id_from_ffi(42);
        let mut bounds = HashMap::new();
        bounds.insert(
            id_a,
            Rect {
                x0: 1,
                y0: 2,
                x1: 3,
                y1: 4,
            },
        );
        bounds.insert(
            id_b,
            Rect {
                x0: 10,
                y0: 11,
                x1: 12,
                y1: 13,
            },
        );

        let hit = HitTestMap { bounds };
        let node_hit = NodeHitTestMap::from(&hit);
        assert_eq!(node_hit.bounds, hit.bounds);
    }

    #[test]
    fn request_widget_rect_without_bounds_falls_back_to_full_invalidation() {
        let mut pending = PendingInvalidation::default();
        let hit = HitTestMap::default();
        let missing = node_id_from_ffi(99);
        pending.request_widget_rect(&hit, missing);
        assert!(pending.flags.content, "content flag should be set");
        assert!(
            pending.content_regions.is_full(),
            "missing widget bounds must escalate to full invalidation to avoid empty region diffs"
        );
    }
}
