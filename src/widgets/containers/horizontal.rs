use crate::compose::ComposeResult;
use crate::widgets::Widget;

use super::thin::delegate_widget_to;
use super::{Row, RowAlign};

pub struct Horizontal {
    row: Row,
}

impl Horizontal {
    pub fn new() -> Self {
        Self { row: Row::new() }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.row = self.row.with_child(child);
        self
    }

    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        self.row = self.row.with_compose(children);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.row.push(child);
    }

    pub fn align(mut self, align: RowAlign) -> Self {
        self.row = self.row.align(align);
        self
    }
}

delegate_widget_to!(Horizontal, row);
