use crate::widgets::Widget;

use super::Grid;
use super::thin::delegate_widget_to;

pub struct ItemGrid {
    inner: Grid,
}

impl ItemGrid {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            inner: Grid::new(rows, cols),
        }
    }

    pub fn set(&mut self, row: usize, col: usize, child: impl Widget + 'static) {
        self.inner.set(row, col, child);
    }

    pub fn with_cell(mut self, row: usize, col: usize, child: impl Widget + 'static) -> Self {
        self.inner = self.inner.with_cell(row, col, child);
        self
    }

    pub fn row_gap(mut self, gap: usize) -> Self {
        self.inner = self.inner.row_gap(gap);
        self
    }

    pub fn col_gap(mut self, gap: usize) -> Self {
        self.inner = self.inner.col_gap(gap);
        self
    }
}

delegate_widget_to!(ItemGrid, inner);
