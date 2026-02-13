use crate::css::{StyleRule, StyleSheet};
use crate::event::{AnimationRequest, BindingHint, InvalidationFlags};
use crate::message::MessageEvent;
use crate::node_id::{NodeId, node_id_from_ffi};
use crate::render::{DirtyRegion, FrameBuffer};
use crate::widgets::{ToastSeverity, border_spacing_from_style};
use crate::worker::WorkerRequest;
use rich_rs::MetaValue;
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
        for y in 0..frame.height {
            for x in 0..frame.width {
                let cell = frame.get(x, y);
                let Some(meta) = cell.meta.as_ref() else {
                    continue;
                };
                let Some(map) = meta.meta.as_ref() else {
                    continue;
                };
                let Some(MetaValue::Int(id)) = map.get("textual:widget_id") else {
                    continue;
                };
                if *id < 0 {
                    continue;
                }
                let wid = node_id_from_ffi(*id as u64);
                let xu = x as u16;
                let yu = y as u16;
                out.bounds
                    .entry(wid)
                    .and_modify(|r| {
                        r.x0 = r.x0.min(xu);
                        r.y0 = r.y0.min(yu);
                        r.x1 = r.x1.max(xu);
                        r.y1 = r.y1.max(yu);
                    })
                    .or_insert(Rect {
                        x0: xu,
                        y0: yu,
                        x1: xu,
                        y1: yu,
                    });
            }
        }
        out
    }

    pub(crate) fn rect(&self, id: NodeId) -> Option<Rect> {
        self.bounds.get(&id).copied()
    }

    /// Translate screen coordinates to content-local coordinates for `target`.
    ///
    /// Simplified root-only version: computes offset from the bounding rect
    /// without CSS inset adjustment. The tree-based
    /// [`NodeHitTestMap::content_local_coords`] provides full inset calculation
    /// when the arena tree is available.
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
/// This is the primary hit-test map for the arena tree render pipeline.
/// [`HitTestMap`] remains for the legacy root-only path.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct NodeHitTestMap {
    pub(crate) bounds: HashMap<NodeId, Rect>,
}

impl NodeHitTestMap {
    /// Build a `NodeHitTestMap` from a rendered frame buffer.
    ///
    /// Decodes the `textual:widget_id` metadata stored in each cell back to
    /// `NodeId` via [`node_id_from_ffi`].
    pub(crate) fn from_frame(frame: &FrameBuffer) -> Self {
        let mut out = NodeHitTestMap::default();
        for y in 0..frame.height {
            for x in 0..frame.width {
                let cell = frame.get(x, y);
                let Some(meta) = cell.meta.as_ref() else {
                    continue;
                };
                let Some(map) = meta.meta.as_ref() else {
                    continue;
                };
                let Some(MetaValue::Int(id)) = map.get("textual:widget_id") else {
                    continue;
                };
                if *id < 0 {
                    continue;
                }
                let nid = node_id_from_ffi(*id as u64);
                let xu = x as u16;
                let yu = y as u16;
                out.bounds
                    .entry(nid)
                    .and_modify(|r| {
                        r.x0 = r.x0.min(xu);
                        r.y0 = r.y0.min(yu);
                        r.x1 = r.x1.max(xu);
                        r.y1 = r.y1.max(yu);
                    })
                    .or_insert(Rect {
                        x0: xu,
                        y0: yu,
                        x1: xu,
                        y1: yu,
                    });
            }
        }
        out
    }

    /// Look up the bounding rectangle for a given `NodeId`.
    pub(crate) fn rect(&self, id: NodeId) -> Option<Rect> {
        self.bounds.get(&id).copied()
    }

    /// Translate screen coordinates to content-local coordinates for `target`.
    ///
    /// Mirrors [`HitTestMap::content_local_coords`] but uses the arena tree
    /// instead of recursive `visit_children_mut`.
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
            let meta = crate::css::selector_meta_generic(node.widget.as_ref());
            let resolved = crate::css::resolve_style(node.widget.as_ref(), &meta);
            let line_pad = resolved.padding.map(|s| s.left as usize).unwrap_or(0);
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
pub(crate) struct DispatchOutcome {
    pub(crate) handled: bool,
    pub(crate) repaint_requested: bool,
    pub(crate) invalidation: InvalidationFlags,
    pub(crate) stop_requested: bool,
    pub(crate) messages: Vec<MessageEvent>,
    pub(crate) animation_requests: Vec<AnimationRequest>,
    /// Worker spawn requests emitted by widgets during this dispatch cycle.
    pub(crate) worker_requests: Vec<WorkerRequest>,
    /// True when at least one handler called `prevent_default()` on the envelope
    /// during message dispatch, signalling the default action should be skipped.
    ///
    /// **Note:** Currently widgets receive `&MessageEvent` (not `&mut MessageEnvelope`),
    /// so this flag cannot be set from widget code yet. It is wired end-to-end
    /// in preparation for a future Widget trait update that passes envelopes
    /// directly to `on_message()`.
    pub(crate) default_prevented: bool,
}

impl DispatchOutcome {
    pub(crate) fn should_repaint(&self) -> bool {
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
        }
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
    use super::{DirtyRegions, Rect};

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
}
