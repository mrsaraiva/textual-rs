use crate::style::{Dock, Style, Split};
use crate::widget_tree::Rect;

/// Extract border spacing (top, bottom, left, right) from a style.
pub(crate) fn border_spacing(style: &Style) -> (u16, u16, u16, u16) {
    let top = if style.border_top.is_set() { 1 } else { 0 };
    let right = if style.border_right.is_set() { 1 } else { 0 };
    let bottom = if style.border_bottom.is_set() { 1 } else { 0 };
    let left = if style.border_left.is_set() { 1 } else { 0 };
    (top, bottom, left, right)
}

// ---------------------------------------------------------------------------
// Region
// ---------------------------------------------------------------------------

/// A positioned rectangle in terminal cells (x, y, width, height form).
///
/// Complements [`Rect`] (x0/y0/x1/y1 form) used by `WidgetTree` for storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Region {
    pub const ZERO: Self = Self {
        x: 0,
        y: 0,
        width: 0,
        height: 0,
    };

    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Convert to the x0/y0/x1/y1 [`Rect`] used by `WidgetTree`.
    pub(crate) fn to_rect(self) -> Rect {
        Rect {
            x0: self.x,
            y0: self.y,
            x1: self.x.saturating_add(self.width),
            y1: self.y.saturating_add(self.height),
        }
    }
}

// ---------------------------------------------------------------------------
// CarveDir: shared direction for dock and split edge-carving
// ---------------------------------------------------------------------------

/// Direction for edge-carving layout (shared by dock and split).
#[derive(Clone, Copy)]
pub(crate) enum CarveDir {
    Top,
    Right,
    Bottom,
    Left,
}

impl From<Dock> for CarveDir {
    fn from(d: Dock) -> Self {
        match d {
            Dock::Top => CarveDir::Top,
            Dock::Right => CarveDir::Right,
            Dock::Bottom => CarveDir::Bottom,
            Dock::Left => CarveDir::Left,
        }
    }
}

impl From<Split> for CarveDir {
    fn from(s: Split) -> Self {
        match s {
            Split::Top => CarveDir::Top,
            Split::Right => CarveDir::Right,
            Split::Bottom => CarveDir::Bottom,
            Split::Left => CarveDir::Left,
        }
    }
}
