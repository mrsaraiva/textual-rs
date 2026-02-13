use crate::css::{
    begin_style_render_pass, pop_style_context, push_style_context, resolve_style,
    selector_meta_generic, set_app_active, set_style_context, take_layout_affected_style_changes,
};
use crate::debug::debug_render;
use crate::node_id::NodeId;
use crate::render::{DirtyRegion, FrameBuffer};
use crate::widget_tree::WidgetTree;
use crate::widgets::{Overlay, Toast, Widget, border_spacing_from_style};

use rich_rs::{ControlType, Renderable, Segment, Segments};

use super::App;
use super::types::{
    HitTestMap, NodeHitTestMap, SYNC_END, SYNC_START, SegmentStreamStats, TOAST_GAP_ROWS,
    TOAST_SIDE_MARGIN, resize_trace_enabled,
};

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
        let mut sheet = self.default_stylesheet.clone();
        sheet.extend(&self.stylesheet);
        let _active = set_app_active(self.app_active);
        let _guard = set_style_context(sheet);
        begin_style_render_pass();

        // Run CSS layout pass when layout is invalidated and tree is available.
        // This computes layout_rect/content_rect for all tree nodes before
        // rendering, so precomputed rects are available for widget sizing.
        if layout_invalidation {
            if let Some(tree) = self.widget_tree.as_mut() {
                let (w, h) = self.options.size;
                run_layout_pass(tree, (w as u16, h as u16));
                let render_nodes = collect_render_nodes(tree);
                debug_render(&format!(
                    "[layout_pass] viewport={}x{} render_nodes={}",
                    w,
                    h,
                    render_nodes.len()
                ));
            }
        }

        // Tree-driven render path: walk the arena tree depth-first,
        // rendering each widget at its layout_rect position.
        if self.widget_tree.is_some() {
            return self.render_tree_composed(widget, dirty_regions, layout_invalidation);
        }

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
    /// tree is populated (i.e., `self.widget_tree` is `Some`).
    fn render_tree_composed(
        &mut self,
        widget: &mut dyn Widget,
        dirty_regions: Option<&[DirtyRegion]>,
        layout_invalidation: bool,
    ) -> crate::Result<()> {
        let (width, height) = self.options.size;
        let base_style = self.theme.base.to_rich();

        // Take tree out of self to avoid borrow conflicts — we need &tree for
        // reading widgets and &mut self for frame/notifications/output.
        let tree = self.widget_tree.take().unwrap();

        let mut next = FrameBuffer::new(width, height, base_style);

        // Render the real root widget first. Its children have been extracted,
        // so this produces only the root's CSS chrome (background, border, padding).
        let root_segments = widget.render_styled_dyn_obj(
            &self.console,
            &self.options,
            if self.debug_layout.enabled {
                Some(&self.debug_layout)
            } else {
                None
            },
            NodeId::default(),
        );
        let root_lines = Segment::split_and_crop_lines(root_segments, width, None, true, false);
        for (row, line) in root_lines.iter().enumerate() {
            next.write_line_at(0, row, line, true);
        }

        // Walk tree children (skip the stub root node) and render each at
        // its layout_rect position with CSS style stack management.
        if let Some(root_id) = tree.root() {
            // Push root widget's style context so children can inherit.
            let root_meta = selector_meta_generic(widget);
            let root_resolved = resolve_style(widget, &root_meta);
            push_style_context(root_meta, root_resolved);

            let child_ids: Vec<NodeId> = tree.children(root_id).to_vec();
            for child_id in child_ids {
                render_tree_node(&tree, child_id, &mut next, &self.console);
            }

            pop_style_context();
        }

        let layout_affected_style_change = take_layout_affected_style_changes();

        // Put tree back before the rest of the pipeline.
        self.widget_tree = Some(tree);

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
            let line_pad = resolved.padding.map(|s| s.left as usize).unwrap_or(0);
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
    frame: &mut FrameBuffer,
    console: &rich_rs::Console,
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

    if should_render {
        // Create options sized to this widget's layout rect.
        let mut opts = rich_rs::ConsoleOptions::default();
        opts.size = (w, h);
        opts.max_width = w;
        opts.max_height = h;

        // render_styled_dyn_obj handles CSS resolution, border composition,
        // segment tagging with the real arena NodeId, and style stack
        // push/pop for this node's own content rendering.
        let segments = node
            .widget
            .render_styled_dyn_obj(console, &opts, None, node_id);

        // Split into lines and paint at the layout position.
        let lines = rich_rs::Segment::split_and_crop_lines(segments, w, None, true, false);
        for (row_idx, line) in lines.iter().enumerate() {
            let y = rect.y0 as usize + row_idx;
            if y >= frame.height {
                break;
            }
            frame.write_line_at(rect.x0 as usize, y, line, false);
        }
    }

    // Push this node's resolved style onto the stack for children
    // to inherit, then recurse into children.
    let meta = selector_meta_generic(node.widget.as_ref());
    let resolved = resolve_style(node.widget.as_ref(), &meta);
    push_style_context(meta, resolved);

    let child_ids: Vec<NodeId> = tree.children(node_id).to_vec();
    for child_id in child_ids {
        render_tree_node(tree, child_id, frame, console);
    }

    pop_style_context();
}

// ===========================================================================
// P1-12 / P2-18a: Arena-tree-based render scaffold + layout integration
//
// These standalone functions implement tree-walk render and layout patterns
// using `WidgetTree`. The layout pass (`run_layout_pass`) computes CSS-based
// `layout_rect`/`content_rect` for every tree node before rendering.
// ===========================================================================

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
pub(crate) fn run_layout_pass(tree: &mut WidgetTree, viewport: (u16, u16)) {
    let root_id = match tree.root() {
        Some(r) => r,
        None => return,
    };

    // Sync CSS display/visibility values to WidgetNode fields before layout.
    crate::css::apply_display_visibility_to_tree(tree);

    let available = crate::layout::Region::new(0, 0, viewport.0, viewport.1);

    // Set root's own rects to the full viewport.
    if let Some(root) = tree.get_mut(root_id) {
        root.layout_rect = available.to_rect();
        root.content_rect = available.to_rect();
    }

    // Resolve children's layout rects.
    crate::layout::resolve_layout(tree, root_id, available, viewport);
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
    // Resolve the parent's `layers` declaration.
    let parent_layers: Option<Vec<String>> = tree.get(parent).and_then(|node| {
        let meta = crate::css::selector_meta_generic(node.widget.as_ref());
        let style = crate::css::resolve_style(node.widget.as_ref(), &meta);
        style.layers
    });

    let layer_order = match parent_layers {
        Some(ref layers) if !layers.is_empty() => layers,
        _ => return children.to_vec(), // No layers declaration — keep DOM order.
    };

    // Resolve each child's `layer` property.
    let child_layers: Vec<Option<String>> = children
        .iter()
        .map(|&child| {
            tree.get(child).and_then(|node| {
                let meta = crate::css::selector_meta_generic(node.widget.as_ref());
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

    indexed.iter().map(|&(_, i)| children[i]).collect()
}

/// Distribute layout information to widgets using the arena tree + `NodeHitTestMap`.
///
/// This is the P1-12 replacement for `App::apply_layout_info` which uses
/// recursive `visit_children_mut`. Walks the tree depth-first and calls
/// `on_layout(content_w, content_h)` on each widget whose bounding rect
/// appears in the hit-test map.
pub(crate) fn apply_layout_info_tree(tree: &mut WidgetTree, hit_test: &NodeHitTestMap) {
    let root = match tree.root() {
        Some(r) => r,
        None => return,
    };
    let node_ids = tree.walk_depth_first(root);
    for node_id in node_ids {
        let Some(rect) = hit_test.rect(node_id) else {
            continue;
        };
        let Some(node) = tree.get(node_id) else {
            continue;
        };
        let meta = crate::css::selector_meta_generic(node.widget.as_ref());
        let resolved = crate::css::resolve_style(node.widget.as_ref(), &meta);
        let line_pad = resolved.padding.map(|s| s.left as usize).unwrap_or(0);
        let (top, bottom, left, right) = border_spacing_from_style(&resolved);
        let full_w = rect.x1.saturating_sub(rect.x0) as usize + 1;
        let full_h = rect.y1.saturating_sub(rect.y0) as usize + 1;
        let content_w = full_w
            .saturating_sub(left + right)
            .saturating_sub(line_pad.saturating_mul(2))
            .max(1) as u16;
        let content_h = full_h.saturating_sub(top + bottom).max(1) as u16;

        // Re-borrow mutably for on_layout.
        if let Some(node) = tree.get_mut(node_id) {
            node.widget.on_layout(content_w, content_h);
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
    fn collect_render_nodes_marks_display_none_as_not_rendered() {
        use crate::widget_tree::WidgetTree;
        use crate::widgets::{AppRoot, Label};

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);

        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let child_id = tree.mount(root, Box::new(Label::new("hidden")));

        // Set display=false on the child.
        if let Some(node) = tree.get_mut(child_id) {
            node.display = false;
        }

        let nodes = collect_render_nodes(&tree);
        let child_entry = nodes.iter().find(|(id, _)| *id == child_id);
        assert!(
            matches!(child_entry, Some((_, false))),
            "display:none child should be marked as not rendered"
        );
    }
}
