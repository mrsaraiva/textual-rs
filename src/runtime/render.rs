use crate::css::{
    AppRuntimePseudos, StyleSheet, begin_style_render_pass, pop_style_context, push_style_context,
    resolve_style, selector_meta_generic, selector_meta_generic_with_classes, set_app_active,
    set_app_runtime_pseudos, set_style_context, take_layout_affected_style_changes,
};
use crate::debug::{debug_layout, debug_render};
use crate::node_id::{NodeId, node_id_to_ffi};
use crate::render::{DirtyRegion, FrameBuffer};
use crate::style::{
    BorderEdge, Color, Constrain, Hatch, KeylineType, Layout, OverlayMode, TextOverflow, TextWrap,
    color_from_simple,
};
use crate::widget_tree::WidgetTree;
use crate::widgets::{
    APP_ROOT_HSCROLLBAR_ID, APP_ROOT_SCROLLBAR_CORNER_ID, APP_ROOT_VSCROLLBAR_ID,
    DATA_TABLE_HSCROLLBAR_ID, KEY_PANEL_VSCROLLBAR_ID, LOG_VSCROLLBAR_ID, Overlay,
    RICH_LOG_VSCROLLBAR_ID, SCROLL_VIEW_HSCROLLBAR_ID, SCROLL_VIEW_SCROLLBAR_CORNER_ID,
    SCROLL_VIEW_VSCROLLBAR_ID, ScrollBar, ScrollbarPolicy, Toast, Widget,
    border_spacing_from_style, crop_line_horizontal,
};

use rich_rs::{ControlType, MetaValue, Renderable, Segment, Segments, StyleMeta};
use std::sync::OnceLock;

use super::App;
use super::types::{
    HitTestMap, SYNC_END, SYNC_START, SegmentStreamStats, TOAST_GAP_ROWS, TOAST_SIDE_MARGIN,
    resize_trace_enabled,
};

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
        let diff = prepend_clear_if_needed(next.diff_to_segments(&self.frame), clear_before_draw);
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
            inline: self.app_inline,
            ansi: self.app_ansi,
            nocolor: self.app_nocolor,
        });

        // Tree-driven render path: walk the arena tree depth-first,
        // rendering each widget at its layout_rect position.
        if self.active_widget_tree().is_some() {
            return self.render_tree_composed(widget, dirty_regions, layout_invalidation);
        }

        let mut sheet = self.default_stylesheet.clone();
        sheet.extend(&self.stylesheet);
        if let Some(screen_sheet) = self.active_screen_stylesheet() {
            sheet.extend(screen_sheet);
        }
        let _guard = set_style_context(sheet);
        begin_style_render_pass();

        // Legacy render path: recursive widget.render_styled() from root.
        let segments = if self.debug_layout.enabled {
            widget.render_styled_with_debug(&self.console, &self.options, &self.debug_layout)
        } else {
            widget.render_styled(&self.console, &self.options)
        };
        let layout_affected_style_change = take_layout_affected_style_changes();
        let (width, height) = self.options.size;
        let lines = rich_rs::Segment::split_and_crop_lines(segments, width, None, true, false);
        let base_style = self.theme.base.to_rich();
        let mut next = FrameBuffer::from_lines(&lines, width, height, base_style);
        self.compose_notifications(&mut next);
        let now = std::time::Instant::now();
        let dt_ms = now.duration_since(self.last_render_at).as_millis();
        self.last_render_at = now;
        let clear_before_draw = self.clear_on_next_render;
        let diff_body = if clear_before_draw {
            next.diff_to_segments(&self.frame)
        } else if let Some(regions) = dirty_regions {
            next.diff_to_segments_in_regions(&self.frame, regions)
        } else {
            next.diff_to_segments(&self.frame)
        };
        let diff = prepend_clear_if_needed(diff_body, clear_before_draw);
        let stream_stats = analyze_segment_stream(&diff, next.width);
        debug_render(&format!(
            "[render_widget] dt={}ms resized={} clear={} size={}x{} prev={}x{} diff.segments={} (control={} text_segments={} text_bytes={})",
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
                "[render_trace] kind=widget size={}x{} controls={} home={} clear={} cr={} move_to={} cursor_moves={} text_segments={} text_bytes={} newlines={} touch_last_col={} overflow_right={} max_cursor=({}, {}) control_head=[{}]",
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

        // Re-establish style context for notification overlays — the per-layer
        // guard was dropped when the `for` loop ended above.
        let notification_sheet = self.stylesheet_for_layer(None);
        let _notif_guard = set_style_context(notification_sheet);
        self.compose_notifications(&mut next);
        drop(_notif_guard);

        let now = std::time::Instant::now();
        let dt_ms = now.duration_since(self.last_render_at).as_millis();
        self.last_render_at = now;
        let clear_before_draw = self.clear_on_next_render;
        let diff_body = if clear_before_draw {
            next.diff_to_segments(&self.frame)
        } else if let Some(regions) = dirty_regions {
            next.diff_to_segments_in_regions(&self.frame, regions)
        } else {
            next.diff_to_segments(&self.frame)
        };
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
        let Some(root) = entry.widget_tree.get(root_id) else {
            return false;
        };

        let sheet = self.stylesheet_for_layer(entry.stylesheet.as_ref());
        let _guard = set_style_context(sheet);
        begin_style_render_pass();
        let meta =
            selector_meta_generic_with_classes(root.widget.as_ref(), root.classes.iter().cloned());
        let resolved = resolve_style(root.widget.as_ref(), &meta);
        resolved.bg.is_some_and(|bg| bg.a == u8::MAX)
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

    pub(super) fn compose_notifications(&mut self, frame: &mut FrameBuffer) {
        if self.notifications.is_empty() {
            return;
        }

        let mut cursor_bottom = frame.height.saturating_sub(2);
        for note in self.notifications.iter().rev() {
            let mut toast = Toast::new(note.message.clone(), note.severity);
            if !note.title.is_empty() {
                toast = toast.with_title(note.title.clone());
            }

            let max_width = frame
                .width
                .saturating_sub(TOAST_SIDE_MARGIN.saturating_mul(2))
                .max(1);
            let preferred = 60usize.min((frame.width / 2).max(1));
            let toast_width = preferred.min(max_width).max(1);
            let toast_height = toast.layout_height().unwrap_or(3).max(1);
            if toast_height > frame.height {
                continue;
            }
            if cursor_bottom + 1 < toast_height {
                break;
            }

            let mut toast_options = self.options.clone();
            toast_options.size = (toast_width, toast_height);
            toast_options.max_width = toast_width;
            toast_options.max_height = toast_height;

            let rendered = toast.render_styled(&self.console, &toast_options);
            let lines = Segment::split_and_crop_lines(rendered, toast_width, None, true, false);
            let lines = Segment::set_shape(&lines, toast_width, Some(toast_height), None, false);
            let toast_buffer = FrameBuffer::from_lines(&lines, toast_width, toast_height, None);

            let x0 = frame
                .width
                .saturating_sub(toast_width.saturating_add(TOAST_SIDE_MARGIN));
            let y0 = cursor_bottom + 1 - toast_height;
            Overlay::compose_overlay_at(frame, &toast_buffer, x0, y0);

            cursor_bottom = y0.saturating_sub(1 + TOAST_GAP_ROWS);
            if cursor_bottom == 0 {
                break;
            }
        }
    }

    /// Distribute layout information to the root widget from the hit-test map.
    ///
    /// Root-only: child widgets receive layout info via the tree-based
    /// [`apply_layout_info_tree`] path when the arena tree is available.
    pub(super) fn apply_layout_info(&self, root: &mut dyn Widget, hit_test: &HitTestMap) {
        if let Some(rect) = hit_test.rect(NodeId::default()) {
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
    let w = (rect.x1.saturating_sub(rect.x0)) as usize;
    let h = (rect.y1.saturating_sub(rect.y0)) as usize;

    // Only render if the node is visible AND has a non-zero extent.
    let should_render = node.visibility == crate::style::Visibility::Visible && w > 0 && h > 0;

    // Resolve style early — needed for outline, hatch, overlay, and children.
    let meta =
        selector_meta_generic_with_classes(node.widget.as_ref(), node.classes.iter().cloned());
    let resolved = resolve_style(node.widget.as_ref(), &meta);

    if should_render {
        let dest_x = i32::from(rect.x0) + ctx.origin_x;
        let dest_y = i32::from(rect.y0) + ctx.origin_y;
        let screen_underlay = if matches!(resolved.overlay, Some(OverlayMode::Screen)) {
            Some(capture_underlay_snapshot(
                frame, dest_x, dest_y, w, h, ctx.clip,
            ))
        } else {
            None
        };

        // Create options sized to this widget's layout rect.
        let mut opts = rich_rs::ConsoleOptions::default();
        opts.size = (w, h);
        opts.max_width = w;
        opts.max_height = h;

        // render_styled_dyn_obj handles CSS resolution, border composition,
        // segment tagging with the real arena NodeId, and style stack
        // push/pop for this node's own content rendering.
        let segments = crate::widgets::render_widget_with_meta(
            node.widget.as_ref(),
            console,
            &opts,
            debug,
            node_id,
            &meta,
            &resolved,
            &format!(
                "{}{}{}",
                node.widget.style_type(),
                node.widget
                    .style_id()
                    .map(|id| format!("#{id}"))
                    .unwrap_or_default(),
                node.widget
                    .style_classes()
                    .iter()
                    .map(|class| format!(".{class}"))
                    .collect::<Vec<_>>()
                    .join("")
            ),
        );
        if node.widget.preserve_underlay() && !segments.is_empty() {
            if let Some(bg) = resolved.bg {
                let widget_clip = ClipRect {
                    x0: dest_x,
                    y0: dest_y,
                    x1: dest_x + w as i32,
                    y1: dest_y + h as i32,
                };
                if let Some(paint_clip) = ctx.clip.intersect(widget_clip) {
                    if bg.a == u8::MAX {
                        fill_rect_with_background(frame, paint_clip, bg);
                    } else if bg.a > 0 {
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

        // P2-28: Paint outline edges outside the widget's layout rect.
        paint_outline(&resolved, dest_x, dest_y, w, h, ctx.clip, frame);

        // P2-34: Apply hatch fill to blank cells within the widget area.
        if let Some(ref hatch) = resolved.hatch {
            apply_hatch_fill(frame, hatch, dest_x, dest_y, w, h, ctx.clip);
        }

        // P2-34: Apply overlay compositing mode.
        if let Some(ref overlay) = resolved.overlay {
            let fallback = Vec::new();
            let underlay = screen_underlay.as_deref().unwrap_or(fallback.as_slice());
            apply_overlay_compositing(frame, overlay, dest_x, dest_y, w, h, ctx.clip, underlay);
        }
    }
    // Clone keyline before push_style_context takes ownership of resolved.
    let inline_style = node.widget.style();
    let node_keyline = resolved
        .keyline
        .or_else(|| inline_style.clone().and_then(|s| s.keyline));
    let node_layout = resolved
        .layout
        .or_else(|| inline_style.and_then(|s| s.layout));
    push_style_context(meta, resolved);

    let mut child_ctx = ctx;
    if node.widget.clips_descendants_to_content() {
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
    let child_ids: Vec<NodeId> = tree.children(node_id).to_vec();
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
        let use_scroll_ctx = has_scroll_viewport && child_uses_parent_scroll(tree, child_id);
        let mut next_ctx = if use_scroll_ctx {
            scrolled_child_ctx
        } else {
            base_child_ctx
        };
        if use_scroll_ctx {
            if let Some(child) = tree.get(child_id) {
                let rect = child.layout_rect;
                let child_clip = ClipRect {
                    x0: i32::from(rect.x0) + base_child_ctx.origin_x,
                    y0: i32::from(rect.y0) + base_child_ctx.origin_y,
                    x1: i32::from(rect.x1) + base_child_ctx.origin_x,
                    y1: i32::from(rect.y1) + base_child_ctx.origin_y,
                };
                if let Some(intersection) = next_ctx.clip.intersect(child_clip) {
                    next_ctx.clip = intersection;
                } else {
                    continue;
                }
            }
        }
        render_tree_node(tree, child_id, next_ctx, frame, console, debug);
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

    pop_style_context();
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

    let root_meta = selector_meta_generic(root_widget);
    let root_resolved = resolve_style(root_widget, &root_meta);
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
    };

    for child_id in child_ids {
        let child_ctx = if root_child_uses_root_scroll(tree, root_id, child_id) {
            scroll_ctx
        } else {
            base_ctx
        };
        render_tree_node(tree, child_id, child_ctx, frame, console, debug);
    }

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
    let root_meta = selector_meta_generic_with_classes(
        root_node.widget.as_ref(),
        root_node.classes.iter().cloned(),
    );
    let root_resolved = resolve_style(root_node.widget.as_ref(), &root_meta);
    let root_rect = root_node.layout_rect;
    let root_scroll = root_node.widget.scroll_offset_f32();
    let child_ids: Vec<NodeId> = tree.children(root_id).to_vec();

    if let Some(clip) = clip_rect_from_tree_rect(root_rect, frame) {
        if let Some(bg) = root_resolved.bg {
            if bg.a == u8::MAX {
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
    };

    for child_id in child_ids {
        let child_ctx = if root_child_uses_root_scroll(tree, root_id, child_id) {
            scroll_ctx
        } else {
            base_ctx
        };
        render_tree_node(tree, child_id, child_ctx, frame, console, debug);
    }

    pop_style_context();
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

fn tint_rect_with_background(frame: &mut FrameBuffer, clip: ClipRect, tint: Color) {
    if tint.a == 0 {
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
    let Some(node) = tree.get(node_id) else {
        return false;
    };
    let meta =
        selector_meta_generic_with_classes(node.widget.as_ref(), node.classes.iter().cloned());
    let resolved = resolve_style(node.widget.as_ref(), &meta);
    resolved.dock.is_some()
}

fn node_is_dedicated_scrollbar(tree: &WidgetTree, node_id: NodeId) -> bool {
    let Some(node) = tree.get(node_id) else {
        return false;
    };
    matches!(
        node.widget.style_id(),
        Some(
            APP_ROOT_VSCROLLBAR_ID
                | APP_ROOT_HSCROLLBAR_ID
                | APP_ROOT_SCROLLBAR_CORNER_ID
                | SCROLL_VIEW_VSCROLLBAR_ID
                | SCROLL_VIEW_HSCROLLBAR_ID
                | SCROLL_VIEW_SCROLLBAR_CORNER_ID
                | LOG_VSCROLLBAR_ID
                | RICH_LOG_VSCROLLBAR_ID
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
// P2-28: Outline rendering (paints OUTSIDE the border box)
// ===========================================================================

/// Paint outline edges outside a widget's layout rect into the frame buffer.
///
/// Unlike CSS `border` which occupies space within the layout rect, `outline`
/// paints in the surrounding cells without affecting layout. Each outline edge
/// is 1 cell wide and uses the same border-character rendering as regular
/// borders, but positioned outside the widget's bounding box.
///
/// Cells that fall outside the frame or clip rect are silently skipped.
fn paint_outline(
    resolved: &crate::style::Style,
    dest_x: i32,
    dest_y: i32,
    w: usize,
    h: usize,
    clip: ClipRect,
    frame: &mut FrameBuffer,
) {
    let outline_top = &resolved.outline_top;
    let outline_right = &resolved.outline_right;
    let outline_bottom = &resolved.outline_bottom;
    let outline_left = &resolved.outline_left;

    if !outline_top.is_set()
        && !outline_right.is_set()
        && !outline_bottom.is_set()
        && !outline_left.is_set()
    {
        return;
    }

    let frame_clip = ClipRect {
        x0: 0,
        y0: 0,
        x1: frame.width as i32,
        y1: frame.height as i32,
    };
    let Some(paint_clip) = clip.intersect(frame_clip) else {
        return;
    };
    // Outline paints just outside the layout rect. Expand the clip by one cell
    // so right/bottom edges are not dropped when descendants are clipped to
    // their content box.
    let paint_clip = ClipRect {
        x0: paint_clip.x0.saturating_sub(1),
        y0: paint_clip.y0.saturating_sub(1),
        x1: paint_clip.x1.saturating_add(1),
        y1: paint_clip.y1.saturating_add(1),
    };

    let fallback_bg =
        crate::style::parse_color_like("$background").unwrap_or(crate::style::Color::rgb(0, 0, 0));

    // Helper: paint a single outline cell if it's within bounds.
    let paint_cell = |frame: &mut FrameBuffer, x: i32, y: i32, ch: char, edge: &BorderEdge| {
        if x < paint_clip.x0 || x >= paint_clip.x1 || y < paint_clip.y0 || y >= paint_clip.y1 {
            return;
        }
        let ux = x as usize;
        let uy = y as usize;
        if ux >= frame.width || uy >= frame.height {
            return;
        }
        let color = edge.color().unwrap_or(fallback_bg);
        let style = rich_rs::Style::new()
            .with_color(color.to_simple_opaque())
            .with_bgcolor(fallback_bg.to_simple_opaque());
        let cell = frame.get_mut(ux, uy);
        cell.text = ch.to_string();
        cell.style = Some(style);
        cell.continuation = false;
    };

    // Top outline: row at dest_y - 1, columns [dest_x .. dest_x + w).
    if outline_top.is_set() {
        let y = dest_y - 1;
        let ch = outline_char_horizontal(outline_top);
        for col in 0..w as i32 {
            paint_cell(frame, dest_x + col, y, ch, outline_top);
        }
    }

    // Bottom outline: row at dest_y + h, columns [dest_x .. dest_x + w).
    if outline_bottom.is_set() {
        let y = dest_y + h as i32;
        let ch = outline_char_horizontal(outline_bottom);
        for col in 0..w as i32 {
            paint_cell(frame, dest_x + col, y, ch, outline_bottom);
        }
    }

    // Left outline: column at dest_x - 1, rows [dest_y .. dest_y + h).
    if outline_left.is_set() {
        let x = dest_x - 1;
        let ch = outline_char_vertical(outline_left);
        for row in 0..h as i32 {
            paint_cell(frame, x, dest_y + row, ch, outline_left);
        }
    }

    // Right outline: column at dest_x + w, rows [dest_y .. dest_y + h).
    if outline_right.is_set() {
        let x = dest_x + w as i32;
        let ch = outline_char_vertical(outline_right);
        for row in 0..h as i32 {
            paint_cell(frame, x, dest_y + row, ch, outline_right);
        }
    }
}

/// Pick horizontal outline character based on border type.
fn outline_char_horizontal(edge: &BorderEdge) -> char {
    match edge {
        BorderEdge::Edge {
            border_type: crate::style::BorderType::Solid,
            ..
        } => '─',
        BorderEdge::Edge {
            border_type: crate::style::BorderType::Heavy,
            ..
        } => '━',
        BorderEdge::Edge {
            border_type: crate::style::BorderType::Block,
            ..
        } => '▀',
        BorderEdge::Edge {
            border_type: crate::style::BorderType::Tall,
            ..
        } => '▔',
        BorderEdge::Edge {
            border_type: crate::style::BorderType::Outer,
            ..
        } => '▀',
        _ => '─',
    }
}

/// Pick vertical outline character based on border type.
fn outline_char_vertical(edge: &BorderEdge) -> char {
    match edge {
        BorderEdge::Edge {
            border_type: crate::style::BorderType::Solid,
            ..
        } => '│',
        BorderEdge::Edge {
            border_type: crate::style::BorderType::Heavy,
            ..
        } => '┃',
        BorderEdge::Edge {
            border_type: crate::style::BorderType::Block,
            ..
        } => '█',
        BorderEdge::Edge {
            border_type: crate::style::BorderType::Tall,
            ..
        } => '▊',
        BorderEdge::Edge {
            border_type: crate::style::BorderType::Outer,
            ..
        } => '▌',
        _ => '│',
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
fn apply_hatch_fill(
    frame: &mut FrameBuffer,
    hatch: &Hatch,
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
    let fg_color = hatch.color.to_simple_opaque();
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
                cell.text = hatch.character.to_string();
                let mut style = cell.style.unwrap_or_else(rich_rs::Style::new);
                style.color = Some(fg_color);
                cell.style = Some(style);
            }
        }
    }
}

// ===========================================================================
// P2-34: Overlay compositing mode
// ===========================================================================

/// Apply overlay compositing to a widget's painted region.
///
/// `OverlayMode::Screen` blends the widget's colors with the underlying frame
/// using a screen-blend formula. `OverlayMode::None` is a no-op (normal paint).
fn apply_overlay_compositing(
    frame: &mut FrameBuffer,
    overlay: &OverlayMode,
    x0: i32,
    y0: i32,
    w: usize,
    h: usize,
    clip: ClipRect,
    underlay: &[OverlayCell],
) {
    match overlay {
        OverlayMode::None => {
            // Normal compositing — already the default paint behavior.
        }
        OverlayMode::Screen => {
            let frame_h = frame.height as i32;
            let frame_w = frame.width as i32;
            let mut idx = 0usize;
            for dy in 0..h {
                let y = y0 + dy as i32;
                for dx in 0..w {
                    let x = x0 + dx as i32;
                    if x < clip.x0
                        || y < clip.y0
                        || x >= clip.x1
                        || y >= clip.y1
                        || x < 0
                        || y < 0
                        || x >= frame_w
                        || y >= frame_h
                    {
                        idx = idx.saturating_add(1);
                        continue;
                    }
                    let Some(base) = underlay.get(idx) else {
                        idx = idx.saturating_add(1);
                        continue;
                    };
                    idx = idx.saturating_add(1);

                    let cell = frame.get_mut(x as usize, y as usize);
                    let mut style = cell.style.unwrap_or_default();

                    if let (Some(over_bg), Some(under_bg)) =
                        (style.bgcolor.map(crate::style::color_from_simple), base.bg)
                    {
                        style.bgcolor = Some(screen_blend(under_bg, over_bg).to_simple_opaque());
                    }

                    if let (Some(over_fg), Some(under_fg)) =
                        (style.color.map(crate::style::color_from_simple), base.fg)
                    {
                        style.color = Some(screen_blend(under_fg, over_fg).to_simple_opaque());
                    }

                    cell.style = Some(style);
                }
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
    let ch = match keyline.keyline_type {
        KeylineType::None => return,
        KeylineType::Thin => match layout {
            Layout::Horizontal => '│',
            _ => '─',
        },
        KeylineType::Heavy => match layout {
            Layout::Horizontal => '┃',
            _ => '━',
        },
        KeylineType::Double => match layout {
            Layout::Horizontal => '║',
            _ => '═',
        },
    };
    let line_style = rich_rs::Style::new().with_color(keyline.color.to_simple_opaque());

    let Some(parent) = tree.get(parent_id) else {
        return;
    };
    let parent_rect = node_content_or_layout_rect(parent);
    let child_ids: Vec<NodeId> = tree.children(parent_id).to_vec();
    if child_ids.len() < 2 {
        return;
    }

    for pair in child_ids.windows(2) {
        let Some(a) = tree.get(pair[0]) else {
            continue;
        };
        let Some(_b) = tree.get(pair[1]) else {
            continue;
        };
        let ar = a.layout_rect;
        match layout {
            Layout::Horizontal => {
                let x = i32::from(ar.x1) + ctx.origin_x;
                let y0 = i32::from(parent_rect.y0) + ctx.origin_y;
                let y1 = i32::from(parent_rect.y1) + ctx.origin_y;
                if x < ctx.clip.x0 || x >= ctx.clip.x1 {
                    continue;
                }
                for y in y0.max(ctx.clip.y0)..y1.min(ctx.clip.y1) {
                    if y < 0 || y >= frame.height as i32 || x < 0 || x >= frame.width as i32 {
                        continue;
                    }
                    let cell = frame.get_mut(x as usize, y as usize);
                    cell.text = ch.to_string();
                    cell.style = Some(line_style);
                    cell.continuation = false;
                }
            }
            _ => {
                let y = i32::from(ar.y1) + ctx.origin_y;
                let x0 = i32::from(parent_rect.x0) + ctx.origin_x;
                let x1 = i32::from(parent_rect.x1) + ctx.origin_x;
                if y < ctx.clip.y0 || y >= ctx.clip.y1 {
                    continue;
                }
                for x in x0.max(ctx.clip.x0)..x1.min(ctx.clip.x1) {
                    if x < 0 || x >= frame.width as i32 || y < 0 || y >= frame.height as i32 {
                        continue;
                    }
                    let cell = frame.get_mut(x as usize, y as usize);
                    cell.text = ch.to_string();
                    cell.style = Some(line_style);
                    cell.continuation = false;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Default)]
struct OverlayCell {
    fg: Option<crate::style::Color>,
    bg: Option<crate::style::Color>,
}

fn capture_underlay_snapshot(
    frame: &FrameBuffer,
    x0: i32,
    y0: i32,
    w: usize,
    h: usize,
    clip: ClipRect,
) -> Vec<OverlayCell> {
    let mut out = Vec::with_capacity(w.saturating_mul(h));
    let frame_h = frame.height as i32;
    let frame_w = frame.width as i32;
    for dy in 0..h {
        let y = y0 + dy as i32;
        for dx in 0..w {
            let x = x0 + dx as i32;
            if x < clip.x0
                || y < clip.y0
                || x >= clip.x1
                || y >= clip.y1
                || x < 0
                || y < 0
                || x >= frame_w
                || y >= frame_h
            {
                out.push(OverlayCell::default());
                continue;
            }
            let cell = frame.get(x as usize, y as usize);
            let style = cell.style.unwrap_or_default();
            out.push(OverlayCell {
                fg: style.color.map(crate::style::color_from_simple),
                bg: style.bgcolor.map(crate::style::color_from_simple),
            });
        }
    }
    out
}

fn screen_blend(base: crate::style::Color, over: crate::style::Color) -> crate::style::Color {
    fn chan(a: u8, b: u8) -> u8 {
        let af = a as f32 / 255.0;
        let bf = b as f32 / 255.0;
        ((1.0 - (1.0 - af) * (1.0 - bf)) * 255.0).round() as u8
    }
    crate::style::Color::rgba(
        chan(base.r, over.r),
        chan(base.g, over.g),
        chan(base.b, over.b),
        over.a,
    )
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
    if let Some(root_id) = tree.root() {
        let root_meta = selector_meta_generic(root);
        let root_resolved = resolve_style(root, &root_meta);
        push_style_context(root_meta, root_resolved);

        let child_ids: Vec<NodeId> = tree.children(root_id).to_vec();
        let (root_scroll_x, root_scroll_y) = root.scroll_offset();
        let base_ctx = TreeRenderCtx {
            origin_x: 0,
            origin_y: 0,
            clip: ClipRect::for_frame(&frame),
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
        };
        for child_id in child_ids {
            let child_ctx = if root_child_uses_root_scroll(tree, root_id, child_id) {
                scroll_ctx
            } else {
                base_ctx
            };
            render_tree_node(tree, child_id, child_ctx, &mut frame, console, debug);
        }

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
        let child_extent_x = child_rect.x1.saturating_sub(content_rect.x0) as usize;
        let child_extent_y = child_rect.y1.saturating_sub(content_rect.y0) as usize;
        virtual_w = virtual_w.max(child_extent_x);
        virtual_h = virtual_h.max(child_extent_y);
    }
    if !saw_visible_child {
        virtual_w = content_rect.x1.saturating_sub(content_rect.x0) as usize;
        virtual_h = content_rect.y1.saturating_sub(content_rect.y0) as usize;
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
        match child.widget.style_id() {
            Some(APP_ROOT_VSCROLLBAR_ID | SCROLL_VIEW_VSCROLLBAR_ID) => {
                children.vertical = Some(child_id)
            }
            Some(APP_ROOT_HSCROLLBAR_ID | SCROLL_VIEW_HSCROLLBAR_ID | DATA_TABLE_HSCROLLBAR_ID) => {
                children.horizontal = Some(child_id)
            }
            Some(APP_ROOT_SCROLLBAR_CORNER_ID | SCROLL_VIEW_SCROLLBAR_CORNER_ID) => {
                children.corner = Some(child_id)
            }
            Some(LOG_VSCROLLBAR_ID | RICH_LOG_VSCROLLBAR_ID | KEY_PANEL_VSCROLLBAR_ID) => {
                children.vertical = Some(child_id)
            }
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
    let mut min_x: Option<u16> = None;
    let mut min_y: Option<u16> = None;
    let mut max_x = 0u16;
    let mut max_y = 0u16;
    let mut saw_visible_child = false;
    for &child_id in tree.children(node_id) {
        if Some(child_id) == scrollbar_children.vertical
            || Some(child_id) == scrollbar_children.horizontal
            || Some(child_id) == scrollbar_children.corner
        {
            continue;
        }
        if node_is_docked(tree, child_id) {
            continue;
        }
        let Some(child) = tree.get(child_id) else {
            continue;
        };
        if !child.display {
            continue;
        }
        saw_visible_child = true;
        let child_rect = child.layout_rect;
        min_x = Some(min_x.map_or(child_rect.x0, |value| value.min(child_rect.x0)));
        min_y = Some(min_y.map_or(child_rect.y0, |value| value.min(child_rect.y0)));
        max_x = max_x.max(child_rect.x1);
        max_y = max_y.max(child_rect.y1);
    }
    if !saw_visible_child {
        let viewport_w = content_rect.x1.saturating_sub(content_rect.x0) as usize;
        let viewport_h = content_rect.y1.saturating_sub(content_rect.y0) as usize;
        if let Some(node) = tree.get(node_id)
            && let Some((virtual_w, virtual_h)) = node.widget.scroll_virtual_content_size()
        {
            return (virtual_w.max(1), virtual_h.max(1), false);
        }
        return (viewport_w.max(1), viewport_h.max(1), false);
    }
    let origin_x = min_x.unwrap_or(content_rect.x0);
    let origin_y = min_y.unwrap_or(content_rect.y0);
    let virtual_w = max_x.saturating_sub(origin_x) as usize;
    let virtual_h = max_y.saturating_sub(origin_y) as usize;
    (virtual_w.max(1), virtual_h.max(1), true)
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

        let (content_rect, style, offset_x, offset_y) = {
            let Some(node) = tree.get(node_id) else {
                continue;
            };
            let meta = selector_meta_generic_with_classes(
                node.widget.as_ref(),
                node.classes.iter().cloned(),
            );
            let style = resolve_style(node.widget.as_ref(), &meta);
            let (offset_x, offset_y) = node.widget.scroll_offset_f32();
            (node.content_rect, style, offset_x, offset_y)
        };
        let content_w = content_rect.x1.saturating_sub(content_rect.x0).max(1) as usize;
        let content_h = content_rect.y1.saturating_sub(content_rect.y0).max(1) as usize;

        let (virtual_w, virtual_h, mut has_content_children) =
            host_content_extent(tree, node_id, content_rect, scrollbar_children);
        let mut geometry = {
            let policy = ScrollbarPolicy::from_style(&style, 2, 1);
            policy.resolve(
                content_w,
                content_h,
                virtual_w.max(content_w),
                virtual_h.max(content_h),
            )
        };

        if geometry.viewport_width != content_w || geometry.viewport_height != content_h {
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

            geometry = {
                let (virtual_w, virtual_h, had_children) =
                    host_content_extent(tree, node_id, content_rect, scrollbar_children);
                has_content_children = had_children;
                let policy = ScrollbarPolicy::from_style(&style, 2, 1);
                policy.resolve(
                    content_w,
                    content_h,
                    virtual_w.max(content_w),
                    virtual_h.max(content_h),
                )
            };
        }

        let viewport_rect = crate::widget_tree::Rect {
            x0: content_rect.x0,
            y0: content_rect.y0,
            x1: content_rect
                .x0
                .saturating_add(geometry.viewport_width as u16),
            y1: content_rect
                .y0
                .saturating_add(geometry.viewport_height as u16),
        };
        if let Some(node) = tree.get_mut(node_id) {
            node.content_rect = viewport_rect;
            if !has_content_children {
                node.layout_rect = viewport_rect;
            }
            node.widget
                .set_virtual_content_size(geometry.content_width, geometry.content_height);
        }

        if let Some(v_id) = scrollbar_children.vertical {
            let show = geometry.vertical_lane_width > 0;
            set_runtime_display(tree, v_id, show);
            let rect = if show {
                crate::widget_tree::Rect {
                    x0: content_rect
                        .x0
                        .saturating_add(geometry.viewport_width as u16),
                    y0: content_rect.y0,
                    x1: content_rect.x1,
                    y1: content_rect
                        .y0
                        .saturating_add(geometry.viewport_height as u16),
                }
            } else {
                crate::widget_tree::Rect::ZERO
            };
            set_layout_rect(tree, v_id, rect);
            if let Some(node) = tree.get_mut(v_id) {
                let any = node.widget.as_mut() as &mut dyn std::any::Any;
                if let Some(scrollbar) = any.downcast_mut::<ScrollBar>() {
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
            let show = geometry.horizontal_lane_height > 0;
            set_runtime_display(tree, h_id, show);
            let rect = if show {
                crate::widget_tree::Rect {
                    x0: content_rect.x0,
                    y0: content_rect
                        .y0
                        .saturating_add(geometry.viewport_height as u16),
                    x1: content_rect
                        .x0
                        .saturating_add(geometry.viewport_width as u16),
                    y1: content_rect.y1,
                }
            } else {
                crate::widget_tree::Rect::ZERO
            };
            set_layout_rect(tree, h_id, rect);
            if let Some(node) = tree.get_mut(h_id) {
                let any = node.widget.as_mut() as &mut dyn std::any::Any;
                if let Some(scrollbar) = any.downcast_mut::<ScrollBar>() {
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
            let show = geometry.vertical_lane_width > 0 && geometry.horizontal_lane_height > 0;
            set_runtime_display(tree, c_id, show);
            let rect = if show {
                crate::widget_tree::Rect {
                    x0: content_rect
                        .x0
                        .saturating_add(geometry.viewport_width as u16),
                    y0: content_rect
                        .y0
                        .saturating_add(geometry.viewport_height as u16),
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
    let move_command_palette_last = |ordered: Vec<NodeId>| -> Vec<NodeId> {
        // CommandPalette is a system-modal surface and should render on top of
        // sibling widgets regardless of mount order/layer assignment.
        let mut regular = Vec::with_capacity(ordered.len());
        let mut palettes = Vec::new();
        for child in ordered {
            let is_command_palette = tree
                .get(child)
                .map(|node| node.widget.style_type() == "CommandPalette")
                .unwrap_or(false);
            if is_command_palette {
                palettes.push(child);
            } else {
                regular.push(child);
            }
        }
        regular.extend(palettes);
        regular
    };

    // Resolve the parent's `layers` declaration.
    let parent_layers: Option<Vec<String>> = tree.get(parent).and_then(|node| {
        let meta = crate::css::selector_meta_generic_with_classes(
            node.widget.as_ref(),
            node.classes.iter().cloned(),
        );
        let style = crate::css::resolve_style(node.widget.as_ref(), &meta);
        style.layers
    });

    let layer_order = match parent_layers {
        Some(ref layers) if !layers.is_empty() => layers,
        _ => return move_command_palette_last(children.to_vec()),
        // No layers declaration — keep DOM order except modal command palette priority.
    };

    // Resolve each child's `layer` property.
    let child_layers: Vec<Option<String>> = children
        .iter()
        .map(|&child| {
            tree.get(child).and_then(|node| {
                let meta = crate::css::selector_meta_generic_with_classes(
                    node.widget.as_ref(),
                    node.classes.iter().cloned(),
                );
                let style = crate::css::resolve_style(node.widget.as_ref(), &meta);
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
    let ordered: Vec<NodeId> = indexed.iter().map(|&(_, i)| children[i]).collect();
    move_command_palette_last(ordered)
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
        let (
            full_w,
            full_h,
            line_pad,
            top,
            bottom,
            left,
            right,
            virtual_content_w,
            virtual_content_h,
        ) = {
            let Some(node) = tree.get(node_id) else {
                continue;
            };
            let widget_any = node.widget.as_ref() as &dyn std::any::Any;
            let uses_viewport_layout_rect = widget_any.is::<crate::widgets::AppRoot>();
            let rect = if uses_viewport_layout_rect {
                node.content_rect
            } else {
                node.layout_rect
            };
            let meta = crate::css::selector_meta_generic_with_classes(
                node.widget.as_ref(),
                node.classes.iter().cloned(),
            );
            let resolved = crate::css::resolve_style(node.widget.as_ref(), &meta);
            let line_pad = resolved.line_pad.unwrap_or(0) as usize;
            let (top, bottom, left, right) = border_spacing_from_style(&resolved);
            let full_w = rect.x1.saturating_sub(rect.x0) as usize;
            let full_h = rect.y1.saturating_sub(rect.y0) as usize;
            let content_rect = node.content_rect;
            let mut virtual_w = 0usize;
            let mut virtual_h = 0usize;
            let mut saw_visible_child = false;

            // For tree-mode scroll containers, derive virtual content extent
            // from laid-out child bounds so scrollbars/offset limits are correct.
            for &child_id in tree.children(node_id) {
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
                let child_extent_x = child_rect.x1.saturating_sub(content_rect.x0) as usize;
                let child_extent_y = child_rect.y1.saturating_sub(content_rect.y0) as usize;
                virtual_w = virtual_w.max(child_extent_x);
                virtual_h = virtual_h.max(child_extent_y);
            }
            if !saw_visible_child {
                virtual_w = content_rect.x1.saturating_sub(content_rect.x0) as usize;
                virtual_h = content_rect.y1.saturating_sub(content_rect.y0) as usize;
            }

            (
                full_w, full_h, line_pad, top, bottom, left, right, virtual_w, virtual_h,
            )
        };

        let content_w = full_w
            .saturating_sub(left + right)
            .saturating_sub(line_pad.saturating_mul(2))
            .max(1) as u16;
        let content_h = full_h.saturating_sub(top + bottom).max(1) as u16;

        if let Some(node) = tree.get_mut(node_id) {
            node.widget.on_layout(content_w, content_h);
            node.widget
                .set_virtual_content_size(virtual_content_w, virtual_content_h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{SYNC_END, SYNC_START};
    use super::*;

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
    fn sort_children_by_layer_with_layers_declaration() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        // Create root with layers: "base overlay"
        let mut root_widget = AppRoot::new();
        {
            let styles = root_widget.styles_mut().unwrap();
            styles.style.layers = Some(vec!["base".into(), "overlay".into()]);
        }

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(root_widget));

        // Child A: layer = "overlay" (should be last)
        let mut label_a = Label::new("A");
        label_a.styles_mut().unwrap().style.layer = Some("overlay".into());
        let a = tree.mount(root, Box::new(label_a));

        // Child B: no layer (should be first = default)
        let b = tree.mount(root, Box::new(Label::new("B")));

        // Child C: layer = "base" (should be between default and overlay)
        let mut label_c = Label::new("C");
        label_c.styles_mut().unwrap().style.layer = Some("base".into());
        let c = tree.mount(root, Box::new(label_c));

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

        let mut root_widget = AppRoot::new();
        root_widget.styles_mut().unwrap().style.layers =
            Some(vec!["base".into(), "overlay".into()]);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(root_widget));

        // Child A: no layer (group 0 = default)
        let a = tree.mount(root, Box::new(Label::new("A")));

        // Child B: layer = "unknown" (group 0 = falls back to default, preserves DOM order)
        let mut label_b = Label::new("B");
        label_b.styles_mut().unwrap().style.layer = Some("unknown".into());
        let b = tree.mount(root, Box::new(label_b));

        // Child C: layer = "base" (group 1 = named)
        let mut label_c = Label::new("C");
        label_c.styles_mut().unwrap().style.layer = Some("base".into());
        let c = tree.mount(root, Box::new(label_c));

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

        let mut root_widget = AppRoot::new();
        root_widget.styles_mut().unwrap().style.layers = Some(vec!["bg".into()]);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(root_widget));

        // Both children in the same layer — DOM order preserved.
        let mut label_a = Label::new("A");
        label_a.styles_mut().unwrap().style.layer = Some("bg".into());
        let a = tree.mount(root, Box::new(label_a));

        let mut label_b = Label::new("B");
        label_b.styles_mut().unwrap().style.layer = Some("bg".into());
        let b = tree.mount(root, Box::new(label_b));

        let children = tree.children(root).to_vec();
        let sorted = sort_children_by_layer(&tree, root, &children);
        assert_eq!(sorted, vec![a, b]);
    }

    #[test]
    fn sort_children_by_layer_moves_command_palette_last_without_layers() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, CommandPalette, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let palette = tree.mount(root, Box::new(CommandPalette::new(Label::new("body"))));
        let other = tree.mount(root, Box::new(Label::new("other")));

        let children = tree.children(root).to_vec();
        let sorted = sort_children_by_layer(&tree, root, &children);
        assert_eq!(
            sorted,
            vec![other, palette],
            "command palette must render last/top-most among siblings"
        );
    }

    #[test]
    fn collect_render_nodes_keeps_command_palette_topmost_even_if_mounted_first() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, CommandPalette, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let palette = tree.mount(root, Box::new(CommandPalette::new(Label::new("body"))));
        let other = tree.mount(root, Box::new(Label::new("other")));

        let nodes = collect_render_nodes(&tree);
        let ids: Vec<NodeId> = nodes.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![root, other, palette]);
    }

    #[test]
    fn command_palette_tree_open_tints_underlay_but_not_panel_surface() {
        use crate::event::{Action, Event, EventCtx};
        use crate::style::{Position, Scalar};
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, CommandPalette, Spacer};
        use rich_rs::{Segment, Segments, SimpleColor, Style};

        struct StyledUnderlay;

        impl Widget for StyledUnderlay {
            fn render(
                &self,
                _console: &rich_rs::Console,
                _options: &rich_rs::ConsoleOptions,
            ) -> Segments {
                let mut segment = Segment::new("Fear is the mind-killer.");
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

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        tree.mount(root, Box::new(StyledUnderlay));

        let mut palette =
            CommandPalette::new(Spacer::new(1)).with_tree_wrapped_child_visible(false);
        if let Some(styles) = palette.styles_mut() {
            styles.style.position = Some(Position::Absolute);
            styles.style.width = Some(Scalar::Percent(100.0));
            styles.style.height = Some(Scalar::Percent(100.0));
        }
        let palette_id = tree.mount(root, Box::new(palette));

        let palette_children = {
            let node = tree.get_mut(palette_id).expect("palette node should exist");
            node.widget.take_composed_children()
        };
        for child in palette_children {
            tree.mount(palette_id, child);
        }

        let resolved_bg = {
            let node = tree.get(palette_id).expect("palette node should exist");
            let meta = crate::css::selector_meta_generic_with_classes(
                node.widget.as_ref(),
                node.classes.iter().cloned(),
            );
            let resolved = crate::css::resolve_style(node.widget.as_ref(), &meta);
            resolved.bg
        };
        let bg = resolved_bg.expect("CommandPalette must resolve a background color");
        assert!(
            bg.a > 0 && bg.a < u8::MAX,
            "CommandPalette background should be translucent for shared dim path (alpha={})",
            bg.a
        );

        let console = rich_rs::Console::new();
        let mut root_widget = AppRoot::new();
        let baseline_frame = render_tree_to_frame(&mut tree, &mut root_widget, &console, 60, 14);
        let palette_rect = tree
            .get(palette_id)
            .expect("palette node should exist")
            .layout_rect;
        let palette_node = tree.get(palette_id).expect("palette node should exist");
        assert!(
            palette_node.widget.preserve_underlay(),
            "palette should opt into preserve-underlay rendering"
        );
        assert!(palette_node.display, "palette node should be displayed");
        assert_eq!(
            palette_node.visibility,
            crate::style::Visibility::Visible,
            "palette node should be visible"
        );
        assert!(
            palette_rect.x1 > palette_rect.x0 && palette_rect.y1 > palette_rect.y0,
            "palette layout rect should be non-zero after layout"
        );
        assert_eq!(
            palette_rect.x0, 0,
            "runtime-host command palette should start at screen left"
        );
        assert_eq!(
            palette_rect.y0, 0,
            "runtime-host command palette should start at screen top"
        );
        let sample_x = usize::from(palette_rect.x0).min(59);
        let sample_y = usize::from(palette_rect.y0).min(13);
        let baseline_underlay_style = baseline_frame
            .get(sample_x, sample_y)
            .style
            .clone()
            .unwrap_or_default();

        {
            let node = tree.get_mut(palette_id).expect("palette node should exist");
            let mut ctx = EventCtx::default();
            node.widget
                .on_event(&Event::Action(Action::CommandPalette), &mut ctx);
        }

        let frame = render_tree_to_frame(&mut tree, &mut root_widget, &console, 60, 14);

        let underlay_style = frame
            .get(sample_x, sample_y)
            .style
            .clone()
            .unwrap_or_default();
        assert!(
            underlay_style.bgcolor != baseline_underlay_style.bgcolor
                || underlay_style.color != baseline_underlay_style.color,
            "underlay outside the palette panel should be visually tinted when palette is open"
        );
        assert_ne!(
            underlay_style.dim,
            Some(true),
            "shared dim path should tint background instead of forcing dim text style"
        );

        let panel_cell = frame.get(1, 3);
        let panel_style = panel_cell.style.clone().unwrap_or_default();
        assert!(
            panel_style.bgcolor.is_some(),
            "panel surface should be painted when palette is open"
        );
        assert_ne!(
            panel_style.dim,
            Some(true),
            "palette panel surface should remain undimmed"
        );
    }

    #[test]
    fn collect_render_nodes_respects_layer_order() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut root_widget = AppRoot::new();
        root_widget.styles_mut().unwrap().style.layers = Some(vec!["base".into(), "top".into()]);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(root_widget));

        let mut label_top = Label::new("top");
        label_top.styles_mut().unwrap().style.layer = Some("top".into());
        let top_id = tree.mount(root, Box::new(label_top));

        let mut label_base = Label::new("base");
        label_base.styles_mut().unwrap().style.layer = Some("base".into());
        let base_id = tree.mount(root, Box::new(label_base));

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
            root.widget.take_composed_children()
        };
        for child in children {
            tree.mount(root_id, child);
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
            app_root.widget.take_composed_children()
        };
        for child in children {
            tree.mount(app_root_id, child);
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
            .find(|(_, node)| node.widget.style_id() == Some(APP_ROOT_VSCROLLBAR_ID))
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
        assert!(
            vertical_scrollbar.1.display,
            "vertical scrollbar node should be visible when content overflows"
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
            app_root.widget.take_composed_children()
        };
        for child in children {
            tree.mount(app_root_id, child);
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
            .filter_map(|&child_id| tree.get(child_id))
            .find(|node| node.widget.style_id() == Some(APP_ROOT_VSCROLLBAR_ID))
            .map(|node| node.display)
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

        let label_width = tree
            .children(app_root_id)
            .iter()
            .filter_map(|&child_id| tree.get(child_id))
            .find(|node| node.widget.style_type() == "Label")
            .map(|node| node.layout_rect.x1.saturating_sub(node.layout_rect.x0))
            .unwrap_or(0);
        assert_eq!(
            label_width, wide_content_w,
            "app root content child should relayout to reclaimed width once scrollbar disappears"
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
}
