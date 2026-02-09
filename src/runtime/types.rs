use crate::event::{AnimationRequest, BindingHint};
use crate::message::MessageEvent;
use crate::render::FrameBuffer;
use crate::widgets::{border_spacing_from_style, ToastSeverity, Widget, WidgetId};
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

#[derive(Debug, Default, Clone)]
pub(crate) struct HitTestMap {
    pub(crate) bounds: HashMap<WidgetId, Rect>,
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
                let wid = WidgetId::from_u64(*id as u64);
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

    pub(crate) fn rect(&self, id: WidgetId) -> Option<Rect> {
        self.bounds.get(&id).copied()
    }

    pub(crate) fn content_local_coords(
        &self,
        root: &mut dyn Widget,
        target: WidgetId,
        screen_x: u16,
        screen_y: u16,
    ) -> (u16, u16) {
        let Some(rect) = self.rect(target) else {
            return (0, 0);
        };

        let mut insets: Option<(u16, u16)> = None;
        fn visit(w: &mut dyn Widget, id: WidgetId, out: &mut Option<(u16, u16)>) {
            if out.is_some() {
                return;
            }
            if w.id() == id {
                let meta = crate::css::selector_meta_generic(w);
                let resolved = crate::css::resolve_style(w, &meta);
                let line_pad = resolved.line_pad.unwrap_or(0);
                let (top, _bottom, left, _right) = border_spacing_from_style(&resolved);
                let inset_x = left.saturating_add(line_pad) as u16;
                let inset_y = top as u16;
                *out = Some((inset_x, inset_y));
                return;
            }
            w.visit_children_mut(&mut |child| visit(child, id, out));
        }
        visit(root, target, &mut insets);
        let (inset_x, inset_y) = insets.unwrap_or((0, 0));

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
    pub(crate) interval: Duration,
    pub(crate) last_checked: Instant,
}

pub(crate) const SYNC_START: &str = "\x1b[?2026h";
pub(crate) const SYNC_END: &str = "\x1b[?2026l";

#[derive(Debug, Clone, Default)]
pub(crate) struct DispatchOutcome {
    pub(crate) handled: bool,
    pub(crate) repaint_requested: bool,
    pub(crate) stop_requested: bool,
    pub(crate) messages: Vec<MessageEvent>,
    pub(crate) animation_requests: Vec<AnimationRequest>,
}

impl DispatchOutcome {
    pub(crate) fn should_repaint(&self) -> bool {
        self.handled || self.repaint_requested
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
