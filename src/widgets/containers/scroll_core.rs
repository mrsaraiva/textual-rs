use super::scroll_view::ScrollView;
use crate::widgets::scrollbar;

/// Shared scroll math helpers used across scroll container wrappers.
///
/// Transitional extraction: this centralizes the line-based helpers previously
/// consumed ad-hoc via `ScrollView::*` associated functions.
pub struct ScrollCore;

impl ScrollCore {
    pub fn max_offset(content_len: usize, viewport_len: usize) -> usize {
        scrollbar::max_offset(content_len, viewport_len)
    }

    pub fn clamp_offset(offset: usize, content_len: usize, viewport_len: usize) -> usize {
        scrollbar::clamp_offset(offset, content_len, viewport_len)
    }

    pub fn scroll_by(offset: usize, delta: i32, content_len: usize, viewport_len: usize) -> usize {
        scrollbar::scroll_by(offset, delta, content_len, viewport_len)
    }

    pub fn scroll_end(content_len: usize, viewport_len: usize) -> usize {
        scrollbar::scroll_end(content_len, viewport_len)
    }

    pub fn thumb(
        track_len: usize,
        content_len: usize,
        viewport_len: usize,
        offset: usize,
    ) -> (usize, usize) {
        scrollbar::thumb_range(track_len, content_len, viewport_len, offset)
    }

    pub fn drag_offset(
        pointer: usize,
        grab_offset: usize,
        track_len: usize,
        content_len: usize,
        viewport_len: usize,
        current_offset: usize,
    ) -> usize {
        scrollbar::drag_to_offset(
            pointer,
            grab_offset,
            track_len,
            content_len,
            viewport_len,
            current_offset,
        )
    }

    pub fn scrollbar_styles() -> (rich_rs::Style, rich_rs::Style, rich_rs::Style) {
        ScrollView::line_scrollbar_styles()
    }
}
