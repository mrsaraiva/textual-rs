use crate::css::{set_app_active, set_style_context};
use crate::debug::debug_render;
use crate::render::FrameBuffer;
use crate::widgets::{Toast, Widget, border_spacing_from_style};
use rich_rs::{ControlType, Renderable, Segment, Segments};

use super::App;
use super::types::{
    HitTestMap, SYNC_END, SYNC_START, SegmentStreamStats, TOAST_GAP_ROWS, TOAST_SIDE_MARGIN,
    resize_trace_enabled,
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
        self.refresh_size()?;
        let mut sheet = self.default_stylesheet.clone();
        sheet.extend(&self.stylesheet);
        let _active = set_app_active(self.app_active);
        let _guard = set_style_context(sheet);
        let segments = if self.debug_layout.enabled {
            widget.render_styled_with_debug(&self.console, &self.options, &self.debug_layout)
        } else {
            widget.render_styled(&self.console, &self.options)
        };
        let (width, height) = self.options.size;
        let lines = rich_rs::Segment::split_and_crop_lines(segments, width, None, true, false);
        let base_style = self.theme.base.to_rich();
        let mut next = FrameBuffer::from_lines(&lines, width, height, base_style);
        self.compose_notifications(&mut next);
        let now = std::time::Instant::now();
        let dt_ms = now.duration_since(self.last_render_at).as_millis();
        self.last_render_at = now;
        let clear_before_draw = self.clear_on_next_render;
        let diff = prepend_clear_if_needed(next.diff_to_segments(&self.frame), clear_before_draw);
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
        self.hit_test = HitTestMap::from_frame(&next);
        self.apply_layout_info(widget);
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
            for y in 0..toast_height {
                for x in 0..toast_width {
                    let cell = toast_buffer.get(x, y).clone();
                    if cell.continuation {
                        continue;
                    }
                    let tx = x0 + x;
                    let ty = y0 + y;
                    if tx < frame.width && ty < frame.height {
                        *frame.get_mut(tx, ty) = cell;
                    }
                }
            }

            cursor_bottom = y0.saturating_sub(1 + TOAST_GAP_ROWS);
            if cursor_bottom == 0 {
                break;
            }
        }
    }

    pub(super) fn apply_layout_info(&self, root: &mut dyn Widget) {
        fn visit(w: &mut dyn Widget, hit_test: &HitTestMap) {
            if let Some(rect) = hit_test.rect(w.id()) {
                let meta = crate::css::selector_meta_generic(w);
                let resolved = crate::css::resolve_style(w, &meta);
                let line_pad = resolved.line_pad.unwrap_or(0);
                let (top, bottom, left, right) = border_spacing_from_style(&resolved);
                let full_w = rect.x1.saturating_sub(rect.x0) as usize + 1;
                let full_h = rect.y1.saturating_sub(rect.y0) as usize + 1;
                let content_w = full_w
                    .saturating_sub(left + right)
                    .saturating_sub(line_pad.saturating_mul(2))
                    .max(1) as u16;
                let content_h = full_h.saturating_sub(top + bottom).max(1) as u16;
                w.on_layout(content_w, content_h);
            }
            w.visit_children_mut(&mut |child| visit(child, hit_test));
        }
        visit(root, &self.hit_test);
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
        use super::super::types::HitTestMap;
        use crate::widgets::{AppRoot, DataTable, Panel, WidgetRenderable};

        let console = rich_rs::Console::new();
        let mut options = console.options().clone();
        options.size = (20, 6);
        options.max_width = 20;
        options.max_height = 6;

        let table = DataTable::new(
            vec!["A".into(), "B".into()],
            vec![
                vec!["r0".into(), "c0".into()],
                vec!["r1".into(), "c1".into()],
            ],
        );
        let table_id = table.id();
        let panel = Panel::new(table);
        let mut root = AppRoot::new().with_child(panel);

        let sheet = crate::css::default_widget_stylesheet();
        let _guard = crate::css::set_style_context(sheet);
        let renderable = WidgetRenderable::new(&root);
        let buf = FrameBuffer::from_renderable(&console, &options, &renderable, None);

        let hit_test = HitTestMap::from_frame(&buf);
        let rect = hit_test.rect(table_id).expect("table bounds missing");
        assert!(
            rect.x0 > 0 || rect.y0 > 0,
            "table should not start at origin"
        );

        let (lx, ly) = hit_test.content_local_coords(&mut root, table_id, rect.x0, rect.y0);
        assert_eq!((lx, ly), (0, 0));
    }
}
