use crate::css::{
    AppRuntimePseudos, StyleSheet, begin_style_render_pass, node_selector_meta,
    node_selector_meta_from_node, pop_style_context, push_style_context, resolve_node_style,
    set_app_active, set_app_runtime_pseudos, set_style_context, take_layout_affected_style_changes,
};
use crate::debug::{debug_layout, debug_render};
use crate::node_id::{NodeId, node_id_to_ffi};
use crate::render::{DirtyRegion, FrameBuffer};
use crate::style::{
    Color, Constrain, Hatch, KeylineType, Layout, OverlayMode, TextOverflow, TextWrap,
    color_from_simple,
};
use crate::widget_tree::WidgetTree;
use crate::widgets::{
    APP_ROOT_HSCROLLBAR_ID, APP_ROOT_SCROLLBAR_CORNER_ID, APP_ROOT_VSCROLLBAR_ID,
    CONTAINER_HSCROLLBAR_ID, CONTAINER_SCROLLBAR_CORNER_ID, CONTAINER_VSCROLLBAR_ID, Container,
    DATA_TABLE_HSCROLLBAR_ID, KEY_PANEL_VSCROLLBAR_ID, LOG_VSCROLLBAR_ID,
    OPTION_LIST_VSCROLLBAR_ID, OutlineCell, RICH_LOG_VSCROLLBAR_ID,
    SCROLL_VIEW_HSCROLLBAR_ID, SCROLL_VIEW_SCROLLBAR_CORNER_ID, SCROLL_VIEW_VSCROLLBAR_ID,
    ScrollBar, ScrollBarCorner, ScrollbarPolicy, Widget, border_spacing_from_style,
    crop_line_horizontal, outline_edge_cells,
};

use rich_rs::{ControlType, MetaValue, Renderable, Segment, Segments, StyleMeta};
use std::collections::BTreeSet;
use std::sync::OnceLock;

use super::App;
use super::dispatch_ctx::set_dispatch_recipient;
use super::types::{
    HitTestMap, SYNC_END, SYNC_START, SegmentStreamStats, resize_trace_enabled,
};

// ===========================================================================
// Frozen ancestor-composited background (Python `visual_style` caching parity)
// ===========================================================================
//
// Python `Widget.visual_style` bakes the composited ancestor background into a
// widget's text (transparent) segments and CACHES it keyed on the widget's OWN
// `styles._cache_key`. A later ANCESTOR-only background change (e.g. an action
// setting `self.screen.styles.background = "red"`) bumps the ancestor's cache
// key but NOT the child's, so the child's cached `visual_style` keeps the base
// background it captured at its own last content render. Meanwhile the widget's
// SURFACE/padding fill uses `background_colors`, which IS live. Result: the
// child's glyph cells keep the cached ancestor surface while the surrounding
// pad turns to the live colour.
//
// Rust bakes the child glyph background LIVE (text.rs `effective_bg` from
// `current_ancestor_composited_background()`), so an ancestor bg change leaks
// into the child's text. To match Python we cache, per node, the composited
// ancestor background captured when the node last re-rendered its own content
// (detected via a fingerprint of the node's OWN, non-inherited style identity).
// When the live ancestor composite diverges from that frozen value we push a
// synthetic opaque style entry (a clone of the parent style with its bg forced
// to the frozen value) so the child's transparent segments composite over the
// frozen ancestor surface — exactly as Python's cached `visual_style` does.
thread_local! {
    static FROZEN_ANCESTOR_BG: std::cell::RefCell<
        std::collections::HashMap<NodeId, (u64, Option<Color>)>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Fingerprint of a node's style-cache identity — the render-time analogue of
/// Python's `visual_style` cache validity. Two components:
///
/// 1. The node's OWN (non-inherited) style identity (Python `styles._cache_key`):
///    it must change when the node's own styling changes (so it re-captures the
///    live ancestor surface). `background` is not an inherited property, so a
///    child's own `bg`/tint/opacity plus interaction state capture the cases
///    that would re-bake `visual_style`.
/// 2. The ANCESTOR selector-identity chain (type/id/classes/pseudo-states from
///    the render-time selector stack). In Python, a class or pseudo-class
///    change anywhere in the chain routes through `app.update_styles(node)`,
///    which re-applies the stylesheet to the node and ALL descendants and
///    clears each descendant's cached `visual_style` (`notify_style_update`) —
///    so e.g. an ancestor losing `:focus` (dropping a `background-tint` rule,
///    the `Select:focus > SelectCurrent` case) re-bakes the child's transparent
///    glyphs over the fresh ancestor surface.
///
/// LOAD-BEARING BOUNDARY: the fingerprint must stay STABLE under ancestor-only
/// resolved-style VALUE changes (a direct inline mutation like
/// `styles.background = "red"` on an ancestor — the `guide/actions` case).
/// Python inline mutations do NOT cascade `notify_style_update` to
/// descendants, so the child keeps the ancestor surface captured at its own
/// last content render — that deliberate staleness is exactly what
/// `FROZEN_ANCESTOR_BG` replicates (see the note above). Hashing ancestor
/// resolved style VALUES here would destroy it; only selector IDENTITY may be
/// hashed.
fn node_own_style_fingerprint(
    resolved: &crate::style::Style,
    node: &crate::widget_tree::WidgetNode,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    // Component 2: ancestor selector identity — re-capture on ancestor
    // class/pseudo-state changes, never on ancestor inline style values.
    crate::css::ancestor_selector_fingerprint().hash(&mut h);
    if let Some(bg) = resolved.bg {
        (bg.r, bg.g, bg.b, bg.a.to_bits()).hash(&mut h);
    } else {
        0u8.hash(&mut h);
    }
    if let Some(tint) = resolved.background_tint {
        (
            tint.color.r,
            tint.color.g,
            tint.color.b,
            tint.color.a.to_bits(),
            tint.percent,
        )
            .hash(&mut h);
    }
    if let Some(op) = resolved.opacity {
        op.hash(&mut h);
    }
    if let Some(fg) = resolved.fg {
        (fg.r, fg.g, fg.b, fg.a.to_bits()).hash(&mut h);
    }
    (
        node.state.focused,
        node.state.hovered,
        node.state.disabled,
        node.state.loading,
    )
        .hash(&mut h);
    node.css_id.hash(&mut h);
    let mut classes: Vec<&str> = node.classes.iter().map(|s| s.as_str()).collect();
    classes.sort_unstable();
    classes.hash(&mut h);
    h.finish()
}

/// Re-key the FROZEN ancestor surface into a node's already-baked content glyph
/// segments (Python `visual_style` cache parity). Only segments that (a) belong
/// to this node, (b) are tagged `textual:no_text_style` (the text renderer's
/// glyph strips — NOT the surface/pad fill, which is untagged), and (c) still
/// carry the LIVE surface as their baked bg are re-keyed to the frozen surface.
/// Condition (c) keeps segments that carry their own opaque bg (spans, per-cell
/// backgrounds) untouched. See `FROZEN_ANCESTOR_BG` for the rationale.
fn recolor_frozen_content_bg(
    segments: Segments,
    node_id: NodeId,
    live: Color,
    frozen: Color,
) -> Segments {
    let live_simple = live.to_simple_opaque();
    let frozen_simple = frozen.to_simple_opaque();
    let want_id = node_id_to_ffi(node_id) as i64;
    segments
        .into_iter()
        .map(|mut seg| {
            if seg.control.is_some() {
                return seg;
            }
            let is_content = seg
                .meta
                .as_ref()
                .and_then(|m| m.meta.as_ref())
                .map(|map| {
                    matches!(map.get("textual:widget_id"), Some(MetaValue::Int(v)) if *v == want_id)
                        && matches!(
                            map.get("textual:no_text_style"),
                            Some(MetaValue::Bool(true))
                        )
                })
                .unwrap_or(false);
            if is_content {
                if let Some(style) = seg.style.as_mut() {
                    if style.bgcolor == Some(live_simple) {
                        style.bgcolor = Some(frozen_simple);
                    }
                }
            }
            seg
        })
        .collect()
}

fn scrollbar_drag_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("TEXTUAL_DEBUG_SCROLLBAR_DRAG_TRACE")
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

impl App {
    pub fn render(&mut self, renderable: &dyn Renderable) -> crate::Result<()> {
        self.refresh_size()?;
        let base_style = self.theme.base.to_rich();
        let next =
            FrameBuffer::from_renderable(&self.console, &self.options, renderable, base_style);
        let now = std::time::Instant::now();
        let dt_ms = now.duration_since(self.last_render_at).as_millis();
        self.last_render_at = now;
        let clear_before_draw = self.clear_on_next_render;
        let diff = prepend_clear_if_needed(
            diff_body_for_draw(
                &next,
                &self.frame,
                clear_before_draw,
                None,
                self.theme.base.to_rich(),
            ),
            clear_before_draw,
        );
        let stream_stats = analyze_segment_stream(&diff, next.width);
        debug_render(&format!(
            "[render] dt={}ms resized={} clear={} size={}x{} prev={}x{} diff.segments={} (control={} text_segments={} text_bytes={})",
            dt_ms,
            self.resized_since_last_render,
            clear_before_draw,
            next.width,
            next.height,
            self.frame.width,
            self.frame.height,
            diff.len(),
            stream_stats.controls,
            stream_stats.text_segments,
            stream_stats.text_bytes
        ));
        if resize_trace_enabled() && (self.resized_since_last_render || clear_before_draw) {
            debug_render(&format!(
                "[render_trace] kind=render size={}x{} controls={} home={} clear={} cr={} move_to={} cursor_moves={} text_segments={} text_bytes={} newlines={} touch_last_col={} overflow_right={} max_cursor=({}, {}) control_head=[{}]",
                next.width,
                next.height,
                stream_stats.controls,
                stream_stats.home,
                stream_stats.clear,
                stream_stats.carriage_return,
                stream_stats.move_to,
                stream_stats.cursor_moves,
                stream_stats.text_segments,
                stream_stats.text_bytes,
                stream_stats.newline_text,
                stream_stats.touch_last_col,
                stream_stats.overflow_right,
                stream_stats.max_cursor_x,
                stream_stats.max_cursor_y,
                control_head(&diff, 12)
            ));
        }
        self.print_segments(&diff)?;
        self.resized_since_last_render = false;
        self.clear_on_next_render = false;
        self.frame = next;
        Ok(())
    }

    pub fn render_widget(&mut self, widget: &mut dyn Widget) -> crate::Result<()> {
        self.render_widget_with_regions(widget, None, true)
    }

    pub(super) fn render_widget_with_regions(
        &mut self,
        widget: &mut dyn Widget,
        dirty_regions: Option<&[DirtyRegion]>,
        layout_invalidation: bool,
    ) -> crate::Result<()> {
        self.refresh_size()?;
        let _active = set_app_active(self.app_active);
        let _pseudo_state = set_app_runtime_pseudos(AppRuntimePseudos {
            dark: self.dark_mode,
            inline: self.app_inline,
            ansi: self.app_ansi,
            nocolor: self.app_nocolor,
        });

        // Arena-tree rendering is the ONLY render path: walk the arena tree
        // depth-first, rendering each widget at its layout_rect position. The
        // legacy recursive `render_styled()`-from-root fallback was retired
        // (AI_GUIDANCE: fallback rendering was removed); every caller builds the
        // widget tree before rendering, so the active tree is always populated.
        self.render_tree_composed(widget, dirty_regions, layout_invalidation)
    }

    /// Tree-driven render path: walk the arena tree depth-first, rendering
    /// each widget at its `layout_rect` position and compositing into a
    /// single FrameBuffer.
    ///
    /// This replaces the legacy recursive `render_styled()` path when the
    /// active tree is populated.
    fn render_tree_composed(
        &mut self,
        widget: &mut dyn Widget,
        dirty_regions: Option<&[DirtyRegion]>,
        layout_invalidation: bool,
    ) -> crate::Result<()> {
        let (width, height) = self.options.size;
        let base_style = self.theme.base.to_rich();
        let mut next = FrameBuffer::new(width, height, base_style);
        let layers = self.collect_visible_render_layers();
        let mut layout_affected_style_change = false;
        let mut has_underlay = false;

        for layer in layers {
            let debug_label = layer.debug_label();
            let screen_stylesheet = match layer {
                CompositedLayer::AppRoot => None,
                CompositedLayer::Screen(index) => self
                    .screen_stack
                    .get(index)
                    .and_then(|entry| entry.stylesheet.as_ref()),
            };
            let sheet = self.stylesheet_for_layer(screen_stylesheet);
            let _style_guard = set_style_context(sheet);
            begin_style_render_pass();

            let mut tree = match layer {
                CompositedLayer::AppRoot => match self.widget_tree.take() {
                    Some(tree) => tree,
                    None => continue,
                },
                CompositedLayer::Screen(index) => {
                    let Some(entry) = self.screen_stack.get_mut(index) else {
                        continue;
                    };
                    std::mem::take(&mut entry.widget_tree)
                }
            };

            if layout_invalidation {
                let (w, h) = self.options.size;
                run_layout_pass(&mut tree, (w as u16, h as u16));
                apply_layout_info_tree_from_layout_rects(&mut tree);
                let render_nodes = collect_render_nodes(&tree);
                debug_render(&format!(
                    "[layout_pass] layer={} viewport={}x{} render_nodes={}",
                    debug_label,
                    w,
                    h,
                    render_nodes.len()
                ));
            }

            apply_root_tree_virtual_content_size_in_tree(&mut tree);
            sync_host_scrollbar_positions(&mut tree);

            // Install the `:focus-within` set for this layer's tree so rules like
            // `Collapsible:focus-within { background-tint: $foreground 5% }` resolve.
            let _focus_within_guard =
                crate::css::set_focus_within(super::routing::focus_within_ids_tree(&tree));

            match layer {
                CompositedLayer::AppRoot => render_app_root_tree_layer(
                    &tree,
                    widget,
                    &mut next,
                    &self.console,
                    if self.debug_layout.enabled {
                        Some(&self.debug_layout)
                    } else {
                        None
                    },
                ),
                CompositedLayer::Screen(_) => render_screen_tree_layer(
                    &tree,
                    &mut next,
                    &self.console,
                    if self.debug_layout.enabled {
                        Some(&self.debug_layout)
                    } else {
                        None
                    },
                    has_underlay,
                ),
            }

            layout_affected_style_change |= take_layout_affected_style_changes();
            has_underlay = true;

            match layer {
                CompositedLayer::AppRoot => {
                    self.widget_tree = Some(tree);
                }
                CompositedLayer::Screen(index) => {
                    if let Some(entry) = self.screen_stack.get_mut(index) {
                        entry.widget_tree = tree;
                    }
                }
            }
        }

        let now = std::time::Instant::now();
        let dt_ms = now.duration_since(self.last_render_at).as_millis();
        self.last_render_at = now;
        let clear_before_draw = self.clear_on_next_render;
        let diff_body = diff_body_for_draw(
            &next,
            &self.frame,
            clear_before_draw,
            dirty_regions,
            self.theme.base.to_rich(),
        );
        let diff = prepend_clear_if_needed(diff_body, clear_before_draw);
        let stream_stats = analyze_segment_stream(&diff, next.width);
        debug_render(&format!(
            "[render_tree] dt={}ms resized={} clear={} size={}x{} prev={}x{} diff.segments={} (control={} text_segments={} text_bytes={})",
            dt_ms,
            self.resized_since_last_render,
            clear_before_draw,
            next.width,
            next.height,
            self.frame.width,
            self.frame.height,
            diff.len(),
            stream_stats.controls,
            stream_stats.text_segments,
            stream_stats.text_bytes
        ));
        if resize_trace_enabled() && (self.resized_since_last_render || clear_before_draw) {
            debug_render(&format!(
                "[render_trace] kind=tree size={}x{} controls={} home={} clear={} cr={} move_to={} cursor_moves={} text_segments={} text_bytes={} newlines={} touch_last_col={} overflow_right={} max_cursor=({}, {}) control_head=[{}]",
                next.width,
                next.height,
                stream_stats.controls,
                stream_stats.home,
                stream_stats.clear,
                stream_stats.carriage_return,
                stream_stats.move_to,
                stream_stats.cursor_moves,
                stream_stats.text_segments,
                stream_stats.text_bytes,
                stream_stats.newline_text,
                stream_stats.touch_last_col,
                stream_stats.overflow_right,
                stream_stats.max_cursor_x,
                stream_stats.max_cursor_y,
                control_head(&diff, 12)
            ));
        }
        self.print_segments(&diff)?;
        self.resized_since_last_render = false;
        self.clear_on_next_render = false;
        let next_hit_test = HitTestMap::from_frame(&next);
        let geometry_changed = self.hit_test != next_hit_test;
        self.hit_test = next_hit_test;
        if layout_invalidation || geometry_changed || layout_affected_style_change {
            self.apply_layout_info(widget, &self.hit_test);
        }
        self.frame = next;
        Ok(())
    }

    fn stylesheet_for_layer(&self, screen_sheet: Option<&StyleSheet>) -> StyleSheet {
        let mut sheet = self.default_stylesheet.clone();
        sheet.extend(&self.stylesheet);
        if let Some(screen_sheet) = screen_sheet {
            sheet.extend(screen_sheet);
        }
        sheet
    }

    fn screen_layer_background_is_opaque(&self, index: usize) -> bool {
        let Some(entry) = self.screen_stack.get(index) else {
            return false;
        };
        let Some(root_id) = entry.widget_tree.root() else {
            return false;
        };
        // Verify the root node exists before resolving its style.
        if entry.widget_tree.get(root_id).is_none() {
            return false;
        }

        let sheet = self.stylesheet_for_layer(entry.stylesheet.as_ref());
        let _guard = set_style_context(sheet);
        begin_style_render_pass();
        let meta = node_selector_meta(&entry.widget_tree, root_id);
        let resolved = resolve_node_style(&entry.widget_tree, root_id, &meta);
        resolved.bg.is_some_and(|bg| bg.a >= 1.0)
    }

    fn collect_visible_render_layers(&self) -> Vec<CompositedLayer> {
        let mut front_to_back = Vec::new();
        let screen_count = self.screen_stack.len();

        if screen_count == 0 {
            if self.widget_tree.is_some() {
                front_to_back.push(CompositedLayer::AppRoot);
            }
            return front_to_back;
        }

        let top = screen_count - 1;
        front_to_back.push(CompositedLayer::Screen(top));

        if self.screen_layer_background_is_opaque(top) {
            front_to_back.reverse();
            return front_to_back;
        }

        let mut blocked_by_opaque_screen = false;
        for index in (0..top).rev() {
            front_to_back.push(CompositedLayer::Screen(index));
            if self.screen_layer_background_is_opaque(index) {
                blocked_by_opaque_screen = true;
                break;
            }
        }

        if !blocked_by_opaque_screen && self.widget_tree.is_some() {
            front_to_back.push(CompositedLayer::AppRoot);
        }

        front_to_back.reverse();
        front_to_back
    }

    /// Distribute layout information to the root widget from the hit-test map.
    ///
    /// Root-only: child widgets receive layout info via the tree-based
    /// [`apply_layout_info_tree`] path when the arena tree is available.
    pub(super) fn apply_layout_info(&self, root: &mut dyn Widget, hit_test: &HitTestMap) {
        if let Some(rect) = hit_test.rect(NodeId::default()) {
            // Legacy (non-tree) root path: use widget-based meta.
            let meta = crate::css::selector_meta_generic(root);
            let resolved = crate::css::resolve_style(root, &meta);
            let line_pad = resolved.line_pad.unwrap_or(0) as usize;
            let (top, bottom, left, right) = border_spacing_from_style(&resolved);
            let full_w = rect.x1.saturating_sub(rect.x0) as usize + 1;
            let full_h = rect.y1.saturating_sub(rect.y0) as usize + 1;
            let content_w = full_w
                .saturating_sub(left + right)
                .saturating_sub(line_pad.saturating_mul(2))
                .max(1) as u16;
            let content_h = full_h.saturating_sub(top + bottom).max(1) as u16;
            root.on_layout(content_w, content_h);
        }
    }

    pub(super) fn print_segments(&mut self, diff: &rich_rs::Segments) -> crate::Result<()> {
        // Headless (run_test): the in-memory FrameBuffer is the only output —
        // never write ANSI to a real terminal.
        if self.headless {
            return Ok(());
        }
        // Some terminals may silently reset runtime modes (including line wrap)
        // during aggressive resize bursts. Reassert before every frame write.
        let _ = self.driver.reassert_runtime_modes();
        console_write_with_optional_sync(&mut self.console, self.sync_output, |console| {
            console.print_segments(diff)
        })?;
        Ok(())
    }
}

pub(crate) fn console_write_with_optional_sync<W: std::io::Write>(
    console: &mut rich_rs::Console<W>,
    sync_enabled: bool,
    write_payload: impl FnOnce(&mut rich_rs::Console<W>) -> std::io::Result<()>,
) -> std::io::Result<()> {
    if sync_enabled {
        console.write_str(SYNC_START)?;
    }

    write_payload(console)?;

    if sync_enabled {
        console.write_str(SYNC_END)?;
    }
    Ok(())
}

pub(crate) fn prepend_clear_if_needed(diff: Segments, clear_before_draw: bool) -> Segments {
    if !clear_before_draw {
        return diff;
    }
    let mut out = Segments::new();
    out.push(Segment::control(ControlType::Clear));
    out.extend(diff);
    out
}

/// Compute the diff segment stream for a frame about to be drawn.
///
/// When `clear_before_draw` is set, a `Clear` control is prepended (see
/// [`prepend_clear_if_needed`]) which blanks the whole terminal. In that case
/// the diff MUST be taken against a BLANK frame of the same size — not the
/// previous frame — otherwise unchanged cells are not re-emitted and the clear
/// wipes them off-screen (a stale-frame diff). When not clearing, the previous
/// frame is used, optionally region-masked.
pub(crate) fn diff_body_for_draw(
    next: &FrameBuffer,
    previous: &FrameBuffer,
    clear_before_draw: bool,
    dirty_regions: Option<&[DirtyRegion]>,
    base_style: Option<rich_rs::Style>,
) -> Segments {
    if clear_before_draw {
        let blank = FrameBuffer::new(next.width, next.height, base_style);
        next.diff_to_segments(&blank)
    } else if let Some(regions) = dirty_regions {
        next.diff_to_segments_in_regions(previous, regions)
    } else {
        next.diff_to_segments(previous)
    }
}

pub(crate) fn analyze_segment_stream(segments: &Segments, width: usize) -> SegmentStreamStats {
    let mut stats = SegmentStreamStats::default();
    let mut cursor_x = 0usize;
    let mut cursor_y = 0usize;

    for segment in segments.iter() {
        if let Some(control) = segment.control.as_ref() {
            stats.controls += 1;
            match control {
                ControlType::Home => {
                    stats.home += 1;
                    cursor_x = 0;
                    cursor_y = 0;
                }
                ControlType::Clear => {
                    stats.clear += 1;
                    cursor_x = 0;
                    cursor_y = 0;
                }
                ControlType::CarriageReturn => {
                    stats.carriage_return += 1;
                    cursor_x = 0;
                }
                ControlType::CursorUp(n) => {
                    stats.cursor_moves += 1;
                    cursor_y = cursor_y.saturating_sub(*n as usize);
                }
                ControlType::CursorDown(n) => {
                    stats.cursor_moves += 1;
                    cursor_y = cursor_y.saturating_add(*n as usize);
                }
                ControlType::CursorForward(n) => {
                    stats.cursor_moves += 1;
                    cursor_x = cursor_x.saturating_add(*n as usize);
                }
                ControlType::CursorBackward(n) => {
                    stats.cursor_moves += 1;
                    cursor_x = cursor_x.saturating_sub(*n as usize);
                }
                ControlType::MoveTo { x, y } => {
                    stats.move_to += 1;
                    cursor_x = *x as usize;
                    cursor_y = *y as usize;
                }
                _ => {}
            }
            stats.max_cursor_x = stats.max_cursor_x.max(cursor_x);
            stats.max_cursor_y = stats.max_cursor_y.max(cursor_y);
            continue;
        }

        if segment.text.is_empty() {
            continue;
        }

        stats.text_segments += 1;
        stats.text_bytes += segment.text.len();
        let newline_count = segment.text.as_ref().matches('\n').count();
        stats.newline_text += newline_count;

        let text_width = rich_rs::cell_len(segment.text.as_ref());
        if width > 0 && text_width > 0 {
            let end_x = cursor_x.saturating_add(text_width - 1);
            if end_x == width - 1 {
                stats.touch_last_col += 1;
            }
            if end_x >= width {
                stats.overflow_right += 1;
            }
        }
        cursor_x = cursor_x.saturating_add(text_width);
        stats.max_cursor_x = stats.max_cursor_x.max(cursor_x);
        stats.max_cursor_y = stats.max_cursor_y.max(cursor_y);
    }

    stats
}

pub(crate) fn control_head(segments: &Segments, limit: usize) -> String {
    let mut labels: Vec<String> = Vec::new();
    for segment in segments.iter() {
        let Some(control) = segment.control.as_ref() else {
            continue;
        };
        let label = match control {
            ControlType::Home => "Home".to_string(),
            ControlType::Clear => "Clear".to_string(),
            ControlType::CarriageReturn => "CR".to_string(),
            ControlType::CursorUp(n) => format!("Up({n})"),
            ControlType::CursorDown(n) => format!("Down({n})"),
            ControlType::CursorForward(n) => format!("Right({n})"),
            ControlType::CursorBackward(n) => format!("Left({n})"),
            ControlType::MoveTo { x, y } => format!("MoveTo({x},{y})"),
            ControlType::EraseInLine(mode) => format!("EraseInLine({mode})"),
            ControlType::ShowCursor => "ShowCursor".to_string(),
            ControlType::HideCursor => "HideCursor".to_string(),
            _ => format!("{control:?}"),
        };
        labels.push(label);
        if labels.len() >= limit {
            break;
        }
    }
    labels.join(", ")
}

// ===========================================================================
// Tree-driven compositor: render each widget at its layout_rect position
// ===========================================================================

/// Recursively render a tree node and its children into the frame buffer.
///
/// Each widget is rendered at its `layout_rect` position with CSS style
/// stack management for proper inheritance. The style stack must already
/// contain the ancestor chain when this function is called.
fn render_tree_node(
    tree: &WidgetTree,
    node_id: NodeId,
    ctx: TreeRenderCtx,
    frame: &mut FrameBuffer,
    console: &rich_rs::Console,
    debug: Option<&crate::debug::DebugLayout>,
    overlays: &mut Vec<QueuedOverlay>,
) {
    let node = match tree.get(node_id) {
        Some(n) => n,
        None => return,
    };

    // Skip non-displayed nodes entirely (no layout, no render).
    if !node.display {
        return;
    }

    let rect = node.layout_rect;
    let w = rect.width() as usize;
    let h = rect.height() as usize;

    // Only render if the node is visible AND has a non-zero extent.
    let should_render = node.visibility == crate::style::Visibility::Visible && w > 0 && h > 0;

    // Resolve style early — needed for outline, hatch, overlay, and children.
    // Use node-record-based meta (reads css_id, node.state, node.classes from
    // the arena) so layout and render are consistent (§T-7, step 3a).
    let meta = node_selector_meta_from_node(node, node_id);
    let resolved = resolve_node_style(tree, node_id, &meta);

    // `overlay: screen` escape (Python `_compositor.py:660-687`): a node with
    // this mode is NOT painted inline at its position in the tree. It is forced
    // to the TOP z of the whole layer with NO clip — how Select dropdowns,
    // CommandPalette, toasts, tooltips and loading float. Queue it (with its
    // arranged position + constrain) and return WITHOUT painting; the queued set
    // is drained AFTER the layer walk by `paint_deferred_overlays`. This mirrors
    // the deferred-paint pattern already used for `outline`/`hatch` below, but at
    // layer scope rather than per-node. The one exception is the deferred pass
    // itself: when this node IS the overlay root being painted, `overlay_root_exempt`
    // matches its id and it renders inline (its descendants still queue nested overlays).
    if should_render
        && matches!(resolved.overlay, Some(OverlayMode::Screen))
        && ctx.overlay_root_exempt != Some(node_id)
    {
        let (cx, cy) = resolve_axis_constrain(&resolved);
        overlays.push(QueuedOverlay {
            node_id,
            natural_x: i32::from(rect.x0) + ctx.origin_x,
            natural_y: i32::from(rect.y0) + ctx.origin_y,
            rect_x0: i32::from(rect.x0),
            rect_y0: i32::from(rect.y0),
            w,
            h,
            cx,
            cy,
        });
        return;
    }

    // Cover-widget parity (Python `Widget._render_widget` + `_compositor.py`
    // cover handling): while a cover widget is set (the `loading` reactive
    // covers a node with a `LoadingIndicator`), the COVER's visuals render in
    // place of this node's own — same region — and the node's children are not
    // painted (`_compositor.py:680`: placements are skipped while covered).
    if let Some(cover) = node.cover_widget.as_deref() {
        if should_render {
            render_cover_widget(
                cover, node, node_id, rect, w, h, ctx, meta, resolved, frame, console, debug,
            );
        }
        return;
    }

    // CSS `outline` is drawn OVER this node's own edge cells (without reserving
    // layout space) AND over any child content composited at those edges. It is
    // therefore computed here (while the ancestor style stack is the base
    // background) but PAINTED after children render — see `deferred_outline`
    // below. Mirrors Python `StylesCache.render_line` outline block.
    let mut deferred_outline: Option<(Vec<OutlineCell>, i32, i32, ClipRect)> = None;

    // CSS `hatch` fills the widget's blank cells with a repeating glyph. In
    // Python this is applied via `line_post` to the widget's OWN content line,
    // so the hatch covers the widget's full inner area — including the content
    // row. In textual-rs, `.class()`/`.id()` on a leaf wraps it in a `Node`
    // (border + hatch on the wrapper, raw text in an inner child). The inner
    // content child renders AFTER the wrapper, so applying hatch before children
    // lets the child's blank content line overpaint (un-hatch) the first inner
    // row. Defer the fill until after children render (it only touches blank
    // cells, preserving real content), mirroring Python's whole-inner-area hatch.
    let mut deferred_hatch: Option<(Hatch, Option<Color>, i32, i32, usize, usize, ClipRect)> = None;

    if should_render {
        let dest_x = i32::from(rect.x0) + ctx.origin_x;
        let dest_y = i32::from(rect.y0) + ctx.origin_y;

        // Create options sized to this widget's layout rect.
        let mut opts = rich_rs::ConsoleOptions::default();
        opts.size = (w, h);
        opts.max_width = w;
        opts.max_height = h;

        // Build the debug label from the node record (css_id, classes).
        let debug_label = {
            let id_part = node
                .css_id
                .as_deref()
                .map(|id| format!("#{id}"))
                .unwrap_or_default();
            let class_parts: Vec<String> = node.classes.iter().map(|c| format!(".{c}")).collect();
            format!(
                "{}{}{}",
                node.widget.style_type(),
                id_part,
                class_parts.join("")
            )
        };

        // Set dispatch context so node_state()/node_id() work during render
        // (§T-6, step 3a): widgets can call self.node_state() inside render().
        let _dispatch_guard = set_dispatch_recipient(node_id, node.state);

        // Python `visual_style` caching parity: capture/refresh the composited
        // ancestor surface this node bakes into its transparent glyph segments,
        // keyed on the node's OWN style identity (`node_own_style_fingerprint`,
        // the render-time analogue of Python `styles._cache_key`). See the note
        // on `FROZEN_ANCESTOR_BG` above.
        let live_ancestor_bg = crate::css::current_composited_background();
        let own_fp = node_own_style_fingerprint(&resolved, node);
        let frozen_ancestor_bg = FROZEN_ANCESTOR_BG.with(|cache| {
            let mut cache = cache.borrow_mut();
            match cache.get(&node_id) {
                Some(&(fp, bg)) if fp == own_fp => bg,
                _ => {
                    cache.insert(node_id, (own_fp, live_ancestor_bg));
                    live_ancestor_bg
                }
            }
        });

        // render_widget_with_meta handles CSS composition, border rendering,
        // segment tagging with the real arena NodeId, and style stack push/pop.
        let mut segments = crate::widgets::render_widget_with_meta(
            node.widget.as_ref(),
            console,
            &opts,
            debug,
            node_id,
            &meta,
            &resolved,
            &debug_label,
        );

        // When the composited ancestor surface has DIVERGED from what this node
        // captured at its own last content render (an ancestor-only background
        // change, e.g. `guide/actions` pressing `r` to set the screen bg), Rust
        // baked the LIVE surface into the node's transparent glyph segments while
        // Python keeps the CACHED base surface in `visual_style`. Re-key ONLY the
        // content glyph segments (`textual:no_text_style`, tagged by the text
        // renderer) whose baked bg still equals the live surface back to the
        // frozen surface — Python's cached `visual_style`. The node's own
        // surface/padding fill (computed after self is popped, from
        // `background_colors`) is untagged and stays LIVE, preserving the
        // documented render-time live-composition invariant for surfaces.
        if let (Some(frozen), Some(live)) = (frozen_ancestor_bg, live_ancestor_bg) {
            if frozen != live {
                segments = recolor_frozen_content_bg(segments, node_id, live, frozen);
            }
        }
        if node.widget.preserve_underlay() && !segments.is_empty() {
            if let Some(bg) = resolved.bg {
                let widget_clip = ClipRect {
                    x0: dest_x,
                    y0: dest_y,
                    x1: dest_x + w as i32,
                    y1: dest_y + h as i32,
                };
                if let Some(paint_clip) = ctx.clip.intersect(widget_clip) {
                    if bg.a >= 1.0 {
                        fill_rect_with_background(frame, paint_clip, bg);
                    } else if bg.a > 0.0 {
                        tint_rect_with_background(frame, paint_clip, bg);
                    }
                }
            }
        }

        // P2-31: When text-wrap is nowrap with an overflow mode, don't pre-crop
        // lines so that apply_text_overflow_to_line can handle truncation with
        // the correct mode (ellipsis/clip). Otherwise, split_and_crop_lines
        // would already crop to `w`, making the overflow step a no-op.
        let overflow_mode = text_overflow_mode(&resolved);
        let crop_width = if overflow_mode.is_some() {
            // Use natural per-line width so overflow truncation runs on the
            // original lines (including explicit `\n` segment breaks).
            let mut natural = 0usize;
            let mut line_width = 0usize;
            for segment in &segments {
                if segment.control.is_some() {
                    natural = natural.max(line_width);
                    line_width = 0;
                    continue;
                }
                let text = segment.text.as_ref();
                if text.is_empty() {
                    continue;
                }
                let mut parts = text.split('\n').peekable();
                while let Some(part) = parts.next() {
                    line_width = line_width.saturating_add(rich_rs::cell_len(part));
                    if parts.peek().is_some() {
                        natural = natural.max(line_width);
                        line_width = 0;
                    }
                }
            }
            natural = natural.max(line_width);
            natural.max(w)
        } else {
            w
        };
        // Structural tree nodes (for example Overlay modal layers) may render
        // no segments of their own. If they also don't paint any surface style,
        // don't synthesize padded blank lines, or they'd erase underlay content.
        let has_surface_paint = resolved.bg.is_some()
            || resolved.hatch.is_some()
            || resolved.border_top.is_set()
            || resolved.border_right.is_set()
            || resolved.border_bottom.is_set()
            || resolved.border_left.is_set()
            || resolved.outline_top.is_set()
            || resolved.outline_right.is_set()
            || resolved.outline_bottom.is_set()
            || resolved.outline_left.is_set();
        let pad_lines = if segments.is_empty() && !has_surface_paint {
            false
        } else {
            !node.widget.preserve_underlay()
        };
        let lines =
            rich_rs::Segment::split_and_crop_lines(segments, crop_width, None, pad_lines, false);

        let lines = if let Some(overflow) = overflow_mode {
            lines
                .into_iter()
                .map(|line| apply_text_overflow_to_line(&line, w, overflow))
                .collect()
        } else {
            lines
        };

        let frame_clip = ClipRect::for_frame(frame);
        let Some(paint_clip) = ctx.clip.intersect(frame_clip) else {
            return;
        };
        for (row_idx, line) in lines.iter().enumerate() {
            let y = dest_y + row_idx as i32;
            if y < paint_clip.y0 {
                continue;
            }
            if y >= paint_clip.y1 {
                break;
            }
            let line_start = dest_x;
            let line_end = dest_x + w as i32;
            let x0 = line_start.max(paint_clip.x0);
            let x1 = line_end.min(paint_clip.x1);
            if x1 <= x0 {
                continue;
            }
            let crop_start = (x0 - line_start) as usize;
            let crop_width = (x1 - x0) as usize;
            let cropped = if crop_start == 0 && crop_width == w {
                line.clone()
            } else {
                crop_line_horizontal(line, crop_start, crop_width)
            };
            frame.write_line_at(x0 as usize, y as usize, &cropped, false);
        }

        // Empty-Screen runtime background composite.
        //
        // The Screen surface widget (`AppRoot`, `style_type() == "Screen"`) bakes
        // its blank surface from its OWN seed style, so a background set at
        // RUNTIME on the Screen *node* — e.g. `query_mut("Screen").set_styles(
        // |s| s.set_bg(red))` or `run_action("set_background('red')")` — never
        // reaches the widget's baked surface (the node style and the widget seed
        // are distinct). Without this, an empty Screen with a dynamically-set
        // inline background paints 0 colored cells: the resolved node bg is red
        // but every surface cell is still the stale theme base.
        //
        // The compositor owns surface compositing, so re-fill the Screen node's
        // CONTENT box here with the RESOLVED node background (which includes the
        // runtime inline bg). This mirrors `render_screen_tree_layer`'s
        // top-of-layer fill for the screen-stack case, and Python's
        // `Screen.styles.background` driving the screen blank. Children render
        // AFTER this block (see below), so they still composite on top. Only runs
        // for opaque backgrounds on the screen-surface node; every other widget
        // is unaffected.
        //
        // The fill is scoped to the CONTENT box (inside any border/padding), NOT
        // the full layout rect, so a Screen with `border:` (e.g. the `screen`
        // styles demo: `Screen { background: darkblue; border: heavy white }`)
        // keeps its border chrome — the widget already rendered the border into
        // its segments above, and this fill must not clobber those edge cells.
        // For a borderless empty Screen the content box equals the layout rect,
        // so the runtime bg still fills the whole surface (actions01/02).
        if node_is_screen_surface(node) {
            if let Some(bg) = resolved.bg {
                if bg.a >= 1.0 {
                    let content = node_content_or_layout_rect(node);
                    let cx0 = i32::from(content.x0) + ctx.origin_x;
                    let cy0 = i32::from(content.y0) + ctx.origin_y;
                    let cx1 = i32::from(content.x1) + ctx.origin_x;
                    let cy1 = i32::from(content.y1) + ctx.origin_y;
                    let content_clip = ClipRect { x0: cx0, y0: cy0, x1: cx1, y1: cy1 };
                    if let Some(fill_clip) = paint_clip.intersect(content_clip) {
                        fill_rect_with_background(frame, fill_clip, bg);
                    }
                }
            }
        }

        // CSS `outline`: compute perimeter cells now (the ancestor style stack
        // still represents the base/parent background), but defer painting until
        // AFTER children render so the outline overdraws the final composited
        // content at this node's edges. This makes outline correct for both leaf
        // widgets and containers that wrap a child (e.g. `Static::new(..).id(..)`,
        // which produces a Node wrapper).
        if resolved.outline_top.is_set()
            || resolved.outline_right.is_set()
            || resolved.outline_bottom.is_set()
            || resolved.outline_left.is_set()
        {
            let outer_bg = crate::css::current_composited_background().unwrap_or_else(|| {
                crate::style::parse_color_like("$background")
                    .unwrap_or(crate::style::Color::rgb(0, 0, 0))
            });
            let inner_bg = resolved
                .bg
                .map(|c| c.flatten_over(outer_bg))
                .unwrap_or(outer_bg);
            let cells = outline_edge_cells(
                w,
                h,
                resolved.outline_top,
                resolved.outline_right,
                resolved.outline_bottom,
                resolved.outline_left,
                inner_bg,
                outer_bg,
            );
            if !cells.is_empty() {
                deferred_outline = Some((cells, dest_x, dest_y, ctx.clip));
            }
        }

        // P2-34: Defer hatch fill until after children render (see the
        // `deferred_hatch` declaration above) so the inner content child of a
        // `.class()`/`.id()` Node wrapper cannot un-hatch the first inner row.
        //
        // Scope the fill to the node's CONTENT box (inside any border/padding),
        // not the full widget box. Python's `line_post` hatch only touches the
        // inner content lines — it must NOT bleed into the border row, where the
        // blank padding spaces around a `border_title` would otherwise be
        // hatched (e.g. ` cross ` -> `╳cross╳`). For a gutterless leaf the
        // content box equals the layout rect, so leaf hatch (Label, no border)
        // is unchanged.
        if let Some(ref hatch) = resolved.hatch {
            let content = node_content_or_layout_rect(node);
            let hx = i32::from(content.x0) + ctx.origin_x;
            let hy = i32::from(content.y0) + ctx.origin_y;
            let hw = content.x1.saturating_sub(content.x0) as usize;
            let hh = content.y1.saturating_sub(content.y0) as usize;
            deferred_hatch = Some((hatch.clone(), resolved.bg, hx, hy, hw, hh, ctx.clip));
        }
    }
    // Clone keyline/layout before push_style_context takes ownership of resolved.
    // These are already folded into `resolved` via resolve_node_style, so no
    // separate inline-style read is needed.
    let node_keyline = resolved.keyline.clone();
    let node_layout = resolved.layout;
    let node_declares_layers = resolved.layers.as_ref().is_some_and(|l| !l.is_empty());
    push_style_context(meta, resolved);

    // Keyline background canvas (Python `layout.py::render_keyline` ->
    // `Canvas.render(primitives, container.rich_style)`): a container with a
    // keyline renders its WHOLE content box as a canvas whose blank cells on every
    // keyline-spanned row carry `fg = base_style.bgcolor` (Canvas.render sets the
    // base span color to the background color), i.e. `fg=<bg> bg=<bg>` — distinct
    // from the screen's `fg=default` base blank. Visible children composite ON TOP
    // of this canvas, so only the cells NOT covered by a visible child (gutter +
    // `visibility:hidden` cells, e.g. the hidden Placeholder in `keyline`) show the
    // canvas color. Paint that solid-fg/bg base here, BEFORE children render, so
    // children overpaint it; the line glyphs are drawn after children by
    // `paint_keylines`. (Every grid/flow content row carries a vertical keyline, so
    // the whole content box is a span row — fill it uniformly.)
    if let Some(ref kl) = node_keyline {
        if kl.keyline_type != KeylineType::None {
            let canvas_bg = crate::css::current_composited_background().unwrap_or_else(|| {
                crate::style::parse_color_like("$background")
                    .unwrap_or(crate::style::Color::rgb(0, 0, 0))
            });
            if let Some(parent_node) = tree.get(node_id) {
                let content_rect = node_content_or_layout_rect(parent_node);
                let cx0 = i32::from(content_rect.x0) + ctx.origin_x;
                let cy0 = i32::from(content_rect.y0) + ctx.origin_y;
                let cx1 = i32::from(content_rect.x1) + ctx.origin_x;
                let cy1 = i32::from(content_rect.y1) + ctx.origin_y;
                let region_clip = ClipRect { x0: cx0, y0: cy0, x1: cx1, y1: cy1 };
                if let Some(paint_clip) = ctx.clip.intersect(region_clip) {
                    fill_rect_solid_fg_bg(frame, paint_clip, canvas_bg);
                }
            }
        }
    }

    // Descendants are never the overlay root; clear the exemption so a nested
    // `overlay: screen` child inside a deferred overlay still escapes to top z.
    let mut ctx = ctx;
    ctx.overlay_root_exempt = None;
    let unclipped_child_ctx = ctx;
    let mut child_ctx = ctx;
    // Clip descendants to this node's content box (inside border + padding) when
    // either the widget opts in (scroll hosts, etc.) OR the node has gutter
    // chrome (border/padding). Python's compositor clips every container's
    // children to `container_region = region.shrink(gutter)` unconditionally
    // (see `_compositor.py` `add_widget`: `sub_clip = clip.intersection(
    // child_region)`), so overflowing children never paint over the parent's
    // own border/padding. For gutterless nodes the content box equals the layout
    // rect, leaving existing behavior unchanged.
    if node.widget.clips_descendants_to_content() || node_has_gutter(node) {
        let clip_rect = node_content_or_layout_rect(node);
        let node_clip = ClipRect {
            x0: i32::from(clip_rect.x0) + ctx.origin_x,
            y0: i32::from(clip_rect.y0) + ctx.origin_y,
            x1: i32::from(clip_rect.x1) + ctx.origin_x,
            y1: i32::from(clip_rect.y1) + ctx.origin_y,
        };
        if let Some(intersection) = child_ctx.clip.intersect(node_clip) {
            child_ctx.clip = intersection;
        } else {
            pop_style_context();
            return;
        }
    }
    // Paint children in CSS-layer order (Python `_compositor.py`: the parent's
    // `layers` declaration orders its layers bottom→top, so a child on a later
    // layer paints on top). The recursive paint walk previously used raw DOM
    // order, so overlapping widgets on DIFFERENT layers stacked by compose
    // order instead — in guide/layout/layers `#box2` (`layer: below`, composed
    // second) painted OVER `#box1` (`layer: above`). Gated on the node's
    // resolved `layers` so layer-less parents keep the plain DOM walk
    // (`sort_children_by_layer` is DOM-stable within a layer).
    let child_ids: Vec<NodeId> = if node_declares_layers {
        sort_children_by_layer(tree, node_id, tree.children(node_id))
    } else {
        tree.children(node_id).to_vec()
    };
    let (scroll_x, scroll_y) = node.widget.scroll_offset_f32();
    let base_child_ctx = child_ctx;
    let mut scrolled_child_ctx = child_ctx;
    scrolled_child_ctx.origin_x -= scroll_x.round() as i32;
    scrolled_child_ctx.origin_y -= scroll_y.round() as i32;
    let has_scroll_viewport =
        if let Some((viewport_w, viewport_h)) = node.widget.scroll_viewport_size() {
            let content_rect = node_content_or_layout_rect(node);
            let clip = ClipRect {
                x0: i32::from(content_rect.x0) + ctx.origin_x,
                y0: i32::from(content_rect.y0) + ctx.origin_y,
                x1: i32::from(content_rect.x0) + ctx.origin_x + viewport_w as i32,
                y1: i32::from(content_rect.y0) + ctx.origin_y + viewport_h as i32,
            };
            if let Some(intersection) = scrolled_child_ctx.clip.intersect(clip) {
                scrolled_child_ctx.clip = intersection;
            }
            true
        } else {
            false
        };
    for child_id in child_ids {
        let is_dedicated_scrollbar = node_is_dedicated_scrollbar(tree, child_id);
        let use_scroll_ctx = has_scroll_viewport
            && child_uses_parent_scroll(tree, child_id)
            && !is_dedicated_scrollbar;
        let mut next_ctx = if is_dedicated_scrollbar {
            unclipped_child_ctx
        } else if use_scroll_ctx {
            scrolled_child_ctx
        } else {
            base_child_ctx
        };
        if is_dedicated_scrollbar {
            // A dedicated scrollbar lives in the host's reserved gutter, which is
            // OUTSIDE the host's (viewport-shrunk) content box. The clip inherited
            // from the host therefore excludes the lane and would erase the bar.
            // Expand the clip to cover the scrollbar's own layout rect (bounded by
            // the frame) so the thumb glyphs and track paint into the gutter.
            if let Some(child) = tree.get(child_id) {
                let rect = child.layout_rect;
                let frame_clip = ClipRect::for_frame(frame);
                let lane_clip = ClipRect {
                    x0: i32::from(rect.x0) + unclipped_child_ctx.origin_x,
                    y0: i32::from(rect.y0) + unclipped_child_ctx.origin_y,
                    x1: i32::from(rect.x1) + unclipped_child_ctx.origin_x,
                    y1: i32::from(rect.y1) + unclipped_child_ctx.origin_y,
                };
                if let Some(lane_clip) = lane_clip.intersect(frame_clip) {
                    next_ctx.clip = ClipRect {
                        x0: next_ctx.clip.x0.min(lane_clip.x0),
                        y0: next_ctx.clip.y0.min(lane_clip.y0),
                        x1: next_ctx.clip.x1.max(lane_clip.x1),
                        y1: next_ctx.clip.y1.max(lane_clip.y1),
                    };
                }
            }
        }
        if use_scroll_ctx {
            if let Some(child) = tree.get(child_id) {
                let rect = child.layout_rect;
                // The child's layout_rect is in the host's VIRTUAL (unscrolled)
                // space; the child paints at the SCROLLED origin
                // (`next_ctx.origin == base - scroll_offset`). The clip must
                // bound where the child actually paints, so translate the rect
                // by the scrolled origin. Using the unscrolled origin here made
                // every child's clip miss its painted position as soon as the
                // host scrolled (offset != 0): the on-screen child was culled
                // (empty clip intersection) and the viewport went blank.
                let child_clip = ClipRect {
                    x0: i32::from(rect.x0) + next_ctx.origin_x,
                    y0: i32::from(rect.y0) + next_ctx.origin_y,
                    x1: i32::from(rect.x1) + next_ctx.origin_x,
                    y1: i32::from(rect.y1) + next_ctx.origin_y,
                };
                if let Some(intersection) = next_ctx.clip.intersect(child_clip) {
                    next_ctx.clip = intersection;
                } else {
                    continue;
                }
            }
        }
        render_tree_node(tree, child_id, next_ctx, frame, console, debug, overlays);
    }

    // P2-34: Paint keylines between children (after children are rendered).
    if let Some(ref keyline) = node_keyline {
        paint_keylines(
            tree,
            node_id,
            node_layout.unwrap_or(Layout::Vertical),
            keyline,
            ctx,
            frame,
        );
    }

    // CSS `hatch`: fill the widget's still-blank inner cells AFTER children have
    // composited, so the inner content row participates in the hatch (it only
    // touches blank cells, preserving real content). Painted before `outline` so
    // an outline still draws on top.
    if let Some((hatch, bg, dest_x, dest_y, w, h, clip)) = deferred_hatch {
        apply_hatch_fill(frame, &hatch, bg, dest_x, dest_y, w, h, clip);
    }

    // CSS `outline`: paint perimeter cells over the final composited content at
    // this node's edges (after children). Glyphs and styles were baked above.
    if let Some((cells, dest_x, dest_y, clip)) = deferred_outline {
        paint_outline_cells(frame, &cells, dest_x, dest_y, clip);
    }

    pop_style_context();
}

/// Paint a node's COVER widget (Python `Widget._render_widget`) into the
/// node's layout rect, replacing the node's own visuals.
///
/// The node's own style context is pushed first (Python `Widget._cover` sets
/// `widget._parent = self`), so the cover's translucent `$boost` surface
/// composes over the covered widget's own background. The cover resolves with
/// the `-textual-loading-indicator` class added by `set_loading`, so the
/// `LoadingIndicator.-textual-loading-indicator { bg: $boost; }` default rule
/// applies exactly as in Python.
#[allow(clippy::too_many_arguments)]
fn render_cover_widget(
    cover: &dyn Widget,
    node: &crate::widget_tree::WidgetNode,
    node_id: NodeId,
    rect: crate::widget_tree::Rect,
    w: usize,
    h: usize,
    ctx: TreeRenderCtx,
    meta: crate::css::SelectorMeta,
    resolved: crate::style::Style,
    frame: &mut FrameBuffer,
    console: &rich_rs::Console,
    debug: Option<&crate::debug::DebugLayout>,
) {
    let dest_x = rect.x0 + ctx.origin_x;
    let dest_y = rect.y0 + ctx.origin_y;

    let opts = rich_rs::ConsoleOptions {
        size: (w, h),
        max_width: w,
        max_height: h,
        ..Default::default()
    };

    let _dispatch_guard = set_dispatch_recipient(node_id, node.state);

    // Parent the cover to this node (Python `_cover`: `widget._parent = self`).
    push_style_context(meta, resolved);

    let cover_meta = crate::css::cover_selector_meta(cover);
    let cover_resolved = crate::css::resolve_style_for_meta(&cover_meta);

    let debug_label = format!("{}(cover)", cover.style_type());
    let segments = crate::widgets::render_widget_with_meta(
        cover,
        console,
        &opts,
        debug,
        node_id,
        &cover_meta,
        &cover_resolved,
        &debug_label,
    );
    pop_style_context();

    let lines = rich_rs::Segment::split_and_crop_lines(segments, w, None, true, false);
    let frame_clip = ClipRect::for_frame(frame);
    let Some(paint_clip) = ctx.clip.intersect(frame_clip) else {
        return;
    };
    for (row_idx, line) in lines.iter().enumerate() {
        let y = dest_y + row_idx as i32;
        if y < paint_clip.y0 {
            continue;
        }
        if y >= paint_clip.y1 {
            break;
        }
        let line_start = dest_x;
        let line_end = dest_x + w as i32;
        let x0 = line_start.max(paint_clip.x0);
        let x1 = line_end.min(paint_clip.x1);
        if x1 <= x0 {
            continue;
        }
        let crop_start = (x0 - line_start) as usize;
        let crop_width = (x1 - x0) as usize;
        let cropped = if crop_start == 0 && crop_width == w {
            line.clone()
        } else {
            crop_line_horizontal(line, crop_start, crop_width)
        };
        frame.write_line_at(x0 as usize, y as usize, &cropped, false);
    }
}

/// Paint precomputed `outline` perimeter cells into the frame buffer.
///
/// Each cell is `(col, row, glyph, style)` in widget-local coordinates; it is
/// written at `(dest_x + col, dest_y + row)` if inside the clip and frame.
fn paint_outline_cells(
    frame: &mut FrameBuffer,
    cells: &[OutlineCell],
    dest_x: i32,
    dest_y: i32,
    clip: ClipRect,
) {
    let frame_clip = ClipRect::for_frame(frame);
    let Some(paint_clip) = clip.intersect(frame_clip) else {
        return;
    };
    for (col, row, ch, style) in cells {
        let x = dest_x + *col as i32;
        let y = dest_y + *row as i32;
        if x < paint_clip.x0 || x >= paint_clip.x1 || y < paint_clip.y0 || y >= paint_clip.y1 {
            continue;
        }
        let (ux, uy) = (x as usize, y as usize);
        if ux >= frame.width || uy >= frame.height {
            continue;
        }
        let cell = frame.get_mut(ux, uy);
        cell.text = ch.to_string();
        cell.style = Some(*style);
        cell.continuation = false;
    }
}

fn render_app_root_tree_layer(
    tree: &WidgetTree,
    root_widget: &mut dyn Widget,
    frame: &mut FrameBuffer,
    console: &rich_rs::Console,
    debug: Option<&crate::debug::DebugLayout>,
) {
    let width = frame.width;
    let height = frame.height;
    let root_node_id = tree.root().unwrap_or_default();

    let mut opts = rich_rs::ConsoleOptions::default();
    opts.size = (width, height);
    opts.max_width = width;
    opts.max_height = height;

    let root_segments = root_widget.render_styled_dyn_obj(console, &opts, debug, root_node_id);
    let root_lines = Segment::split_and_crop_lines(root_segments, width, None, true, false);
    for (row, line) in root_lines.iter().enumerate() {
        frame.write_line_at(0, row, line, true);
    }

    let Some(root_id) = tree.root() else {
        return;
    };

    // NOTE: tree.root() is a TreeStubWidget. Use the real root_widget for
    // style resolution so root inline styles propagate to children.
    let root_meta = crate::css::selector_meta_generic(root_widget);
    let root_resolved = crate::css::resolve_style(root_widget, &root_meta);
    push_style_context(root_meta, root_resolved);

    let child_ids: Vec<NodeId> = tree.children(root_id).to_vec();
    let (root_scroll_x, root_scroll_y) = root_widget.scroll_offset_f32();
    if scrollbar_drag_trace_enabled()
        && (root_scroll_x.abs() > f32::EPSILON || root_scroll_y.abs() > f32::EPSILON)
    {
        let mut child_summary = Vec::new();
        for &child_id in &child_ids {
            if let Some(node) = tree.get(child_id) {
                let docked = node_is_docked(tree, child_id);
                let rect = node.layout_rect;
                child_summary.push(format!(
                    "{}:{}:{}..{}x{}..{}",
                    node.widget.style_type(),
                    if docked { "dock" } else { "flow" },
                    rect.x0,
                    rect.x1,
                    rect.y0,
                    rect.y1
                ));
            }
        }
        debug_render(&format!(
            "[render-root-scroll] layer=app root_scroll=({:.3}, {:.3}) children=[{}]",
            root_scroll_x,
            root_scroll_y,
            child_summary.join(", ")
        ));
    }

    let base_ctx = TreeRenderCtx {
        origin_x: 0,
        origin_y: 0,
        clip: ClipRect::for_frame(frame),
        overlay_root_exempt: None,
    };
    let scroll_clip = root_widget
        .scroll_viewport_size()
        .map(|(vw, vh)| ClipRect {
            x0: 0,
            y0: 0,
            x1: vw.min(width) as i32,
            y1: vh.min(height) as i32,
        })
        .unwrap_or_else(|| ClipRect::for_frame(frame));
    let scroll_ctx = TreeRenderCtx {
        origin_x: -(root_scroll_x.round() as i32),
        origin_y: -(root_scroll_y.round() as i32),
        clip: scroll_clip,
        overlay_root_exempt: None,
    };

    let mut overlays: Vec<QueuedOverlay> = Vec::new();
    for child_id in child_ids {
        let child_ctx = if root_child_uses_root_scroll(tree, root_id, child_id) {
            scroll_ctx
        } else {
            base_ctx
        };
        render_tree_node(tree, child_id, child_ctx, frame, console, debug, &mut overlays);
    }

    // Drain `overlay: screen` escapes at top z with the root style context still
    // active, so each overlay composites over the SCREEN surface (Python: the
    // overlay's z-parent is the screen, not its DOM parent).
    paint_deferred_overlays(tree, &mut overlays, frame, console, debug);

    pop_style_context();
}

fn render_screen_tree_layer(
    tree: &WidgetTree,
    frame: &mut FrameBuffer,
    console: &rich_rs::Console,
    debug: Option<&crate::debug::DebugLayout>,
    has_underlay: bool,
) {
    let Some(root_id) = tree.root() else {
        return;
    };
    let Some(root_node) = tree.get(root_id) else {
        return;
    };
    let root_meta = node_selector_meta(tree, root_id);
    let root_resolved = resolve_node_style(tree, root_id, &root_meta);
    let root_rect = root_node.layout_rect;
    let root_scroll = root_node.widget.scroll_offset_f32();
    let child_ids: Vec<NodeId> = tree.children(root_id).to_vec();

    if let Some(clip) = clip_rect_from_tree_rect(root_rect, frame) {
        if let Some(bg) = root_resolved.bg {
            if bg.a >= 1.0 {
                fill_rect_with_background(frame, clip, bg);
            } else if has_underlay {
                tint_rect_with_background(frame, clip, bg);
            } else {
                fill_rect_with_background(frame, clip, bg.flatten_over(Color::rgb(0, 0, 0)));
            }
        }
        stamp_owner_meta_in_rect(frame, clip, root_id);
    }

    push_style_context(root_meta, root_resolved);
    if scrollbar_drag_trace_enabled()
        && (root_scroll.0.abs() > f32::EPSILON || root_scroll.1.abs() > f32::EPSILON)
    {
        let mut child_summary = Vec::new();
        for &child_id in &child_ids {
            if let Some(node) = tree.get(child_id) {
                let docked = node_is_docked(tree, child_id);
                let rect = node.layout_rect;
                child_summary.push(format!(
                    "{}:{}:{}..{}x{}..{}",
                    node.widget.style_type(),
                    if docked { "dock" } else { "flow" },
                    rect.x0,
                    rect.x1,
                    rect.y0,
                    rect.y1
                ));
            }
        }
        debug_render(&format!(
            "[render-root-scroll] layer=screen root_scroll=({:.3}, {:.3}) children=[{}]",
            root_scroll.0,
            root_scroll.1,
            child_summary.join(", ")
        ));
    }

    let width = frame.width;
    let height = frame.height;
    let base_ctx = TreeRenderCtx {
        origin_x: 0,
        origin_y: 0,
        clip: ClipRect::for_frame(frame),
        overlay_root_exempt: None,
    };
    let scroll_clip = root_node
        .widget
        .scroll_viewport_size()
        .map(|(vw, vh)| ClipRect {
            x0: 0,
            y0: 0,
            x1: vw.min(width) as i32,
            y1: vh.min(height) as i32,
        })
        .unwrap_or_else(|| ClipRect::for_frame(frame));
    let scroll_ctx = TreeRenderCtx {
        origin_x: -(root_scroll.0.round() as i32),
        origin_y: -(root_scroll.1.round() as i32),
        clip: scroll_clip,
        overlay_root_exempt: None,
    };

    let mut overlays: Vec<QueuedOverlay> = Vec::new();
    for child_id in child_ids {
        let child_ctx = if root_child_uses_root_scroll(tree, root_id, child_id) {
            scroll_ctx
        } else {
            base_ctx
        };
        render_tree_node(tree, child_id, child_ctx, frame, console, debug, &mut overlays);
    }

    // Drain `overlay: screen` escapes at top z (see `render_app_root_tree_layer`).
    paint_deferred_overlays(tree, &mut overlays, frame, console, debug);

    pop_style_context();
}

/// Paint the `overlay: screen` escapes collected during a layer walk.
///
/// Each queued overlay is placed at its arranged position, constrained into the
/// viewport via [`constrain_overlay_position`], and rendered UNCLIPPED (full
/// frame) at the top z of the layer — after every normal sibling, so its cells
/// (and their `textual:widget_id` meta stamps) land last and occlude correctly
/// for both paint and hit-testing. The overlay's OWN descendants clip normally
/// to its content box. Painting an overlay may enqueue nested `overlay: screen`
/// descendants, which the index walk picks up in FIFO (encounter) order.
///
/// Must be called while the layer's root style context is still on the stack so
/// each overlay composites over the SCREEN surface (Python: an `overlay: screen`
/// widget's z-parent is the screen, not its DOM parent).
fn paint_deferred_overlays(
    tree: &WidgetTree,
    overlays: &mut Vec<QueuedOverlay>,
    frame: &mut FrameBuffer,
    console: &rich_rs::Console,
    debug: Option<&crate::debug::DebugLayout>,
) {
    let vw = frame.width;
    let vh = frame.height;
    let mut i = 0;
    while i < overlays.len() {
        let item = overlays[i];
        i += 1;
        let (px, py) = constrain_overlay_position(
            item.natural_x,
            item.natural_y,
            item.w,
            item.h,
            vw,
            vh,
            item.cx,
            item.cy,
        );
        let ctx = TreeRenderCtx {
            origin_x: px - item.rect_x0,
            origin_y: py - item.rect_y0,
            clip: ClipRect::for_frame(frame),
            overlay_root_exempt: Some(item.node_id),
        };
        render_tree_node(tree, item.node_id, ctx, frame, console, debug, overlays);
    }
}

fn clip_rect_from_tree_rect(
    rect: crate::widget_tree::Rect,
    frame: &FrameBuffer,
) -> Option<ClipRect> {
    let frame_clip = ClipRect::for_frame(frame);
    let rect_clip = ClipRect {
        x0: i32::from(rect.x0),
        y0: i32::from(rect.y0),
        x1: i32::from(rect.x1),
        y1: i32::from(rect.y1),
    };
    frame_clip.intersect(rect_clip)
}

fn fill_rect_with_background(frame: &mut FrameBuffer, clip: ClipRect, bg: Color) {
    let style = rich_rs::Style::new().with_bgcolor(bg.to_simple_opaque());
    for y in clip.y0.max(0) as usize..clip.y1.max(0) as usize {
        for x in clip.x0.max(0) as usize..clip.x1.max(0) as usize {
            frame.set_cell(x, y, crate::render::Cell::blank(Some(style)));
        }
    }
}

/// Fill a rect with a blank where BOTH the foreground and the background carry
/// `bg`. Python renders a `visibility:hidden` widget's region as a solid block
/// of the widget's surface color (the blank `Segment`'s style sets fg = bg), so
/// the hidden cell reads as `fg=<bg> bg=<bg>` — distinct from the screen's base
/// blank (`fg=default`). `fill_rect_with_background` only sets `bg`, leaving
/// `fg=default`, which diverges from Python for hidden cells (e.g. `keyline`).
fn fill_rect_solid_fg_bg(frame: &mut FrameBuffer, clip: ClipRect, bg: Color) {
    let opaque = bg.to_simple_opaque();
    let style = rich_rs::Style::new()
        .with_bgcolor(opaque)
        .with_color(opaque);
    for y in clip.y0.max(0) as usize..clip.y1.max(0) as usize {
        for x in clip.x0.max(0) as usize..clip.x1.max(0) as usize {
            frame.set_cell(x, y, crate::render::Cell::blank(Some(style)));
        }
    }
}

fn tint_rect_with_background(frame: &mut FrameBuffer, clip: ClipRect, tint: Color) {
    if tint.a <= 0.0 {
        return;
    }
    for y in clip.y0.max(0) as usize..clip.y1.max(0) as usize {
        for x in clip.x0.max(0) as usize..clip.x1.max(0) as usize {
            let mut cell = frame.get(x, y).clone();
            let mut style = cell.style.unwrap_or_default();
            let under_bg = style
                .bgcolor
                .map(color_from_simple)
                .unwrap_or_else(|| Color::rgb(0, 0, 0));
            style = style.with_bgcolor(tint.flatten_over(under_bg).to_simple_opaque());
            if let Some(fg) = style.color.map(color_from_simple) {
                style = style.with_color(tint.flatten_over(fg).to_simple_opaque());
            }
            cell.style = Some(style);
            frame.set_cell(x, y, cell);
        }
    }
}

fn stamp_owner_meta_in_rect(frame: &mut FrameBuffer, clip: ClipRect, owner: NodeId) {
    let owner_value = node_id_to_ffi(owner) as i64;
    for y in clip.y0.max(0) as usize..clip.y1.max(0) as usize {
        for x in clip.x0.max(0) as usize..clip.x1.max(0) as usize {
            let mut cell = frame.get(x, y).clone();
            let mut map = cell
                .meta
                .as_ref()
                .and_then(|meta| meta.meta.as_ref())
                .map(|meta| (**meta).clone())
                .unwrap_or_default();
            map.insert("textual:widget_id".to_string(), MetaValue::Int(owner_value));
            let mut meta = cell.meta.unwrap_or_else(StyleMeta::new);
            meta.meta = Some(std::sync::Arc::new(map));
            cell.meta = Some(meta);
            frame.set_cell(x, y, cell);
        }
    }
}

fn root_child_uses_root_scroll(tree: &WidgetTree, root_id: NodeId, child_id: NodeId) -> bool {
    let _ = root_id;
    child_uses_parent_scroll(tree, child_id)
}

fn child_uses_parent_scroll(tree: &WidgetTree, child_id: NodeId) -> bool {
    !node_is_docked(tree, child_id) && !node_is_dedicated_scrollbar(tree, child_id)
}

fn node_is_docked(tree: &WidgetTree, node_id: NodeId) -> bool {
    super::helpers::resolve_style_in_tree(tree, node_id).is_some_and(|style| style.dock.is_some())
}

fn node_is_dedicated_scrollbar(tree: &WidgetTree, node_id: NodeId) -> bool {
    let Some(node) = tree.get(node_id) else {
        return false;
    };
    // Read css_id from node record (canonical source of truth after RA-2 step 6).
    let css_id = node.css_id.as_deref();
    matches!(
        css_id,
        Some(
            APP_ROOT_VSCROLLBAR_ID
                | APP_ROOT_HSCROLLBAR_ID
                | APP_ROOT_SCROLLBAR_CORNER_ID
                | SCROLL_VIEW_VSCROLLBAR_ID
                | SCROLL_VIEW_HSCROLLBAR_ID
                | SCROLL_VIEW_SCROLLBAR_CORNER_ID
                | CONTAINER_VSCROLLBAR_ID
                | CONTAINER_HSCROLLBAR_ID
                | CONTAINER_SCROLLBAR_CORNER_ID
                | LOG_VSCROLLBAR_ID
                | RICH_LOG_VSCROLLBAR_ID
                | OPTION_LIST_VSCROLLBAR_ID
                | KEY_PANEL_VSCROLLBAR_ID
                | DATA_TABLE_HSCROLLBAR_ID
        )
    )
}

#[derive(Clone, Copy)]
struct TreeRenderCtx {
    origin_x: i32,
    origin_y: i32,
    clip: ClipRect,
    /// When set, the node whose id matches is the ROOT of a deferred
    /// `overlay: screen` paint pass and must be painted inline (not re-queued).
    /// `None` during the normal layer walk, so every `overlay: screen` node is
    /// queued and escaped to top z. See [`QueuedOverlay`] / [`paint_deferred_overlays`].
    overlay_root_exempt: Option<NodeId>,
}

/// A node with `overlay: screen` deferred out of the normal tree walk to be
/// painted at the TOP z of the whole layer with NO clip — the real port of
/// Python's placement/clip ESCAPE (`_compositor.py` `no_clip` + `((1,0,0),)`
/// order), NOT a colour blend. The node keeps its normally-arranged position
/// (`natural_x/y`); only z-order and clip change, then the position is
/// constrained into the viewport via [`constrain_overlay_position`].
#[derive(Clone, Copy)]
struct QueuedOverlay {
    node_id: NodeId,
    /// Screen-absolute position the node was arranged at (`rect.x0 + origin_x`).
    natural_x: i32,
    natural_y: i32,
    /// The node's own layout-rect origin, used to derive the deferred render
    /// origin so `render_tree_node` paints the node at the constrained position.
    rect_x0: i32,
    rect_y0: i32,
    w: usize,
    h: usize,
    cx: Constrain,
    cy: Constrain,
}

#[derive(Clone, Copy)]
enum CompositedLayer {
    AppRoot,
    Screen(usize),
}

impl CompositedLayer {
    fn debug_label(self) -> String {
        match self {
            CompositedLayer::AppRoot => "app".to_string(),
            CompositedLayer::Screen(index) => format!("screen[{index}]"),
        }
    }
}

#[derive(Clone, Copy)]
struct ClipRect {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl ClipRect {
    fn for_frame(frame: &FrameBuffer) -> Self {
        Self {
            x0: 0,
            y0: 0,
            x1: frame.width as i32,
            y1: frame.height as i32,
        }
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let x0 = self.x0.max(other.x0);
        let y0 = self.y0.max(other.y0);
        let x1 = self.x1.min(other.x1);
        let y1 = self.y1.min(other.y1);
        if x1 <= x0 || y1 <= y0 {
            None
        } else {
            Some(Self { x0, y0, x1, y1 })
        }
    }
}

// ===========================================================================
// P2-34: Hatch fill (repeating character background)
// ===========================================================================

/// Apply hatch fill to a rendered widget's lines.
///
/// Hatch replaces empty/space cells with the hatch character in the specified
/// color, creating a repeating pattern fill effect. Only cells that are
/// currently blank (space or empty) are filled; existing content is preserved.
#[allow(clippy::too_many_arguments)]
fn apply_hatch_fill(
    frame: &mut FrameBuffer,
    hatch: &Hatch,
    resolved_bg: Option<Color>,
    x0: i32,
    y0: i32,
    w: usize,
    h: usize,
    clip: ClipRect,
) {
    let frame_clip = ClipRect {
        x0: 0,
        y0: 0,
        x1: frame.width as i32,
        y1: frame.height as i32,
    };
    let Some(paint_clip) = clip.intersect(frame_clip) else {
        return;
    };

    // Python paints the hatch glyph with foreground = `(background + color)`,
    // i.e. the hatch color (already carrying its opacity-scaled alpha) blended
    // over the cell's background. The cell background itself is left unchanged.
    let fallback_bg = resolved_bg
        .map(|c| Color::rgb(c.r, c.g, c.b))
        .unwrap_or(Color::rgb(0, 0, 0));
    for row in 0..h {
        let y = y0 + row as i32;
        if y < paint_clip.y0 || y >= paint_clip.y1 {
            continue;
        }
        for col in 0..w {
            let x = x0 + col as i32;
            if x < paint_clip.x0 || x >= paint_clip.x1 {
                continue;
            }
            let cell = frame.get_mut(x as usize, y as usize);
            if cell.continuation {
                continue;
            }
            let is_blank = cell.text.is_empty() || cell.text.chars().all(|c| c == ' ');
            if is_blank {
                // Resolve the under-color from the cell's painted background,
                // falling back to the resolved widget background.
                let under = cell
                    .style
                    .as_ref()
                    .and_then(|s| s.bgcolor)
                    .map(crate::style::color_from_simple)
                    .unwrap_or(fallback_bg);
                let fg = hatch.color.flatten_over(under);
                cell.text = hatch.character.to_string();
                let mut style = cell.style.unwrap_or_else(rich_rs::Style::new);
                style.color = Some(fg.to_simple_opaque());
                cell.style = Some(style);
            }
        }
    }
}

// ===========================================================================
// P2-34: Keyline rendering (lines between children)
// ===========================================================================

/// Draw keyline separators between a parent's child widgets.
///
/// Keylines are horizontal or vertical lines drawn between sibling widgets
/// inside a container. The keyline type determines the character used.
fn paint_keylines(
    tree: &WidgetTree,
    parent_id: NodeId,
    layout: Layout,
    keyline: &crate::style::Keyline,
    ctx: TreeRenderCtx,
    frame: &mut FrameBuffer,
) {
    if keyline.keyline_type == KeylineType::None {
        return;
    }
    let line_style = rich_rs::Style::new().with_color(keyline.color.to_simple_opaque());

    let Some(parent) = tree.get(parent_id) else {
        return;
    };
    let parent_rect = node_content_or_layout_rect(parent);
    let child_ids: Vec<NodeId> = tree.children(parent_id).to_vec();
    if child_ids.len() < 2 {
        return;
    }

    if layout == Layout::Grid {
        let (h_char, v_char) = match keyline.keyline_type {
            KeylineType::None => return,
            KeylineType::Thin => ('─', '│'),
            KeylineType::Heavy => ('━', '┃'),
            KeylineType::Double => ('═', '║'),
        };
        // Python (`layout.py::render_keyline`) draws a `Rectangle` per VISIBLE
        // child, inset by 1 cell into the surrounding gutter, and combines the
        // overlapping line segments into junction characters. A `column-span` /
        // `row-span` child is a SINGLE bigger region, so no interior divider is
        // drawn through it — unlike a cross-product of every column/row boundary,
        // which would bleed a lower row's cell edge up through a spanned cell.
        paint_grid_keyline_rectangles(
            tree,
            &child_ids,
            parent_rect,
            ctx,
            frame,
            line_style,
            keyline.keyline_type,
            h_char,
            v_char,
        );
        return;
    }

    // Build the set of horizontal and vertical line positions for
    // Horizontal / Vertical layouts.  Python draws a Rectangle around every
    // child, which naturally produces outer-boundary lines and proper corner /
    // T-junction characters.  We replicate that by collecting:
    //   Horizontal layout: outer top/bottom + vertical dividers at each
    //     child's right edge (except the last).
    //   Vertical layout:   outer left/right + horizontal dividers at each
    //     child's bottom edge (except the last).
    // We then delegate to the same junction-aware rasteriser used for Grid.

    let (h_char, v_char) = match keyline.keyline_type {
        KeylineType::None => return,
        KeylineType::Thin => ('─', '│'),
        KeylineType::Heavy => ('━', '┃'),
        KeylineType::Double => ('═', '║'),
    };

    let x_start = i32::from(parent_rect.x0) + ctx.origin_x;
    let y_start = i32::from(parent_rect.y0) + ctx.origin_y;
    // x_end / y_end are inclusive pixel positions of the last column/row.
    let x_end = (i32::from(parent_rect.x1) + ctx.origin_x).saturating_sub(1);
    let y_end = (i32::from(parent_rect.y1) + ctx.origin_y).saturating_sub(1);
    if x_start > x_end || y_start > y_end {
        return;
    }

    let mut verticals: BTreeSet<i32> = BTreeSet::new();
    let mut horizontals: BTreeSet<i32> = BTreeSet::new();
    // Always include the outer boundary.
    verticals.insert(x_start);
    verticals.insert(x_end);
    horizontals.insert(y_start);
    horizontals.insert(y_end);

    match layout {
        Layout::Horizontal => {
            // Add a vertical divider at the right edge of every child.
            // `layout_rect.x1` is exclusive (first column AFTER the child),
            // which is exactly where the divider line sits — same convention
            // as the grid code.  The last child's x1 equals the parent's x1,
            // which maps to x_end after the saturating_sub(1) above, so it
            // gets clamped to x_end and merges with the outer boundary.
            for child_id in &child_ids {
                let Some(child) = tree.get(*child_id) else {
                    continue;
                };
                let x = i32::from(child.layout_rect.x1) + ctx.origin_x;
                verticals.insert(x.clamp(x_start, x_end));
            }
        }
        _ => {
            // Vertical layout: add a horizontal divider at the bottom edge
            // of every child.
            for child_id in &child_ids {
                let Some(child) = tree.get(*child_id) else {
                    continue;
                };
                let y = i32::from(child.layout_rect.y1) + ctx.origin_y;
                horizontals.insert(y.clamp(y_start, y_end));
            }
        }
    }

    paint_grid_keylines(
        tree,
        &child_ids,
        parent_rect,
        ctx,
        frame,
        line_style,
        keyline.keyline_type,
        h_char,
        v_char,
        Some((&verticals, &horizontals)),
    );
}

fn keyline_junction_char(
    keyline_type: KeylineType,
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    h_char: char,
    v_char: char,
) -> char {
    match keyline_type {
        KeylineType::None => ' ',
        KeylineType::Thin => match (up, down, left, right) {
            (true, true, true, true) => '┼',
            (true, true, true, false) => '┤',
            (true, true, false, true) => '├',
            (true, false, true, true) => '┴',
            (false, true, true, true) => '┬',
            (false, true, false, true) => '┌',
            (false, true, true, false) => '┐',
            (true, false, false, true) => '└',
            (true, false, true, false) => '┘',
            (_, _, true, true) => h_char,
            (true, true, _, _) => v_char,
            _ => {
                if left || right {
                    h_char
                } else {
                    v_char
                }
            }
        },
        KeylineType::Heavy => match (up, down, left, right) {
            (true, true, true, true) => '╋',
            (true, true, true, false) => '┫',
            (true, true, false, true) => '┣',
            (true, false, true, true) => '┻',
            (false, true, true, true) => '┳',
            (false, true, false, true) => '┏',
            (false, true, true, false) => '┓',
            (true, false, false, true) => '┗',
            (true, false, true, false) => '┛',
            (_, _, true, true) => h_char,
            (true, true, _, _) => v_char,
            _ => {
                if left || right {
                    h_char
                } else {
                    v_char
                }
            }
        },
        KeylineType::Double => match (up, down, left, right) {
            (true, true, true, true) => '╬',
            (true, true, true, false) => '╣',
            (true, true, false, true) => '╠',
            (true, false, true, true) => '╩',
            (false, true, true, true) => '╦',
            (false, true, false, true) => '╔',
            (false, true, true, false) => '╗',
            (true, false, false, true) => '╚',
            (true, false, true, false) => '╝',
            (_, _, true, true) => h_char,
            (true, true, _, _) => v_char,
            _ => {
                if left || right {
                    h_char
                } else {
                    v_char
                }
            }
        },
    }
}

/// Rasterise grid keylines by drawing a rectangle perimeter per VISIBLE child,
/// inset by 1 cell into the surrounding gutter, and combining overlapping line
/// segments into junction characters.  This mirrors Python's
/// `layout.py::render_keyline` (a `Rectangle` per `widget.region` expanded by
/// `(-1,-1)` / `+2`), so a `column-span` / `row-span` child draws ONE region
/// boundary instead of internal dividers, and a `visibility:hidden` cell
/// contributes no keyline of its own (its neighbours still bound the gutter).
#[allow(clippy::too_many_arguments)]
fn paint_grid_keyline_rectangles(
    tree: &WidgetTree,
    child_ids: &[NodeId],
    parent_rect: crate::widget_tree::Rect,
    ctx: TreeRenderCtx,
    frame: &mut FrameBuffer,
    line_style: rich_rs::Style,
    keyline_type: KeylineType,
    h_char: char,
    v_char: char,
) {
    use std::collections::HashMap;

    let frame_w = frame.width as i32;
    let frame_h = frame.height as i32;
    let parent_x0 = i32::from(parent_rect.x0) + ctx.origin_x;
    let parent_y0 = i32::from(parent_rect.y0) + ctx.origin_y;
    let parent_x1 = (i32::from(parent_rect.x1) + ctx.origin_x).saturating_sub(1);
    let parent_y1 = (i32::from(parent_rect.y1) + ctx.origin_y).saturating_sub(1);
    if parent_x0 > parent_x1 || parent_y0 > parent_y1 {
        return;
    }

    // Per-cell accumulated direction bits (up/down/left/right) in frame coords.
    // Each rectangle edge ORs the directions of the line passing through a cell;
    // overlapping rectangles in the shared gutter naturally form T/cross junctions.
    #[derive(Default, Clone, Copy)]
    struct Dir {
        up: bool,
        down: bool,
        left: bool,
        right: bool,
    }
    let mut cells: HashMap<(i32, i32), Dir> = HashMap::new();

    let mark = |cells: &mut HashMap<(i32, i32), Dir>,
                x: i32,
                y: i32,
                up: bool,
                down: bool,
                left: bool,
                right: bool| {
        if x < parent_x0 || x > parent_x1 || y < parent_y0 || y > parent_y1 {
            return;
        }
        let e = cells.entry((x, y)).or_default();
        e.up |= up;
        e.down |= down;
        e.left |= left;
        e.right |= right;
    };

    for child_id in child_ids {
        let Some(child) = tree.get(*child_id) else {
            continue;
        };
        // Python: `if widget.visible` — display:none and visibility:hidden cells
        // contribute no rectangle of their own.
        if !child.display || child.visibility != crate::style::Visibility::Visible {
            continue;
        }
        let rect = child.layout_rect;
        // Rectangle inset 1 cell into the gutter: spans columns [x0-1 ..= x1],
        // rows [y0-1 ..= y1] in frame coordinates (x1/y1 are exclusive edges).
        let rx0 = i32::from(rect.x0) + ctx.origin_x - 1;
        let ry0 = i32::from(rect.y0) + ctx.origin_y - 1;
        let rx1 = i32::from(rect.x1) + ctx.origin_x;
        let ry1 = i32::from(rect.y1) + ctx.origin_y;
        if rx1 <= rx0 || ry1 <= ry0 {
            continue;
        }
        // Top + bottom horizontal edges (each cell carries left/right unless it is
        // the line's own endpoint, but corners also get the perpendicular dir).
        for x in rx0..=rx1 {
            let left = x > rx0;
            let right = x < rx1;
            // top edge: also has `down` at the corners so the vertical edge joins.
            mark(&mut cells, x, ry0, false, false, left, right);
            mark(&mut cells, x, ry1, false, false, left, right);
        }
        // Left + right vertical edges.
        for y in ry0..=ry1 {
            let up = y > ry0;
            let down = y < ry1;
            mark(&mut cells, rx0, y, up, down, false, false);
            mark(&mut cells, rx1, y, up, down, false, false);
        }
        // Corners: ensure both legs are present (a corner cell sits on both a
        // horizontal and a vertical edge of THIS rectangle).
        mark(&mut cells, rx0, ry0, false, true, false, true); // top-left ┌
        mark(&mut cells, rx1, ry0, false, true, true, false); // top-right ┐
        mark(&mut cells, rx0, ry1, true, false, false, true); // bottom-left └
        mark(&mut cells, rx1, ry1, true, false, true, false); // bottom-right ┘
    }

    for ((x, y), dir) in cells {
        if x < 0
            || y < 0
            || x >= frame_w
            || y >= frame_h
            || x < ctx.clip.x0
            || y < ctx.clip.y0
            || x >= ctx.clip.x1
            || y >= ctx.clip.y1
        {
            continue;
        }
        let ch = keyline_junction_char(
            keyline_type,
            dir.up,
            dir.down,
            dir.left,
            dir.right,
            h_char,
            v_char,
        );
        let cell = frame.get_mut(x as usize, y as usize);
        cell.text = ch.to_string();
        let existing_bg = cell.style.and_then(|s| s.bgcolor);
        let merged = if let Some(bg) = existing_bg {
            line_style.with_bgcolor(bg)
        } else {
            line_style
        };
        cell.style = Some(merged);
        cell.continuation = false;
    }
}

/// Rasterise keyline box-drawing for both Grid layouts (where `precomputed`
/// is `None`) and for Horizontal/Vertical layouts (where the caller passes
/// pre-computed `verticals` and `horizontals` sets).
///
/// When `precomputed` is `None` the function derives the divider positions
/// from every child's right/bottom edge (the original Grid behaviour).
fn paint_grid_keylines(
    tree: &WidgetTree,
    child_ids: &[NodeId],
    parent_rect: crate::widget_tree::Rect,
    ctx: TreeRenderCtx,
    frame: &mut FrameBuffer,
    line_style: rich_rs::Style,
    keyline_type: KeylineType,
    h_char: char,
    v_char: char,
    precomputed: Option<(&BTreeSet<i32>, &BTreeSet<i32>)>,
) {
    let frame_w = frame.width as i32;
    let frame_h = frame.height as i32;
    let x_start = i32::from(parent_rect.x0) + ctx.origin_x;
    let y_start = i32::from(parent_rect.y0) + ctx.origin_y;
    let x_end = i32::from(parent_rect.x1)
        .saturating_add(ctx.origin_x)
        .saturating_sub(1);
    let y_end = i32::from(parent_rect.y1)
        .saturating_add(ctx.origin_y)
        .saturating_sub(1);
    if x_start > x_end || y_start > y_end {
        return;
    }

    // Either use caller-supplied line positions, or derive them from child rects.
    let (verticals, horizontals);
    let (v_ref, h_ref): (&BTreeSet<i32>, &BTreeSet<i32>) = if let Some((pv, ph)) = precomputed {
        (pv, ph)
    } else {
        let mut v: BTreeSet<i32> = BTreeSet::new();
        let mut h: BTreeSet<i32> = BTreeSet::new();
        v.insert(x_start);
        v.insert(x_end);
        h.insert(y_start);
        h.insert(y_end);

        for child_id in child_ids {
            let Some(child) = tree.get(*child_id) else {
                continue;
            };
            let rect = child.layout_rect;
            let x = i32::from(rect.x1) + ctx.origin_x;
            let y = i32::from(rect.y1) + ctx.origin_y;
            v.insert(x.clamp(x_start, x_end));
            h.insert(y.clamp(y_start, y_end));
        }

        verticals = v;
        horizontals = h;
        (&verticals, &horizontals)
    };

    let x0 = x_start.max(ctx.clip.x0).max(0);
    let x1 = x_end.min(ctx.clip.x1 - 1).min(frame_w - 1);
    let y0 = y_start.max(ctx.clip.y0).max(0);
    let y1 = y_end.min(ctx.clip.y1 - 1).min(frame_h - 1);
    if x0 > x1 || y0 > y1 {
        return;
    }

    for y in y0..=y1 {
        let on_h = h_ref.contains(&y);
        for x in x0..=x1 {
            let on_v = v_ref.contains(&x);
            if !on_h && !on_v {
                continue;
            }
            let ch = if on_h && on_v {
                let up = v_ref.contains(&x) && y > y_start;
                let down = v_ref.contains(&x) && y < y_end;
                let left = h_ref.contains(&y) && x > x_start;
                let right = h_ref.contains(&y) && x < x_end;
                keyline_junction_char(keyline_type, up, down, left, right, h_char, v_char)
            } else if on_h {
                h_char
            } else {
                v_char
            };
            let cell = frame.get_mut(x as usize, y as usize);
            cell.text = ch.to_string();
            // Preserve the existing cell background and overlay only the
            // keyline foreground colour.  Python renders keylines as a canvas
            // overlay whose style carries only a foreground; the background
            // stays whatever the surface beneath already painted.
            let existing_bg = cell.style.and_then(|s| s.bgcolor);
            let merged = if let Some(bg) = existing_bg {
                line_style.with_bgcolor(bg)
            } else {
                line_style
            };
            cell.style = Some(merged);
            cell.continuation = false;
        }
    }
}

// ===========================================================================
// P2-31: Text wrap/overflow post-processing
// ===========================================================================

/// Apply text-overflow truncation to a line of segments.
///
/// When `TextOverflow::Ellipsis` is active and a line exceeds `max_width`,
/// the line is truncated and an ellipsis character is appended.
/// `TextOverflow::Clip` truncates without ellipsis.
/// `TextOverflow::Fold` wraps content (handled at widget level).
pub fn apply_text_overflow_to_line(
    line: &[Segment],
    max_width: usize,
    overflow: TextOverflow,
) -> Vec<Segment> {
    let line_width = Segment::get_line_length(line);
    if line_width <= max_width {
        return line.to_vec();
    }

    match overflow {
        TextOverflow::Clip => crop_line_horizontal(line, 0, max_width),
        TextOverflow::Ellipsis => {
            if max_width == 0 {
                return Vec::new();
            }
            let truncated = crop_line_horizontal(line, 0, max_width.saturating_sub(1));
            let mut result = truncated;
            // Append ellipsis with the style of the last segment.
            let (last_style, last_meta) = result
                .iter()
                .rev()
                .find(|segment| segment.control.is_none())
                .map(|segment| (segment.style, segment.meta.clone()))
                .unwrap_or((None, None));
            let mut ellipsis = Segment::styled("…".to_string(), last_style.unwrap_or_default());
            ellipsis.meta = last_meta;
            result.push(ellipsis);
            result
        }
        TextOverflow::Fold => {
            // Fold wraps content — no truncation at this level.
            // Widget-level rendering handles fold/wrap.
            line.to_vec()
        }
    }
}

/// Check if a style has text-wrap: nowrap and return the text-overflow mode.
///
/// Returns `Some(overflow_mode)` when text-wrap is NoWrap, indicating the
/// caller should apply overflow truncation. Returns `None` for normal wrapping.
pub fn text_overflow_mode(resolved: &crate::style::Style) -> Option<TextOverflow> {
    match resolved.text_wrap {
        Some(TextWrap::NoWrap) => Some(resolved.text_overflow.unwrap_or(TextOverflow::Clip)),
        _ => None,
    }
}

// ===========================================================================
// P2-35: Constrain/expand for overlay/tooltip positioning
// ===========================================================================

/// Resolve axis-specific constrain values for overlay positioning.
///
/// Returns `(constrain_x, constrain_y)` where each axis uses the specific
/// override (`constrain-x`/`constrain-y`) if set, otherwise falls back to
/// the generic `constrain` property.
pub fn resolve_axis_constrain(resolved: &crate::style::Style) -> (Constrain, Constrain) {
    let base = resolved.constrain.unwrap_or(Constrain::None);
    let cx = resolved.constrain_x.unwrap_or(base);
    let cy = resolved.constrain_y.unwrap_or(base);
    (cx, cy)
}

/// Apply axis-specific constrain to an overlay position.
///
/// Given a proposed overlay position `(x, y)` with size `(w, h)` inside a
/// viewport `(vw, vh)`, clamp or inflect the position based on constrain mode.
pub fn constrain_overlay_position(
    x: i32,
    y: i32,
    w: usize,
    h: usize,
    vw: usize,
    vh: usize,
    cx: Constrain,
    cy: Constrain,
) -> (i32, i32) {
    let mut out_x = x;
    let mut out_y = y;

    match cx {
        Constrain::None => {}
        Constrain::Inside => {
            // Clamp to viewport bounds.
            if out_x < 0 {
                out_x = 0;
            }
            if out_x + w as i32 > vw as i32 {
                out_x = (vw as i32 - w as i32).max(0);
            }
        }
        Constrain::Inflect => {
            // If overflowing right, flip to the left side.
            if out_x + w as i32 > vw as i32 {
                out_x = out_x - w as i32;
                if out_x < 0 {
                    out_x = 0;
                }
            }
        }
    }

    match cy {
        Constrain::None => {}
        Constrain::Inside => {
            if out_y < 0 {
                out_y = 0;
            }
            if out_y + h as i32 > vh as i32 {
                out_y = (vh as i32 - h as i32).max(0);
            }
        }
        Constrain::Inflect => {
            if out_y + h as i32 > vh as i32 {
                out_y = out_y - h as i32;
                if out_y < 0 {
                    out_y = 0;
                }
            }
        }
    }

    (out_x, out_y)
}

// ===========================================================================
// P2-29: Border title/subtitle styling
// ===========================================================================

fn node_content_or_layout_rect(node: &crate::widget_tree::WidgetNode) -> crate::widget_tree::Rect {
    let content = node.content_rect;
    if content.x1 > content.x0 && content.y1 > content.y0 {
        content
    } else {
        node.layout_rect
    }
}

/// Whether the node reserves any gutter (border/padding) between its layout rect
/// and its content box.
///
/// When true, descendant content must be clipped to the content box so it cannot
/// paint over the node's own border/padding chrome (Python clips children to
/// `region.shrink(gutter)` for every container). A node with no gutter has its
/// content box equal to the layout rect, so clipping is a no-op and is skipped.
fn node_has_gutter(node: &crate::widget_tree::WidgetNode) -> bool {
    let c = node.content_rect;
    let l = node.layout_rect;
    // A degenerate/unset content rect (zero extent) means "no resolved content
    // box" — treat as no gutter and fall back to the layout rect elsewhere.
    c.x1 > c.x0 && c.y1 > c.y0 && (c.x0 > l.x0 || c.y0 > l.y0 || c.x1 < l.x1 || c.y1 < l.y1)
}

/// Whether this node is the Screen surface (`AppRoot`, which reports
/// `style_type() == "Screen"`). The Screen surface owns the screen background;
/// the compositor re-fills its rect with the resolved node background so a
/// runtime-set inline bg on the Screen node composites even when the surface
/// widget baked a stale per-seed background (see the empty-Screen fill above).
fn node_is_screen_surface(node: &crate::widget_tree::WidgetNode) -> bool {
    node.widget.style_type() == "Screen"
        || node.widget.style_type_aliases().contains(&"Screen")
}

// ===========================================================================
// Standalone tree-render utility (for integration tests)
// ===========================================================================

/// Render a widget tree to a [`FrameBuffer`] without requiring a full [`App`].
///
/// This is a standalone version of the tree-driven render pipeline suitable
/// for integration tests.  It:
///
/// 1. Installs the default widget stylesheet context so CSS resolution works.
/// 2. Runs the CSS layout pass so every tree node has a valid `layout_rect`.
/// 3. Renders the `root` widget's own chrome (children are already extracted).
/// 4. Walks tree children depth-first, painting each at its `layout_rect`.
///
/// The returned `FrameBuffer` can be inspected with `as_plain_lines()` or
/// `HitTestMap::from_frame()`.
pub fn render_tree_to_frame(
    tree: &mut WidgetTree,
    root: &mut dyn Widget,
    console: &rich_rs::Console,
    width: usize,
    height: usize,
) -> FrameBuffer {
    render_tree_to_frame_with_debug(tree, root, console, width, height, None)
}

/// Render a widget tree to a [`FrameBuffer`] using an explicit stylesheet.
///
/// Useful for integration tests that need custom CSS instead of
/// `default_widget_stylesheet()`.
pub fn render_tree_to_frame_with_stylesheet(
    tree: &mut WidgetTree,
    root: &mut dyn Widget,
    console: &rich_rs::Console,
    width: usize,
    height: usize,
    stylesheet: crate::css::StyleSheet,
) -> FrameBuffer {
    render_tree_to_frame_with_debug_and_stylesheet(
        tree, root, console, width, height, None, stylesheet,
    )
}

/// Render a widget tree to a [`FrameBuffer`] with optional debug-layout overlay.
pub fn render_tree_to_frame_with_debug(
    tree: &mut WidgetTree,
    root: &mut dyn Widget,
    console: &rich_rs::Console,
    width: usize,
    height: usize,
    debug: Option<&crate::debug::DebugLayout>,
) -> FrameBuffer {
    // Install default stylesheet context for CSS resolution during layout + render.
    let sheet = crate::css::default_widget_stylesheet();
    render_tree_to_frame_with_debug_and_stylesheet(tree, root, console, width, height, debug, sheet)
}

fn render_tree_to_frame_with_debug_and_stylesheet(
    tree: &mut WidgetTree,
    root: &mut dyn Widget,
    console: &rich_rs::Console,
    width: usize,
    height: usize,
    debug: Option<&crate::debug::DebugLayout>,
    stylesheet: crate::css::StyleSheet,
) -> FrameBuffer {
    let _guard = crate::css::set_style_context(stylesheet);
    let _focus_within_guard =
        crate::css::set_focus_within(super::routing::focus_within_ids_tree(tree));

    // Run layout so all tree nodes get their layout_rect populated.
    run_layout_pass(tree, (width as u16, height as u16));
    apply_layout_info_tree_from_layout_rects(tree);
    apply_root_tree_virtual_content_size(root, tree);

    let mut frame = FrameBuffer::new(width, height, None);

    let root_node_id = tree.root().unwrap_or_default();

    // Render root widget chrome (children extracted — only own border/bg/padding).
    let mut opts = rich_rs::ConsoleOptions::default();
    opts.size = (width, height);
    opts.max_width = width;
    opts.max_height = height;
    let root_segments = root.render_styled_dyn_obj(console, &opts, debug, root_node_id);
    let root_lines =
        rich_rs::Segment::split_and_crop_lines(root_segments, width, None, true, false);
    for (row, line) in root_lines.iter().enumerate() {
        frame.write_line_at(0, row, line, true);
    }

    // Walk tree children and render each at its layout_rect.
    // NOTE: root is the real root widget; tree.root() holds a TreeStubWidget.
    // Use node_selector_meta for the SELECTOR_STACK entry so that node-level
    // state (focused, hovered, etc.) set via set_focus_state / is_initially_focused
    // is visible to descendant CSS rules (e.g. `Tabs:focus & .-active`).
    // Use the real root widget's style() for inline-style contribution so
    // widget-owned styles (e.g. Tabs dock) still apply to children.
    if let Some(root_id) = tree.root() {
        let root_meta = crate::css::node_selector_meta(tree, root_id);
        let root_resolved = crate::css::resolve_style(root, &root_meta);
        push_style_context(root_meta, root_resolved);

        let child_ids: Vec<NodeId> = tree.children(root_id).to_vec();
        let (root_scroll_x, root_scroll_y) = root.scroll_offset();
        let base_ctx = TreeRenderCtx {
            origin_x: 0,
            origin_y: 0,
            clip: ClipRect::for_frame(&frame),
            overlay_root_exempt: None,
        };
        let scroll_clip = root
            .scroll_viewport_size()
            .map(|(vw, vh)| ClipRect {
                x0: 0,
                y0: 0,
                x1: vw.min(width) as i32,
                y1: vh.min(height) as i32,
            })
            .unwrap_or_else(|| ClipRect::for_frame(&frame));
        let scroll_ctx = TreeRenderCtx {
            origin_x: -(root_scroll_x as i32),
            origin_y: -(root_scroll_y as i32),
            clip: scroll_clip,
            overlay_root_exempt: None,
        };
        let mut overlays: Vec<QueuedOverlay> = Vec::new();
        for child_id in child_ids {
            let child_ctx = if root_child_uses_root_scroll(tree, root_id, child_id) {
                scroll_ctx
            } else {
                base_ctx
            };
            render_tree_node(tree, child_id, child_ctx, &mut frame, console, debug, &mut overlays);
        }

        // Drain `overlay: screen` escapes at top z (see `render_app_root_tree_layer`).
        paint_deferred_overlays(tree, &mut overlays, &mut frame, console, debug);

        pop_style_context();
    }

    frame
}

fn root_tree_virtual_content_size(tree: &WidgetTree) -> Option<(usize, usize)> {
    let root_id = tree.root()?;
    let root = tree.get(root_id)?;
    let content_rect = root.content_rect;
    let mut virtual_w = 0usize;
    let mut virtual_h = 0usize;
    let mut saw_visible_child = false;
    for &child_id in tree.children(root_id) {
        let Some(child) = tree.get(child_id) else {
            continue;
        };
        if node_is_dedicated_scrollbar(tree, child_id) {
            continue;
        }
        if !child.display {
            continue;
        }
        saw_visible_child = true;
        let child_rect = child.layout_rect;
        let child_extent_x = (child_rect.x1 - content_rect.x0).max(0) as usize;
        let child_extent_y = (child_rect.y1 - content_rect.y0).max(0) as usize;
        virtual_w = virtual_w.max(child_extent_x);
        virtual_h = virtual_h.max(child_extent_y);
    }
    if !saw_visible_child {
        virtual_w = content_rect.width() as usize;
        virtual_h = content_rect.height() as usize;
    }
    Some((virtual_w, virtual_h))
}

fn apply_root_tree_virtual_content_size(root: &mut dyn Widget, tree: &WidgetTree) {
    let Some((virtual_w, virtual_h)) = root_tree_virtual_content_size(tree) else {
        return;
    };
    root.set_virtual_content_size(virtual_w, virtual_h);
}

fn apply_root_tree_virtual_content_size_in_tree(tree: &mut WidgetTree) {
    let Some((virtual_w, virtual_h)) = root_tree_virtual_content_size(tree) else {
        return;
    };
    let Some(root_id) = tree.root() else {
        return;
    };
    let Some(root_node) = tree.get_mut(root_id) else {
        return;
    };
    root_node
        .widget
        .set_virtual_content_size(virtual_w, virtual_h);
}

// ===========================================================================
// P1-12 / P2-18a: Arena-tree-based render scaffold + layout integration
//
// These standalone functions implement tree-walk render and layout patterns
// using `WidgetTree`. The layout pass (`run_layout_pass`) computes CSS-based
// `layout_rect`/`content_rect` for every tree node before rendering.
// ===========================================================================

#[derive(Default, Clone, Copy)]
struct ScrollbarHostChildren {
    vertical: Option<NodeId>,
    horizontal: Option<NodeId>,
    corner: Option<NodeId>,
}

fn host_scrollbar_children(tree: &WidgetTree, parent: NodeId) -> ScrollbarHostChildren {
    let mut children = ScrollbarHostChildren::default();
    for &child_id in tree.children(parent) {
        let Some(child) = tree.get(child_id) else {
            continue;
        };
        // Read css_id from node record (canonical source of truth after RA-2 step 6).
        let css_id = child.css_id.as_deref();
        match css_id {
            Some(APP_ROOT_VSCROLLBAR_ID | SCROLL_VIEW_VSCROLLBAR_ID | CONTAINER_VSCROLLBAR_ID) => {
                children.vertical = Some(child_id)
            }
            Some(
                APP_ROOT_HSCROLLBAR_ID
                | SCROLL_VIEW_HSCROLLBAR_ID
                | CONTAINER_HSCROLLBAR_ID
                | DATA_TABLE_HSCROLLBAR_ID,
            ) => children.horizontal = Some(child_id),
            Some(
                APP_ROOT_SCROLLBAR_CORNER_ID
                | SCROLL_VIEW_SCROLLBAR_CORNER_ID
                | CONTAINER_SCROLLBAR_CORNER_ID,
            ) => children.corner = Some(child_id),
            Some(
                LOG_VSCROLLBAR_ID
                | RICH_LOG_VSCROLLBAR_ID
                | OPTION_LIST_VSCROLLBAR_ID
                | KEY_PANEL_VSCROLLBAR_ID,
            ) => children.vertical = Some(child_id),
            _ => {}
        }
    }
    children
}

fn set_runtime_display(tree: &mut WidgetTree, node_id: NodeId, show: bool) {
    if let Some(node) = tree.get_mut(node_id) {
        node.runtime_display = show;
        node.display = node.css_display && node.runtime_display;
    }
}

fn set_layout_rect(tree: &mut WidgetTree, node_id: NodeId, rect: crate::widget_tree::Rect) {
    if let Some(node) = tree.get_mut(node_id) {
        node.layout_rect = rect;
        node.content_rect = rect;
    }
}

fn host_content_extent(
    tree: &WidgetTree,
    node_id: NodeId,
    content_rect: crate::widget_tree::Rect,
    scrollbar_children: ScrollbarHostChildren,
) -> (usize, usize, bool) {
    let mut min_x: Option<i32> = None;
    let mut min_y: Option<i32> = None;
    let mut max_x = 0i32;
    let mut max_y = 0i32;
    let mut saw_visible_child = false;
    // Python's scrollable virtual size (`_compositor` + `_arrange.py`) is the flow
    // (non-docked) content extent PLUS the spacing consumed by docked children on
    // each edge: `virtual = flow_span + dock_spacing`. A docked Header/Footer
    // therefore enlarges the scrollable height by its own height even though it is
    // painted outside the scroll window. `dock_spacing` is the MAX thickness per
    // edge (matching `_arrange_dock_widgets`, which uses `max(...)` per edge).
    let mut top_dock = 0u16;
    let mut bottom_dock = 0u16;
    let mut left_dock = 0u16;
    let mut right_dock = 0u16;
    // Layers occupied by FLOW (non-docked) content children. A docked child on a
    // DISTINCT layer is an overlay (Python `_arrange.py` arranges each layer
    // independently): it does NOT carve flow space and therefore does NOT
    // contribute its thickness to the scrollable virtual extent. Mirrors the
    // per-layer dock isolation in `layout::resolve_layout`. Without this a
    // bottom/right-docked `layer: ruler` overlay (the width/height_comparison
    // demos) inflates virtual_h/virtual_w by its own size and triggers a phantom
    // scrollbar lane that shrinks the flow region by the lane width.
    let flow_layers: std::collections::HashSet<Option<String>> = tree
        .children(node_id)
        .iter()
        .filter(|&&c| {
            Some(c) != scrollbar_children.vertical
                && Some(c) != scrollbar_children.horizontal
                && Some(c) != scrollbar_children.corner
                && !node_is_docked(tree, c)
                && tree.get(c).map(|n| n.display).unwrap_or(false)
        })
        .map(|&c| {
            super::helpers::resolve_style_in_tree(tree, c).and_then(|style| style.layer)
        })
        .collect();
    for &child_id in tree.children(node_id) {
        if Some(child_id) == scrollbar_children.vertical
            || Some(child_id) == scrollbar_children.horizontal
            || Some(child_id) == scrollbar_children.corner
        {
            continue;
        }
        let docked = node_is_docked(tree, child_id);
        let Some(child) = tree.get(child_id) else {
            continue;
        };
        if !child.display {
            continue;
        }
        let child_rect = child.layout_rect;
        if docked {
            // Overlay-layer dock (distinct from every flow child's layer): do not
            // count it toward the scrollable extent (it does not carve flow space).
            let dock_layer =
                super::helpers::resolve_style_in_tree(tree, child_id).and_then(|s| s.layer);
            if !flow_layers.is_empty() && !flow_layers.contains(&dock_layer) {
                continue;
            }
            let w = child_rect.width();
            let h = child_rect.height();
            match super::helpers::resolve_style_in_tree(tree, child_id).and_then(|style| style.dock)
            {
                Some(crate::style::Dock::Top) => top_dock = top_dock.max(h),
                Some(crate::style::Dock::Bottom) => bottom_dock = bottom_dock.max(h),
                Some(crate::style::Dock::Left) => left_dock = left_dock.max(w),
                Some(crate::style::Dock::Right) => right_dock = right_dock.max(w),
                None => {}
            }
            continue;
        }
        // Only flow children define the content span and flip `has_content_children`;
        // self-rendering hosts (RichLog/Log/OptionList) keep their fallback path.
        saw_visible_child = true;
        // Python's `DockArrangeResult` unions each placement grown by its MARGIN
        // (`spatial_map.insert(placement.region.grow(placement.margin), …)`), so a
        // flow child's margin enlarges the scrollable extent. Use the margin-box.
        let margin = super::helpers::resolve_style_in_tree(tree, child_id)
            .map(|style| style.effective_margin())
            .unwrap_or_default();
        let cx0 = child_rect.x0 - i32::from(margin.left);
        let cy0 = child_rect.y0 - i32::from(margin.top);
        let cx1 = child_rect.x1 + i32::from(margin.right);
        let cy1 = child_rect.y1 + i32::from(margin.bottom);
        min_x = Some(min_x.map_or(cx0, |value| value.min(cx0)));
        min_y = Some(min_y.map_or(cy0, |value| value.min(cy0)));
        max_x = max_x.max(cx1);
        max_y = max_y.max(cy1);
    }
    if !saw_visible_child {
        let viewport_w = content_rect.width() as usize;
        let viewport_h = content_rect.height() as usize;
        if let Some(node) = tree.get(node_id)
            && let Some((virtual_w, virtual_h)) = node.widget.scroll_virtual_content_size()
        {
            return (virtual_w.max(1), virtual_h.max(1), false);
        }
        return (viewport_w.max(1), viewport_h.max(1), false);
    }
    let origin_x = min_x.unwrap_or(content_rect.x0);
    let origin_y = min_y.unwrap_or(content_rect.y0);
    let virtual_w =
        ((max_x - origin_x).max(0) + i32::from(left_dock) + i32::from(right_dock)) as usize;
    let virtual_h =
        ((max_y - origin_y).max(0) + i32::from(top_dock) + i32::from(bottom_dock)) as usize;
    (virtual_w.max(1), virtual_h.max(1), true)
}

/// Whether `node_id` is a plain container that should host runtime-injected
/// scrollbar lanes for this layout pass.
///
/// A plain container (`Container`/`Horizontal`/`Vertical`/…) does not inject
/// scrollbar lanes at compose time — whether it scrolls depends on its
/// CSS-resolved `overflow`, which is only known here. This returns true when:
///  - the node resolves `overflow-x` or `overflow-y` to `auto`/`scroll`, AND
///  - it does not already own dedicated scrollbar lanes (so we never touch
///    `ScrollView`/`AppRoot`/`Log`/… which inject their own), AND
///  - it has at least one flow content child (so self-rendering widgets that
///    happen to resolve to `overflow: scroll` are not affected), AND
///  - it is not a suppressed content holder (the inner child of a `ScrollView`).
fn plain_container_wants_scrollbar_lanes(tree: &WidgetTree, node_id: NodeId) -> bool {
    let existing = host_scrollbar_children(tree, node_id);
    if existing.vertical.is_some() || existing.horizontal.is_some() || existing.corner.is_some() {
        return false;
    }

    let Some(node) = tree.get(node_id) else {
        return false;
    };
    // Content holders (ScrollView's inner Container) never host lanes.
    let any = node.widget.as_ref() as &dyn std::any::Any;
    if let Some(container) = any.downcast_ref::<Container>()
        && container.is_scrollbar_suppressed()
    {
        return false;
    }

    let meta = node_selector_meta(tree, node_id);
    let style = resolve_node_style(tree, node_id, &meta);
    let fallback = style.overflow;
    let overflow_x = style.overflow_x.or(fallback);
    let overflow_y = style.overflow_y.or(fallback);
    let scrollable = matches!(
        overflow_x,
        Some(crate::style::Overflow::Auto | crate::style::Overflow::Scroll)
    ) || matches!(
        overflow_y,
        Some(crate::style::Overflow::Auto | crate::style::Overflow::Scroll)
    );
    if !scrollable {
        return false;
    }

    // Require at least one visible, non-docked flow child so self-rendering
    // widgets (which scroll their own content) are not turned into hosts.
    tree.children(node_id).iter().any(|&child_id| {
        tree.get(child_id).is_some_and(|c| c.display) && !node_is_docked(tree, child_id)
    })
}

/// Lazily mount dedicated scrollbar lane children into plain containers whose
/// resolved `overflow` is `auto`/`scroll` (Python parity: a plain container
/// reserves a scrollbar gutter and scrolls when overflow allows). Idempotent —
/// only containers without existing lanes are touched. Runs at the start of the
/// layout pass so the new nodes participate in display sync, gutter reservation
/// (`apply_host_scrollbar_layout`), and rendering.
fn ensure_container_scrollbar_lanes(tree: &mut WidgetTree) {
    let Some(root) = tree.root() else {
        return;
    };
    let candidates: Vec<NodeId> = tree
        .walk_depth_first(root)
        .into_iter()
        .filter(|&id| plain_container_wants_scrollbar_lanes(tree, id))
        .collect();

    for node_id in candidates {
        let mut vbar = ScrollBar::new(true, 2);
        vbar.seed.css_id = Some(CONTAINER_VSCROLLBAR_ID.to_string());
        tree.mount(node_id, Box::new(vbar));

        let mut hbar = ScrollBar::new(false, 1);
        hbar.seed.css_id = Some(CONTAINER_HSCROLLBAR_ID.to_string());
        tree.mount(node_id, Box::new(hbar));

        let mut corner = ScrollBarCorner::new();
        corner.seed.css_id = Some(CONTAINER_SCROLLBAR_CORNER_ID.to_string());
        tree.mount(node_id, Box::new(corner));
    }
}

fn apply_host_scrollbar_layout(tree: &mut WidgetTree, viewport: (u16, u16)) {
    let Some(root) = tree.root() else {
        return;
    };
    let node_ids = tree.walk_depth_first(root);
    for node_id in node_ids {
        let scrollbar_children = host_scrollbar_children(tree, node_id);
        if scrollbar_children.vertical.is_none()
            && scrollbar_children.horizontal.is_none()
            && scrollbar_children.corner.is_none()
        {
            continue;
        }

        let (content_rect, outer_rect, style, offset_x, offset_y) = {
            let Some(node) = tree.get(node_id) else {
                continue;
            };
            let content_rect = node.content_rect;
            let outer_rect = node.layout_rect;
            let (offset_x, offset_y) = node.widget.scroll_offset_f32();
            let meta = node_selector_meta(tree, node_id);
            let style = resolve_node_style(tree, node_id, &meta);
            (content_rect, outer_rect, style, offset_x, offset_y)
        };
        let content_w = (content_rect.width() as usize).max(1);
        let content_h = (content_rect.height() as usize).max(1);

        let (virtual_w, virtual_h, mut has_content_children) =
            host_content_extent(tree, node_id, content_rect, scrollbar_children);
        // Pass the ACTUAL virtual content extent per axis (not clamped up to the
        // viewport) so a lane is reserved only on genuine overflow (see the
        // ScrollbarPolicy::resolve note about the phantom cross-axis scrollbar).
        let mut geometry = ScrollbarPolicy::from_style(&style, 2, 1)
            .resolve(content_w, content_h, virtual_w, virtual_h);

        // Re-layout the children at the resolved viewport, then recompute, until
        // the reserved lanes stabilize (capped). The children were initially laid
        // out at the full content box, so the first pass may over-reserve (e.g. a
        // child that measured tall before its own `on_layout` corrected its
        // width). Re-laying out and recomputing converges, and crucially
        // RE-EXPANDS the children when a lane turns out to be unneeded — without
        // this the gutter was reserved on a stale measurement and never released.
        let mut laid_out_w = content_w;
        let mut laid_out_h = content_h;
        for _ in 0..3 {
            if geometry.viewport_width == laid_out_w && geometry.viewport_height == laid_out_h {
                break;
            }
            if let Some(id) = scrollbar_children.vertical {
                set_runtime_display(tree, id, false);
            }
            if let Some(id) = scrollbar_children.horizontal {
                set_runtime_display(tree, id, false);
            }
            if let Some(id) = scrollbar_children.corner {
                set_runtime_display(tree, id, false);
            }
            crate::layout::resolve_layout(
                tree,
                node_id,
                crate::layout::Region::new(
                    content_rect.x0,
                    content_rect.y0,
                    geometry.viewport_width as u16,
                    geometry.viewport_height as u16,
                ),
                viewport,
            );
            laid_out_w = geometry.viewport_width;
            laid_out_h = geometry.viewport_height;
            let (virtual_w, virtual_h, had_children) =
                host_content_extent(tree, node_id, content_rect, scrollbar_children);
            has_content_children = had_children;
            geometry = ScrollbarPolicy::from_style(&style, 2, 1)
                .resolve(content_w, content_h, virtual_w, virtual_h);
        }

        let viewport_rect = crate::widget_tree::Rect {
            x0: content_rect.x0,
            y0: content_rect.y0,
            x1: content_rect.x0 + geometry.viewport_width as i32,
            y1: content_rect.y0 + geometry.viewport_height as i32,
        };
        if let Some(node) = tree.get_mut(node_id) {
            node.content_rect = viewport_rect;
            if !has_content_children {
                // Self-rendering hosts (no content children: RichLog, Log,
                // OptionList, …) reserve the scrollbar lane out of their CONTENT
                // box. For a CHROME-LESS host (no border/padding: Log, RichLog)
                // the layout box and content box coincide, so the box tracks the
                // viewport exactly as before. For a host WITH chrome (border or
                // padding: OptionList) the outer box is fixed by layout and must
                // be preserved — collapsing it to the content viewport would drop
                // the border frame and the reserved gutter.
                let has_chrome = outer_rect.x0 != content_rect.x0
                    || outer_rect.y0 != content_rect.y0
                    || outer_rect.x1 != content_rect.x1
                    || outer_rect.y1 != content_rect.y1;
                node.layout_rect = if has_chrome {
                    outer_rect
                } else {
                    viewport_rect
                };
            }
            node.widget
                .set_virtual_content_size(geometry.content_width, geometry.content_height);
        }

        if let Some(v_id) = scrollbar_children.vertical {
            // Fix B: use geometry.show_vertical (content overflows AND allowed) as the
            // widget visibility flag, keeping lane/gutter RESERVATION (vertical_lane_width)
            // separate from widget VISIBILITY.  Python parity: `_arrange_scrollbars` uses
            // `show_vertical_scrollbar` (which respects overflow + scrollbar_gutter separately
            // from `_get_scrollbar_region`'s stable-gutter reservation).
            let show = geometry.show_vertical;
            // Display (the bar PAINT) is gated on `show && paint`: the bar is
            // drawn only when content overflows (`show`) AND visibility is not
            // hidden (`paint`). Python: the compositor adds the chrome widget
            // only when `show_vertical_scrollbar` AND
            // `scrollbar_visibility == "visible"`.
            // A `scrollbar-size: .. 0` lane reserves nothing and paints nothing
            // (Python: a 0-size scrollbar region is empty), so also gate on the
            // resolved lane width — otherwise the zero-rect bar would inherit
            // the host clip and paint its `thickness.max(1)` glyphs over content.
            let show = show && geometry.vertical_lane_width > 0;
            set_runtime_display(tree, v_id, show && geometry.paint_vertical);
            // The lane RECT is driven by lane RESERVATION
            // (`vertical_lane_width > 0`), NOT by `show`. Under
            // `scrollbar-gutter: stable` with no overflow the gutter is reserved
            // (lane width == 2) even though the bar is not shown — so the lane
            // node still owns its 2-column rect (Python
            // `scrollbar_size_vertical` returns the full size whenever
            // gutter==stable and overflow==auto, regardless of show_vertical).
            // Gating the rect on `show` orphaned those reserved columns.
            let rect = if geometry.vertical_lane_width > 0 {
                crate::widget_tree::Rect {
                    x0: content_rect.x0 + geometry.viewport_width as i32,
                    y0: content_rect.y0,
                    x1: content_rect.x1,
                    y1: content_rect.y0 + geometry.viewport_height as i32,
                }
            } else {
                crate::widget_tree::Rect::ZERO
            };
            set_layout_rect(tree, v_id, rect);
            if let Some(node) = tree.get_mut(v_id) {
                let any = node.widget.as_mut() as &mut dyn std::any::Any;
                if let Some(scrollbar) = any.downcast_mut::<ScrollBar>() {
                    // Width of the vertical bar = the CSS-resolved vertical lane
                    // (`scrollbar-size` vertical). The lane RECT alone is not
                    // enough: ScrollBar paints `thickness`-wide glyphs from its
                    // own field, which defaults to 2 at creation.
                    scrollbar.set_thickness(geometry.vertical_lane_width.max(1));
                    scrollbar.set_window_virtual_size(geometry.content_height);
                    scrollbar.set_window_size(geometry.viewport_height);
                    if !scrollbar.grabbed() {
                        let max_offset = geometry.max_offset_y() as f32;
                        scrollbar.set_position(offset_y.clamp(0.0, max_offset));
                    }
                }
            }
        }

        if let Some(h_id) = scrollbar_children.horizontal {
            // Fix B: same as vertical — use show_horizontal not horizontal_lane_height > 0.
            let show = geometry.show_horizontal;
            // See the vertical block: display gated on `show && paint` (and a
            // non-zero lane — `scrollbar-size: 0 ..` paints nothing); the lane
            // RECT is driven by lane reservation (`horizontal_lane_height > 0`),
            // so a stable-gutter reserved lane keeps its rect even with no
            // overflow or hidden visibility.
            let show = show && geometry.horizontal_lane_height > 0;
            set_runtime_display(tree, h_id, show && geometry.paint_horizontal);
            let rect = if geometry.horizontal_lane_height > 0 {
                crate::widget_tree::Rect {
                    x0: content_rect.x0,
                    y0: content_rect.y0 + geometry.viewport_height as i32,
                    x1: content_rect.x0 + geometry.viewport_width as i32,
                    y1: content_rect.y1,
                }
            } else {
                crate::widget_tree::Rect::ZERO
            };
            set_layout_rect(tree, h_id, rect);
            if let Some(node) = tree.get_mut(h_id) {
                let any = node.widget.as_mut() as &mut dyn std::any::Any;
                if let Some(scrollbar) = any.downcast_mut::<ScrollBar>() {
                    // Height of the horizontal bar = the CSS-resolved horizontal
                    // lane (`scrollbar-size` horizontal); see vertical note above.
                    scrollbar.set_thickness(geometry.horizontal_lane_height.max(1));
                    scrollbar.set_window_virtual_size(geometry.content_width);
                    scrollbar.set_window_size(geometry.viewport_width);
                    if !scrollbar.grabbed() {
                        let max_offset = geometry.max_offset_x() as f32;
                        scrollbar.set_position(offset_x.clamp(0.0, max_offset));
                    }
                }
            }
        }

        if let Some(c_id) = scrollbar_children.corner {
            // Corner is PAINTED only when BOTH scrollbar widgets are shown AND
            // painted. Under `scrollbar-visibility: hidden` the lanes stay
            // reserved but neither bar (nor the corner) is painted.
            let show = geometry.show_vertical
                && geometry.show_horizontal
                && geometry.vertical_lane_width > 0
                && geometry.horizontal_lane_height > 0;
            let paint = geometry.paint_vertical && geometry.paint_horizontal;
            set_runtime_display(tree, c_id, show && paint);
            // Corner RECT is driven by lane reservation (both lanes reserved),
            // matching the vertical/horizontal lane-rect policy.
            let rect = if geometry.vertical_lane_width > 0
                && geometry.horizontal_lane_height > 0
            {
                crate::widget_tree::Rect {
                    x0: content_rect.x0 + geometry.viewport_width as i32,
                    y0: content_rect.y0 + geometry.viewport_height as i32,
                    x1: content_rect.x1,
                    y1: content_rect.y1,
                }
            } else {
                crate::widget_tree::Rect::ZERO
            };
            set_layout_rect(tree, c_id, rect);
        }
    }
}

/// Update existing host scrollbar widgets from current host scroll offsets.
///
/// Unlike `apply_host_scrollbar_layout`, this does not run layout or recompute
/// geometry from child bounds. It's intended for animation frames where only the
/// scroll position changed and we need smooth thumb movement without a full relayout.
fn sync_host_scrollbar_positions(tree: &mut WidgetTree) {
    let Some(root) = tree.root() else {
        return;
    };
    let node_ids = tree.walk_depth_first(root);
    for node_id in node_ids {
        let scrollbar_children = host_scrollbar_children(tree, node_id);
        if scrollbar_children.vertical.is_none() && scrollbar_children.horizontal.is_none() {
            continue;
        }

        let (offset_x, offset_y, viewport_w, viewport_h, virtual_w, virtual_h) = {
            let Some(node) = tree.get(node_id) else {
                continue;
            };
            let Some((virtual_w, virtual_h)) = node.widget.scroll_virtual_content_size() else {
                // Host doesn't expose virtual content metrics; keep the last
                // layout-applied scrollbar sizing.
                continue;
            };
            let (offset_x, offset_y) = node.widget.scroll_offset_f32();
            let viewport_w = (node.content_rect.width() as usize).max(1);
            let viewport_h = (node.content_rect.height() as usize).max(1);
            (
                offset_x,
                offset_y,
                viewport_w,
                viewport_h,
                virtual_w.max(1),
                virtual_h.max(1),
            )
        };

        if let Some(v_id) = scrollbar_children.vertical
            && let Some(node) = tree.get_mut(v_id)
        {
            let any = node.widget.as_mut() as &mut dyn std::any::Any;
            if let Some(scrollbar) = any.downcast_mut::<ScrollBar>() {
                scrollbar.set_window_virtual_size(virtual_h);
                scrollbar.set_window_size(viewport_h);
                if !scrollbar.grabbed() {
                    let max_offset = virtual_h.saturating_sub(viewport_h.max(1)) as f32;
                    scrollbar.set_position(offset_y.clamp(0.0, max_offset));
                }
            }
        }

        if let Some(h_id) = scrollbar_children.horizontal
            && let Some(node) = tree.get_mut(h_id)
        {
            let any = node.widget.as_mut() as &mut dyn std::any::Any;
            if let Some(scrollbar) = any.downcast_mut::<ScrollBar>() {
                scrollbar.set_window_virtual_size(virtual_w);
                scrollbar.set_window_size(viewport_w);
                if !scrollbar.grabbed() {
                    let max_offset = virtual_w.saturating_sub(viewport_w.max(1)) as f32;
                    scrollbar.set_position(offset_x.clamp(0.0, max_offset));
                }
            }
        }
    }
}

fn hide_host_scrollbar_children_for_flow_layout(tree: &mut WidgetTree) {
    let Some(root) = tree.root() else {
        return;
    };
    for node_id in tree.walk_depth_first(root) {
        let children = host_scrollbar_children(tree, node_id);
        if children.vertical.is_none() && children.horizontal.is_none() && children.corner.is_none()
        {
            continue;
        }
        if let Some(v_id) = children.vertical {
            set_runtime_display(tree, v_id, false);
        }
        if let Some(h_id) = children.horizontal {
            set_runtime_display(tree, h_id, false);
        }
        if let Some(c_id) = children.corner {
            set_runtime_display(tree, c_id, false);
        }
    }
}

/// Keep every `Collapsible`'s title child in sync with the parent's `collapsed`
/// state so the rendered ▼/▶ symbol is correct after a runtime toggle.
///
/// The `CollapsibleTitle` is a separate arena node that renders its glyph from
/// its own `collapsed` field. Python keeps them in sync via
/// `Collapsible._update_collapsed` (`self._title.collapsed = collapsed`); in the
/// arena the parent no longer owns the child post-mount, so the runtime performs
/// the same propagation here — idempotent and driven from the Collapsible's own
/// state (not a runtime-only source of truth).
fn sync_collapsible_titles(tree: &mut WidgetTree) {
    let Some(root) = tree.root() else {
        return;
    };
    for node_id in tree.walk_depth_first(root) {
        let collapsed = match tree.get(node_id) {
            Some(node) => {
                let any = node.widget.as_ref() as &dyn std::any::Any;
                match any.downcast_ref::<crate::widgets::Collapsible>() {
                    Some(collapsible) => collapsible.is_collapsed(),
                    None => continue,
                }
            }
            None => continue,
        };
        // The CollapsibleTitle is the Collapsible's first composed child.
        let Some(&title_id) = tree.children(node_id).first() else {
            continue;
        };
        if let Some(title_node) = tree.get_mut(title_id) {
            let any = title_node.widget.as_mut() as &mut dyn std::any::Any;
            if let Some(title) = any.downcast_mut::<crate::widgets::CollapsibleTitle>() {
                title.set_collapsed(collapsed);
            }
        }
    }
}

/// Run the CSS-layout pass on the widget tree.
///
/// Sets the root node's `layout_rect`/`content_rect` to the full viewport,
/// then calls [`resolve_layout`](crate::layout::resolve_layout) to compute
/// rects for the root's children. Call this before rendering so that
/// precomputed rects are available for widget sizing and positioning.
///
/// **Note:** The caller must ensure the CSS stylesheet context is active
/// (via [`set_style_context`](crate::css::set_style_context)) before calling
/// this function, because the layout solver resolves styles from the stylesheet.
pub fn run_layout_pass(tree: &mut WidgetTree, viewport: (u16, u16)) {
    let root_id = match tree.root() {
        Some(r) => r,
        None => return,
    };

    // Lazily mount scrollbar lanes into plain containers whose resolved overflow
    // is auto/scroll (must precede display sync + flow layout so the new nodes
    // are visible to the rest of the pass).
    ensure_container_scrollbar_lanes(tree);

    // Propagate each Collapsible's `collapsed` state to its title child so the
    // rendered ▼/▶ symbol tracks the parent (Python `Collapsible._update_collapsed`
    // -> `self._title.collapsed = collapsed`). Runs before the display sync so a
    // runtime toggle repaints the correct glyph in the same relayout pass.
    sync_collapsible_titles(tree);

    // Sync CSS display/visibility values to WidgetNode fields before layout.
    crate::css::apply_display_visibility_to_tree(tree);
    hide_host_scrollbar_children_for_flow_layout(tree);

    let available = crate::layout::Region::new(0, 0, viewport.0, viewport.1);

    // Set root's own rects to the full viewport.
    if let Some(root) = tree.get_mut(root_id) {
        root.layout_rect = available.to_rect();
        root.content_rect = available.to_rect();
    }

    // Resolve children's layout rects.
    crate::layout::resolve_layout(tree, root_id, available, viewport);
    apply_host_scrollbar_layout(tree, viewport);

    // Optional per-node rect trace for layout debugging.
    if std::env::var("TEXTUAL_DEBUG_LAYOUT_FILE").is_ok() {
        let walk = tree.walk_depth_first(root_id);
        debug_layout(&format!(
            "[layout_tree] viewport={}x{} nodes={}",
            viewport.0,
            viewport.1,
            walk.len()
        ));
        for node_id in walk {
            let Some(node) = tree.get(node_id) else {
                continue;
            };
            let lr = node.layout_rect;
            let cr = node.content_rect;
            debug_layout(&format!(
                "[layout_tree] id={} type={} display={} visibility={:?} lr=({},{}..{},{} w={} h={}) cr=({},{}..{},{} w={} h={})",
                crate::node_id::node_id_to_ffi(node_id),
                node.widget.style_type(),
                node.display,
                node.visibility,
                lr.x0,
                lr.y0,
                lr.x1,
                lr.y1,
                lr.x1.saturating_sub(lr.x0),
                lr.y1.saturating_sub(lr.y0),
                cr.x0,
                cr.y0,
                cr.x1,
                cr.y1,
                cr.x1.saturating_sub(cr.x0),
                cr.y1.saturating_sub(cr.y0),
            ));
        }
    }
}

/// Walk visible nodes in depth-first order and collect render metadata.
///
/// Returns a list of `(NodeId, bool)` pairs — `true` if the node should
/// be rendered (displayed + visible), `false` if hidden via `display:none`
/// or `visibility:hidden`. Nodes with `visibility:hidden` still participate
/// in layout (their space is preserved) but produce no rendered output.
///
/// Children of each parent are ordered according to the parent's `layers`
/// CSS property: children assigned to an earlier layer render first (lower
/// z-index), children assigned to a later layer render last (on top).
/// Children without a `layer` assignment come before any named layers.
pub(crate) fn collect_render_nodes(tree: &WidgetTree) -> Vec<(NodeId, bool)> {
    let root = match tree.root() {
        Some(r) => r,
        None => return Vec::new(),
    };
    let mut result = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let render = tree
            .get(id)
            .map(|node| node.display && node.visibility == crate::style::Visibility::Visible)
            .unwrap_or(false);
        result.push((id, render));

        // Collect children in layer-sorted order.
        let children = tree.children(id).to_vec();
        if children.is_empty() {
            continue;
        }

        let sorted = sort_children_by_layer(tree, id, &children);

        // Push in reverse so the first child is processed first.
        for &child in sorted.iter().rev() {
            if tree.get(child).is_some() {
                stack.push(child);
            }
        }
    }
    result
}

/// Sort a list of child `NodeId`s according to the parent's `layers` declaration.
///
/// The parent's `layers` property defines named layer ordering. Children are
/// grouped by their `layer` CSS property:
/// - Children without a `layer` assignment come first (default layer).
/// - Children assigned to named layers are ordered according to the parent's
///   `layers` list (earlier = rendered first = lower z-index).
/// - Children assigned to a layer name not in the parent's `layers` list are
///   placed after the default group but before any declared layers.
///
/// Within each group, the original DOM order is preserved (stable sort).
fn sort_children_by_layer(tree: &WidgetTree, parent: NodeId, children: &[NodeId]) -> Vec<NodeId> {
    // Resolve the parent's `layers` declaration using node-record-based meta.
    let parent_layers: Option<Vec<String>> = tree.get(parent).and_then(|_node| {
        let meta = node_selector_meta(tree, parent);
        let style = resolve_node_style(tree, parent, &meta);
        style.layers
    });

    let layer_order = match parent_layers {
        Some(ref layers) if !layers.is_empty() => layers,
        // No layers declaration — keep DOM order. The CommandPalette is mounted
        // DOM-last by `AppRoot::compose` and floats on top via that ordering; a
        // real `overlay: screen` surface floats via `paint_deferred_overlays`.
        // No type-string special-case is needed here.
        _ => return children.to_vec(),
    };

    // Resolve each child's `layer` property using node-record-based meta.
    let child_layers: Vec<Option<String>> = children
        .iter()
        .map(|&child| {
            tree.get(child).and_then(|_node| {
                let meta = node_selector_meta(tree, child);
                let style = resolve_node_style(tree, child, &meta);
                style.layer
            })
        })
        .collect();

    // Assign a sort key: (group, original_index)
    // group 0 = no layer / unknown layer name (default bucket, preserves DOM order),
    // group 1..N = position in parent's layers list + 1
    let mut indexed: Vec<(usize, usize)> = children
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let group = match &child_layers[i] {
                None => 0,
                Some(name) => {
                    if let Some(pos) = layer_order.iter().position(|l| l == name) {
                        pos + 1
                    } else {
                        0 // Unknown layer name falls back to default bucket
                    }
                }
            };
            (group, i)
        })
        .collect();

    indexed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    indexed.iter().map(|&(_, i)| children[i]).collect()
}

/// Distribute layout information to widgets using precomputed `layout_rect`s.
///
/// After `run_layout_pass` but before paint, widgets receive `on_layout(...)`
/// based on their solved tree geometry so layout-dependent render state is
/// correct on the first rendered frame (and remains stable across subsequent
/// post-render layout propagation).
pub(crate) fn apply_layout_info_tree_from_layout_rects(tree: &mut WidgetTree) {
    let root = match tree.root() {
        Some(r) => r,
        None => return,
    };
    let node_ids = tree.walk_depth_first(root);
    for node_id in node_ids {
        let (content_w, content_h, virtual_content_w, virtual_content_h) = {
            let Some(node) = tree.get(node_id) else {
                continue;
            };
            let content_rect = node.content_rect;
            let scrollbar_children = host_scrollbar_children(tree, node_id);
            let (virtual_w, virtual_h, _) =
                host_content_extent(tree, node_id, content_rect, scrollbar_children);

            (
                content_rect.x1.saturating_sub(content_rect.x0) as usize,
                content_rect.y1.saturating_sub(content_rect.y0) as usize,
                virtual_w,
                virtual_h,
            )
        };

        if let Some(node) = tree.get_mut(node_id) {
            node.widget
                .on_layout(content_w.max(1) as u16, content_h.max(1) as u16);
            node.widget
                .set_virtual_content_size(virtual_content_w, virtual_content_h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{SYNC_END, SYNC_START};
    use super::*;

    fn diff_text(segments: &Segments) -> String {
        let mut out = String::new();
        for s in segments.iter() {
            if s.control.is_none() {
                out.push_str(&s.text);
            }
        }
        out
    }

    #[test]
    fn fill_rect_solid_fg_bg_sets_both_fg_and_bg() {
        // Keyline canvas base (Python `Canvas.render` spanned rows): a hidden /
        // gutter cell must carry fg == bg == the surface color, NOT fg=default.
        let mut frame = FrameBuffer::new(6, 3, None);
        let bg = Color::rgb(0x12, 0x12, 0x12);
        let clip = ClipRect { x0: 1, y0: 1, x1: 4, y1: 2 };
        fill_rect_solid_fg_bg(&mut frame, clip, bg);

        // Inside the clip: fg and bg both set to the surface color.
        let cell = frame.get(2, 1);
        let style = cell.style.expect("filled cell has style");
        let want = bg.to_simple_opaque();
        assert_eq!(style.bgcolor.map(color_from_simple), Some(bg));
        assert_eq!(style.color.map(color_from_simple), Some(bg));
        assert_eq!(style.bgcolor, Some(want));
        assert_eq!(style.color, Some(want));

        // Outside the clip: untouched (still the frame base = None).
        assert!(frame.get(0, 0).style.is_none(), "outside clip stays bare");
        // Contrast with the bg-only fill, which leaves fg=default.
        let mut frame2 = FrameBuffer::new(6, 3, None);
        fill_rect_with_background(&mut frame2, clip, bg);
        let s2 = frame2.get(2, 1).style.expect("bg-only fill has style");
        assert_eq!(s2.bgcolor.map(color_from_simple), Some(bg));
        assert!(s2.color.is_none(), "bg-only fill must leave fg=default");
    }

    #[test]
    fn render_plus_compose_draws_own_content_and_child() {
        // Regression (how-to/render_compose parity): a widget that BOTH overrides
        // render() to paint its own surface AND composes a child must have its own
        // render output drawn beneath/around the child — not dropped. Mirrors the
        // Python `Splash(Container)` that returns a gradient from render() and
        // yields a Static from compose().
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Container, Static};

        struct RenderPlusCompose {
            container: Container,
        }
        impl RenderPlusCompose {
            fn new() -> Self {
                Self {
                    container: Container::new().with_child(Static::new("HI")),
                }
            }
        }
        impl crate::widgets::Widget for RenderPlusCompose {
            fn render(
                &self,
                _console: &rich_rs::Console,
                options: &rich_rs::ConsoleOptions,
            ) -> Segments {
                // Fill the whole content area with 'X' cells (stand-in for the
                // gradient surface in the real example).
                let (w, h) = options.size;
                let mut out = Segments::new();
                for y in 0..h.max(1) {
                    out.push(Segment::new("X".repeat(w.max(1))));
                    if y + 1 < h.max(1) {
                        out.push(Segment::line());
                    }
                }
                out
            }
            fn compose(&mut self) -> crate::compose::ComposeResult {
                self.container.compose()
            }
            fn style_type(&self) -> &'static str {
                "RenderPlusCompose"
            }
        }

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let host_id = tree.mount(root, Box::new(RenderPlusCompose::new()));
        let kids = {
            let node = tree.get_mut(host_id).unwrap();
            node.widget.compose()
        };
        for kid in kids {
            tree.mount(host_id, kid.into_widget());
        }

        let console = rich_rs::Console::new();
        let mut root_widget = AppRoot::new();
        let frame = render_tree_to_frame(&mut tree, &mut root_widget, &console, 20, 5);

        let mut dump = String::new();
        for y in 0..5 {
            for x in 0..20 {
                dump.push_str(&frame.get(x, y).text);
            }
            dump.push('\n');
        }
        let x_count = dump.matches('X').count();
        assert!(
            x_count > 0,
            "host's own render() output (X fill) must appear beneath the child; got:\n{dump}"
        );
        assert!(
            dump.contains("HI"),
            "composed child (HI) must also be drawn on top; got:\n{dump}"
        );
    }

    #[test]
    fn clear_before_draw_reemits_unchanged_content() {
        // Regression: when a frame is drawn with clear_before_draw, the terminal
        // is wiped, so the diff must be taken against a BLANK frame — not the
        // previous frame — or unchanged content is not re-emitted and vanishes.
        let (w, h) = (10, 1);
        let lines = vec![vec![Segment::new("HELLO".to_string())]];
        let next = FrameBuffer::from_lines(&lines, w, h, None);
        let previous = FrameBuffer::from_lines(&lines, w, h, None);

        // No clear, identical frames: nothing to redraw.
        let no_clear = diff_body_for_draw(&next, &previous, false, None, None);
        assert!(
            !diff_text(&no_clear).contains("HELLO"),
            "identical frames without clear must not re-emit content"
        );

        // Clear set: must re-emit all visible content despite an identical
        // previous frame (diff against blank, not the stale frame).
        let with_clear = diff_body_for_draw(&next, &previous, true, None, None);
        assert!(
            diff_text(&with_clear).contains("HELLO"),
            "clear must re-emit visible content (diff against blank, not stale frame)"
        );
    }

    #[test]
    fn sync_output_wraps_payload_when_enabled() {
        let mut console = rich_rs::Console::capture();
        console_write_with_optional_sync(&mut console, true, |console| {
            console.write_str("PAYLOAD")
        })
        .unwrap();
        let out = console.get_captured_bytes();
        assert!(out.starts_with(SYNC_START.as_bytes()));
        assert!(out.ends_with(SYNC_END.as_bytes()));
        assert!(out.windows(b"PAYLOAD".len()).any(|w| w == b"PAYLOAD"));
    }

    #[test]
    fn sync_output_does_not_wrap_payload_when_disabled() {
        let mut console = rich_rs::Console::capture();
        console_write_with_optional_sync(&mut console, false, |console| {
            console.write_str("PAYLOAD")
        })
        .unwrap();
        let out = console.get_captured_bytes();
        assert_eq!(out, b"PAYLOAD");
    }

    #[test]
    fn prepend_clear_only_when_requested() {
        let mut diff = Segments::new();
        diff.push(Segment::control(ControlType::Home));
        diff.push(Segment::new("x"));

        let without_clear = prepend_clear_if_needed(diff.clone(), false);
        let with_clear = prepend_clear_if_needed(diff.clone(), true);

        assert_eq!(without_clear.len(), diff.len());
        assert_eq!(with_clear.len(), diff.len() + 1);
        assert!(matches!(
            without_clear
                .iter()
                .next()
                .and_then(|seg| seg.control.as_ref()),
            Some(ControlType::Home)
        ));
        assert!(matches!(
            with_clear
                .iter()
                .next()
                .and_then(|seg| seg.control.as_ref()),
            Some(ControlType::Clear)
        ));
    }

    #[test]
    fn hit_test_translates_screen_to_widget_local_coords() {
        use super::super::types::{NodeHitTestMap, Rect};
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, DataTable};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        // Build a WidgetTree so the DataTable gets a real NodeId.
        let table = DataTable::new(
            vec!["A".into(), "B".into()],
            vec![
                vec!["r0".into(), "c0".into()],
                vec!["r1".into(), "c1".into()],
            ],
        );
        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(AppRoot::new()));
        let table_id = tree.mount(root_id, Box::new(table));

        // Synthesize hit-test bounds — pretend the table starts at (3, 2)
        // on screen (as if wrapped inside a Panel with a border).
        let mut hit = NodeHitTestMap::default();
        hit.bounds.insert(
            table_id,
            Rect {
                x0: 3,
                y0: 2,
                x1: 18,
                y1: 5,
            },
        );

        let rect = hit.rect(table_id).expect("table bounds missing");
        assert!(
            rect.x0 > 0 || rect.y0 > 0,
            "table should not start at origin"
        );

        // DataTable resolves with no border or line_pad by default, so
        // content origin == bound origin and (rect.x0, rect.y0) maps to (0, 0).
        let (lx, ly) = hit.content_local_coords(&mut tree, table_id, rect.x0, rect.y0);
        assert_eq!((lx, ly), (0, 0));
    }

    // ---- Layer sorting tests ----

    #[test]
    fn sort_children_by_layer_no_layers_preserves_dom_order() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let a = tree.mount(root, Box::new(Label::new("A")));
        let b = tree.mount(root, Box::new(Label::new("B")));
        let c = tree.mount(root, Box::new(Label::new("C")));

        let children = tree.children(root).to_vec();
        let sorted = sort_children_by_layer(&tree, root, &children);
        assert_eq!(sorted, vec![a, b, c]);
    }

    #[test]
    fn grid_keyline_spanned_cell_has_no_interior_divider() {
        // A grid keyline draws a rectangle per child; a child that spans two
        // columns is ONE region, so no vertical divider should appear inside it,
        // even though a NARROW cell beneath it ends at the inner column boundary.
        use crate::widget_tree::{Rect, WidgetTree};
        use crate::widgets::{AppRoot, Label};
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));

        // Wide child spans columns 0..1 (x in [0,78)), top row (y in [0,10)).
        let wide = tree.mount(root, Box::new(Label::new("wide")));
        // Narrow child below, only the left column (x in [0,38)), bottom row.
        let narrow = tree.mount(root, Box::new(Label::new("narrow")));

        if let Some(n) = tree.get_mut(wide) {
            n.layout_rect = Rect {
                x0: 1,
                y0: 1,
                x1: 78,
                y1: 10,
            };
        }
        if let Some(n) = tree.get_mut(narrow) {
            n.layout_rect = Rect {
                x0: 1,
                y0: 11,
                x1: 38,
                y1: 19,
            };
        }

        let parent_rect = Rect {
            x0: 0,
            y0: 0,
            x1: 80,
            y1: 20,
        };
        let mut frame = FrameBuffer::new(80, 20, None);
        let ctx = TreeRenderCtx {
            origin_x: 0,
            origin_y: 0,
            clip: ClipRect::for_frame(&frame),
            overlay_root_exempt: None,
        };
        let line_style = rich_rs::Style::new();
        let child_ids = vec![wide, narrow];
        paint_grid_keyline_rectangles(
            &tree,
            &child_ids,
            parent_rect,
            ctx,
            &mut frame,
            line_style,
            KeylineType::Heavy,
            '━',
            '┃',
        );

        // The narrow cell's right edge sits at x=38 (rect.x1). Its vertical line
        // spans only the narrow cell's gutter rows (y in [10..19]). It must NOT
        // bleed up into the wide cell's interior row (y=5).
        let interior = &frame.get(38, 5).text;
        assert!(
            interior.trim().is_empty(),
            "no interior keyline divider inside the spanned cell at (38,5), got {interior:?}"
        );
        // But the narrow cell DOES get its own right-edge vertical at x=38, y~14.
        let narrow_edge = &frame.get(38, 14).text;
        assert!(
            !narrow_edge.trim().is_empty(),
            "narrow cell's own right edge should draw a keyline at (38,14)"
        );
        // The wide cell's outer-right boundary at x=78 must be present in its rows.
        let wide_edge = &frame.get(78, 5).text;
        assert!(
            !wide_edge.trim().is_empty(),
            "wide cell's right edge should draw a keyline at (78,5)"
        );
    }

    #[test]
    fn sort_children_by_layer_with_layers_declaration() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        // Set layers on root via tree API (node-record owns inline styles after RA-2 step 6).
        tree.update_styles(root, |s| {
            s.style.layers = Some(vec!["base".into(), "overlay".into()]);
        });

        // Child A: layer = "overlay" (should be last)
        let a = tree.mount(root, Box::new(Label::new("A")));
        tree.update_styles(a, |s| s.style.layer = Some("overlay".into()));

        // Child B: no layer (should be first = default)
        let b = tree.mount(root, Box::new(Label::new("B")));

        // Child C: layer = "base" (should be between default and overlay)
        let c = tree.mount(root, Box::new(Label::new("C")));
        tree.update_styles(c, |s| s.style.layer = Some("base".into()));

        let children = tree.children(root).to_vec();
        let sorted = sort_children_by_layer(&tree, root, &children);
        // Expected: B (no layer=0), C (base=2), A (overlay=3)
        assert_eq!(sorted, vec![b, c, a]);
    }

    #[test]
    fn sort_children_by_layer_unknown_layer_falls_back_to_default() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        // Set layers on root via tree API (node-record owns inline styles after RA-2 step 6).
        tree.update_styles(root, |s| {
            s.style.layers = Some(vec!["base".into(), "overlay".into()]);
        });

        // Child A: no layer (group 0 = default)
        let a = tree.mount(root, Box::new(Label::new("A")));

        // Child B: layer = "unknown" (group 0 = falls back to default, preserves DOM order)
        let b = tree.mount(root, Box::new(Label::new("B")));
        tree.update_styles(b, |s| s.style.layer = Some("unknown".into()));

        // Child C: layer = "base" (group 1 = named)
        let c = tree.mount(root, Box::new(Label::new("C")));
        tree.update_styles(c, |s| s.style.layer = Some("base".into()));

        let children = tree.children(root).to_vec();
        let sorted = sort_children_by_layer(&tree, root, &children);
        // Expected: A (default=0), B (unknown→default=0, DOM order), C (base=1)
        assert_eq!(sorted, vec![a, b, c]);
    }

    #[test]
    fn sort_children_by_layer_preserves_dom_order_within_same_layer() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        // Set layers on root via tree API (node-record owns inline styles after RA-2 step 6).
        tree.update_styles(root, |s| s.style.layers = Some(vec!["bg".into()]));

        // Both children in the same layer — DOM order preserved.
        let a = tree.mount(root, Box::new(Label::new("A")));
        tree.update_styles(a, |s| s.style.layer = Some("bg".into()));

        let b = tree.mount(root, Box::new(Label::new("B")));
        tree.update_styles(b, |s| s.style.layer = Some("bg".into()));

        let children = tree.children(root).to_vec();
        let sorted = sort_children_by_layer(&tree, root, &children);
        assert_eq!(sorted, vec![a, b]);
    }

    #[test]
    fn sort_children_by_layer_without_layers_preserves_dom_order() {
        // Overlays float on top via DOM-last mount order, and real `overlay: screen`
        // surfaces float via `paint_deferred_overlays`. With no `layers:` declaration,
        // `sort_children_by_layer` must preserve DOM order with no widget-type
        // special-casing.
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let first = tree.mount(root, Box::new(Label::new("first")));
        let other = tree.mount(root, Box::new(Label::new("other")));

        let children = tree.children(root).to_vec();
        let sorted = sort_children_by_layer(&tree, root, &children);
        assert_eq!(
            sorted,
            vec![first, other],
            "no layers declaration must preserve DOM order (no widget-type special-case)"
        );
    }

    #[test]
    fn overlay_screen_escapes_to_top_z_over_later_sibling_and_is_hittable() {
        // RA2.4 mechanism: an `overlay: screen` node is NOT painted inline at its
        // position in the tree. It is deferred and painted UNCLIPPED at the TOP z
        // of the layer AFTER every sibling — even a sibling that comes LATER in DOM
        // order (which would normally occlude it). This is Python's placement/clip
        // ESCAPE, not a colour blend. Painting last also stamps the overlay's
        // `textual:widget_id` meta last, so hit-test occlusion is correct for free.
        use crate::widget_tree::{Rect, WidgetTree};
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));

        // DOM order: overlay FIRST, opaque sibling SECOND. Without the escape the
        // sibling (painted second) would occlude the overlay at the overlap.
        let overlay = tree.mount(root, Box::new(Label::new("OOOOOO")));
        let sibling = tree.mount(root, Box::new(Label::new("RRRRRRRRRRRRRRRRRRRR")));

        // Both occupy row 0. Manual layout rects (skip run_layout_pass) so the
        // geometry is deterministic; the overlap is cols 0..6.
        if let Some(n) = tree.get_mut(overlay) {
            n.layout_rect = Rect { x0: 0, y0: 0, x1: 6, y1: 1 };
        }
        if let Some(n) = tree.get_mut(sibling) {
            n.layout_rect = Rect { x0: 0, y0: 0, x1: 20, y1: 1 };
        }
        tree.update_styles(overlay, |s| {
            s.style.overlay = Some(crate::style::OverlayMode::Screen);
            s.style.constrain_x = Some(crate::style::Constrain::None);
            s.style.constrain_y = Some(crate::style::Constrain::Inside);
        });

        let console = rich_rs::Console::new();
        let mut frame = FrameBuffer::new(20, 1, None);
        let ctx = TreeRenderCtx {
            origin_x: 0,
            origin_y: 0,
            clip: ClipRect::for_frame(&frame),
            overlay_root_exempt: None,
        };
        let mut overlays: Vec<QueuedOverlay> = Vec::new();

        // Walk in DOM order: overlay escapes (queued, paints nothing), then the
        // sibling paints across the whole row.
        render_tree_node(&tree, overlay, ctx, &mut frame, &console, None, &mut overlays);
        assert_eq!(
            overlays.len(),
            1,
            "the overlay: screen node must be queued, not painted inline"
        );
        render_tree_node(&tree, sibling, ctx, &mut frame, &console, None, &mut overlays);
        assert_eq!(
            frame.get(0, 0).text,
            "R",
            "before the deferred pass, the later sibling owns the overlap"
        );

        // Deferred top-z pass: the overlay paints over the sibling.
        paint_deferred_overlays(&tree, &mut overlays, &mut frame, &console, None);
        assert_eq!(
            frame.get(0, 0).text,
            "O",
            "overlay: screen must paint on top of the later sibling at the overlap"
        );
        assert_eq!(
            frame.get(10, 0).text,
            "R",
            "outside the overlay, the sibling is untouched (overlay did not erase it)"
        );

        // Hit-test occlusion: the overlay's meta stamped last, so a click in the
        // overlap resolves to the overlay; the sibling is occluded there.
        let hit = crate::runtime::HitTestMap::from_frame(&frame);
        let overlay_rect = hit.rect(overlay).expect("overlay must be hittable");
        assert!(
            overlay_rect.x0 == 0 && overlay_rect.x1 >= 5,
            "overlay hit-rect must cover the overlap (got {overlay_rect:?})"
        );
        let sibling_rect = hit.rect(sibling).expect("sibling must be hittable");
        assert!(
            sibling_rect.x0 >= 6,
            "sibling must be occluded by the overlay across cols 0..6 (got {sibling_rect:?})"
        );
    }

    #[test]
    fn paint_walk_orders_overlapping_children_by_css_layer() {
        // Python `_compositor.py`: a parent's `layers` declaration orders its
        // layers bottom→top, so a child on a LATER layer paints on top of a
        // child on an earlier layer — regardless of compose (DOM) order. The
        // recursive paint walk previously iterated raw DOM order, so in
        // guide/layout/layers `#box2` (`layer: below`, composed second) painted
        // OVER `#box1` (`layer: above`).
        use crate::widget_tree::{Rect, WidgetTree};
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        // DOM order: the `above` child FIRST, the `below` child SECOND. A raw
        // DOM walk paints the `below` child last (wrongly on top).
        let above = tree.mount(root, Box::new(Label::new("AAAAAA")));
        let below = tree.mount(root, Box::new(Label::new("BBBBBB")));

        tree.update_styles(root, |s| {
            s.style.layers = Some(vec!["below".to_string(), "above".to_string()]);
        });
        tree.update_styles(above, |s| {
            s.style.layer = Some("above".to_string());
        });
        tree.update_styles(below, |s| {
            s.style.layer = Some("below".to_string());
        });

        // Both children fully overlap on row 0; manual rects keep the geometry
        // deterministic (this test is about PAINT order, not layout).
        if let Some(n) = tree.get_mut(root) {
            n.layout_rect = Rect { x0: 0, y0: 0, x1: 20, y1: 1 };
        }
        for id in [above, below] {
            if let Some(n) = tree.get_mut(id) {
                n.layout_rect = Rect { x0: 0, y0: 0, x1: 6, y1: 1 };
            }
        }

        let console = rich_rs::Console::new();
        let mut frame = FrameBuffer::new(20, 1, None);
        let ctx = TreeRenderCtx {
            origin_x: 0,
            origin_y: 0,
            clip: ClipRect::for_frame(&frame),
            overlay_root_exempt: None,
        };
        let mut overlays: Vec<QueuedOverlay> = Vec::new();
        render_tree_node(&tree, root, ctx, &mut frame, &console, None, &mut overlays);

        assert_eq!(
            frame.get(0, 0).text,
            "A",
            "the `above`-layer child must paint on top of the `below`-layer \
             child, regardless of DOM order"
        );
    }

    #[test]
    fn overlay_screen_is_drained_by_the_render_entry() {
        // Wiring guard: an `overlay: screen` node returns EARLY from the tree walk
        // (queued, not painted). If the render entry did not drain the queue via
        // `paint_deferred_overlays`, the node would vanish. Asserting its content
        // appears in the final frame proves the entry drains the escape queue.
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let overlay = tree.mount(root, Box::new(Label::new("WIRED")));
        tree.update_styles(overlay, |s| {
            s.style.overlay = Some(crate::style::OverlayMode::Screen);
        });

        let console = rich_rs::Console::new();
        let mut root_widget = AppRoot::new();
        let frame = render_tree_to_frame(&mut tree, &mut root_widget, &console, 20, 3);
        assert_eq!(
            frame.get(0, 0).text,
            "W",
            "overlay: screen content must be drained + painted by the render entry"
        );
    }

    #[test]
    fn collect_render_nodes_respects_layer_order() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        // Set layers on root via tree API (node-record owns inline styles after RA-2 step 6).
        tree.update_styles(root, |s| {
            s.style.layers = Some(vec!["base".into(), "top".into()]);
        });

        let top_id = tree.mount(root, Box::new(Label::new("top")));
        tree.update_styles(top_id, |s| s.style.layer = Some("top".into()));

        let base_id = tree.mount(root, Box::new(Label::new("base")));
        tree.update_styles(base_id, |s| s.style.layer = Some("base".into()));

        let nodes = collect_render_nodes(&tree);
        let ids: Vec<NodeId> = nodes.iter().map(|(id, _)| *id).collect();
        // Root first, then base (earlier layer), then top (later layer)
        assert_eq!(ids, vec![root, base_id, top_id]);
    }

    // ---- Layout pass activation tests ----

    #[test]
    fn run_layout_pass_sets_root_rects() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::AppRoot;

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));

        run_layout_pass(&mut tree, (80, 24));

        let node = tree.get(root).expect("root should exist");
        // Root should span the full viewport.
        assert_eq!(node.layout_rect.x0, 0);
        assert_eq!(node.layout_rect.y0, 0);
        assert_eq!(node.layout_rect.x1, 80);
        assert_eq!(node.layout_rect.y1, 24);
        assert_eq!(node.content_rect.x0, 0);
        assert_eq!(node.content_rect.y0, 0);
    }

    #[test]
    fn run_layout_pass_computes_child_rects() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let child_id = tree.mount(root, Box::new(Label::new("hello")));

        run_layout_pass(&mut tree, (80, 24));

        // After layout, the child should have a layout_rect set.
        let child = tree.get(child_id).expect("child should exist");
        let lr = child.layout_rect;
        // The child should be positioned within the viewport.
        assert!(
            lr.x1 > lr.x0 || lr.y1 > lr.y0 || (lr.x0 == 0 && lr.y0 == 0),
            "child should have a non-degenerate or zero-origin layout rect"
        );
    }

    #[test]
    fn layout_info_sets_vertical_scroll_virtual_content_in_tree_mode() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{Label, VerticalScroll};
        use rich_rs::{Console, ConsoleOptions, Segment};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(
            VerticalScroll::new().with_child(Label::new("line\n".repeat(80))),
        ));

        // Enter tree mode by extracting children into the arena tree.
        let children = {
            let root = tree.get_mut(root_id).expect("root exists");
            root.widget.compose()
        };
        for child in children {
            tree.mount(root_id, child.into_widget());
        }

        run_layout_pass(&mut tree, (40, 10));
        apply_layout_info_tree_from_layout_rects(&mut tree);

        let console = Console::default();
        let mut options = ConsoleOptions::default();
        options.size = (40, 10);
        options.max_width = 40;
        options.max_height = 10;

        let root = tree.get(root_id).expect("root exists");
        let rendered = root.widget.render_styled(&console, &options);
        let lines = Segment::split_and_crop_lines(rendered, 40, None, true, false);
        assert_eq!(
            lines.len(),
            10,
            "vertical scroll should render full viewport"
        );
        assert!(
            lines.iter().any(|line| line.len() > 1),
            "tree-mode vertical scroll should paint scrollbar chrome when content exceeds viewport"
        );
    }

    #[test]
    fn layout_info_sets_app_root_virtual_content_in_tree_mode() {
        use crate::css::StyleSheet;
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{APP_ROOT_VSCROLLBAR_ID, AppRoot, Container, Label};

        let mut sheet = crate::css::default_widget_stylesheet();
        sheet.extend(&StyleSheet::parse(
            "AppRoot { scrollbar-visibility: visible; scrollbar-gutter: stable; }",
        ));
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(Container::new()));
        let app_root_id = tree.mount(
            root_id,
            Box::new(AppRoot::new().with_child(Label::new("line\nline"))),
        );

        // Extract AppRoot children into the arena (including dedicated scrollbar lanes).
        let children = {
            let app_root = tree.get_mut(app_root_id).expect("app root exists");
            app_root.widget.compose()
        };
        for child in children {
            tree.mount(app_root_id, child.into_widget());
        }

        run_layout_pass(&mut tree, (40, 10));
        apply_layout_info_tree_from_layout_rects(&mut tree);
        let root = tree.get(app_root_id).expect("app root exists");
        let viewport_rect = root.content_rect;
        assert_eq!(
            viewport_rect.x1.saturating_sub(viewport_rect.x0),
            38,
            "app root viewport should exclude dedicated vertical scrollbar lane"
        );
        let app_viewport = (root.widget.as_ref() as &dyn std::any::Any)
            .downcast_ref::<AppRoot>()
            .and_then(|app_root| app_root.scroll_viewport_size())
            .expect("app root viewport size should be available after layout info");
        assert_eq!(
            app_viewport.0, 38,
            "app root internal viewport width should match computed content viewport width"
        );

        let vertical_scrollbar = tree
            .children(app_root_id)
            .iter()
            .filter_map(|&child_id| tree.get(child_id).map(|node| (child_id, node)))
            .find(|(child_id, _)| tree.css_id(*child_id) == Some(APP_ROOT_VSCROLLBAR_ID))
            .expect("app root vertical scrollbar child should exist");
        let lane_rect = vertical_scrollbar.1.layout_rect;
        assert_eq!(
            lane_rect.x1.saturating_sub(lane_rect.x0),
            2,
            "vertical scrollbar lane width should match Screen defaults"
        );
        assert_eq!(
            lane_rect.y1.saturating_sub(lane_rect.y0),
            10,
            "vertical scrollbar lane height should span viewport height"
        );
        // Python parity: `scrollbar-visibility: visible` does NOT force the bar
        // to show; `_refresh_scrollbars` keys `show_vertical` off overflow alone.
        // The content here ("line\nline") does NOT overflow the 10-row viewport,
        // so the bar is NOT displayed. The `scrollbar-gutter: stable` lane is
        // still RESERVED (width 2 above) — reservation and display are distinct.
        assert!(
            !vertical_scrollbar.1.display,
            "vertical scrollbar must NOT be displayed when content does not overflow, \
             even though the stable gutter still reserves the lane"
        );
    }

    #[test]
    fn app_root_releases_scrollbar_lane_when_resize_removes_overflow() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{APP_ROOT_VSCROLLBAR_ID, AppRoot, Container, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(Container::new()));
        let long_lines = std::iter::repeat_n("fear is the mind-killer.", 20)
            .collect::<Vec<_>>()
            .join("\n");
        let app_root_id = tree.mount(
            root_id,
            Box::new(AppRoot::new().with_child(Label::new(long_lines))),
        );

        let children = {
            let app_root = tree.get_mut(app_root_id).expect("app root should exist");
            app_root.widget.compose()
        };
        for child in children {
            tree.mount(app_root_id, child.into_widget());
        }

        run_layout_pass(&mut tree, (40, 10));
        let narrow = tree
            .get(app_root_id)
            .expect("app root should exist")
            .content_rect;
        let narrow_w = narrow.x1.saturating_sub(narrow.x0);

        let narrow_vbar_visible = tree
            .children(app_root_id)
            .iter()
            .copied()
            .filter_map(|child_id| tree.get(child_id).map(|node| (child_id, node)))
            .find(|(child_id, _)| tree.css_id(*child_id) == Some(APP_ROOT_VSCROLLBAR_ID))
            .map(|(_, node)| node.display)
            .unwrap_or(false);
        assert!(
            narrow_vbar_visible,
            "app root should show a vertical scrollbar when content overflows in narrow viewport"
        );

        run_layout_pass(&mut tree, (120, 40));
        let wide_node = tree.get(app_root_id).expect("app root should exist");
        let wide_content_w = wide_node
            .content_rect
            .x1
            .saturating_sub(wide_node.content_rect.x0);
        let wide_layout_w = wide_node
            .layout_rect
            .x1
            .saturating_sub(wide_node.layout_rect.x0);

        assert!(
            wide_content_w > narrow_w,
            "wider viewport should reclaim horizontal content space from previous scrollbar lane"
        );
        assert_eq!(
            wide_content_w, wide_layout_w,
            "when overflow is gone, app root content rect should expand to full layout width"
        );

        // The Label defaults to `width: auto` (Python parity), so it sizes to its
        // rendered content width ("fear is the mind-killer." = 24 cells) rather
        // than filling the content area. Scrollbar-lane reclamation is verified by
        // the app-root content_rect expansion above; here we confirm the auto-width
        // child relaid out to its content width and fits within the reclaimed area.
        let label_width = tree
            .children(app_root_id)
            .iter()
            .filter_map(|&child_id| tree.get(child_id))
            .find(|node| node.widget.style_type() == "Label")
            .map(|node| node.layout_rect.x1.saturating_sub(node.layout_rect.x0))
            .unwrap_or(0);
        assert_eq!(
            label_width, 24,
            "auto-width Label should size to its rendered content width"
        );
        assert!(
            label_width <= wide_content_w,
            "auto-width Label must fit within the reclaimed content area"
        );
    }

    #[test]
    fn collect_render_nodes_marks_display_none_as_not_rendered() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let child_id = tree.mount(root, Box::new(Label::new("hidden")));

        // Set runtime display=false on the child.
        tree.set_runtime_display(child_id, false);

        let nodes = collect_render_nodes(&tree);
        let child_entry = nodes.iter().find(|(id, _)| *id == child_id);
        assert!(
            matches!(child_entry, Some((_, false))),
            "display:none child should be marked as not rendered"
        );
    }

    #[test]
    fn run_layout_pass_preserves_runtime_hidden_when_css_display_is_visible() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let child_id = tree.mount(root, Box::new(Label::new("hidden by runtime")));

        // Hide via runtime control (not CSS).
        tree.set_runtime_display(child_id, false);
        run_layout_pass(&mut tree, (80, 24));
        assert!(
            !tree.get(child_id).expect("child exists").display,
            "runtime-hidden child must stay hidden after CSS display sync"
        );

        // Re-enable runtime visibility and ensure layout pass can show it again.
        tree.set_runtime_display(child_id, true);
        run_layout_pass(&mut tree, (80, 24));
        assert!(
            tree.get(child_id).expect("child exists").display,
            "runtime-visible child should render when CSS display allows it"
        );
    }

    #[test]
    fn run_layout_pass_applies_parent_selector_display_rules() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Widget};

        struct Parent;

        impl Widget for Parent {
            fn style_type(&self) -> &'static str {
                "Parent"
            }

            fn render(
                &self,
                _console: &rich_rs::Console,
                _options: &rich_rs::ConsoleOptions,
            ) -> Segments {
                Segments::new()
            }
        }

        struct Child;

        impl Widget for Child {
            fn style_type(&self) -> &'static str {
                "Child"
            }

            fn render(
                &self,
                _console: &rich_rs::Console,
                _options: &rich_rs::ConsoleOptions,
            ) -> Segments {
                Segments::new()
            }
        }

        let mut sheet = crate::css::default_widget_stylesheet();
        sheet.extend(&crate::css::StyleSheet::parse(
            r#"
Parent > Child { display: none; }
Parent.show > Child { display: block; }
"#,
        ));
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let parent_id = tree.mount(root, Box::new(Parent));
        let child_id = tree.mount(parent_id, Box::new(Child));

        run_layout_pass(&mut tree, (80, 24));
        assert!(
            !tree.get(child_id).expect("child exists").display,
            "child should be hidden by parent-combinator display:none rule"
        );

        // Toggle class on the tree node (canonical class owner after RA-2 step 6).
        tree.add_class(parent_id, "show");

        run_layout_pass(&mut tree, (80, 24));
        assert!(
            tree.get(child_id).expect("child exists").display,
            "child should become visible when parent class toggles matching rule"
        );
    }

    #[test]
    fn run_layout_pass_syncs_collapsible_title_symbol() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Collapsible, Widget};

        let _guard = crate::css::set_style_context(crate::css::default_widget_stylesheet());

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));

        // Mount an expanded Collapsible + its composed children (title + contents).
        let mut collapsible = Collapsible::new("Section").collapsed(false);
        let children = collapsible.compose();
        let collapsible_id = tree.mount(root, Box::new(collapsible));
        for child in children {
            tree.mount(collapsible_id, child.into_widget());
        }

        let title_id = *tree
            .children(collapsible_id)
            .first()
            .expect("collapsible has a title child");

        let title_symbol = |tree: &WidgetTree, id| -> String {
            let node = tree.get(id).expect("title node exists");
            let console = rich_rs::Console::new();
            let mut opts = rich_rs::ConsoleOptions::default();
            opts.size = (20, 1);
            opts.max_width = 20;
            opts.max_height = 1;
            Widget::render(node.widget.as_ref(), &console, &opts)
                .iter()
                .map(|s| s.text.to_string())
                .collect::<String>()
        };

        // Expanded: layout sync leaves the expanded (▼) symbol on the title.
        run_layout_pass(&mut tree, (80, 24));
        assert!(
            title_symbol(&tree, title_id).contains('\u{25bc}'),
            "expanded Collapsible title should render the ▼ symbol"
        );

        // Toggle the parent to collapsed; the title child must follow (▶) after
        // the next layout pass, mirroring Python `_update_collapsed`.
        {
            let node = tree.get_mut(collapsible_id).expect("collapsible node exists");
            let any = node.widget.as_mut() as &mut dyn std::any::Any;
            any.downcast_mut::<Collapsible>()
                .expect("node is a Collapsible")
                .toggle();
        }
        run_layout_pass(&mut tree, (80, 24));
        assert!(
            title_symbol(&tree, title_id).contains('\u{25b6}'),
            "collapsed Collapsible title should render the ▶ symbol after relayout"
        );
    }

    fn render_with_optional_screen(screen: Option<Box<dyn crate::screen::Screen>>) -> String {
        let mut app = App::new().expect("app should initialize");
        app.options.size = (80, 24);
        app.options.max_width = 80;
        app.options.max_height = 24;

        let mut root =
            crate::widgets::AppRoot::new().with_child(crate::widgets::Label::new("BASE_VISIBLE"));
        app.build_widget_tree(&mut root);
        if let Some(screen) = screen {
            app.push_screen(screen);
        }
        app.render_widget(&mut root).expect("render should succeed");
        app.frame.as_plain_lines().join("\n")
    }

    #[test]
    fn modal_screen_layer_preserves_underlay_text() {
        struct EmptyOverlayWidget;

        impl Widget for EmptyOverlayWidget {
            fn render(
                &self,
                _console: &rich_rs::Console,
                _options: &rich_rs::ConsoleOptions,
            ) -> Segments {
                Segments::new()
            }
        }

        struct ModalOverlayScreen;

        impl crate::screen::Screen for ModalOverlayScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(EmptyOverlayWidget)
            }
        }

        let lines = render_with_optional_screen(Some(Box::new(ModalOverlayScreen)));
        assert!(
            lines.contains("BASE_VISIBLE"),
            "modal screen with translucent background should preserve underlay content"
        );
    }

    #[test]
    fn modal_screen_layer_tints_underlay_colors() {
        use rich_rs::{Segment, Segments, SimpleColor, Style};

        struct StyledUnderlay;

        impl Widget for StyledUnderlay {
            fn render(
                &self,
                _console: &rich_rs::Console,
                _options: &rich_rs::ConsoleOptions,
            ) -> Segments {
                let mut segment = Segment::new("BASE_VISIBLE");
                segment.style = Some(
                    Style::new()
                        .with_color(SimpleColor::Rgb {
                            r: 240,
                            g: 240,
                            b: 240,
                        })
                        .with_bgcolor(SimpleColor::Rgb {
                            r: 80,
                            g: 30,
                            b: 30,
                        }),
                );
                vec![segment].into()
            }
        }

        struct EmptyOverlayWidget;

        impl Widget for EmptyOverlayWidget {
            fn render(
                &self,
                _console: &rich_rs::Console,
                _options: &rich_rs::ConsoleOptions,
            ) -> Segments {
                Segments::new()
            }
        }

        struct ModalOverlayScreen;

        impl crate::screen::Screen for ModalOverlayScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(EmptyOverlayWidget)
            }
        }

        let mut app = App::new().expect("app should initialize");
        app.options.size = (80, 24);
        app.options.max_width = 80;
        app.options.max_height = 24;

        let mut root = crate::widgets::AppRoot::new().with_child(StyledUnderlay);
        app.build_widget_tree(&mut root);
        app.render_widget(&mut root)
            .expect("baseline render should succeed");
        let baseline = app.frame.get(0, 0).style.clone().unwrap_or_default();

        app.push_screen(Box::new(ModalOverlayScreen));
        app.render_widget(&mut root)
            .expect("modal render should succeed");
        let modal = app.frame.get(0, 0).style.clone().unwrap_or_default();

        assert!(
            modal.bgcolor != baseline.bgcolor || modal.color != baseline.color,
            "modal screen should tint underlay colors through shared alpha compositing path"
        );
        assert_ne!(
            modal.dim,
            Some(true),
            "modal dim path should not force dim text style flag"
        );
    }

    #[test]
    fn non_modal_screen_layer_hides_underlay_text() {
        struct EmptyOverlayWidget;

        impl Widget for EmptyOverlayWidget {
            fn render(
                &self,
                _console: &rich_rs::Console,
                _options: &rich_rs::ConsoleOptions,
            ) -> Segments {
                Segments::new()
            }
        }

        struct NonModalOverlayScreen;

        impl crate::screen::Screen for NonModalOverlayScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(EmptyOverlayWidget)
            }

            fn is_modal(&self) -> bool {
                false
            }
        }

        let lines = render_with_optional_screen(Some(Box::new(NonModalOverlayScreen)));
        assert!(
            !lines.contains("BASE_VISIBLE"),
            "opaque non-modal screen should hide underlay app content"
        );
    }

    #[test]
    fn screen_stylesheet_does_not_leak_to_underlay_layer() {
        struct EmptyOverlayWidget;

        impl Widget for EmptyOverlayWidget {
            fn render(
                &self,
                _console: &rich_rs::Console,
                _options: &rich_rs::ConsoleOptions,
            ) -> Segments {
                Segments::new()
            }
        }

        struct ModalSheetIsolationScreen;

        impl crate::screen::Screen for ModalSheetIsolationScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(EmptyOverlayWidget)
            }

            fn css(&self) -> Option<&str> {
                Some("Label { display: none; }")
            }
        }

        let lines = render_with_optional_screen(Some(Box::new(ModalSheetIsolationScreen)));
        assert!(
            lines.contains("BASE_VISIBLE"),
            "screen-specific stylesheet rules should not affect app underlay layer"
        );
    }

    /// Regression for scroll/screen parity with Python Textual:
    /// `host_content_extent` (the scrollable virtual size) must include the
    /// spacing consumed by docked children on each edge AND the margin-box of
    /// flow children, mirroring Python's `DockArrangeResult.total_region`
    /// (`spatial_map` unions each placement grown by its margin, then the result
    /// is grown by the docked scroll spacing). Without this, a screen with a
    /// docked Header/Footer (e.g. `guide/screens/modal01`) under-reports its
    /// virtual height by the dock height, shifting the scrollbar thumb glyph;
    /// and a horizontal scroll of margined columns (`how-to/layout06`)
    /// under-reports its virtual width by the outer column margins.
    #[test]
    fn host_content_extent_includes_dock_spacing_and_child_margins() {
        use crate::style::{Dock, Spacing};
        use crate::widget_tree::{Rect, WidgetTree};
        use crate::widgets::{Label, VerticalScroll};

        let _guard = crate::css::set_style_context(crate::css::default_widget_stylesheet());

        let mut tree = WidgetTree::new();
        let host = tree.set_root(Box::new(VerticalScroll::new()));
        let flow = tree.mount(host, Box::new(Label::new("x")));
        let footer = tree.mount(host, Box::new(Label::new("f")));

        {
            let node = tree.get_mut(flow).expect("flow child exists");
            node.layout_rect = Rect {
                x0: 5,
                y0: 3,
                x1: 35,
                y1: 23,
            };
            node.content_rect = node.layout_rect;
            // Horizontal margin of 2 on each side (vertical margin 0).
            node.styles.style.margin = Some(Spacing {
                top: 0,
                right: 2,
                bottom: 0,
                left: 2,
            });
        }
        {
            let node = tree.get_mut(footer).expect("footer child exists");
            node.layout_rect = Rect {
                x0: 0,
                y0: 27,
                x1: 40,
                y1: 30,
            };
            node.content_rect = node.layout_rect;
            node.styles.style.dock = Some(Dock::Bottom);
        }

        let content_rect = Rect {
            x0: 0,
            y0: 0,
            x1: 40,
            y1: 30,
        };
        let (virtual_w, virtual_h, has_content) =
            host_content_extent(&tree, host, content_rect, ScrollbarHostChildren::default());

        assert!(
            has_content,
            "the flow child (not the docked footer) must flag content presence"
        );
        // Flow margin-box spans x: (5-2)..(35+2) = 3..37 => width 34. No L/R dock.
        assert_eq!(
            virtual_w, 34,
            "flow child horizontal margins must enlarge the virtual width"
        );
        // Flow span y: 3..23 = 20, plus the bottom-docked footer height (3) = 23.
        assert_eq!(
            virtual_h, 23,
            "a bottom-docked footer must enlarge the scrollable virtual height"
        );
    }
}
