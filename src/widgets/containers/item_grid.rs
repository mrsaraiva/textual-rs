use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::compose::ComposeResult;
use crate::widgets::{Container, Widget};

use crate::widgets::delegate::delegate_widget_to;

pub struct ItemGrid {
    inner: Container,
    stretch_height: AtomicBool,
    min_column_width: AtomicUsize,
    max_column_width: AtomicUsize,
    regular: AtomicBool,
}

impl ItemGrid {
    pub fn new() -> Self {
        Self {
            inner: Container::new(),
            stretch_height: AtomicBool::new(true),
            min_column_width: AtomicUsize::new(0),
            max_column_width: AtomicUsize::new(0),
            regular: AtomicBool::new(false),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.inner.push(child);
        self
    }

    pub fn with_compose(mut self, children: ComposeResult) -> Self {
        self.inner = self.inner.with_compose(children);
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.inner.push(child);
    }

    pub fn stretch_height(self, stretch_height: bool) -> Self {
        self.stretch_height.store(stretch_height, Ordering::Relaxed);
        self
    }

    pub fn min_column_width(self, width: Option<usize>) -> Self {
        self.min_column_width
            .store(width.unwrap_or(0), Ordering::Relaxed);
        self
    }

    pub fn max_column_width(self, width: Option<usize>) -> Self {
        self.max_column_width
            .store(width.unwrap_or(0), Ordering::Relaxed);
        self
    }

    pub fn regular(self, regular: bool) -> Self {
        self.regular.store(regular, Ordering::Relaxed);
        self
    }

    pub fn stretch_height_value(&self) -> bool {
        self.stretch_height.load(Ordering::Relaxed)
    }

    pub fn min_column_width_value(&self) -> Option<usize> {
        let width = self.min_column_width.load(Ordering::Relaxed);
        if width == 0 { None } else { Some(width) }
    }

    pub fn max_column_width_value(&self) -> Option<usize> {
        let width = self.max_column_width.load(Ordering::Relaxed);
        if width == 0 { None } else { Some(width) }
    }

    pub fn regular_value(&self) -> bool {
        self.regular.load(Ordering::Relaxed)
    }
}

impl Default for ItemGrid {
    fn default() -> Self {
        Self::new()
    }
}

delegate_widget_to!(ItemGrid, inner);
